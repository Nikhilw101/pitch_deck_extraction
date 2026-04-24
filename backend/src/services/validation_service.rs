use crate::models::structured_output::{StructuredSectionData, ValidationError, ValidationResults};
use crate::utils::regex_utils;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;
use tracing::info;

/// Pre-compiled date patterns to avoid panic and repeated compilation in is_valid_date
static DATE_PATTERN_YYYY_MM_DD: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("date pattern"));
static DATE_PATTERN_MM_DD_YYYY: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\d{2}/\d{2}/\d{4}$").expect("date pattern"));
static DATE_PATTERN_M_D_YYYY: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\d{1,2}/\d{1,2}/\d{4}$").expect("date pattern"));
static DATE_PATTERN_YEAR: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{4}$").expect("date pattern"));
static MONTH_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(?:january|february|march|april|may|june|july|august|september|october|november|december|jan|feb|mar|apr|jun|jul|aug|sep|oct|nov|dec)\b").expect("month pattern")
});

/// Service for validating structured data using regex and numeric checks
pub struct ValidationService;

impl ValidationService {
    /// Validate structured section data
    ///
    /// Validates currency, percentages, dates, and other numeric fields
    ///
    /// # Arguments
    /// * `section_data` - The structured section data to validate
    ///
    /// # Returns
    /// * `ValidationResults` - Validation results with errors and score
    pub fn validate_section(&self, section_data: &mut StructuredSectionData) -> ValidationResults {
        info!("Validating section: {}", section_data.section_name);

        let mut errors = Vec::new();
        let mut fields_validated = 0;
        let mut fields_passed = 0;

        // Validate JSON structure recursively
        self.validate_json_value(
            &section_data.data,
            "",
            &mut errors,
            &mut fields_validated,
            &mut fields_passed,
        );

        // Calculate score
        let score = if fields_validated > 0 {
            fields_passed as f32 / fields_validated as f32
        } else {
            1.0 // No fields to validate = perfect score
        };

        let results = ValidationResults {
            fields_validated,
            fields_passed,
            errors: errors.clone(),
            score,
        };

        // Update section data
        section_data.validation = results.clone();

        results
    }

