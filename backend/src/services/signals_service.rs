use crate::errors::app_error::AppError;
use crate::models::structured_output::{InvestmentSignal, RedFlag, StructuredSectionData};
use crate::services::llm_service::LlmService;
use futures::future::join_all;
use serde_json::Value;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::info;

/// Service for extracting investment signals and red flags
pub struct SignalsService {
    llm_service: Arc<dyn LlmService>,
}

#[derive(Debug, Deserialize)]
struct SignalsResponse {
    signals: Vec<SignalItem>,
    red_flags: Vec<RedFlagItem>,
}

#[derive(Debug, Deserialize)]
struct SignalItem {
    #[serde(rename = "type")]
    signal_type: String,
    description: String,
    #[serde(default)]
    confidence: f32,
}

#[derive(Debug, Deserialize)]
struct RedFlagItem {
    #[serde(rename = "type")]
    flag_type: String,
    description: String,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    evidence_text: Option<String>,
    #[serde(default)]
    evidence_slide_number: Option<usize>,
}

impl SignalsService {
    /// Create a new signals service
    pub fn new(llm_service: Arc<dyn LlmService>) -> Self {
        Self { llm_service }
    }

    /// Extract signals and red flags from all sections
    ///
    /// # Arguments
    /// * `sections` - Structured section data to analyze
    ///
    /// # Returns
    /// * `Ok((Vec<InvestmentSignal>, Vec<RedFlag>))` - Signals and red flags
    /// * `Err(AppError)` - Error if extraction fails
    pub async fn extract_signals_and_flags(
        &self,
        sections: &mut [StructuredSectionData],
    ) -> Result<(Vec<InvestmentSignal>, Vec<RedFlag>), AppError> {
        info!(
            "Extracting signals and red flags from {} sections",
            sections.len()
        );

        // Limit concurrent LLM calls for signals extraction (configurable via env)
        let max_concurrency = std::env::var("LLM_SIGNALS_CONCURRENCY")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&v| (1..=8).contains(&v))
            .unwrap_or(1);
        let semaphore = Arc::new(Semaphore::new(max_concurrency));

        // Start with any signals/red flags that are already present on the sections
        // (e.g. populated by a combined extraction call).
        let mut all_signals: Vec<InvestmentSignal> =
            sections.iter().flat_map(|s| s.signals.clone()).collect();
        let mut all_red_flags: Vec<RedFlag> =
            sections.iter().flat_map(|s| s.red_flags.clone()).collect();

