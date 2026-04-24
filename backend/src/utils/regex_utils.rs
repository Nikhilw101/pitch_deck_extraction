use once_cell::sync::Lazy;
use regex::Regex;

// ── Pre-compiled regex constants ──────────────────────────────────────────────

/// Matches amounts with optional currency symbols and scale suffixes.
/// Supports: $5M  ₹50Cr  3.5 lakh  2B  €100K  Rs. 25L  etc.
static MONEY_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(?:[\$₹€£]|INR|Rs\.?\s*)?\s*([\d,]+(?:\.\d+)?)\s*(crore|cr|lakh|l|billion|bn|million|mn|thousand|k|m|b)?\b"
    ).expect("MONEY_REGEX must compile")
});

/// Matches percentage values: 25%, 15.5%, 100%
static PERCENTAGE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\d+(?:\.\d+)?)\s*%").expect("PERCENTAGE_REGEX must compile"));

/// Matches plain numbers with optional comma separators
static NUMBER_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(\d+(?:,\d{3})*(?:\.\d+)?)\b").expect("NUMBER_REGEX must compile"));

// ── Scale multipliers ─────────────────────────────────────────────────────────

fn scale_multiplier(unit: &str) -> f64 {
    match unit.to_lowercase().trim() {
        "crore" | "cr" => 10_000_000.0,
        "lakh" | "l" => 100_000.0,
        "billion" | "bn" | "b" => 1_000_000_000.0,
        "million" | "mn" | "m" => 1_000_000.0,
        "thousand" | "k" => 1_000.0,
        _ => 1.0,
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Extract monetary values normalized to base units.
/// "₹50 Cr" → 500_000_000.0 | "$2.5M" → 2_500_000.0 | "2.5 lakh" → 250_000.0
pub fn extract_money(text: &str) -> Vec<f64> {
    MONEY_REGEX
        .captures_iter(text)
        .filter_map(|cap| {
            let raw = cap.get(1)?.as_str().replace(',', "");
            let num: f64 = raw.parse().ok()?;
            let scale = cap
                .get(2)
                .map(|m| scale_multiplier(m.as_str()))
                .unwrap_or(1.0);
            // Skip bare integers without a currency/scale prefix to avoid false matches
            if scale == 1.0
                && cap
                    .get(0)?
                    .as_str()
                    .trim_start()
                    .starts_with(|c: char| c.is_ascii_digit())
            {
                return None;
            }
            Some(num * scale)
        })
        .collect()
}

/// Extract percentage values.
pub fn extract_percentages(text: &str) -> Vec<f64> {
    PERCENTAGE_REGEX
        .captures_iter(text)
        .filter_map(|cap| cap.get(1)?.as_str().parse().ok())
        .collect()
}

/// Extract all plain numbers from text.
pub fn extract_numbers(text: &str) -> Vec<f64> {
    NUMBER_REGEX
        .captures_iter(text)
        .filter_map(|cap| cap.get(1)?.as_str().replace(',', "").parse().ok())
        .collect()
}

/// Returns true if text contains any numeric data.
pub fn contains_numeric_data(text: &str) -> bool {
    NUMBER_REGEX.is_match(text) || PERCENTAGE_REGEX.is_match(text)
}

/// Normalize first monetary or numeric value found to its base float.
/// Returns None if no parseable number is found.
pub fn normalize_to_base(text: &str) -> Option<f64> {
    // Try monetary first (has proper scale), then fallback to bare number
    extract_money(text)
        .into_iter()
        .next()
        .or_else(|| {
            // Try with explicit scale suffix even without currency symbol
            MONEY_REGEX.captures(text).and_then(|cap| {
                let unit = cap.get(2)?.as_str();
                if scale_multiplier(unit) > 1.0 {
                    let raw = cap.get(1)?.as_str().replace(',', "");
                    let num: f64 = raw.parse().ok()?;
                    Some(num * scale_multiplier(unit))
                } else {
                    None
                }
            })
        })
        .or_else(|| extract_numbers(text).into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_money_usd() {
        let v = extract_money("Revenue $5M");
        assert_eq!(v[0], 5_000_000.0);
    }

    #[test]
    fn test_extract_money_inr_cr() {
        let v = extract_money("₹50 Cr ARR");
        assert!(!v.is_empty(), "Should parse ₹50 Cr");
        assert_eq!(v[0], 500_000_000.0);
    }

    #[test]
    fn test_extract_money_lakh() {
        let v = extract_money("2.5 lakh users");
        assert_eq!(v[0], 250_000.0);
    }

    #[test]
    fn test_extract_money_billion() {
        let v = extract_money("$2.5B valuation");
        assert_eq!(v[0], 2_500_000_000.0);
    }

    #[test]
    fn test_extract_percentages() {
        assert_eq!(extract_percentages("Growth: 25%"), vec![25.0]);
        assert_eq!(extract_percentages("15.5% increase"), vec![15.5]);
    }

    #[test]
    fn test_extract_numbers() {
        let nums = extract_numbers("We have 1,000 users and 50 employees");
        assert!(nums.contains(&1000.0));
        assert!(nums.contains(&50.0));
    }

    #[test]
    fn test_normalize_to_base_cr() {
        assert_eq!(normalize_to_base("10 Cr"), Some(100_000_000.0));
    }

    #[test]
    fn test_normalize_to_base_m() {
        assert_eq!(normalize_to_base("$1.5M"), Some(1_500_000.0));
    }
}
