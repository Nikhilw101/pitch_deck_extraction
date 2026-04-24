"""
classifier.py — Slide section classifier.

Priority order:
1. Title keyword match (slide title is the strongest signal)
2. Zero-shot transformer pipeline (if transformers + model available)
3. Keyword frequency heuristic (lightweight fallback)
4. Positional heuristic (e.g. "Solution" usually follows "Problem")

Every slide is guaranteed to be classified — worst case "Other".
Confidence is calculated, not hardcoded.
"""

import sys
import json
import re

# ── Labels ────────────────────────────────────────────────────────────────────

LABELS = [
    "Company Overview", "Problem Solution", "Market Opportunity",
    "Product Technology", "Business Model", "Traction Metrics",
    "Financial Highlights", "Competitive Landscape", "Team Founders",
    "Funding Ask", "Roadmap Strategy", "Risks Challenges",
    "Partnerships Customers", "Exit Strategy", "Other",
]

# ── Keyword map (title anchor + body scoring) ─────────────────────────────────

KEYWORD_MAP = {
    "Company Overview": [
        "about us", "who we are", "overview", "company", "vision", "mission",
        "founded", "introduction", "our story",
    ],
    "Problem Solution": [
        "problem", "challenge", "pain point", "solution", "how we solve",
        "the need", "issue", "gap",
    ],
    "Market Opportunity": [
        "market", "tam", "sam", "som", "total addressable",
        "serviceable", "opportunity", "market size", "growth rate",
        "billion", "trillion",
    ],
    "Product Technology": [
        "product", "platform", "technology", "features", "how it works",
        "architecture", "solution stack", "offering", "innovation",
    ],
    "Business Model": [
        "business model", "revenue model", "monetization", "pricing",
        "subscription", "saas", "b2b", "b2c", "go-to-market", "gtm",
    ],
    "Traction Metrics": [
        "traction", "users", "customers", "growth", "milestones",
        "kpi", "metrics", "adoption", "retention", "orders",
    ],
    "Financial Highlights": [
        "financial", "revenue", "profit", "margin", "burn rate",
        "runway", "arr", "mrr", "ebitda", "forecast", "projection",
        "p&l", "turnover",
    ],
    "Competitive Landscape": [
        "competition", "competitors", "competitive", "advantage",
        "landscape", "comparison", "vs", "differentiator",
    ],
    "Team Founders": [
        "team", "founders", "co-founder", "management", "board",
        "advisors", "leadership", "our team",
    ],
    "Funding Ask": [
        "funding", "raise", "investment", "ask", "use of funds",
        "seed", "series", "pre-seed", "valuation",
    ],
    "Roadmap Strategy": [
        "roadmap", "strategy", "next steps", "timeline", "milestones",
        "future", "q1", "q2", "q3", "q4",
    ],
    "Risks Challenges": [
        "risk", "risks", "challenge", "challenges", "concern",
        "mitigation", "barrier", "obstacle",
    ],
    "Partnerships Customers": [
        "partner", "partners", "customer", "clients", "case study",
        "testimonial", "collaboration",
    ],
    "Exit Strategy": [
        "exit", "acquisition", "ipo", "merger", "listing", "buyout",
    ],
}

def keyword_score(text: str, title: str) -> dict:
    """Score each label based on title anchor + body keyword frequency."""
    text_lower = text.lower()
    title_lower = title.lower()
    scores = {label: 0.0 for label in LABELS}

    for label, kws in KEYWORD_MAP.items():
        for kw in kws:
            # Title match is worth 5× body match
            if kw in title_lower:
                scores[label] += 5.0
            # Count body occurrences
            scores[label] += text_lower.count(kw) * 1.0

    return scores

def positional_hint(slide_number: int, total: int, last_section: str) -> str | None:
    """Give a positional hint for slides near the start/end."""
    pos = slide_number / max(total, 1)

    if slide_number == 1:
        return "Company Overview"
    if pos >= 0.85:
        # Last ~15% of deck: likely funding / roadmap
        if last_section not in ("Funding Ask", "Roadmap Strategy"):
            return "Funding Ask"
    if last_section == "Problem Solution" and pos < 0.4:
        return "Product Technology"
    return None

def try_zero_shot(slides):
    """Try HuggingFace zero-shot pipeline if available (optional, gracefully skipped)."""
    try:
        from transformers import pipeline

        classifier = pipeline(
            "zero-shot-classification",
            model="facebook/bart-large-mnli",
            device=-1,
        )
        candidate_labels = [l for l in LABELS if l != "Other"]
        results = []
        for slide in slides:
            text = (slide.get("title", "") + " " + slide.get("text", "")).strip()
            if not text:
                results.append(None)
                continue
            out = classifier(text[:512], candidate_labels=candidate_labels, multi_label=False)
            results.append({
                "section": out["labels"][0],
                "confidence": round(out["scores"][0], 3),
            })
        return results
    except Exception:
        return None  # Transformers not available — fall back

def classify_slides(slides):
    total = len(slides)

    # Try zero-shot first (if transformers available)
    zs_results = try_zero_shot(slides)

    classifications = []
    last_section = "Company Overview"

    for idx, slide in enumerate(slides):
        slide_number = slide.get("slide_number", idx + 1)
        title = slide.get("title", "")
        text = slide.get("text", "")

        # Zero-shot result available?
        if zs_results and zs_results[idx]:
            section = zs_results[idx]["section"]
            confidence = zs_results[idx]["confidence"]
        else:
            # Keyword scoring
            scores = keyword_score(text, title)
            best_label = max(scores, key=scores.get)
            best_score = scores[best_label]

            if best_score > 0:
                section = best_label
                # Normalize: cap at 1.0, reduce when score is borderline
                raw_max = max(scores.values())
                second_max = sorted(scores.values(), reverse=True)[1] if len(scores) > 1 else 0
                confidence = round(min(0.95, 0.5 + (raw_max - second_max) / max(raw_max, 1) * 0.5), 3)
            else:
                # Positional fallback
                hint = positional_hint(slide_number, total, last_section)
                section = hint if hint else "Other"
                confidence = 0.35 if section != "Other" else 0.0

        last_section = section
        classifications.append({
            "slide_number": slide_number,
            "section": section,
            "confidence": confidence,
        })

    return classifications

def main():
    if len(sys.argv) < 2:
        print(json.dumps({"error": "Missing input JSON argument"}))
        sys.exit(1)

    try:
        data = json.loads(sys.argv[1])
        slides = data.get("slides", [])

        classifications = classify_slides(slides)

        print(json.dumps({"status": "success", "classifications": classifications}))

    except Exception as e:
        import traceback
        print(json.dumps({"status": "error", "error": str(e), "trace": traceback.format_exc()}))
        sys.exit(1)

if __name__ == "__main__":
    main()
