use crate::errors::app_error::AppError;
use crate::models::structured_output::{InvestmentSignal, RedFlag, StructuredSectionData};
use crate::services::llm_service::LlmService;
use futures::future::join_all;
use serde::Deserialize;
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
                let llm_service = self.llm_service.clone();
                let semaphore = semaphore.clone();

                async move {
                    let _permit = semaphore.acquire().await.expect("semaphore closed");
                    info!("Extracting signals for section: {}", section_name);

                    match llm_service.extract_signals(&section_text).await {
                        Ok(json_str) => match serde_json::from_str::<SignalsResponse>(&json_str) {
                            Ok(response) => Some((section_name, response)),
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
        for (section_name, response) in signals_results.into_iter().flatten() {
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

            let section_flags: Vec<RedFlag> = response
                .red_flags
                .into_iter()
                .map(|f| RedFlag {
                    flag_type: f.flag_type,
                    description: f.description,
                    severity: f.severity,
                    section: section_name.clone(),
                })
                .collect();

            if let Some(section) = sections.iter_mut().find(|s| s.section_name == section_name) {
                section.signals = section_signals.clone();
                section.red_flags = section_flags.clone();
            }

            all_signals.extend(section_signals);
            all_red_flags.extend(section_flags);
        }

        info!(
            "Signals extraction complete: {} signals, {} red flags",
            all_signals.len(),
            all_red_flags.len()
        );

        Ok((all_signals, all_red_flags))
    }
}
