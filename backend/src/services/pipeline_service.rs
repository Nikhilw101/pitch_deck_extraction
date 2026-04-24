use crate::errors::app_error::AppError;
use crate::models::deck_model::ExtractedDeck;
use crate::models::section_model::GroupedDeck;
use crate::models::structured_output::StructuredDeckOutput;
use crate::models::vector_model::EmbeddingRecord;
use crate::services::embedding_service::EmbeddingService;
use crate::services::llm_service::LlmService;
use crate::services::scoring_service;
use crate::services::section_classification_service::{
    SectionClassificationService, SectionGroupingService,
};
use crate::services::signals_service::SignalsService;
use crate::services::structuring_service::StructuringService;
use crate::services::summarization_service::SummarizationService;
use crate::services::validation_service::ValidationService;
use crate::services::vector_store_service::VectorStore;
use crate::services::web_fetch_service::WebFetchService;
use crate::services::web_validation_service::WebValidationService;
use chrono::Utc;
use std::sync::Arc;
use tracing::{info, warn};

/// Consolidated result of pipeline phases 2-5.
pub struct PipelineExecutionResult {
    pub indexing_status: String,
    pub grouped_deck: Option<GroupedDeck>,
    pub structured_output: Option<StructuredDeckOutput>,
}

/// Service that orchestrates Phase 2 processing: embedding generation and vector indexing.
pub struct PipelineService {
    embedding_service: Arc<dyn EmbeddingService>,
    vector_store: Arc<VectorStore>,
}

impl PipelineService {
    pub fn new(
        embedding_service: Arc<dyn EmbeddingService>,
        vector_store: Arc<VectorStore>,
    ) -> Self {
        Self {
            embedding_service,
            vector_store,
        }
    }

