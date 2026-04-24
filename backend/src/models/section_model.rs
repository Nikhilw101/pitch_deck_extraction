use crate::models::deck_model::Slide;
use serde::{Deserialize, Serialize};

/// Pitch deck section categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionType {
    CompanyOverview,
    ProblemSolution,
    MarketOpportunity,
    ProductTechnology,
    BusinessModel,
    TractionMetrics,
    FinancialHighlights,
    CompetitiveLandscape,
    TeamFounders,
    FundingAsk,
    RoadmapStrategy,
    RisksChallenges,
    PartnershipsCustomers,
    ExitStrategy,
    Other, // Catch-all for unclassified slides
}

impl SectionType {
    /// Get normalized section key for JSON output
    pub fn as_key(&self) -> &'static str {
        match self {
            SectionType::CompanyOverview => "Company_Overview",
            SectionType::ProblemSolution => "Problem_Solution",
            SectionType::MarketOpportunity => "Market_Opportunity",
            SectionType::ProductTechnology => "Product_Technology",
            SectionType::BusinessModel => "Business_Model",
            SectionType::TractionMetrics => "Traction_Metrics",
            SectionType::FinancialHighlights => "Financial_Highlights",
            SectionType::CompetitiveLandscape => "Competitive_Landscape",
            SectionType::TeamFounders => "Team_Founders",
            SectionType::FundingAsk => "Funding_Ask",
            SectionType::RoadmapStrategy => "Roadmap_Strategy",
            SectionType::RisksChallenges => "Risks_Challenges",
            SectionType::PartnershipsCustomers => "Partnerships_Customers",
            SectionType::ExitStrategy => "Exit_Strategy",
            SectionType::Other => "Other",
        }
    }

    /// Get human-readable section name
    pub fn as_name(&self) -> &'static str {
        match self {
            SectionType::CompanyOverview => "Company Overview & Vision",
            SectionType::ProblemSolution => "Problem Statement & Solution",
            SectionType::MarketOpportunity => "Market Opportunity",
            SectionType::ProductTechnology => "Product / Technology Description",
            SectionType::BusinessModel => "Business & Revenue Model",
            SectionType::TractionMetrics => "Traction & Key Metrics",
            SectionType::FinancialHighlights => "Financial Highlights",
            SectionType::CompetitiveLandscape => "Competitive Landscape",
            SectionType::TeamFounders => "Team & Founder Information",
            SectionType::FundingAsk => "Funding Ask & Use of Funds",
            SectionType::RoadmapStrategy => "Roadmap & Future Strategy",
            SectionType::RisksChallenges => "Risks & Challenges",
            SectionType::PartnershipsCustomers => "Partnerships / Customers",
            SectionType::ExitStrategy => "Exit Strategy",
            SectionType::Other => "Other / Unclassified",
        }
    }
}

/// Classification result for a single slide
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideClassification {
    pub slide_number: usize,
    pub section: SectionType,
    pub confidence_score: f32,
    pub reasoning: Option<String>,
}

/// Grouped slides by section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionGroup {
    pub section_name: String,
    pub section_key: String,
    pub slides: Vec<Slide>,
    pub slide_count: usize,
}

/// Complete grouped deck structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupedDeck {
    pub deck_id: String,
    pub filename: String,
    pub sections: Vec<SectionGroup>,
    pub classification_metadata: ClassificationMetadata,
}

/// Metadata about the classification process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationMetadata {
    pub total_slides: usize,
    pub classified_slides: usize,
    pub unclassified_slides: usize,
    pub classification_timestamp: String,
    pub method: String,
}
