use crate::errors::app_error::AppError;
use crate::models::deck_model::{ExtractedDeck, Slide};
use crate::models::section_model::{SectionGroup, SectionType, SlideClassification};
use crate::services::embedding_service::EmbeddingService;
use crate::services::vector_store_service::VectorStore;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{info, warn};

/// Semantic queries for each section type
/// These queries are used to find slides that match each section's semantic meaning
const SECTION_QUERIES: &[(&str, SectionType)] = &[
    (
        "company overview vision mission values what we do who we are",
        SectionType::CompanyOverview,
    ),
    (
        "problem statement pain point solution how we solve customer problem",
        SectionType::ProblemSolution,
    ),
    (
        "market opportunity TAM SAM SOM total addressable market size growth",
        SectionType::MarketOpportunity,
    ),
    (
        "product technology features how it works platform software hardware",
        SectionType::ProductTechnology,
    ),
    (
        "business model revenue model monetization pricing how we make money",
        SectionType::BusinessModel,
    ),
    (
        "traction metrics growth users customers revenue milestones achievements",
        SectionType::TractionMetrics,
    ),
    (
        "financial highlights revenue profit margin burn rate unit economics",
        SectionType::FinancialHighlights,
    ),
    (
        "competitive landscape competitors competitive advantage differentiation",
        SectionType::CompetitiveLandscape,
    ),
    (
        "team founders management advisors board members key people",
        SectionType::TeamFounders,
    ),
    (
        "funding ask investment raise use of funds how much we need",
        SectionType::FundingAsk,
    ),
    (
        "roadmap future strategy next steps milestones timeline plan",
        SectionType::RoadmapStrategy,
    ),
    (
        "risks challenges risk factors potential problems mitigation",
        SectionType::RisksChallenges,
    ),
    (
        "partnerships customers clients case studies testimonials",
        SectionType::PartnershipsCustomers,
    ),
    (
        "exit strategy IPO acquisition merger exit plan",
        SectionType::ExitStrategy,
    ),
];

/// Minimum similarity score for a slide to be assigned to a specific section.
/// Below this threshold the slide is classified as Other.
/// (L2 distance converted via 1/(1+d): distance=1.86 → similarity=0.35)
const MIN_SIMILARITY_THRESHOLD: f32 = 0.35;

/// Service for classifying slides into pitch deck sections using semantic similarity
pub struct SectionClassificationService {
    embedding_service: Arc<dyn EmbeddingService>,
    vector_store: Arc<VectorStore>,
}

impl SectionClassificationService {
    /// Create a new section classification service
    pub fn new(
        embedding_service: Arc<dyn EmbeddingService>,
        vector_store: Arc<VectorStore>,
    ) -> Self {
        Self {
            embedding_service,
            vector_store,
        }
    }

