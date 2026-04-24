//! Text preprocessing service — runs BEFORE the LLM.
//!
//! Responsibilities:
//!   1. Strip control characters, null bytes, and PDF garble
//!   2. Normalize Indian + Western currency units to annotated form
//!   3. Collapse excessive whitespace
//!   4. Return both cleaned text (for LLM) and pre-extracted values (for scoring)

use once_cell::sync::Lazy;
use regex::Regex;

// ── Static regexes ────────────────────────────────────────────────────────────

/// Matches Indian/Western monetary expressions for annotation
static AMOUNT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)([\$₹€£]|INR|Rs\.?\s*)?\s*([\d,]+(?:\.\d+)?)\s*(crore|cr|lakh|l|billion|bn|million|mn|thousand|k|m|b)\b"
    ).expect("AMOUNT_RE")
});

/// Matches control/garble characters to strip
static CONTROL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[\x00-\x08\x0B\x0C\x0E-\x1F\x7F]").expect("CONTROL_RE"));

/// Collapses 3+ spaces/newlines to single separator
static WHITESPACE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]{3,}").expect("WHITESPACE_RE"));

// ── Types ─────────────────────────────────────────────────────────────────────

/// A monetary value extracted during preprocessing.
#[derive(Debug, Clone)]
pub struct ExtractedValue {
    /// Original text fragment (e.g. "₹50 Cr")
    pub raw: String,
    /// Canonical float in base units (e.g. 500_000_000.0)
    pub normalized: f64,
    /// Currency hint ("INR" or "USD" or "EUR" or "unknown")
    pub currency: String,
}

/// Output of preprocessing for one section.
pub struct PreprocessedText {
    /// Clean, LLM-ready text with inline value annotations
    pub cleaned: String,
    /// Numeric values extracted (for use in scoring without re-parsing)
    pub extracted_values: Vec<ExtractedValue>,
}

// ── Service ───────────────────────────────────────────────────────────────────

pub struct PreprocessingService;

impl PreprocessingService {
    /// Preprocess raw section text before it is sent to the LLM.
    ///
    /// Steps:
    ///   1. Strip control characters
    ///   2. Annotate monetary expressions with their normalized value
    ///   3. Collapse excessive whitespace
    pub fn preprocess(section_name: &str, raw: &str) -> PreprocessedText {
        // Step 1: Strip control characters
        let s = CONTROL_RE.replace_all(raw, " ");

        // Step 2: Annotate monetary values
        let mut extracted_values: Vec<ExtractedValue> = Vec::new();

        let annotated = AMOUNT_RE.replace_all(&s, |caps: &regex::Captures| {
            let full = caps.get(0).map(|m| m.as_str()).unwrap_or("");
            let num_str = caps
                .get(2)
                .map(|m| m.as_str())
                .unwrap_or("0")
                .replace(',', "");
            let unit_str = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            let currency_sym = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();

            if let Ok(num) = num_str.parse::<f64>() {
                let scale = Self::scale(unit_str);
                let normalized = num * scale;
                let currency = Self::detect_currency(currency_sym);

                extracted_values.push(ExtractedValue {
                    raw: full.trim().to_string(),
                    normalized,
                    currency: currency.clone(),
                });

                // Annotate: keep original + add normalized in parentheses
                let formatted = Self::format_normalized(normalized, &currency);
                format!("{full} [≈{formatted}]")
            } else {
                full.to_string()
            }
        });

        // Step 3: Collapse excessive whitespace
        let clean = WHITESPACE_RE.replace_all(&annotated, "  ");
        let clean = clean
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        // Prefix with section context for LLM
        let prefixed = format!("## Section: {}\n\n{}", section_name, clean);

        PreprocessedText {
            cleaned: prefixed,
            extracted_values,
        }
    }

    /// Also preprocess a Vec<String> (e.g. bullet list items).
    pub fn preprocess_lines(section_name: &str, lines: &[String]) -> PreprocessedText {
        let joined = lines.join("\n");
        Self::preprocess(section_name, &joined)
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn scale(unit: &str) -> f64 {
        match unit.to_lowercase().as_str() {
            "crore" | "cr" => 10_000_000.0,
            "lakh" | "l" => 100_000.0,
            "billion" | "bn" | "b" => 1_000_000_000.0,
            "million" | "mn" | "m" => 1_000_000.0,
            "thousand" | "k" => 1_000.0,
            _ => 1.0,
        }
    }

    fn detect_currency(sym: &str) -> String {
        match sym.trim().to_lowercase().as_str() {
            "$" | "usd" => "USD".to_string(),
            "₹" | "inr" | "rs" | "rs." => "INR".to_string(),
            "€" | "eur" => "EUR".to_string(),
            "£" | "gbp" => "GBP".to_string(),
            _ => "unknown".to_string(),
        }
    }

    fn format_normalized(val: f64, currency: &str) -> String {
        let sym = match currency {
            "USD" => "$",
            "INR" => "₹",
            "EUR" => "€",
            "GBP" => "£",
            _ => "",
        };
        if val >= 1_000_000_000.0 {
            format!("{}{:.2}B", sym, val / 1_000_000_000.0)
        } else if val >= 1_000_000.0 {
            format!("{}{:.2}M", sym, val / 1_000_000.0)
        } else if val >= 1_000.0 {
            format!("{}{:.1}K", sym, val / 1_000.0)
        } else {
            format!("{}{:.0}", sym, val)
        }
    }
}

/// Quick helper: preprocess just the text and return the cleaned string.
pub fn clean_text(section_name: &str, raw: &str) -> String {
    PreprocessingService::preprocess(section_name, raw).cleaned
}

/// Truncate text for LLM to avoid long prompts and timeouts. Appends "... [truncated]" if cut.
pub fn truncate_for_llm(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }
    let cut = text
        .char_indices()
        .take(max_chars)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    format!("{}... [truncated]", text[..cut].trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_annotates_inr_cr() {
        let out = PreprocessingService::preprocess("Financial", "Revenue ₹50 Cr last year");
        assert!(out.cleaned.contains("[≈"), "Should annotate ₹50 Cr");
        assert!(!out.extracted_values.is_empty());
        assert_eq!(out.extracted_values[0].normalized, 500_000_000.0);
    }

    #[test]
    fn test_annotates_usd_m() {
        let out = PreprocessingService::preprocess("Financial", "Raised $2.5M seed round");
        assert!(!out.extracted_values.is_empty());
        assert_eq!(out.extracted_values[0].normalized, 2_500_000.0);
        assert_eq!(out.extracted_values[0].currency, "USD");
    }

    #[test]
    fn test_strips_control_chars() {
        let raw = "Hello\x00World\x1FTest";
        let out = PreprocessingService::preprocess("Test", raw);
        assert!(!out.cleaned.contains('\x00'));
    }
}
