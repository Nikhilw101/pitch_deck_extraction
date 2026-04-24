# LLM Prompts and Accuracy Guide

This document describes the prompt design used for Phase 4 (LLM) processing and recommendations for higher-accuracy extraction.

---

## Prompt Types Used in the Pipeline

### 1. Structured Data Extraction Prompt

- **Role**: The model is instructed to act as a **data extraction assistant**.
- **Input**: Section name, a **schema hint** (section-specific field list), and **cleaned slide text**.
- **Output**: Valid JSON only — no markdown, explanations, or commentary.
- **Rules**: Numeric values, currencies, percentages, and dates should be normalized when possible. Missing values must be `null` or omitted; the model must not invent values.

*Implemented in*: `StructuringService` + `LlmService::generate_structured_json`; schema hints from `get_schema_hint()`.

### 2. Section Summary Prompt

- **Role**: Generate a short summary of the section using the **structured JSON** produced earlier.
- **Instructions**: Concise, typically **2–3 sentences**; focus on the most important facts (business model, market size, traction, financial metrics).
- **Output**: Plain text only.

*Implemented in*: `SummarizationService::generate_section_summaries` → `generate_summary(..., Some(3))`.

### 3. Overall Deck Summary Prompt

- **Role**: After all sections are processed, produce a brief **executive summary** of the entire pitch deck.
- **Input**: Combined section summaries (or structured data when summary is missing).
- **Instructions**: **3–5 sentences**; highlight company, opportunity, traction, and key strengths or risks.
- **Output**: Plain text only.

*Implemented in*: `SummarizationService::generate_overall_summary` → `generate_summary(..., Some(5))`.

### 4. Investment Signals Prompt

- **Role**: Analyze structured section data and identify **positive investment signals**.
- **Examples**: Strong market growth, high revenue growth, large TAM, strong founding team, clear product–market fit.
- **Output**: JSON with `signals[]`: each has `type`, `description`, and `confidence` (0–1).

*Implemented in*: `SignalsService::extract_signals_and_flags` → `LlmService::extract_signals`.

### 5. Red Flags Prompt

- **Role**: Detect **potential risks or weaknesses** in the section data.
- **Examples**: Small market size, declining revenue, unrealistic growth claims, short runway, missing traction.
- **Output**: JSON with `red_flags[]`: each has `type`, `description`, and `severity` (low | medium | high | critical).

*Implemented in*: Same `extract_signals` call returns both `signals` and `red_flags` in one JSON object.

---

## Key Prompt Rules (Enforced in Code)

| Rule | How it’s enforced |
|------|-------------------|
| Return **structured output** when JSON is required | Prompts say “Return ONLY valid JSON”; we use Ollama `format: "json"` and slice `{ ... }` from the response. |
| **Concise, deterministic** responses | Temperature set to **0** (or 0.1); no extra commentary requested. |
| **No explanations** when structured data is requested | Instructions explicitly say “no markdown, no explanations”. |
| **Missing values = null/empty** | Schema instructions say “If a value is not present, return null. Do not invent values.” |

---

## Section-Specific Schema Hints

Different sections use different expected fields to improve precision:

| Section type | Example fields |
|--------------|----------------|
| Company / Overview | Name, Website, Founded, Mission, Employees, Location, Industry |
| Market / Opportunity | TAM, SAM, SOM, MarketSize, GrowthRate, Trends |
| Financial / Revenue | Revenue, RevenueGrowth, BurnRate, Runway, UnitEconomics, ProfitMargin |
| Traction / Metrics | Users, Growth, Milestones, Metrics |
| Team / Founder | Founders, TeamSize, KeyPeople, Advisors |
| Funding / Ask | Amount, UseOfFunds, PreviousFunding, Investors |

*Defined in*: `StructuringService::get_schema_hint()`.

---

## Accuracy Recommendations and Current Implementation

### 1. Use a Stronger Model

- **Recommendation**: Small models are fast but miss details. For higher accuracy, use a stronger model.
- **Suggested local models** (quantized, ~16 GB RAM): **Mistral 7B**, **Llama 3.1 8B**, **Phi-3 Medium**.
- **Current default**: `llama3.2:3b` (configurable via `OLLAMA_MODEL` in `.env`).
- **Action**: Set `OLLAMA_MODEL=mistral:7b` or `llama3.1:8b` (after `ollama pull`) for better extraction.