    /// Classify all slides in a deck into appropriate sections
    ///
    /// Uses semantic search to match slides against section queries.
    /// Each slide is assigned to the section with the highest similarity score.
    ///
    /// # Arguments
    /// * `deck` - The extracted deck with slides to classify
    ///
    /// # Returns
    /// * `Ok(Vec<SlideClassification>)` - Classification results for each slide
    /// * `Err(AppError)` - Error if classification fails
    pub async fn classify_slides(
        &self,
        deck: &ExtractedDeck,
    ) -> Result<Vec<SlideClassification>, AppError> {
        info!("Starting section classification for deck: {}", deck.filename);

        if deck.slides.is_empty() {
            warn!("No slides to classify");
            return Ok(vec![]);
        }

        // Try Python zero-shot / heuristics script first
        let mut py_input = serde_json::Map::new();
        let mut slides_arr = Vec::new();
        
        for slide in &deck.slides {
            let mut slide_obj = serde_json::Map::new();
            slide_obj.insert("slide_number".to_string(), serde_json::Value::Number(slide.slide_number.into()));
            
            let mut text = String::new();
            let mut title = String::new();
            for elem in &slide.elements {
                match elem {
                    crate::models::extraction_model::Element::Title { text: t, .. } => {
                        title.push_str(t);
                        title.push(' ');
                    }
                    crate::models::extraction_model::Element::TextBlock { text: t, .. } 
                    | crate::models::extraction_model::Element::Subtitle { text: t, .. } 
                    | crate::models::extraction_model::Element::SectionHeader { text: t, .. } => {
                        text.push_str(t);
                        text.push(' ');
                    }
                    _ => {}
                }
            }
            slide_obj.insert("title".to_string(), serde_json::Value::String(title));
            slide_obj.insert("text".to_string(), serde_json::Value::String(text));
            slides_arr.push(serde_json::Value::Object(slide_obj));
        }
        py_input.insert("slides".to_string(), serde_json::Value::Array(slides_arr));
        
        let py_input_str = serde_json::to_string(&py_input).unwrap_or_default();
        
        let script_path = std::path::Path::new("scripts").join("classifier.py");
        if let Ok(output) = tokio::process::Command::new("python")
            .arg(&script_path)
            .arg(&py_input_str)
            .output()
            .await 
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    if parsed.get("status").and_then(|s| s.as_str()) == Some("success") {
                        if let Some(arr) = parsed.get("classifications").and_then(|c| c.as_array()) {
                            let mut classifications = Vec::new();
                            for item in arr {
                                let slide_num = item.get("slide_number").and_then(|n| n.as_u64()).unwrap_or(0) as usize;
                                let section_str = item.get("section").and_then(|s| s.as_str()).unwrap_or("Other");
                                let confidence = item.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.0) as f32;
                                
                                let section_type = match section_str {
                                    "Company Overview" => SectionType::CompanyOverview,
                                    "Problem Solution" => SectionType::ProblemSolution,
                                    "Market Opportunity" => SectionType::MarketOpportunity,
                                    "Product Technology" => SectionType::ProductTechnology,
                                    "Business Model" => SectionType::BusinessModel,
                                    "Traction Metrics" => SectionType::TractionMetrics,
                                    "Financial Highlights" => SectionType::FinancialHighlights,
                                    "Competitive Landscape" => SectionType::CompetitiveLandscape,
                                    "Team Founders" => SectionType::TeamFounders,
                                    "Funding Ask" => SectionType::FundingAsk,
                                    "Roadmap Strategy" => SectionType::RoadmapStrategy,
                                    "Risks Challenges" => SectionType::RisksChallenges,
                                    "Partnerships Customers" => SectionType::PartnershipsCustomers,
                                    "Exit Strategy" => SectionType::ExitStrategy,
                                    _ => SectionType::Other,
                                };
                                
                                classifications.push(SlideClassification {
                                    slide_number: slide_num,
                                    section: section_type,
                                    confidence_score: confidence,
                                    reasoning: Some(format!("Classified as {} via Python script", section_str)),
                                });
                            }
                            let other_count = classifications.iter().filter(|c| c.section == SectionType::Other).count();
                            let other_ratio = other_count as f32 / classifications.len().max(1) as f32;
                            
                            if other_ratio > 0.3 {
                                warn!("Python classification returned too many 'Other' slides ({:.0}%), falling back to semantic similarity", other_ratio * 100.0);
                            } else {
                                info!("Classification complete via Python: {} slides classified", classifications.len());
                                return Ok(classifications);
                            }
                        }
                    }
                }
            }
        }
        
        warn!("Falling back to semantic similarity classification");

        // Generate embeddings for all section queries
        let query_texts: Vec<String> = SECTION_QUERIES
            .iter()
            .map(|(query, _)| query.to_string())
            .collect();

        info!(
            "Generating embeddings for {} section queries",
            query_texts.len()
        );
        let query_embeddings = self
            .embedding_service
            .generate_embeddings(query_texts, "search_query")
            .await?;

        if query_embeddings.len() != SECTION_QUERIES.len() {
            return Err(AppError::InternalServerError(
                "Mismatch between section queries and embeddings".to_string(),
            ));
        }

        // For each slide, find the best matching section
        let mut classifications = Vec::new();
        let mut slide_to_section_scores: HashMap<u32, Vec<(SectionType, f32)>> = HashMap::new();

        // Search for each section query and collect scores per slide
        for (idx, (_, section_type)) in SECTION_QUERIES.iter().enumerate() {
            let query_embedding = &query_embeddings[idx];

            let search_limit = (deck.slides.len() * 2).clamp(10, 50);
            let results = self
                .vector_store
                .search_within_deck(query_embedding, &deck.deck_id, search_limit)
                .await?;

            for result in results {
                let similarity = 1.0 / (1.0 + result.score);
                slide_to_section_scores
                    .entry(result.slide_number as u32)
                    .or_default()
                    .push((*section_type, similarity));
            }
        }

        // Classify each slide based on highest similarity score
        for slide in &deck.slides {
            let slide_num = slide.slide_number;

            if let Some(scores) = slide_to_section_scores.get(&slide_num) {
                let (best_section, best_score) = scores
                    .iter()
                    .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap_or(&(SectionType::Other, 0.0));

                classifications.push(SlideClassification {
                    slide_number: slide_num as usize,
                    section: if *best_score >= MIN_SIMILARITY_THRESHOLD {
                        *best_section
                    } else {
                        SectionType::Other
                    },
                    confidence_score: *best_score,
                    reasoning: Some(format!(
                        "Matched {} section with similarity {:.4}{}",
                        best_section.as_name(),
                        best_score,
                        if *best_score < MIN_SIMILARITY_THRESHOLD { " (below threshold → Other)" } else { "" }
                    )),
                });
            } else {
                classifications.push(SlideClassification {
                    slide_number: slide_num as usize,
                    section: SectionType::Other,
                    confidence_score: 0.0,
                    reasoning: Some("No semantic matches found".to_string()),
                });
            }
        }

        let classified_slide_nums: HashSet<u32> = classifications
            .iter()
            .map(|c| c.slide_number as u32)
            .collect();

        for slide in &deck.slides {
            if !classified_slide_nums.contains(&slide.slide_number) {
                classifications.push(SlideClassification {
                    slide_number: slide.slide_number as usize,
                    section: SectionType::Other,
                    confidence_score: 0.0,
                    reasoning: Some("Slide not found during classification".to_string()),
                });
            }
        }

        classifications.sort_by_key(|c| c.slide_number);
        info!("Classification complete: {} slides classified", classifications.len());
        Ok(classifications)
    }
}

