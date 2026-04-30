use crate::errors::app_error::AppError;
use crate::models::section_model::GroupedDeck;
use crate::models::structured_output::{InvestmentSignal, RedFlag, StructuredSectionData};
use crate::services::llm_service::LlmService;
use futures::future::join_all;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn};

/// Service for extracting structured JSON from grouped deck sections
pub struct StructuringService {
    llm_service: Arc<dyn LlmService>,
}

impl StructuringService {
    /// Create a new structuring service
    pub fn new(llm_service: Arc<dyn LlmService>) -> Self {
        Self { llm_service }
    }

    /// Extract structured data from a grouped deck
    ///
    /// Processes each section through LLM to extract structured key-value pairs
    ///
    /// # Arguments
    /// * `grouped_deck` - The grouped deck with sections
    ///
    /// # Returns
    /// * `Ok(Vec<StructuredSectionData>)` - Structured data for each section
    /// * `Err(AppError)` - Error if extraction fails
    pub async fn extract_structured_data(
        &self,
        grouped_deck: &GroupedDeck,
    ) -> Result<Vec<StructuredSectionData>, AppError> {
        info!(
            "Starting structured extraction for deck: {} ({} sections)",
            grouped_deck.filename,
            grouped_deck.sections.len()
        );

        let max_concurrency = std::env::var("LLM_STRUCTURING_CONCURRENCY")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&v| (1..=8).contains(&v))
            .unwrap_or(1);
        let semaphore = Arc::new(Semaphore::new(max_concurrency));

