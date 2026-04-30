use crate::errors::app_error::AppError;
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, error, warn};

const DEFAULT_MODEL: &str = "llama3.2:3b";
const DEFAULT_MAX_CONCURRENCY: usize = 1;
// Limit how many tokens the model is allowed to generate per call.
// This keeps responses fast while still allowing rich JSON output.
const DEFAULT_NUM_PREDICT: i64 = 2048;

/// Trait for LLM services (supports multiple backends)
#[async_trait]
pub trait LlmService: Send + Sync {
    /// Generate structured JSON from text content
    ///
    /// # Arguments
    /// * `prompt` - The prompt/instruction for the LLM
    /// * `content` - The content to process
    /// * `schema_hint` - Optional JSON schema hint for structured output
    ///
    /// # Returns
    /// * `Ok(String)` - JSON string response
    /// * `Err(AppError)` - Error if LLM call fails
    async fn generate_structured_json(
        &self,
        prompt: &str,
        content: &str,
        schema_hint: Option<&str>,
    ) -> Result<String, AppError>;

    /// Generate summary text
    ///
    /// # Arguments
    /// * `content` - Content to summarize
    /// * `max_sentences` - Maximum number of sentences (default: 3)
    ///
    /// # Returns
    /// * `Ok(String)` - Summary text
    /// * `Err(AppError)` - Error if generation fails
    async fn generate_summary(
        &self,
        content: &str,
        max_sentences: Option<usize>,
    ) -> Result<String, AppError>;

    /// Extract investment signals and red flags
    ///
    /// # Arguments
    /// * `content` - Content to analyze
    ///
    /// # Returns
    /// * `Ok(String)` - JSON string with signals and red flags
    /// * `Err(AppError)` - Error if extraction fails
    async fn extract_signals(&self, content: &str) -> Result<String, AppError>;
}

/// Ollama API client for local LLM inference
pub struct OllamaClient {
    client: Client,
    model: String,
    base_url: String,
    limiter: Arc<Semaphore>,
}

impl OllamaClient {
    /// Create a new Ollama client
    ///
    /// # Arguments
    /// * `model` - Model name (default: "llama3.2:3b")
    /// * `base_url` - Base URL for Ollama API (default: "http://localhost:11434")
    ///
    /// # Returns
    /// * `Ok(OllamaClient)` - Successfully created client
    /// * `Err(AppError)` - Error if HTTP client creation fails
    pub fn new(model: Option<String>, base_url: Option<String>) -> Result<Self, AppError> {
        // Timeout disabled: set to 0 so we wait as long as the model needs
        let timeout_secs: u64 = std::env::var("OLLAMA_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let mut builder = Client::builder();
        if timeout_secs > 0 {
            builder = builder.timeout(Duration::from_secs(timeout_secs));
        }
        let client = builder.build().map_err(|e| {
            AppError::InternalServerError(format!("Failed to build HTTP client: {}", e))
        })?;

        // Global concurrency limiter for Ollama:
        // This is the "queue + workers" that prevents flooding the local model.
        let max_concurrency = std::env::var("OLLAMA_MAX_CONCURRENCY")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&v| (1..=8).contains(&v))
            .unwrap_or(DEFAULT_MAX_CONCURRENCY);

        Ok(Self {
            client,
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            base_url: base_url.unwrap_or_else(|| "http://localhost:11434".to_string()),
            limiter: Arc::new(Semaphore::new(max_concurrency)),
        })
    }

    /// Build full API URL
    fn api_url(&self) -> String {
        format!("{}/api/generate", self.base_url)
    }

    /// Call Ollama API with streaming support
    async fn call_ollama(
        &self,
        prompt: &str,
        stream: bool,
        json_format: bool,
    ) -> Result<String, AppError> {
        // Backpressure: wait until a worker slot is available.
        let _permit = self
            .limiter
            .acquire()
            .await
            .map_err(|_| AppError::InternalServerError("LLM limiter closed".to_string()))?;

        // Allow basic tuning via environment variables without code changes.
        let num_predict = std::env::var("LLM_NUM_PREDICT")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|v| *v > 0 && *v <= 8192)
            .unwrap_or(DEFAULT_NUM_PREDICT);

        let temperature = std::env::var("LLM_TEMPERATURE")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .map(|t| t.clamp(0.0, 1.0))
            .unwrap_or(0.0);

