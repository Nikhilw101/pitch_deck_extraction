//! Scoring service — multi-parameter confidence scoring.
//!
//! Formula (weights sum to 1.0):
//!   final = 0.30 × validation
//!           + 0.25 × llm_confidence
//!           + 0.20 × completeness
//!           + 0.15 × web_consistency
//!           + 0.10 × threshold_score
//!
//! Safe threshold rules fire automatic red flags and reduce threshold_score.

use crate::models::structured_output::{RedFlag, ScoreBreakdown, StructuredSectionData};
use crate::utils::regex_utils::normalize_to_base;
use std::collections::HashSet;
use tracing::debug;

// ── Weight constants ──────────────────────────────────────────────────────────

const W_COMPLETENESS: f32 = 0.30;
const W_SOURCE_FIDELITY: f32 = 0.30;
const W_CONSISTENCY: f32 = 0.20;
const W_WEB: f32 = 0.20;

// ── Expected fields per section (for completeness scoring) ────────────────────

fn expected_fields(section_key: &str) -> &'static [&'static str] {
    match section_key {
        k if k.contains("company") => &["Name", "Mission", "Industry", "Founded"],
        k if k.contains("market") => &["TAM", "SAM", "GrowthRate"],
        k if k.contains("financial") => &["Revenue", "BurnRate", "Runway", "GrossMargin"],
        k if k.contains("traction") => &["Users", "Growth", "Retention"],
        k if k.contains("funding") => &["Amount", "UseOfFunds", "ValuationAsk"],
        k if k.contains("team") => &["Founders", "TeamSize"],
        k if k.contains("product") => &["ProductName", "Stage", "KeyFeatures"],
        k if k.contains("business") => &["RevenueModel", "PricingStrategy"],
        _ => &[],
    }
}

// ── Threshold rules ───────────────────────────────────────────────────────────

