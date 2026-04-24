//! Web comparison and validation: compare extracted deck data vs web sources,
//! update structured JSON and flag inconsistencies. Sets web_validation per section.

use crate::models::structured_output::StructuredSectionData;
use crate::services::web_fetch_service::WebFacts;
use serde_json::Value;
use tracing::info;

/// Compares extracted sections with web-fetched facts and sets web_validation per section.
pub struct WebValidationService;

impl WebValidationService {
    /// Apply web validation to structured sections using fetched web facts.
    /// Sets each section's `web_validation` and can add red flags for inconsistencies.
    pub fn apply_validation(sections: &mut [StructuredSectionData], web_facts: &WebFacts) -> f32 {
        if web_facts.source_label == "none" || web_facts.source_url.is_empty() {
            for section in sections.iter_mut() {
                section.web_validation = Some("No web source available".to_string());
            }
            return 0.0; // no web data -> web consistency score 0
        }

        let mut total_score = 0.0f32;
        let mut count = 0usize;

        for section in sections.iter_mut() {
            let (msg, score) = Self::validate_section(section, web_facts);
            section.web_validation = Some(msg);
            total_score += score;
            count += 1;
        }

        if count == 0 {
            return 0.0;
        }
        (total_score / count as f32).min(1.0)
    }

    /// Validate one section against web facts; return (message, score 0..1)
    fn validate_section(
        section: &mut StructuredSectionData,
        web_facts: &WebFacts,
    ) -> (String, f32) {
        let name_lower = section.section_name.to_lowercase();
        // Score STARTS at 0.0 and earns trust from verified signals
        let mut score = 0.0f32;
        let mut verified: Vec<&str> = Vec::new();
        let mut inconsistencies: Vec<String> = Vec::new();

        // Company / overview checks
        if name_lower.contains("company") || name_lower.contains("overview") {
            if let Some(extracted_name) =
                get_string_from_data(&section.data, &["Name", "Company Name", "Company"])
            {
                let mut matched = false;
                
                if let Some(web_name) = &web_facts.company_name {
                    if fuzzy_match(&extracted_name, web_name) {
                        matched = true;
                    }
                }
                
                if !matched {
                    if let Some(meta_desc) = &web_facts.meta_description {
                        if fuzzy_match(&extracted_name, meta_desc) {
                            matched = true;
                        }
                    }
                }
                
                if matched {
                    score = (score + 0.40).min(1.0);
                    verified.push("company name via meta/title");
                } else {
                    inconsistencies.push(format!(
                        "Company name mismatch: deck says '{}', not found in web meta/title",
                        extracted_name
                    ));
                }
            }
            if let Some(emp_str) =
                get_string_from_data(&section.data, &["Employees", "Team Size", "Employee Count"])
            {
                if web_facts.employees.is_some() {
                    let snippet_hit = web_facts
                        .snippets
                        .join(" ")
                        .to_lowercase()
                        .contains(emp_str.to_lowercase().as_str());
                    if snippet_hit {
                        score = (score + 0.10).min(1.0);
                        verified.push("employee count");
                    }
                }
            }
            // If web snippets are present and non-trivial, give partial credit
            if !web_facts.snippets.is_empty() && web_facts.snippets.iter().any(|s| s.len() > 30) {
                score = (score + 0.30).min(1.0);
                verified.push("web presence confirmed");
            }
        }

        // Market / traction: web snippets verify the space
        if (name_lower.contains("market") || name_lower.contains("traction"))
            && web_facts.snippets.iter().any(|s| s.len() > 50)
        {
            score = (score + 0.20).min(1.0);
            verified.push("market context");
        }

        // Other sections: give neutral score if web source exists
        if score == 0.0 && !web_facts.source_url.is_empty() {
            score = 0.10; // found the company online but section not directly verifiable
        }

        let message = if inconsistencies.is_empty() {
            if verified.is_empty() {
                format!(
                    "Web source found ({}) — section not directly verifiable",
                    web_facts.source_label
                )
            } else {
                format!(
                    "Verified via {}: {}",
                    web_facts.source_label,
                    verified.join(", ")
                )
            }
        } else {
            format!(
                "Inconsistencies found ({}): {}",
                web_facts.source_label,
                inconsistencies.join("; ")
            )
        };

        info!(
            section = %section.section_name,
            web_validation = %message,
            score = score,
            "Web validation applied"
        );
        (message, score)
    }
}

/// Get first string value from JSON data using possible keys (case-sensitive)
fn get_string_from_data(data: &Value, keys: &[&str]) -> Option<String> {
    let obj = data.as_object()?;
    for key in keys {
        if let Some(v) = obj.get(*key) {
            if let Some(s) = v.as_str() {
                return Some(s.trim().to_string());
            }
            if let Some(n) = v.as_i64() {
                return Some(n.to_string());
            }
            if let Some(n) = v.as_f64() {
                return Some(n.to_string());
            }
        }
    }
    None
}

/// Simple fuzzy match: normalized comparison
fn fuzzy_match(a: &str, b: &str) -> bool {
    let na: String = a
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    let nb: String = b
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    na == nb || na.contains(nb.as_str()) || nb.contains(na.as_str())
}