    /// Run Phase 2 -> Phase 5 in order without duplicating orchestration logic in controllers.
    pub async fn run_full_pipeline(
        &self,
        deck: &ExtractedDeck,
        llm_service: Arc<dyn LlmService>,
    ) -> PipelineExecutionResult {
        // Phase 2: Embeddings & Indexing
        let indexing_status = match self.process_deck(deck).await {
            Ok(_) => "indexed".to_string(),
            Err(e) => {
                warn!("Phase 2 processing failed: {}", e);
                format!("failed: {}", e)
            }
        };

        // Phase 3: Section Classification & Grouping
        let grouped_deck = if indexing_status == "indexed" {
            match self.classify_and_group_slides(deck).await {
                Ok(grouped) => Some(grouped),
                Err(e) => {
                    warn!("Phase 3 section classification failed: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Phase 4: LLM processing
        let mut structured_output = if let Some(ref grouped) = grouped_deck {
            match self.process_with_llm(grouped, llm_service).await {
                Ok(structured) => Some(structured),
                Err(e) => {
                    warn!("Phase 4 LLM processing failed: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Phase 5: Web validation
        if let Some(ref mut output) = structured_output {
            self.apply_web_validation(output).await;
        }

        PipelineExecutionResult {
            indexing_status,
            grouped_deck,
            structured_output,
        }
    }

    /// Process an extracted deck through Phase 2 pipeline: generate embeddings and index vectors.
    pub async fn process_deck(&self, deck: &ExtractedDeck) -> Result<(), AppError> {
        info!("Starting Phase 2 processing for deck: {}", deck.filename);

        let mut slide_texts = Vec::new();
        let mut records = Vec::new();

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        for slide in &deck.slides {
            let mut parts = Vec::new();

            for element in &slide.elements {
                use crate::models::extraction_model::Element;
                match element {
                    Element::Title { text, .. } => {
                        parts.push(format!("Slide Title: {}", text.trim()))
                    }
                    Element::Subtitle { text, .. } | Element::SectionHeader { text, .. } => {
                        parts.push(format!("Section: {}", text.trim()))
                    }
                    Element::TextBlock { text, .. } => parts.push(text.trim().to_string()),
                    Element::Statistic { value, label, .. } => {
                        parts.push(format!("Key Metric - {}: {}", label.trim(), value.trim()))
                    }
                    Element::BulletList { items, .. } => {
                        for item in items {
                            parts.push(format!("• {}", item.trim()));
                        }
                    }
                    Element::Table { headers, rows, .. } => {
                        if !headers.is_empty() {
                            parts.push(format!("Table headers: {}", headers.join(" | ")));
                        }
                        for row in rows {
                            let non_empty: Vec<&str> = row
                                .iter()
                                .map(|s| s.as_str())
                                .filter(|s| !s.trim().is_empty())
                                .collect();
                            if !non_empty.is_empty() {
                                parts.push(non_empty.join(" | "));
                            }
                        }
                    }
                    Element::Image {
                        ocr_text: Some(text),
                        ..
                    } => {
                        let t = text.trim();
                        if t.len() > 5 {
                            parts.push(format!("Image Text: {}", t));
                        }
                    }
                    _ => {}
                }
            }

            let combined_text = parts.join(" ");

            if combined_text.trim().is_empty() {
                continue;
            }

            // Generate a simple hash for deduplication
            let mut hasher = DefaultHasher::new();
            combined_text.hash(&mut hasher);
            let text_hash = format!("{:016x}", hasher.finish());

            slide_texts.push(combined_text.clone());
            records.push(EmbeddingRecord {
                deck_id: deck.deck_id.clone(),
                slide_number: slide.slide_number as usize,
                text: combined_text,
                text_hash,
            });
        }

        if slide_texts.is_empty() {
            warn!(
                "No meaningful text found in deck for indexing: {}",
                deck.filename
            );
            return Ok(());
        }

        let embeddings = self
            .embedding_service
            .generate_embeddings(slide_texts, "search_document")
            .await?;
        self.vector_store.add_vectors(embeddings, records).await?;

        info!("Phase 2 processing completed for deck: {}", deck.filename);
        Ok(())
    }

    /// Classify slides into sections and group them (Phase 3)
    pub async fn classify_and_group_slides(
        &self,
        deck: &ExtractedDeck,
    ) -> Result<GroupedDeck, AppError> {
        info!(
            "Starting section classification and grouping for deck: {}",
            deck.filename
        );

        let classification_service = SectionClassificationService::new(
            self.embedding_service.clone(),
            self.vector_store.clone(),
        );

        let classifications = classification_service.classify_slides(deck).await?;
        let grouped_deck = SectionGroupingService::create_grouped_deck(deck, &classifications);

        info!(
            "Classification complete: {} sections, {} classified slides",
            grouped_deck.sections.len(),
            grouped_deck.classification_metadata.classified_slides
        );

        Ok(grouped_deck)
    }

    /// Process grouped deck through Phase 4: LLM-based structured extraction, validation, summaries, and signals
    pub async fn process_with_llm(
        &self,
        grouped_deck: &GroupedDeck,
        llm_service: Arc<dyn LlmService>,
    ) -> Result<StructuredDeckOutput, AppError> {
        info!(
            "Starting Phase 4 LLM processing for deck: {}",
            grouped_deck.filename
        );
        info!("Phase 4: starting LLM processing pipeline");

        // Step 1: Extract structured data (now uses preprocessing_service internally via structuring_service)
        info!(
            "Phase 4.1: extracting structured JSON from {} sections",
            grouped_deck.sections.len()
        );
        let structuring_service = StructuringService::new(llm_service.clone());
        let mut structured_sections = structuring_service
            .extract_structured_data(grouped_deck)
            .await?;
        info!("Phase 4.1: structured extraction complete");

        // Step 2: Validate structured data
        info!("Phase 4.2: validating extracted data (currency, percentages, dates, numbers)");
        let validation_service = ValidationService;
        for section in &mut structured_sections {
            validation_service.validate_section(section);
        }
        info!("Phase 4.2: validation complete");

        // Step 3: Generate section summaries
        info!(
            "Phase 4.3: generating summaries for {} sections",
            structured_sections.len()
        );
        let summarization_service = SummarizationService::new(llm_service.clone());
        summarization_service
            .generate_section_summaries(&mut structured_sections)
            .await?;
        info!("Phase 4.3: summary generation complete");

        // Step 4: Extract signals and red flags
        info!("Phase 4.4: extracting investment signals and red flags");
        let signals_service = SignalsService::new(llm_service.clone());
        let (overall_signals, mut overall_red_flags) = signals_service
            .extract_signals_and_flags(&mut structured_sections)
            .await?;
        info!("Phase 4.4: signals extraction complete");

        // Step 4b: Set per-section confidence
        for section in &mut structured_sections {
            section.confidence = scoring_service::section_confidence(section);
        }

        // Step 5: Generate overall summary
        info!("Phase 4.5: generating overall deck summary");
        let overall_summary = summarization_service
            .generate_overall_summary(&structured_sections)
            .await
            .ok();
        info!("Phase 4.5: overall summary complete");

        // Step 6: Calculate overall confidence score and populate score_breakdown
        info!("Phase 4.6: calculating confidence scores and threshold flags");
        let score_breakdown = scoring_service::calculate_overall_score(
            &mut structured_sections,
            0.0, // web baseline
            &mut overall_red_flags,
        );
        let confidence_score = score_breakdown.final_score;
        info!(
            "Phase 4.6: confidence score computed: {:.4}",
            confidence_score
        );

        let (company_name, company_website) = Self::extract_company_identifiers(&structured_sections);

        let output = StructuredDeckOutput {
            deck_id: grouped_deck.deck_id.clone(),
            filename: grouped_deck.filename.clone(),
            company_name,
            company_website,
            sections: structured_sections,
            overall_summary,
            overall_signals,
            overall_red_flags,
            confidence_score,
            score_breakdown,
            extraction_timestamp: Utc::now().to_rfc3339(),
        };

        info!(
            "Phase 4 LLM processing complete for deck: {}",
            grouped_deck.filename
        );
        info!("Phase 4: all LLM processing complete");
        Ok(output)
    }

    /// Phase 5: Web data fetch + validation.
    pub async fn apply_web_validation(&self, output: &mut StructuredDeckOutput) {
        info!("Phase 5: web fetch and validation");
        let (company_name, company_website) = Self::extract_company_identifiers(&output.sections);
        let web_fetch = WebFetchService::new();
        let web_facts = match web_fetch
            .fetch_company_facts(company_name.as_deref(), company_website.as_deref())
            .await
        {
            Ok(f) => f,
            Err(e) => {
                warn!("Web fetch failed: {}; continuing without web validation", e);
                for section in &mut output.sections {
                    section.web_validation = Some("Unable to verify (fetch failed)".to_string());
                }
                return;
            }
        };
        let web_consistency =
            WebValidationService::apply_validation(&mut output.sections, &web_facts);

        let mut overall_red_flags = output.overall_red_flags.clone();
        let breakdown = scoring_service::calculate_overall_score(
            &mut output.sections,
            web_consistency,
            &mut overall_red_flags,
        );

        output.confidence_score = breakdown.final_score;
        output.score_breakdown = breakdown;
        output.overall_red_flags = overall_red_flags;

        info!(
            "Phase 5: web validation complete (consistency: {:.2}, final score: {:.2})",
            web_consistency, output.confidence_score
        );
    }

    fn extract_company_identifiers(
        sections: &[crate::models::structured_output::StructuredSectionData],
    ) -> (Option<String>, Option<String>) {
        let keys_name = ["Name", "Company Name", "Company"];
        let keys_website = ["Website", "URL", "Company Website", "Site"];
        for section in sections {
            let data = &section.data;
            let name = get_str(data, &keys_name);
            let website = get_str(data, &keys_website)
                .filter(|s| s.starts_with("http://") || s.starts_with("https://"));
            if name.is_some() || website.is_some() {
                return (name, website);
            }
        }
        (None, None)
    }
}

fn get_str(data: &serde_json::Value, keys: &[&str]) -> Option<String> {
    let obj = data.as_object()?;
    for k in keys {
        if let Some(v) = obj.get(*k) {
            if let Some(s) = v.as_str() {
                let t = s.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}