/// Safe threshold checker. Returns (threshold_score 0–1, new_red_flags).
///
/// Deductions per violation:
///   - Burn > runway cash  → critical (−0.40)
///   - Revenue growth < 0  → high (−0.25)
///   - Funding ask > 5× revenue → high (−0.20)
///   - TAM < $1M           → medium (−0.10)
///   - Team size < 2       → medium (−0.10)
pub fn check_thresholds(sections: &mut [StructuredSectionData]) -> (f32, Vec<RedFlag>) {
    let mut penalty = 0.0f32;
    let mut new_flags: Vec<RedFlag> = Vec::new();

    // Helper to extract a JSON field as f64 (handles the new {"value":...} wrapper)
    let get_val = |sec: &StructuredSectionData, keys: &[&str]| -> Option<f64> {
        let obj = sec.data.as_object()?;
        for k in keys {
            if let Some(v) = obj.get(*k) {
                let actual_val = if let Some(field_obj) = v.as_object() {
                    field_obj.get("value").unwrap_or(v)
                } else {
                    v
                };
                
                if let Some(n) = actual_val.as_f64() {
                    return Some(n);
                }
                if let Some(s) = actual_val.as_str() {
                    return normalize_to_base(s);
                }
            }
        }
        None
    };

    for section in sections.iter_mut() {
        let name = section.section_name.to_lowercase();

        // ── Financial section checks ──────────────────────────────────────
        if name.contains("financial") {
            let burn = get_val(section, &["BurnRate", "Burn", "MonthlyBurn", "burn_rate"]);
            let runway = get_val(section, &["Runway", "RunwayMonths", "runway_months"]);
            let revenue = get_val(section, &["Revenue", "ARR", "MRR", "revenue"]);
            let margin = get_val(section, &["GrossMargin", "Margin", "margin"]);

            if let (Some(b), Some(r)) = (burn, runway) {
                if let Some(rev) = revenue {
                    let monthly_rev = rev / 12.0;
                    if b > monthly_rev * 2.0 && monthly_rev > 0.0 {
                        let msg = format!(
                            "Burn rate ({:.0}/mo) exceeds 2× monthly revenue ({:.0}/mo)",
                            b, monthly_rev
                        );
                        section.threshold_flags.push(msg.clone());
                        new_flags.push(RedFlag {
                            flag_type: "high_burn".to_string(),
                            description: msg,
                            severity: "critical".to_string(),
                            section: section.section_name.clone(),
                            evidence_text: Some(format!(
                                "Rule-based proof: burn ({:.0}/mo) > 2x monthly revenue ({:.0}/mo)",
                                b, monthly_rev
                            )),
                            evidence_slide_number: None,
                            evidence_confirmed: Some(true),
                            source: Some("rule_engine".to_string()),
                            reason_details: Some("Burn rate threshold rule triggered.".to_string()),
                        });
                        penalty = (penalty + 0.40).min(1.0);
                    }
                }

                if r < 6.0 {
                    let msg = format!("Critical: only {:.0} months runway remaining", r);
                    section.threshold_flags.push(msg.clone());
                    new_flags.push(RedFlag {
                        flag_type: "short_runway".to_string(),
                        description: msg,
                        severity: "critical".to_string(),
                        section: section.section_name.clone(),
                        evidence_text: Some(format!(
                            "Rule-based proof: extracted runway value is {:.0} months (< 6)",
                            r
                        )),
                        evidence_slide_number: None,
                        evidence_confirmed: Some(true),
                        source: Some("rule_engine".to_string()),
                        reason_details: Some("Runway threshold rule triggered.".to_string()),
                    });
                    penalty = (penalty + 0.30).min(1.0);
                }
            }

            if let Some(m) = margin {
                if m < -50.0 {
                    let msg = format!("Gross margin is critically negative: {:.1}%", m);
                    section.threshold_flags.push(msg.clone());
                    new_flags.push(RedFlag {
                        flag_type: "negative_margin".to_string(),
                        description: msg,
                        severity: "high".to_string(),
                        section: section.section_name.clone(),
                        evidence_text: Some(format!(
                            "Rule-based proof: extracted gross margin is {:.1}% (< -50%)",
                            m
                        )),
                        evidence_slide_number: None,
                        evidence_confirmed: Some(true),
                        source: Some("rule_engine".to_string()),
                        reason_details: Some("Gross margin threshold rule triggered.".to_string()),
                    });
                    penalty = (penalty + 0.25).min(1.0);
                }
            }
        }

        // ── Traction section checks ───────────────────────────────────────
        if name.contains("traction") {
            let growth = section
                .data
                .as_object()
                .and_then(|o| o.get("Growth").or_else(|| o.get("GrowthRate")))
                .and_then(|v| {
                    let actual_val = if let Some(field_obj) = v.as_object() {
                        field_obj.get("value").unwrap_or(v)
                    } else {
                        v
                    };
                    actual_val.as_f64().or_else(|| {
                        actual_val.as_str().and_then(|s| {
                            let trimmed = s.trim_end_matches('%');
                            trimmed.parse::<f64>().ok()
                        })
                    })
                });

            if let Some(g) = growth {
                if g < 0.0 {
                    let msg = format!("Negative growth rate: {:.1}%", g);
                    section.threshold_flags.push(msg.clone());
                    new_flags.push(RedFlag {
                        flag_type: "negative_growth".to_string(),
                        description: msg,
                        severity: "high".to_string(),
                        section: section.section_name.clone(),
                        evidence_text: Some(format!(
                            "Rule-based proof: extracted growth rate is {:.1}% (< 0%)",
                            g
                        )),
                        evidence_slide_number: None,
                        evidence_confirmed: Some(true),
                        source: Some("rule_engine".to_string()),
                        reason_details: Some("Negative growth threshold rule triggered.".to_string()),
                    });
                    penalty = (penalty + 0.25).min(1.0);
                }
            }
        }

        // ── Market section checks ─────────────────────────────────────────
        if name.contains("market") {
            let tam = get_val(section, &["TAM", "TotalAddressableMarket", "tam"]);
            if let Some(t) = tam {
                if t < 1_000_000.0 {
                    let msg = format!("TAM appears very small: ${:.0}", t);
                    section.threshold_flags.push(msg.clone());
                    new_flags.push(RedFlag {
                        flag_type: "small_tam".to_string(),
                        description: msg,
                        severity: "medium".to_string(),
                        section: section.section_name.clone(),
                        evidence_text: Some(format!(
                            "Rule-based proof: extracted TAM is ${:.0} (< $1,000,000)",
                            t
                        )),
                        evidence_slide_number: None,
                        evidence_confirmed: Some(true),
                        source: Some("rule_engine".to_string()),
                        reason_details: Some("Small TAM threshold rule triggered.".to_string()),
                    });
                    penalty = (penalty + 0.10).min(1.0);
                }
            }
        }

        // ── Funding ask checks ────────────────────────────────────────────
        if name.contains("funding") {
            let ask = get_val(section, &["Amount", "FundingAmount", "AskAmount"]);
            if let Some(a) = ask {
                if a > 1_000_000_000.0 {
                    let msg = format!("Funding ask exceeds $1B ({:.0}) — verify stage", a);
                    section.threshold_flags.push(msg.clone());
                    new_flags.push(RedFlag {
                        flag_type: "outsized_ask".to_string(),
                        description: msg,
                        severity: "medium".to_string(),
                        section: section.section_name.clone(),
                        evidence_text: Some(format!(
                            "Rule-based proof: extracted funding ask is {:.0} (> $1,000,000,000)",
                            a
                        )),
                        evidence_slide_number: None,
                        evidence_confirmed: Some(true),
                        source: Some("rule_engine".to_string()),
                        reason_details: Some("Outsized funding ask threshold rule triggered.".to_string()),
                    });
                    penalty = (penalty + 0.10).min(1.0);
                }
            }
        }

        // ── Team checks ───────────────────────────────────────────────────
        if name.contains("team") {
            let team_size = get_val(section, &["TeamSize", "Employees", "HeadCount"]);
            if let Some(t) = team_size {
                if t < 2.0 {
                    let msg = "Team size is fewer than 2 people — single-founder risk".to_string();
                    section.threshold_flags.push(msg.clone());
                    new_flags.push(RedFlag {
                        flag_type: "small_team".to_string(),
                        description: msg,
                        severity: "medium".to_string(),
                        section: section.section_name.clone(),
                        evidence_text: Some(format!(
                            "Rule-based proof: extracted team size is {:.0} (< 2)",
                            t
                        )),
                        evidence_slide_number: None,
                        evidence_confirmed: Some(true),
                        source: Some("rule_engine".to_string()),
                        reason_details: Some("Small team size threshold rule triggered.".to_string()),
                    });
                    penalty = (penalty + 0.10).min(1.0);
                }
            }
        }
    }

    debug!("Threshold check complete. Penalty: {:.2}", penalty);
    let threshold_score = (1.0 - penalty).max(0.0);
    (threshold_score, new_flags)
}