/// Service for grouping classified slides by section
pub struct SectionGroupingService;

impl SectionGroupingService {
    /// Group slides by their classified sections
    ///
    /// # Arguments
    /// * `deck` - The extracted deck
    /// * `classifications` - Classification results for each slide
    ///
    /// # Returns
    /// * `Vec<SectionGroup>` - Slides grouped by section
    pub fn group_slides_by_section(
        deck: &ExtractedDeck,
        classifications: &[SlideClassification],
    ) -> Vec<SectionGroup> {
        // Create a map from slide number to slide
        let slide_map: HashMap<usize, &Slide> = deck
            .slides
            .iter()
            .map(|s| (s.slide_number as usize, s))
            .collect();

        // Group classifications by section
        let mut section_to_slides: HashMap<SectionType, Vec<&Slide>> = HashMap::new();

        for classification in classifications {
            if let Some(slide) = slide_map.get(&classification.slide_number) {
                section_to_slides
                    .entry(classification.section)
                    .or_default()
                    .push(slide);
            }
        }

        // Convert to SectionGroup vector, sorted by section order
        let section_order = vec![
            SectionType::CompanyOverview,
            SectionType::ProblemSolution,
            SectionType::MarketOpportunity,
            SectionType::ProductTechnology,
            SectionType::BusinessModel,
            SectionType::TractionMetrics,
            SectionType::FinancialHighlights,
            SectionType::CompetitiveLandscape,
            SectionType::TeamFounders,
            SectionType::FundingAsk,
            SectionType::RoadmapStrategy,
            SectionType::RisksChallenges,
            SectionType::PartnershipsCustomers,
            SectionType::ExitStrategy,
            SectionType::Other,
        ];

        let mut groups = Vec::new();
        for section_type in section_order {
            if let Some(slides) = section_to_slides.get(&section_type) {
                // Clone slides for the group
                let slides_cloned: Vec<Slide> = slides.iter().map(|s| (*s).clone()).collect();
                let slide_count = slides_cloned.len();

                groups.push(SectionGroup {
                    section_name: section_type.as_name().to_string(),
                    section_key: section_type.as_key().to_string(),
                    slides: slides_cloned,
                    slide_count,
                });
            }
        }

        groups
    }

    /// Create grouped deck structure with metadata
    pub fn create_grouped_deck(
        deck: &ExtractedDeck,
        classifications: &[SlideClassification],
    ) -> crate::models::section_model::GroupedDeck {
        let groups = Self::group_slides_by_section(deck, classifications);

        let classified_count = classifications
            .iter()
            .filter(|c| c.section != SectionType::Other)
            .count();
        let unclassified_count = classifications.len() - classified_count;

        use crate::models::section_model::{ClassificationMetadata, GroupedDeck};
        GroupedDeck {
            deck_id: deck.deck_id.clone(),
            filename: deck.filename.clone(),
            sections: groups,
            classification_metadata: ClassificationMetadata {
                total_slides: deck.total_slides,
                classified_slides: classified_count,
                unclassified_slides: unclassified_count,
                classification_timestamp: Utc::now().to_rfc3339(),
                method: "semantic_similarity".to_string(),
            },
        }
    }
}