    /// Recursively validate JSON value
    fn validate_json_value(
        &self,
        value: &Value,
        path: &str,
        errors: &mut Vec<ValidationError>,
        fields_validated: &mut usize,
        fields_passed: &mut usize,
    ) {
        match value {
            Value::Object(map) => {
                let key_lower = path.to_lowercase();
                
                // Specific struct validation
                if key_lower.contains("turnover") || key_lower.contains("revenue") {
                    if map.contains_key("value") && map.contains_key("currency") {
                        *fields_validated += 1;
                        let val = map.get("value").unwrap();
                        let curr = map.get("currency").unwrap();
                        
                        let is_valid = match val {
                            Value::Number(n) => n.as_f64().unwrap_or(0.0) > 0.0,
                            Value::String(s) => regex_utils::extract_numbers(s).iter().any(|&n| n > 0.0),
                            _ => false,
                        };
                        
                        if is_valid && curr.is_string() {
                            *fields_passed += 1;
                        } else {
                            errors.push(ValidationError {
                                field: path.to_string(),
                                error_type: "invalid_turnover_object".to_string(),
                                message: "Turnover must have positive value and string currency".to_string(),
                                value: Some(value.to_string()),
                            });
                        }
                    }
                }
            
                for (key, val) in map {
                    let new_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    self.validate_json_value(
                        val,
                        &new_path,
                        errors,
                        fields_validated,
                        fields_passed,
                    );
                }
            }
            Value::String(s) => {
                *fields_validated += 1;
                let key_lower = path.to_lowercase();
                let mut passed = false;

                // Typed extractors based on key
                if key_lower.contains("founded") {
                    if let Ok(year) = s.parse::<i32>() {
                        if (1900..=2026).contains(&year) {
                            passed = true;
                        }
                    }
                    if !passed {
                        errors.push(ValidationError {
                            field: path.to_string(),
                            error_type: "invalid_year".to_string(),
                            message: format!("Founded must be a year between 1900-2026, got: {}", s),
                            value: Some(s.clone()),
                        });
                    }
                } else if key_lower.contains("website") || key_lower.contains("url") {
                    if s.contains("http://") || s.contains("https://") || s.contains("www.") || s.contains(".com") || s.contains(".in") {
                        passed = true;
                    } else {
                        errors.push(ValidationError {
                            field: path.to_string(),
                            error_type: "invalid_url".to_string(),
                            message: format!("Invalid URL format: {}", s),
                            value: Some(s.clone()),
                        });
                    }
                } else if key_lower.contains("teamsize") || key_lower.contains("employees") {
                    let num = s.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
                    if let Ok(size) = num.parse::<i32>() {
                        if (1..=10000).contains(&size) {
                            passed = true;
                        }
                    }
                    if !passed {
                        errors.push(ValidationError {
                            field: path.to_string(),
                            error_type: "invalid_teamsize".to_string(),
                            message: format!("TeamSize must be 1-10000, got: {}", s),
                            value: Some(s.clone()),
                        });
                    }
                } else {
                    // Generic heuristics for unknown fields
                    if self.looks_like_currency(s) {
                        if regex_utils::extract_money(s).is_empty() {
                            errors.push(ValidationError {
                                field: path.to_string(),
                                error_type: "invalid_currency".to_string(),
                                message: format!("Invalid currency format: {}", s),
                                value: Some(s.clone()),
                            });
                        } else { passed = true; }
                    } else if s.contains('%') {
                        if regex_utils::extract_percentages(s).is_empty() {
                            errors.push(ValidationError {
                                field: path.to_string(),
                                error_type: "invalid_percentage".to_string(),
                                message: format!("Invalid percentage format: {}", s),
                                value: Some(s.clone()),
                            });
                        } else { passed = true; }
                    } else if self.looks_like_date(s) {
                        if !self.is_valid_date(s) {
                            errors.push(ValidationError {
                                field: path.to_string(),
                                error_type: "invalid_date".to_string(),
                                message: format!("Invalid date format: {}", s),
                                value: Some(s.clone()),
                            });
                        } else { passed = true; }
                    } else if self.looks_like_number(s) {
                        // Avoid overly aggressive number validation if it looks like a fraction or string code
                        if s.contains('/') && s.len() < 10 {
                            errors.push(ValidationError {
                                field: path.to_string(),
                                error_type: "invalid_number_fraction".to_string(),
                                message: format!("Suspicious number format (fraction/date mixed): {}", s),
                                value: Some(s.clone()),
                            });
                        } else if !regex_utils::extract_numbers(s).is_empty() {
                            passed = true;
                        } else {
                            errors.push(ValidationError {
                                field: path.to_string(),
                                error_type: "invalid_number".to_string(),
                                message: format!("Invalid number format: {}", s),
                                value: Some(s.clone()),
                            });
                        }
                    } else {
                        // String field - just count as passed
                        passed = true;
                    }
                }
                
                if passed {
                    *fields_passed += 1;
                }
            }
            Value::Number(n) => {
                *fields_validated += 1;
                let key_lower = path.to_lowercase();
                let mut passed = true;
                
                if key_lower.contains("founded") {
                    let year = n.as_i64().unwrap_or(0);
                    if !(1900..=2026).contains(&year) {
                        passed = false;
                        errors.push(ValidationError {
                            field: path.to_string(),
                            error_type: "invalid_year".to_string(),
                            message: format!("Founded must be 1900-2026, got: {}", year),
                            value: Some(year.to_string()),
                        });
                    }
                } else if key_lower.contains("teamsize") || key_lower.contains("employees") {
                    let size = n.as_i64().unwrap_or(0);
                    if !(1..=10000).contains(&size) {
                        passed = false;
                        errors.push(ValidationError {
                            field: path.to_string(),
                            error_type: "invalid_teamsize".to_string(),
                            message: format!("TeamSize must be 1-10000, got: {}", size),
                            value: Some(size.to_string()),
                        });
                    }
                }
                
                if passed { *fields_passed += 1; }
            }
            Value::Bool(_) => {
                *fields_validated += 1;
                *fields_passed += 1;
            }
            Value::Array(arr) => {
                for (idx, item) in arr.iter().enumerate() {
                    let new_path = format!("{}[{}]", path, idx);
                    self.validate_json_value(
                        item,
                        &new_path,
                        errors,
                        fields_validated,
                        fields_passed,
                    );
                }
            }
            Value::Null => {}
        }
    }

    /// Check if string looks like currency
    fn looks_like_currency(&self, s: &str) -> bool {
        let lower = s.to_lowercase();
        s.contains('$')
            || s.contains('₹')
            || s.contains('€')
            || s.contains('£')
            || lower.contains("usd")
            || lower.contains("dollar")
            || lower.contains("inr")
            || lower.contains("rupee")
    }

    /// Check if string looks like a date (avoids false positives on phrases like "AI-Powered").
    /// Requires: slash or hyphen with digits (e.g. 2024-01-15), or a month name in a short/date-like context.
    fn looks_like_date(&self, s: &str) -> bool {
        let has_digit = s.chars().any(|c| c.is_ascii_digit());
        let has_slash = s.contains('/');
        let has_hyphen = s.contains('-');
        // Numeric date patterns: must have digit and / or -
        if (has_slash || has_hyphen) && has_digit {
            return true;
        }
        // Month names only in short tokens (e.g. "Jan 2024", "December")
        if s.len() <= 25 && MONTH_PATTERN.is_match(s) {
            return true;
        }
        false
    }

    /// Check if string looks like a number
    fn looks_like_number(&self, s: &str) -> bool {
        s.chars().any(|c| c.is_ascii_digit())
            && (s.contains(',') || s.contains('.') || s.parse::<f64>().is_ok())
    }

    /// Basic date validation (uses pre-compiled regexes to avoid panic and repeated compilation)
    fn is_valid_date(&self, s: &str) -> bool {
        if DATE_PATTERN_YYYY_MM_DD.is_match(s)
            || DATE_PATTERN_MM_DD_YYYY.is_match(s)
            || DATE_PATTERN_M_D_YYYY.is_match(s)
            || DATE_PATTERN_YEAR.is_match(s)
        {
            return true;
        }

        // Check for month names using compiled regex with word boundaries
        MONTH_PATTERN.is_match(s)
    }
}