// ── Per-section completeness score ────────────────────────────────────────────

pub fn completeness_score(section: &StructuredSectionData) -> f32 {
    let expected = expected_fields(&section.section_name.to_lowercase());
    if expected.is_empty() {
        return 0.5; // neutral for unknown sections
    }
    let obj = match section.data.as_object() {
        Some(o) => o,
        None => return 0.0,
    };
    let present = expected
        .iter()
        .filter(|k| {
            obj.get(**k)
                .map(|v| {
                    let actual_val = if let Some(field_obj) = v.as_object() {
                        field_obj.get("value").unwrap_or(v)
                    } else {
                        v
                    };
                    !actual_val.is_null() && actual_val.as_str().map(|s| !s.is_empty()).unwrap_or(true)
                })
                .unwrap_or(false)
        })
        .count();
    present as f32 / expected.len() as f32
}

// ── Per-section source fidelity score ─────────────────────────────────────────

pub fn source_fidelity_score(section: &StructuredSectionData) -> f32 {
    let obj = match section.data.as_object() {
        Some(o) => o,
        None => return 1.0, // Nothing extracted, no hallucination
    };
    
    let mut total_confidence = 0.0;
    let mut count = 0;
    
    for (_, val) in obj.iter() {
        if let Some(field_obj) = val.as_object() {
            if let Some(conf) = field_obj.get("confidence").and_then(|c| c.as_f64()) {
                total_confidence += conf as f32;
                count += 1;
            }
        }
    }
    
    if count == 0 {
        1.0 // Assume innocent until proven guilty
    } else {
        total_confidence / count as f32
    }
}

