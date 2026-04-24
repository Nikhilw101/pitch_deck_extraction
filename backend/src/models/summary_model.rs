use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DeckSummary {
    pub summary: String,
    pub score: f32,
}