        let mut request_body = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "stream": stream,
            "options": {
                "temperature": temperature,   // Default deterministic extraction; overridable via env
                "top_p": 0.9,
                "num_predict": num_predict,
            }
        });

        if json_format {
            request_body["format"] = serde_json::json!("json");
        }

        // Avoid dumping prompts/responses to stdout. If you want request-level logs, set RUST_LOG.
        debug!(
            model = %self.model,
            json_format,
            prompt_chars = prompt.len(),
            "Calling Ollama API"
        );

        // Retry policy for local inference hiccups.
        // We retry on network errors/timeouts; server-side non-2xx is returned immediately.
        let max_retries = std::env::var("OLLAMA_RETRIES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&v| v <= 5)
            .unwrap_or(2);

        let mut attempt = 0usize;
        let response = loop {
            attempt += 1;
            let res = self
                .client
                .post(self.api_url())
                .json(&request_body)
                .send()
                .await;

            match res {
                Ok(r) => break r,
                Err(e) => {
                    error!("Ollama API request failed (attempt {}/{}): {}", attempt, max_retries + 1, e);
                    if attempt >= max_retries + 1 {
                        return Err(AppError::InternalServerError(format!(
                            "Ollama API failed after retries: {}. Make sure Ollama is running.",
                            e
                        )));
                    }
                    // Small backoff to avoid hammering the local server.
                    let backoff_ms = (500u64).saturating_mul(attempt as u64);
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    continue;
                }
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Ollama API error ({}): {}", status, error_text);
            return Err(AppError::InternalServerError(format!(
                "Ollama API error: {} - {}",
                status, error_text
            )));
        }

        // Parse streaming response
        let text = response.text().await.map_err(|e| {
            error!("Failed to read Ollama response: {}", e);
            AppError::InternalServerError(format!("Failed to parse Ollama response: {}", e))
        })?;

        // Handle streaming JSON lines format
        let mut full_response = String::new();
        for line in text.lines() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(response_text) = json.get("response").and_then(|r| r.as_str()) {
                    full_response.push_str(response_text);
                }
                // Check if done
                if json.get("done").and_then(|d| d.as_bool()) == Some(true) {
                    break;
                }
            }
        }

        if full_response.is_empty() {
            warn!("Empty response from Ollama, trying direct parse");
            // Try parsing as single JSON object (non-streaming)
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(response_text) = json.get("response").and_then(|r| r.as_str()) {
                    return Ok(response_text.to_string());
                }
            }
            return Err(AppError::InternalServerError(
                "Empty response from Ollama".to_string(),
            ));
        }

        // Raw model output can be huge; keep it at DEBUG with truncation.
        debug!(
            response_chars = full_response.len(),
            response_preview = %full_response.chars().take(400).collect::<String>(),
            "Ollama response received"
        );
        Ok(full_response)
    }
}

#[async_trait]
impl LlmService for OllamaClient {
    async fn generate_structured_json(
        &self,
        prompt: &str,
        content: &str,
        schema_hint: Option<&str>,
    ) -> Result<String, AppError> {
        let schema_instruction = schema_hint.unwrap_or("Extract structured data as valid JSON.");

        let full_prompt = format!(
            r#"You are a strict data extraction assistant. {prompt}
            
{schema_instruction}

Content:
{content}

Instructions:
1. Extract ALL key-value pairs from the content exhaustively.
2. Return ONLY valid JSON, no markdown, no explanations.
3. Use the field names from the schema above when present in the text.
4. If a value is not present, DO NOT INVENT IT. Omit the field or use null. Do not guess company names or URLs.
5. Include numeric values where applicable; normalize dates, currencies, and percentages.
6. Every extracted field MUST be an object with the following structure:
   "FieldName": {{ "value": <value>, "source_text": "<exact verbatim phrase from content>", "slide_number": <number>, "confidence": <float 1.0 for exact match, 0.7 for inference> }}

JSON:"#,
            prompt = prompt,
            schema_instruction = schema_instruction,
            content = content
        );

        debug!(
            prompt_chars = full_prompt.len(),
            "Sending structured extraction prompt to Ollama"
        );
        let response = self.call_ollama(&full_prompt, false, true).await?;

        // Clean response - extract JSON block if wrapped in text
        let cleaned = if let (Some(start), Some(end)) = (response.find('{'), response.rfind('}')) {
            response[start..=end].to_string()
        } else {
            response.trim().to_string()
        };

        // Validate JSON
        serde_json::from_str::<serde_json::Value>(&cleaned).map_err(|e| {
            error!("Invalid JSON from LLM: {}. Raw response: {}", e, response);
            AppError::InternalServerError(format!("LLM returned invalid JSON: {}", e))
        })?;

        Ok(cleaned)
    }

    async fn generate_summary(
        &self,
        content: &str,
        max_sentences: Option<usize>,
    ) -> Result<String, AppError> {
        let n = max_sentences.unwrap_or(3);
        let prompt = format!(
            "Summarize the following content in exactly {} sentences. Return ONLY the summary text, no preamble.\n\nContent:\n{}",
            n, content
        );
        self.call_ollama(&prompt, false, false).await
    }

    async fn extract_signals(&self, content: &str) -> Result<String, AppError> {
        let prompt = format!(
            r#"Analyze the following pitch deck section for investment signals and red flags.
Extract ALL relevant signals (e.g. market size, traction, team, revenue, differentiators, risks) and red flags (missing info, weak claims, inconsistencies).
CRITICAL RULES:
- Signal = factual claim indicating opportunity/strength. Must be extracted exactly from text.
- Red flag = risk/challenge explicitly stated.
- Confidence = based on clarity.
- DO NOT USE PLACEHOLDERS (e.g. "string" or "number"). Return [] if no signal is found.
- If a field is unknown, omit it entirely. Do not return null.

Return valid JSON ONLY, no other text:
{{
  "signals": [
    {{ "type": "market_growth|traction|team_strength|...", "description": "<verbatim quote from text>", "confidence": <float> }}
  ],
  "red_flags": [
    {{ "type": "risk_factor|...", "description": "<verbatim quote from text>", "severity": "low|medium|high|critical", "evidence_text": "<exact supporting quote>", "evidence_slide_number": <number> }}
  ]
}}

Content:
{}

JSON:"#,
            content
        );

        let response = self.call_ollama(&prompt, false, true).await?;

        // Clean response
        let cleaned = if let (Some(start), Some(end)) = (response.find('{'), response.rfind('}')) {
            response[start..=end].to_string()
        } else {
            response.trim().to_string()
        };

        // Validate
        let json: serde_json::Value = serde_json::from_str(&cleaned).map_err(|e| {
            error!(
                "Invalid signals JSON from LLM: {}. Raw response: {}",
                e, response
            );
            AppError::InternalServerError(format!("LLM returned invalid signals JSON: {}", e))
        })?;

        // Ensure required fields exist
        if !json.is_object() {
            return Err(AppError::InternalServerError(
                "LLM response is not a JSON object".to_string(),
            ));
        }

        Ok(cleaned)
    }
}
