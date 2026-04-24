use crate::models::extraction_model::Element;
use once_cell::sync::Lazy;
use regex::Regex;

// ── Static pre-compiled regexes (compiled once at startup) ───────────────────
static YEAR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(20\d{2}|19\d{2})\b").expect("YEAR_RE"));
static PERCENT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d+[\.,]?\d*\s*%").expect("PERCENT_RE"));
static MONEY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)[\$₹€£]?[\d\.,]+\s*(?:K|M|B|Cr|crore|L|lakh|mn|bn)?\b").expect("MONEY_RE")
});
static ISOLATED_STAT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*[\$₹€£]?\s*[\d\.,]+\s*(?:K|M|B|Cr|L|crore|lakh|%|x)?\s*$")
        .expect("ISOLATED_STAT_RE")
});
static BULLET_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^\s*[•▪►❑]\s*(.+)$").expect("BULLET_RE"));
static DASH_BULLET_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^\s*[-*]\s+(.+)$").expect("DASH_BULLET_RE"));

/// Classify a raw `Element::TextBlock` into a more specific element type.
///
/// Priority order (critical for correct detection):
/// 0. Noise filtering (page numbers, footers)
/// 1. Table detection (multi-line with aligned columns)
/// 2. Chart detection (year/percent/number clusters)
/// 3. Statistic detection (isolated numeric values like "$50M", "85%")
/// 4. Title detection (short text at top of page, ALL CAPS, or large bbox)
/// 5. Subtitle detection (short text below title zone)
/// 6. SectionHeader detection (short text mid-page)
/// 7. Bullet list detection (lines with bullet markers)
/// 8. Default: remains TextBlock
pub fn classify_element(elem: &mut Element) {
    let (text, bbox) = match elem {
        Element::TextBlock { text, bbox } => (text, bbox),
        _ => return,
    };

    let text_clone = text.trim().to_string();
    if text_clone.is_empty() {
        return;
    }

    let word_count = text_clone.split_whitespace().count();
    let lines: Vec<&str> = text_clone
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    let lines_count = lines.len();

    // ── 0. Noise Filtering ───────────────────────────────────────────────
    if word_count <= 3 && lines_count == 1 {
        if let Some(bx) = bbox {
            let is_near_edge = bx.y1 > 730.0 || bx.y0 > 700.0;
            let is_page_num = text_clone
                .chars()
                .all(|c| c.is_numeric() || c == '/' || c == '-' || c.is_whitespace() || c == '|')
                || text_clone.to_lowercase().starts_with("page ")
                || text_clone.to_lowercase().starts_with("slide ");

            if is_near_edge && is_page_num {
                *elem = Element::TextBlock {
                    text: "".to_string(),
                    bbox: None,
                };
                return;
            }
        }
    }

    // ── 1. Table Detection ───────────────────────────────────────────────
    // Require ≥3 lines and consistent column structure
    if lines_count >= 3 {
        let mut aligned_lines = 0;
        for line in &lines {
            if line.contains('\t') || line.contains(" | ") {
                aligned_lines += 1;
            } else {
                // Check for consistent multi-space gaps (column alignment in PDFs)
                let space_runs: Vec<_> = line.match_indices("   ").collect();
                if !space_runs.is_empty() {
                    aligned_lines += 1;
                }
            }
        }
        // Promote to table if majority of lines have column separators
        if aligned_lines >= (lines_count / 2) && aligned_lines >= 2 {
            *elem = Element::Table {
                headers: vec![],
                rows: lines.iter().map(|l| vec![l.to_string()]).collect(),
                bbox: bbox.clone(),
            };
            return;
        }
    }

    // ── 2. Chart Candidate ───────────────────────────────────────────────
    let mut chart_score = 0;
    for line in &lines {
        if YEAR_RE.is_match(line) {
            chart_score += 1;
        }
        if PERCENT_RE.is_match(line) {
            chart_score += 1;
        }
    }
    if chart_score >= 3 && word_count < 25 {
        *elem = Element::Chart {
            chart_type: crate::models::extraction_model::ChartType::Unknown,
            title: None,
            data: crate::models::extraction_model::ChartData {
                x_axis: None,
                series: vec![],
            },
            bbox: bbox.clone(),
        };
        return;
    }

    // ── 3. Statistic Detection ───────────────────────────────────────────
    // Single-line or very short text that is primarily a number/metric
    if word_count <= 8 && lines_count <= 2 {
        if let Some(first_line) = lines.first() {
            if ISOLATED_STAT_RE.is_match(first_line)
                || (PERCENT_RE.is_match(first_line) && word_count <= 4)
                || (MONEY_RE.is_match(first_line) && word_count <= 4)
            {
                let words: Vec<&str> = text_clone.split_whitespace().collect();
                let value = words[0].to_string();
                let label = words[1..].join(" ");
                *elem = Element::Statistic {
                    value,
                    label,
                    bbox: bbox.clone(),
                };
                return;
            }
        }
    }

    // ── 4. Title Detection ───────────────────────────────────────────────
    // Must come BEFORE bullet detection — single-line short text near top = Title
    let is_all_caps = text_clone.len() > 2
        && text_clone.chars().filter(|c| c.is_alphabetic()).count() >= 2
        && text_clone
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(|c| c.is_uppercase());

    if lines_count <= 2 && (1..=15).contains(&word_count) {
        let mut is_title = false;

        if let Some(bx) = bbox {
            // Large bbox height relative to text = likely a heading/title
            let bbox_height = bx.y1 - bx.y0;
            let height_per_word = if word_count > 0 {
                bbox_height / word_count as f32
            } else {
                0.0
            };

            if bx.y0 < 100.0 && lines_count == 1 {
                // Very top of page → almost certainly a title
                is_title = true;
            } else if bx.y0 < 150.0 && (is_all_caps || word_count <= 8) && lines_count == 1 {
                is_title = true;
            } else if is_all_caps && word_count <= 10 {
                // ALL CAPS text anywhere on page → likely a heading
                is_title = true;
            } else if height_per_word > 15.0 && word_count <= 6 && lines_count == 1 {
                // Large font proxy: big bbox for few words
                is_title = true;
            }
        } else if (word_count <= 6 && lines_count == 1) || (is_all_caps && word_count <= 10) {
            is_title = true;
        }

        if is_title {
            *elem = Element::Title {
                text: text_clone,
                bbox: bbox.clone(),
            };
            return;
        }
    }

    // ── 5. Subtitle Detection ────────────────────────────────────────────
    if word_count <= 20 && lines_count <= 2 {
        if let Some(bx) = bbox {
            if bx.y0 >= 80.0 && bx.y0 < 250.0 && word_count <= 15 {
                *elem = Element::Subtitle {
                    text: text_clone,
                    bbox: bbox.clone(),
                };
                return;
            }
        }
    }

    // ── 6. SectionHeader Detection ───────────────────────────────────────
    if word_count <= 10 && lines_count == 1 {
        if let Some(bx) = bbox {
            if bx.y0 >= 250.0 && bx.y0 < 600.0 {
                *elem = Element::SectionHeader {
                    text: text_clone,
                    bbox: bbox.clone(),
                };
                return;
            }
        }
    }

    // ── 7. Bullet List Detection ─────────────────────────────────────────
    // Only when genuinely bulleted lines are present
    let bullet_matches = lines
        .iter()
        .filter(|l| BULLET_RE.is_match(l) || DASH_BULLET_RE.is_match(l))
        .count();

    if bullet_matches >= 2 && (bullet_matches as f32 / lines_count as f32) >= 0.5 {
        let items: Vec<String> = lines
            .iter()
            .map(|l| {
                let mut t = l.trim().to_string();
                let symbols = ['•', '▪', '►', '❑'];
                t = t.trim_start_matches(&symbols[..]).trim().to_string();
                if t.starts_with("- ") {
                    t = t[2..].to_string();
                }
                if t.starts_with("* ") {
                    t = t[2..].to_string();
                }
                // Handle numbered bullets "1. "
                if let Some(pos) = t.find(". ") {
                    if pos <= 3 && t[..pos].chars().all(|c| c.is_numeric()) {
                        t = t[pos + 2..].to_string();
                    }
                }
                t.trim().to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();

        if !items.is_empty() {
            *elem = Element::BulletList {
                items,
                level: None,
                bbox: bbox.clone(),
            };
        }
    }

    // ── 8. Default: remains TextBlock ─────────────────────────────────────
}
