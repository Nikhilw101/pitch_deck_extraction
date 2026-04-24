use crate::errors::app_error::AppError;
use crate::models::structured_output::StructuredSectionData;
use crate::services::llm_service::LlmService;
use futures::future::join_all;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::info;

/// Service for generating summaries from structured sections
pub struct SummarizationService {
    llm_service: Arc<dyn LlmService>,
}

impl SummarizationService {
    /// Create a new summarization service
    pub fn new(llm_service: Arc<dyn LlmService>) -> Self {
        Self { llm_service }
    }

    /// Generate summaries for all sections
    ///
    /// # Arguments
    /// * `sections` - Structured section data to summarize
    ///
    /// # Returns
    /// * `Ok(())` - Successfully generated summaries
    /// * `Err(AppError)` - Error if summarization fails
    pub async fn generate_section_summaries(
        &self,
        sections: &mut [StructuredSectionData],
    ) -> Result<(), AppError> {
        info!("Generating summaries for {} sections", sections.len());

        // Limit concurrent LLM calls for summaries (configurable via env)
        let max_concurrency = std::env::var("LLM_SUMMARY_CONCURRENCY")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&v| (1..=8).contains(&v))
            .unwrap_or(2);
        let semaphore = Arc::new(Semaphore::new(max_concurrency));

        // Process summaries in parallel (bounded concurrency)
        let summary_futures: Vec<_> = sections
            .iter_mut()
            .filter(|section| {
                section.summary.is_none()
                    && !section.data.is_null()
                    && !section
                        .data
                        .as_object()
                        .map(|o| o.is_empty())
                        .unwrap_or(true)
            })
            .map(|section| {
                let section_name = section.section_name.clone();
                let section_text = self.format_section_for_summary(section);
                let llm_service = self.llm_service.clone();
                let semaphore = semaphore.clone();

                async move {
                    let _permit = semaphore.acquire().await.expect("semaphore closed");
                    info!("Generating summary for section: {}", section_name);

                    match llm_service.generate_summary(&section_text, Some(3)).await {
                        Ok(summary) => Some((section_name, summary)),
                        Err(e) => {
                            tracing::warn!(
                                "Failed to generate summary for {}: {}",
                                section_name,
                                e
                            );
                            None
                        }
                    }
                }
            })
            .collect();

        // Wait for all summaries to complete in parallel
        info!(
            "Generating {} summaries in parallel...",
            summary_futures.len()
        );
        let summaries = join_all(summary_futures).await;

        // Update sections with summaries
        for (section_name, summary) in summaries.into_iter().flatten() {
            if let Some(section) = sections.iter_mut().find(|s| s.section_name == section_name) {
                section.summary = Some(summary);
            }
        }

        info!("Summary generation complete");
        Ok(())
    }

    /// Generate overall deck summary
    ///
    /// # Arguments
    /// * `sections` - All structured sections
    ///
    /// # Returns
    /// * `Ok(String)` - Overall summary
    /// * `Err(AppError)` - Error if generation fails
    pub async fn generate_overall_summary(
        &self,
        sections: &[StructuredSectionData],
    ) -> Result<String, AppError> {
        info!("Generating overall deck summary");

        // Combine all section summaries
        let mut combined_text = String::new();
        for section in sections {
            if let Some(ref summary) = section.summary {
                combined_text.push_str(&format!("{}: {}\n\n", section.section_name, summary));
            } else {
                // If no summary, use structured data
                combined_text.push_str(&format!("{}: {}\n\n", section.section_name, section.data));
            }
        }

        if combined_text.trim().is_empty() {
            return Ok("No content available for summary.".to_string());
        }

        const PREFIX: &str = "The following are section summaries and/or structured data from a pitch deck. Produce an executive summary in 3–5 sentences. Always produce a summary.\n\n";
        let prefixed = format!("{}{}", PREFIX, combined_text);
        self.llm_service.generate_summary(&prefixed, Some(5)).await
    }

    /// Format section data for summarization
    fn format_section_for_summary(&self, section: &StructuredSectionData) -> String {
        // Instruct LLM to always summarize structured data; avoids "no summary to provide" refusals
        const PREFIX: &str = "This is structured pitch-deck data. Summarize the key facts in 2–3 sentences. Focus on company name, metrics, revenue, traction, team, or financials that ARE present. Always produce a summary; do not refuse even if some fields are null.\n\n";
        format!(
            "{}Section: {}\nData: {}",
            PREFIX,
            section.section_name,
            serde_json::to_string_pretty(&section.data).unwrap_or_default()
        )
    }
}
