use serde::{Deserialize, Serialize};

// ── Section-level structures ──────────────────────────────────────────────────

/// Structured data extracted from a pitch deck section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredSectionData {
    /// Section name (e.g., "Financial Highlights")
    pub section_name: String,
    /// LLM-extracted key-value pairs as JSON
    pub data: serde_json::Value,
    /// Regex/format validation results
    pub validation: ValidationResults,
    /// LLM-generated section summary
    pub summary: Option<String>,
    /// Positive investment signals detected
    pub signals: Vec<InvestmentSignal>,
    /// Risk indicators detected
    pub red_flags: Vec<RedFlag>,
    /// Per-section composite confidence (0.0–1.0)
    #[serde(default)]
    pub confidence: f32,
    /// Business-rule threshold violations (e.g. "Burn rate exceeds runway")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub threshold_flags: Vec<String>,
    /// Web validation status (e.g. "Verified via company site")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_validation: Option<String>,
}

/// Validation results for a section's numeric/format fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResults {
    pub fields_validated: usize,
    pub fields_passed: usize,
    pub errors: Vec<ValidationError>,
    /// 0.0–1.0: fields_passed / fields_validated (1.0 if nothing to validate)
    pub score: f32,
}

/// One field that failed validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub error_type: String,
    pub message: String,
    pub value: Option<String>,
}

/// A positive investment signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvestmentSignal {
    pub signal_type: String,
    pub description: String,
    /// Confidence 0.0–1.0
    pub confidence: f32,
    pub section: String,
}

/// A risk indicator or concern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedFlag {
    pub flag_type: String,
    pub description: String,
    /// "low" | "medium" | "high" | "critical"
    pub severity: String,
    pub section: String,
}

// ── Scoring ───────────────────────────────────────────────────────────────────

/// Per-dimension score breakdown exposed to the frontend.
///
/// Weights (sum = 1.0):
///   final = 0.30 × validation  + 0.25 × llm_confidence
///           + 0.20 × completeness + 0.15 × web_consistency
///           + 0.10 × threshold_score
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScoreBreakdown {
    /// Regex field validation score (0–1)
    pub validation_score: f32,
    /// Average LLM signal confidence (0–1; 0 = no signals)
    pub llm_confidence: f32,
    /// Completeness: present fields / expected fields per section (0–1)
    pub completeness_score: f32,
    /// Web-source consistency (0–1; 0 = no web source)
    pub web_consistency: f32,
    /// 1.0 minus cumulative threshold violation penalty (0–1)
    pub threshold_score: f32,
    /// Weighted composite
    pub final_score: f32,
}

// ── Deck-level output ─────────────────────────────────────────────────────────

/// Complete structured analysis output for one pitch deck.
/// This is the primary payload returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredDeckOutput {
    pub deck_id: String,
    pub filename: String,
    /// Best-effort extracted company name (if found in any section fields)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_name: Option<String>,
    /// Best-effort extracted company website (if found in any section fields)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_website: Option<String>,
    /// Per-section analysis
    pub sections: Vec<StructuredSectionData>,
    /// Executive summary across all sections
    pub overall_summary: Option<String>,
    /// Consolidated investment signals
    pub overall_signals: Vec<InvestmentSignal>,
    /// Consolidated red flags
    pub overall_red_flags: Vec<RedFlag>,
    /// Composite confidence score (0.0–1.0)
    pub confidence_score: f32,
    /// Per-dimension score breakdown for frontend display
    pub score_breakdown: ScoreBreakdown,
    pub extraction_timestamp: String,
}