### 2. Two-Step Extraction (Future Improvement)

- **Recommendation**: Step 1 — extract facts in text form (“List the facts clearly”). Step 2 — convert those facts into JSON. Reduces hallucination and improves numeric accuracy.
- **Current**: Single-step extraction (direct JSON from section text).
- **Possible extension**: Add `extract_facts()` then `facts_to_json()` in `StructuringService` for an optional high-accuracy mode.

### 3. Strong Schema Instructions

- **Recommendation**: Explicit field list + “If not present, return null. Do not invent values.”
- **Current**: Schema hints list expected fields; prompts now include the null/invent rule in the structured extraction instruction.

### 4. Context Quality (Preprocessing)

- **Recommendation**: Remove duplicates, UI elements; merge bullets; normalize numbers and currencies.
- **Current**: `preprocessing_service::clean_text()`; slide text is combined with clear labels (Title, Header, Metric, etc.). Validation layer normalizes currencies (lakh/crore) and numbers via regex.

### 5. Temperature = 0

- **Recommendation**: Use `temperature = 0` and `top_p ≈ 0.9` for deterministic extraction.
- **Current**: Ollama options set to `temperature: 0`, `top_p: 0.9` in `call_ollama()`.

### 6. Validate Numbers with Regex

- **Recommendation**: After extraction, validate revenue, percentages, market sizes, dates with regex; prefer regex when it conflicts with LLM output.
- **Current**: `ValidationService` runs regex and numeric checks on extracted section data; scoring uses validated/normalized values where applicable.

### 7. Self-Verification Step (Future Improvement)

- **Recommendation**: After JSON extraction, send a second prompt: “Review the extracted JSON and the original text. Correct any incorrect values and remove any fields not supported by the text.”
- **Current**: Not implemented. Can be added as an optional pass in `StructuringService` or pipeline.

### 8. Reduce Parallel Requests

- **Recommendation**: Use 1–2 concurrent LLM requests to avoid model overload and unstable outputs.
- **Current**: Bounded concurrency via `Semaphore(2)` for structured extraction, section summaries, and signals extraction. Full content is sent (no truncation) so 2 in flight balances stability and completion.

### 9. Section-Specific Prompts

- **Recommendation**: Different sections get different instructions (e.g. Market: TAM/SAM/growth; Financial: revenue/burn/runway).
- **Current**: Implemented via `get_schema_hint(section_name)` and section-specific schema text in the extraction prompt.

### 10. Confidence Scoring

- **Recommendation**: Ask the model for confidence per field (0–1). Low-confidence fields can be flagged.
- **Current**: Signals have per-signal `confidence`; section-level and deck-level confidence are computed in `scoring_service`. Field-level confidence in the raw JSON is not yet requested in the extraction prompt (possible extension).

---

## Example High-Accuracy Pipeline (Target)

1. PDF parsing  
2. Slide text cleaning  
3. Section classification  
4. **Fact extraction (LLM)** ← optional two-step  
5. **JSON structuring (LLM)**  
6. Numeric validation (regex)  
7. Summary generation  
8. Signals and red-flag detection  
9. **Self-verification pass** ← optional  
10. Final scoring  

*Current pipeline* implements all except optional two-step extraction and self-verification.

**Quality-first pipeline (complete output over speed)**
- **No truncation**: Full section content is sent to the LLM for structured extraction, summaries, and signals. Ensures complete, accurate output; runtime may be longer but output is not cut off.
- **Timeout**: HTTP timeout for Ollama is **300s (5 min)** per call; sufficient for full sections on stronger models.
- **Concurrency**: **2** concurrent LLM calls per phase to avoid overload and ensure stable responses.
- **Summary prompts**: Section and overall summaries include instructions to always produce output for structured data (avoids "no summary to provide" refusals).
- **Signals prompt**: Explicitly requests exhaustive extraction of signals and red flags; empty arrays only when genuinely none.

Runtime with local model: typically **~7–15 minutes** for a full deck, prioritizing completeness over speed.