        // Process additional signals extraction in parallel (bounded concurrency)
        let signals_futures: Vec<_> = sections
            .iter()
            .filter(|section| {
                section.signals.is_empty()
                    && section.red_flags.is_empty()
                    && !section.data.is_null()
                    && !section
                        .data
                        .as_object()
                        .map(|o| o.is_empty())
                        .unwrap_or(true)
            })
            .map(|section| {
                let section_name = section.section_name.clone();
                let section_text = format!(
                    "Section: {}\nData: {}",
                    section.section_name,
                    serde_json::to_string_pretty(&section.data).unwrap_or_default()
                );
                let section_text = crate::services::preprocessing_service::truncate_for_llm(&section_text, 4000);
                let section_text_lower = section_text.to_lowercase();
                let llm_service = self.llm_service.clone();
                let semaphore = semaphore.clone();

                async move {
                    let _permit = semaphore.acquire().await.expect("semaphore closed");
                    info!("Extracting signals for section: {}", section_name);

                    match llm_service.extract_signals(&section_text).await {
                        Ok(json_str) => match serde_json::from_str::<SignalsResponse>(&json_str) {
                            Ok(response) => Some((section_name, section_text_lower, response)),
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to parse signals JSON for {}: {}",
                                    section_name,
                                    e
                                );
                                None
                            }
                        },
                        Err(e) => {
                            tracing::warn!("Failed to extract signals for {}: {}", section_name, e);
                            None
                        }
                    }
                }
            })
            .collect();

        // Wait for all signals extractions to complete in parallel
        info!(
            "Extracting signals from {} sections in parallel...",
            signals_futures.len()
        );
        let signals_results = join_all(signals_futures).await;

        // Update sections with signals and collect overall lists
        for (section_name, section_text_lower, response) in signals_results.into_iter().flatten() {
            let section_signals: Vec<InvestmentSignal> = response
                .signals
                .into_iter()
                .map(|s| InvestmentSignal {
                    signal_type: s.signal_type,
                    description: s.description,
                    confidence: s.confidence,
                    section: section_name.clone(),
                })
                .collect();

            let section_data = sections
                .iter()
                .find(|s| s.section_name == section_name)
                .map(|s| s.data.clone())
                .unwrap_or_else(|| serde_json::json!({}));
            let missing_fields = Self::missing_or_empty_fields(&section_data);

            let mut section_flags: Vec<RedFlag> = response
                .red_flags
                .into_iter()
                .map(|f| {
                    let mut description = f.description.trim().to_string();
                    if Self::is_placeholder(&description) {
                        description.clear();
                    }
                    let mut flag_type = f.flag_type.trim().to_lowercase();
                    if flag_type.contains('|') {
                        flag_type = flag_type
                            .split('|')
                            .map(str::trim)
                            .find(|p| !p.is_empty())
                            .unwrap_or("risk_factor")
                            .to_string();
                    }
                    let severity = if f.severity.trim().is_empty() {
                        "medium".to_string()
                    } else {
                        f.severity.trim().to_lowercase()
                    };
                    let normalized_evidence = f
                        .evidence_text
                        .as_ref()
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty() && !Self::is_placeholder(s))
                        .map(|s| s.to_string());
                    let proof_text = normalized_evidence
                        .clone()
                        .or_else(|| (!description.is_empty()).then_some(description.clone()))
                        .or_else(|| {
                            if flag_type.contains("missing") && !missing_fields.is_empty() {
                                Some(format!("Missing fields: {}", missing_fields.join(", ")))
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| "Derived from structured section data".to_string());

                    RedFlag {
                        evidence_confirmed: Some(
                            if proof_text.starts_with("Missing fields: ") {
                                !missing_fields.is_empty()
                            } else if proof_text == "Derived from structured section data" {
                                false
                            } else {
                                section_text_lower.contains(&proof_text.to_lowercase())
                            },
                        ),
                        flag_type,
                        description: description.clone(),
                        severity,
                        section: section_name.clone(),
                        evidence_text: Some(proof_text),
                        evidence_slide_number: f.evidence_slide_number,
                        source: Some(if description.is_empty() && normalized_evidence.is_none() {
                            "derived_analysis".to_string()
                        } else {
                            "llm_signals".to_string()
                        }),
                        reason_details: Some(if !description.is_empty() {
                            format!("Signal extraction model flagged: {}", description)
                        } else if !missing_fields.is_empty() {
                            format!(
                                "Derived from missing/empty structured fields: {}",
                                missing_fields.join(", ")
                            )
                        } else {
                            "Flag generated by signal extraction model without detailed description.".to_string()
                        }),
                    }
                })
                .collect();
            Self::clean_noise_red_flags(&mut section_flags);

            if let Some(section) = sections.iter_mut().find(|s| s.section_name == section_name) {
                section.signals = section_signals.clone();
                section.red_flags = section_flags.clone();
            }

            all_signals.extend(section_signals);
            all_red_flags.extend(section_flags);
        }

        for section in sections.iter_mut() {
            Self::dedup_section_red_flags(&mut section.red_flags);
        }
        Self::dedup_global_red_flags(&mut all_red_flags);

        info!(
            "Signals extraction complete: {} signals, {} red flags",
            all_signals.len(),
            all_red_flags.len()
        );

        Ok((all_signals, all_red_flags))
    }

    fn dedup_section_red_flags(red_flags: &mut Vec<RedFlag>) {
        let mut seen = HashSet::new();
        let mut deduped = Vec::with_capacity(red_flags.len());

        for flag in red_flags.drain(..) {
            let key = format!(
                "{}|{}|{}",
                flag.section.trim().to_lowercase(),
                flag.flag_type.trim().to_lowercase(),
                flag.description.trim().to_lowercase()
            );
            if seen.insert(key) {
                deduped.push(flag);
            }
        }

        *red_flags = deduped;
    }

    fn dedup_global_red_flags(red_flags: &mut Vec<RedFlag>) {
        let mut seen = HashSet::new();
        let mut deduped = Vec::with_capacity(red_flags.len());

        for flag in red_flags.drain(..) {
            // Global dedup excludes section to avoid repeated same flag across sections.
            let key = format!(
                "{}|{}",
                flag.flag_type.trim().to_lowercase(),
                flag.description.trim().to_lowercase()
            );
            if seen.insert(key) {
                deduped.push(flag);
            }
        }

        *red_flags = deduped;
    }

    fn clean_noise_red_flags(red_flags: &mut Vec<RedFlag>) {
        red_flags.retain(|f| {
            let desc_empty = f.description.trim().is_empty();
            let proof = f.evidence_text.as_deref().unwrap_or("").trim();
            let useless_proof = proof.is_empty() || proof.eq_ignore_ascii_case("null");
            // Drop only fully-empty/noisy rows that give no user value.
            !(desc_empty && useless_proof)
        });
    }

    fn is_placeholder(value: &str) -> bool {
        let v = value.trim().to_lowercase();
        v.is_empty() || v == "null" || v == "none" || v == "n/a" || v == "-"
    }

    fn missing_or_empty_fields(section_data: &Value) -> Vec<String> {
        let mut out = Vec::new();
        let Some(obj) = section_data.as_object() else {
            return out;
        };
        for (k, v) in obj {
            let is_missing = if v.is_null() {
                true
            } else if let Some(field_obj) = v.as_object() {
                match field_obj.get("value") {
                    Some(val) => val.is_null() || val.as_str().map(Self::is_placeholder).unwrap_or(false),
                    None => false,
                }
            } else {
                v.as_str().map(Self::is_placeholder).unwrap_or(false)
            };
            if is_missing {
                out.push(k.clone());
            }
        }
        out
    }
}