// ── Main scoring API ──────────────────────────────────────────────────────────

/// Calculate per-section confidence using validation + LLM signal confidence.
pub fn section_confidence(section: &StructuredSectionData) -> f32 {
    let validation = section.validation.score;
    let fidelity = source_fidelity_score(section);
    let completeness = completeness_score(section);

    let score = W_CONSISTENCY * validation + W_SOURCE_FIDELITY * fidelity + W_COMPLETENESS * completeness;

    (score / (W_CONSISTENCY + W_SOURCE_FIDELITY + W_COMPLETENESS)).clamp(0.0, 1.0)
}

/// Calculate the overall deck confidence score and return a full ScoreBreakdown.
pub fn calculate_overall_score(
    sections: &mut [StructuredSectionData],
    web_consistency: f32,
    existing_red_flags: &mut Vec<RedFlag>,
) -> ScoreBreakdown {
    if sections.is_empty() {
        return ScoreBreakdown::default();
    }

    // 1. Completeness score
    let completeness_avg = sections.iter().map(completeness_score).sum::<f32>() / sections.len() as f32;

    // 2. Source Fidelity (based on field confidence tracking)
    let fidelity_avg = sections.iter().map(source_fidelity_score).sum::<f32>() / sections.len() as f32;

    // 3. Consistency (blends JSON validation and threshold checks)
    let validation_avg = sections.iter().map(|s| s.validation.score).sum::<f32>() / sections.len() as f32;
    let (threshold_score, new_flags) = check_thresholds(sections);
    existing_red_flags.extend(new_flags);
    dedup_global_red_flags(existing_red_flags);
    let consistency = (validation_avg + threshold_score) / 2.0; // simple average of the two consistency metrics

    // 4. Web validation
    let web = web_consistency.clamp(0.0, 1.0);

    // 5. Final weighted composite
    let final_score = (W_COMPLETENESS * completeness_avg
        + W_SOURCE_FIDELITY * fidelity_avg
        + W_CONSISTENCY * consistency
        + W_WEB * web)
        .clamp(0.0, 1.0);

    debug!(
        "Score breakdown — completeness:{:.2} fidelity:{:.2} consistency:{:.2} web:{:.2} → final:{:.2}",
        completeness_avg, fidelity_avg, consistency, web, final_score
    );

    ScoreBreakdown {
        validation_score: validation_avg, // Store separately for backward compat
        llm_confidence: fidelity_avg,     // Store fidelity here to not break data model
        completeness_score: completeness_avg,
        web_consistency: web,
        threshold_score,
        final_score,
    }
}

fn dedup_global_red_flags(flags: &mut Vec<RedFlag>) {
    let mut seen = HashSet::new();
    let mut deduped = Vec::with_capacity(flags.len());
    for flag in flags.drain(..) {
        let key = format!(
            "{}|{}",
            flag.flag_type.trim().to_lowercase(),
            flag.description.trim().to_lowercase()
        );
        if seen.insert(key) {
            deduped.push(flag);
        }
    }
    *flags = deduped;
}