        // Process sections in parallel (with bounded concurrency)
        let extraction_futures: Vec<_> = grouped_deck.sections
            .iter()
            .filter(|section| !section.slides.is_empty())
            .map(|section| {
                let raw_section_text = self.combine_section_text(section);
                let section_name = section.section_name.clone();
                // Pass text through preprocessing filter BEFORE sending to LLM
                let section_text = crate::services::preprocessing_service::clean_text(
                    &section_name,
                    &raw_section_text,
                );
                // Truncate text to avoid long prompts and timeouts
                let section_text = crate::services::preprocessing_service::truncate_for_llm(&section_text, 4000);
                let schema_hint = self.get_schema_hint(&section.section_name);
                let llm_service = self.llm_service.clone();
                let semaphore = semaphore.clone();

                async move {
                    // Acquire permit to respect concurrency limit
                    let _permit = semaphore.acquire().await.expect("semaphore closed");

                    if section_text.trim().is_empty() {
                        warn!("Empty section text for: {}", section_name);
                        return Ok(Self::empty_structured_section(section_name));
                    }

                    info!("Extracting structured data for section: {}", section_name);

                    let snippet = if section_text.len() > 100 {
                        format!("{}...", &section_text[..100])
                    } else {
                        section_text.clone()
                    };
                    info!("Section text snippet for {}: {}", section_name, snippet.replace('\n', " "));

                    // Extract structured JSON
                    let structured_json = match llm_service
                        .generate_structured_json(
                            &format!("Extract structured data, a concise summary, and investment signals from the {} section", section_name),
                            &section_text,
                            Some(&schema_hint),
                        )
                        .await
                    {
                        Ok(json) => json,
                        Err(e) => {
                            warn!(
                                "Structured extraction failed for {}: {}. Using empty structured section.",
                                section_name, e
                            );
                            return Ok(Self::empty_structured_section(section_name));
                        }
                    };

                    let root: serde_json::Value = serde_json::from_str(&structured_json).unwrap_or_else(|e| {
                        warn!("Failed to parse combined JSON for {}: {}. Response was: {}", section_name, e, structured_json);
                        serde_json::json!({})
                    });

                    // Support both the new combined schema and the older "data only" schema.
                    let (data, summary, signals, red_flags) = if let serde_json::Value::Object(map) = &root {
                        // Prefer "structured_data", fall back to "data", then whole object.
                        let data_value = map
                            .get("structured_data")
                            .or_else(|| map.get("data"))
                            .cloned()
                            .unwrap_or_else(|| root.clone());

                        let summary_value = map
                            .get("summary")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.trim().is_empty())
                            .map(|s| s.to_string());

                        let section_signals: Vec<InvestmentSignal> = map
                            .get("signals")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|item| {
                                        let obj = item.as_object()?;
                                        let signal_type = obj.get("type")?.as_str()?.to_string();
                                        let mut description = obj
                                            .get("description")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .trim()
                                            .to_string();
                                        if description.is_empty() {
                                            description = "Signal reported by model".to_string();
                                        }
                                        let confidence = obj
                                            .get("confidence")
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(0.0) as f32;
                                        Some(InvestmentSignal {
                                            signal_type,
                                            description,
                                            confidence,
                                            section: section_name.clone(),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();

                        let mut section_red_flags: Vec<RedFlag> = map
                            .get("red_flags")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|item| {
                                        let obj = item.as_object()?;
                                        let flag_type = obj.get("type")?.as_str()?.to_string();
                                        let mut description = obj
                                            .get("description")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .trim()
                                            .to_string();
                                        if description.is_empty() {
                                            description = format!("Flag reported by model: {}", flag_type);
                                        }
                                        let severity = obj
                                            .get("severity")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("medium")
                                            .to_string();
                                        let evidence_text = obj
                                            .get("evidence_text")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.trim().to_string())
                                            .filter(|s| !s.is_empty());
                                        let evidence_slide_number = obj
                                            .get("evidence_slide_number")
                                            .and_then(|v| v.as_u64())
                                            .map(|n| n as usize);
                                        Some(RedFlag {
                                            flag_type,
                                            description,
                                            severity,
                                            section: section_name.clone(),
                                            evidence_text,
                                            evidence_slide_number,
                                            evidence_confirmed: None,
                                            source: Some("llm_structuring".to_string()),
                                            reason_details: None,
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        Self::normalize_and_dedup_red_flags(
                            &mut section_red_flags,
                            &section_text,
                            &data_value,
                        );

                        // Implement Post-Extraction Hallucination Filter
                        let mut final_data_value = data_value.clone();
                        let section_text_lower = section_text.to_lowercase();
                        
                        if let serde_json::Value::Object(ref mut map) = final_data_value {
                            let mut keys_to_remove = Vec::new();
                            
                            for (key, val) in map.iter_mut() {
                                if let serde_json::Value::Object(ref mut field_obj) = val {
                                    let mut confidence = 0.0;
                                    let mut is_hallucinated = false;
                                    
                                    if let Some(source_str) = field_obj.get("source_text").and_then(|s| s.as_str()) {
                                        if source_str.trim().is_empty() || source_str.to_lowercase() == "null" {
                                            confidence = 0.0;
                                            is_hallucinated = true;
                                        } else if section_text_lower.contains(&source_str.to_lowercase()) {
                                            confidence = 1.0; // verbatim match
                                        } else {
                                            // Check if all significant words exist
                                            let words: Vec<&str> = source_str.split_whitespace().filter(|w| w.len() > 3).collect();
                                            let mut found_words = 0;
                                            for w in &words {
                                                if section_text_lower.contains(&w.to_lowercase()) {
                                                    found_words += 1;
                                                }
                                            }
                                            if !words.is_empty() && (found_words as f32 / words.len() as f32) > 0.5 {
                                                confidence = 0.7; // logical inference
                                            } else {
                                                confidence = 0.0;
                                                is_hallucinated = true;
                                            }
                                        }
                                    } else {
                                        is_hallucinated = true; // no source provided
                                    }
                                    
                                    // Rule: never invent URLs or company names
                                    let key_lower = key.to_lowercase();
                                    if is_hallucinated {
                                        if key_lower.contains("website") || key_lower.contains("url") || key_lower.contains("name") {
                                            keys_to_remove.push(key.clone());
                                        }
                                    }
                                    
                                    field_obj.insert("confidence".to_string(), serde_json::json!(confidence));
                                }
                            }
                            
                            for k in keys_to_remove {
                                map.remove(&k);
                            }
                        }

                        (final_data_value, summary_value, section_signals, section_red_flags)
                    } else {
                        (root, None, Vec::new(), Vec::new())
                    };

                    Ok(StructuredSectionData {
                        section_name,
                        data,
                        validation: crate::models::structured_output::ValidationResults {
                            fields_validated: 0,
                            fields_passed: 0,
                            errors: vec![],
                            score: 0.0,
                        },
                        summary,
                        signals,
                        red_flags,
                        confidence: 0.0,
                        threshold_flags: vec![],
                        web_validation: None,
                    })
                }
            })
            .collect();

        // Wait for all extractions to complete in parallel
        info!(
            "Processing {} sections in parallel...",
            extraction_futures.len()
        );
        let structured_sections_results: Vec<Result<StructuredSectionData, AppError>> =
            join_all(extraction_futures).await;

        // Collect results and propagate any error
        let mut structured_sections = Vec::new();
        for res in structured_sections_results {
            structured_sections.push(res?);
        }

        info!(
            "Structured extraction complete: {} sections processed",
            structured_sections.len()
        );

        Ok(structured_sections)
    }

    fn empty_structured_section(section_name: String) -> StructuredSectionData {
        StructuredSectionData {
            section_name,
            data: serde_json::json!({}),
            validation: crate::models::structured_output::ValidationResults {
                fields_validated: 0,
                fields_passed: 0,
                errors: vec![],
                score: 0.0,
            },
            summary: None,
            signals: vec![],
            red_flags: vec![],
            confidence: 0.0,
            threshold_flags: vec![],
            web_validation: None,
        }
    }

    fn normalize_and_dedup_red_flags(
        red_flags: &mut Vec<RedFlag>,
        section_text: &str,
        section_data: &serde_json::Value,
    ) {
        let section_text_lower = section_text.to_lowercase();
        let mut seen = HashSet::new();
        let mut deduped = Vec::with_capacity(red_flags.len());
        let missing_fields = Self::missing_or_empty_fields(section_data);

        for mut flag in red_flags.drain(..) {
            let normalized_type = flag.flag_type.trim().to_lowercase();
            let normalized_desc = flag.description.trim().to_lowercase();
            let key = format!("{normalized_type}|{normalized_desc}");
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);

            let evidence = flag
                .evidence_text
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                // Non-strict fallback: always include a source string.
                .unwrap_or_else(|| {
                    if !flag.description.trim().is_empty() {
                        flag.description.trim().to_string()
                    } else if !missing_fields.is_empty() {
                        format!("Missing structured fields: {}", missing_fields.join(", "))
                    } else {
                        format!("Derived from model output for {}", flag.flag_type)
                    }
                });
            let evidence_confirmed = section_text_lower.contains(&evidence.to_lowercase());
            flag.evidence_text = Some(evidence);
            flag.evidence_confirmed = Some(evidence_confirmed);
            if flag.reason_details.is_none() {
                flag.reason_details = if !missing_fields.is_empty() {
                    Some(format!(
                        "Flag likely triggered by missing/empty fields: {}",
                        missing_fields.join(", ")
                    ))
                } else {
                    Some("Flag extracted by structuring LLM analysis.".to_string())
                };
            }
            if flag.source.is_none() {
                flag.source = Some("llm_structuring".to_string());
            }

            deduped.push(flag);
        }

        *red_flags = deduped;
    }

    fn missing_or_empty_fields(section_data: &serde_json::Value) -> Vec<String> {
        let mut out = Vec::new();
        let Some(obj) = section_data.as_object() else {
            return out;
        };
        for (k, v) in obj {
            let is_missing = if v.is_null() {
                true
            } else if let Some(field_obj) = v.as_object() {
                match field_obj.get("value") {
                    Some(val) => val.is_null() || val.as_str().map(|s| s.trim().is_empty()).unwrap_or(false),
                    None => false,
                }
            } else {
                v.as_str().map(|s| s.trim().is_empty()).unwrap_or(false)
            };
            if is_missing {
                out.push(k.clone());
            }
        }
        out
    }

    fn combine_section_text(&self, section: &crate::models::section_model::SectionGroup) -> String {
        let mut parts = Vec::new();

        for slide in &section.slides {
            for element in &slide.elements {
                use crate::models::extraction_model::Element;
                match element {
                    Element::Title { text, .. } if !text.trim().is_empty() => {
                        parts.push(format!("Title: {}", text.trim()));
                    }
                    Element::Subtitle { text, .. } | Element::SectionHeader { text, .. }
                        if !text.trim().is_empty() =>
                    {
                        parts.push(format!("Header: {}", text.trim()));
                    }
                    Element::TextBlock { text, .. } if !text.trim().is_empty() => {
                        parts.push(text.trim().to_string());
                    }
                    Element::BulletList { items, .. } => {
                        for item in items {
                            if !item.trim().is_empty() {
                                parts.push(format!("\u{2022} {}", item.trim()));
                            }
                        }
                    }
                    Element::Table { headers, rows, .. } => {
                        if !headers.is_empty() {
                            parts.push(format!("Table: {}", headers.join(" | ")));
                        }
                        for row in rows {
                            let cells: Vec<&str> = row
                                .iter()
                                .map(|c| c.trim())
                                .filter(|c| !c.is_empty())
                                .collect();
                            if !cells.is_empty() {
                                parts.push(format!("Row: {}", cells.join(" | ")));
                            }
                        }
                    }
                    Element::Statistic { value, label, .. } if !value.trim().is_empty() => {
                        if label.trim().is_empty() {
                            parts.push(format!("Metric: {}", value.trim()));
                        } else {
                            parts.push(format!(
                                "Metric: {} \u{2014} {}",
                                label.trim(),
                                value.trim()
                            ));
                        }
                    }
                    Element::Image {
                        ocr_text: Some(ocr),
                        ..
                    } => {
                        let t = ocr.trim();
                        if t.len() > 5 {
                            parts.push(format!("Image text: {}", t));
                        }
                    }
                    _ => {}
                }
            }
        }

        parts.join("\n")
    }

    /// Get schema hint based on section name.
    /// Strong schema instructions reduce hallucinations: use these fields when present; null if absent.
    fn get_schema_hint(&self, section_name: &str) -> String {
        const NULL_RULE: &str = "\nUse only these fields when present in the text. If a value is not present, return null. Do not invent values.";
        let name_lower = section_name.to_lowercase();

        let base = if name_lower.contains("company") || name_lower.contains("overview") {
            r#"Extract:
- Name: company name
- Website: company website URL (if mentioned)
- Founded: founding year/date
- Mission: mission statement
- Employees: number of employees
- Location: headquarters location
- Industry: industry/sector"#
        } else if name_lower.contains("market") || name_lower.contains("opportunity") {
            r#"Extract:
- TAM: total addressable market (number)
- SAM: serviceable addressable market (number)
- SOM: serviceable obtainable market (number)
- MarketSize: market size in currency
- GrowthRate: growth rate percentage
- Trends: key market trends"#
        } else if name_lower.contains("financial") || name_lower.contains("revenue") {
            r#"Extract:
- Revenue: current revenue (number)
- RevenueGrowth: revenue growth percentage
- BurnRate: monthly burn rate (number)
- Runway: months of runway (number)
- UnitEconomics: unit economics details
- ProfitMargin: profit margin percentage"#
        } else if name_lower.contains("traction") || name_lower.contains("metrics") {
            r#"Extract:
- Users: number of users/customers
- Growth: growth metrics
- Milestones: key milestones achieved
- Metrics: important KPIs"#
        } else if name_lower.contains("team") || name_lower.contains("founder") {
            r#"Extract:
- Founders: founder names and backgrounds
- TeamSize: team size
- KeyPeople: key team members
- Advisors: advisors/board members"#
        } else if name_lower.contains("funding") || name_lower.contains("ask") {
            r#"Extract:
- Amount: funding amount requested (number)
- UseOfFunds: how funds will be used
- PreviousFunding: previous funding rounds
- Investors: current investors"#
        } else {
            return "Extract key-value pairs relevant to this section. Include numbers, dates, and important facts. If a value is not present, return null. Do not invent values.\n\nReturn a JSON object with the following top-level keys:\n- structured_data: A JSON OBJECT (NOT an array) where EACH KEY is the field name, and the value is an object: { \"value\": <value>, \"source_text\": \"<exact quote>\", \"slide_number\": <number>, \"confidence\": <float> }\n- summary: string (2–3 sentences)\n- signals: array of objects\n- red_flags: array of objects with evidence_text and evidence_slide_number".to_string();
        };

        format!(
            "{base}{null_rule}\n\nReturn a JSON object with the following top-level keys:\n- structured_data: A JSON OBJECT (NOT an array) where EACH KEY is the field name, and the value is an object: {{ \"value\": <value>, \"source_text\": \"<exact quote>\", \"slide_number\": <number>, \"confidence\": <float> }}\n- summary: string (2–3 sentences summarizing this section)\n- signals: array of objects {{ \"type\": string, \"description\": \"<verbatim quote>\", \"confidence\": number }}\n- red_flags: array of objects {{ \"type\": string, \"description\": \"<verbatim quote>\", \"severity\": \"low\" | \"medium\" | \"high\" | \"critical\", \"evidence_text\": \"<exact supporting quote>\", \"evidence_slide_number\": <number> }}",
            base = base,
            null_rule = NULL_RULE
        )
    }
}
