//! Web data fetch service: uses reqwest + scraper to fetch external data
//! for validation (e.g. company website, Crunchbase-style sources).

use crate::errors::app_error::AppError;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};

/// Facts extracted from web sources for validation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebFacts {
    /// Company name as found on web (if any)
    pub company_name: Option<String>,
    /// Meta description from the page
    pub meta_description: Option<String>,
    /// Employee count or range as text
    pub employees: Option<String>,
    /// Funding/revenue snippet
    pub funding: Option<String>,
    /// Source URL that was fetched
    pub source_url: String,
    /// Raw text snippets from the page (for comparison)
    pub snippets: Vec<String>,
    /// Source label, e.g. "company_site", "crunchbase"
    pub source_label: String,
}

/// Fetches external data for validation using reqwest and scraper
pub struct WebFetchService {
    client: reqwest::Client,
    max_snippet_len: usize,
}

impl Default for WebFetchService {
    fn default() -> Self {
        Self::new()
    }
}

impl WebFetchService {
    /// Create a new web fetch service with default timeouts
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .user_agent("PitchDeckExtractor/1.0 (Validation)")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            max_snippet_len: 500,
        }
    }

    /// Fetch company facts from a URL (e.g. company website).
    /// Parses HTML and extracts text for later comparison.
    pub async fn fetch_url(&self, url: &str, source_label: &str) -> Result<WebFacts, AppError> {
        info!(url = %url, "Fetching URL for web validation");
        let body = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::InternalServerError(format!("Web fetch failed: {}", e)))?
            .error_for_status()
            .map_err(|e| AppError::InternalServerError(format!("HTTP error: {}", e)))?
            .text()
            .await
            .map_err(|e| AppError::InternalServerError(format!("Read body: {}", e)))?;

        let facts = self.parse_html_to_facts(&body, url, source_label);
        Ok(facts)
    }

    /// Fetch company facts: tries company website if provided, else skips.
    /// Returns Ok(WebFacts) if at least one source was fetched, otherwise Ok(empty WebFacts) so pipeline can continue.
    pub async fn fetch_company_facts(
        &self,
        company_name: Option<&str>,
        company_website: Option<&str>,
    ) -> Result<WebFacts, AppError> {
        let website = company_website
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .filter(|s| s.starts_with("http://") || s.starts_with("https://"));

        if let Some(url) = website {
            match self.fetch_url(url, "company_site").await {
                Ok(mut facts) => {
                    if company_name.is_some() && facts.company_name.is_none() {
                        facts.company_name = company_name.map(String::from);
                    }
                    return Ok(facts);
                }
                Err(e) => {
                    warn!("Failed to fetch company website: {}", e);
                }
            }
        }

        // No URL or fetch failed: return empty facts so validation can still run (no web consistency)
        Ok(WebFacts {
            source_url: String::new(),
            source_label: "none".to_string(),
            ..Default::default()
        })
    }

    /// Parse HTML body into WebFacts: extract text, optional meta/selectors
    fn parse_html_to_facts(&self, html: &str, source_url: &str, source_label: &str) -> WebFacts {
        let document = Html::parse_document(html);

        let mut snippets = Vec::new();

        // Body text: try common content selectors
        let selectors = [
            "main",
            "article",
            "[role='main']",
            ".content",
            ".main",
            "body",
        ];
        for sel_str in &selectors {
            if let Ok(sel) = Selector::parse(sel_str) {
                for el in document.select(&sel) {
                    let text = el.text().collect::<Vec<_>>().join(" ");
                    let trimmed = text.trim();
                    if trimmed.len() > 20 {
                        snippets.push(self.truncate_snippet(trimmed));
                    }
                }
                if !snippets.is_empty() {
                    break;
                }
            }
        }

        // Fallback: all paragraph text
        if snippets.is_empty() {
            if let Ok(sel) = Selector::parse("p") {
                for el in document.select(&sel) {
                    let text = el.text().collect::<Vec<_>>().join(" ");
                    let trimmed = text.trim();
                    if trimmed.len() > 15 {
                        snippets.push(self.truncate_snippet(trimmed));
                    }
                }
            }
        }

        // Fallback: first block of text from body
        if snippets.is_empty() {
            if let Ok(sel) = Selector::parse("body") {
                let text = document
                    .select(&sel)
                    .next()
                    .map(|el| el.text().collect::<Vec<_>>().join(" "))
                    .unwrap_or_default();
                let trimmed = text.trim();
                if trimmed.len() > 20 {
                    snippets.push(self.truncate_snippet(trimmed));
                }
            }
        }

        // Try to pull company name from title
        let company_name = Selector::parse("title")
            .ok()
            .and_then(|sel| {
                document
                    .select(&sel)
                    .next()
                    .map(|el| el.text().collect::<Vec<_>>().join(" "))
            })
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let mut meta_description = None;
        if let Ok(meta_sel) = Selector::parse("meta[name='description'], meta[property='og:description']") {
            if let Some(el) = document.select(&meta_sel).next() {
                if let Some(desc) = el.value().attr("content") {
                    let trimmed = desc.trim();
                    if trimmed.len() > 5 {
                        meta_description = Some(trimmed.to_string());
                        snippets.push(self.truncate_snippet(trimmed));
                    }
                }
            }
        }

        WebFacts {
            company_name,
            meta_description,
            employees: None,
            funding: None,
            source_url: source_url.to_string(),
            snippets,
            source_label: source_label.to_string(),
        }
    }

    /// Truncate at valid UTF-8 char boundary to avoid panic on multi-byte chars.
    fn truncate_snippet(&self, s: &str) -> String {
        if s.len() <= self.max_snippet_len {
            return s.to_string();
        }
        let mut end = self.max_snippet_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}
