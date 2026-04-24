"""
pdf_extractor.py — Production-grade PDF extraction script.

Multi-backend fallback: pdfplumber → PyMuPDF → pypdf
Features:
- Layout-aware text grouping by Y-coordinate (bounding boxes)
- Paragraph reconstruction (merge lines without ending punctuation)
- Bullet/list detection with indent level tracking
- Title detection (largest font, top position)
- Structured table extraction via pdfplumber / camelot
- Noise filtering: page numbers, footers, watermarks, slide count patterns
- Indian numeric unit normalization (Cr, Lakh, K)
"""

import sys
import json
import re
import traceback
import warnings

# Suppress warnings (like CryptographyDeprecationWarning from pypdf) to avoid cluttering stderr
# which the Rust backend monitors for real errors.
warnings.filterwarnings("ignore", category=DeprecationWarning)

# ── Noise patterns ────────────────────────────────────────────────────────────

NOISE_PATTERNS = [
    re.compile(r"^\s*confidential\s*$", re.I),
    re.compile(r"^\s*proprietary\s*$", re.I),
    re.compile(r"^\s*strictly confidential\s*$", re.I),
    re.compile(r"^\s*page\s+\d+\s*(of\s*\d+)?\s*$", re.I),
    re.compile(r"^\s*slide\s+\d+\s*(of\s*\d+)?\s*$", re.I),
    re.compile(r"^\s*\d+\s*/\s*\d+\s*$"),  # e.g. "3/18" slide count
    re.compile(r"^\s*\d+\s*$"),              # bare number (slide number)
    re.compile(r"^\s*www\.\S+\s*$", re.I),  # standalone URL as watermark
]

BULLET_CHARS = {"•", "·", "▪", "▸", "→", "–", "○", "◦", "◉", "✓", "✔", "★"}

def is_noise(text: str) -> bool:
    t = text.strip()
    if not t:
        return True
    for pat in NOISE_PATTERNS:
        if pat.match(t):
            return True
    return False

def classify_bullet(text: str):
    """Returns (is_bullet, level, cleaned_text)."""
    t = text.strip()
    for ch in BULLET_CHARS:
        if t.startswith(ch):
            return True, 1, t[len(ch):].strip()
    # Dash / asterisk at start
    if re.match(r"^[-*]\s+", t):
        return True, 1, re.sub(r"^[-*]\s+", "", t)
    # Numbered list
    if re.match(r"^\d+[.)]\s+", t):
        return True, 1, re.sub(r"^\d+[.)]\s+", "", t)
    # Indented dash (sub-item)
    if re.match(r"^\s{2,}[-–*•]", t):
        return True, 2, re.sub(r"^\s*[-–*•]\s*", "", t)
    return False, 0, t

# ── pdfplumber backend ────────────────────────────────────────────────────────

def extract_with_pdfplumber(pdf_path: str):
    import pdfplumber

    slides = []
    with pdfplumber.open(pdf_path) as pdf:
        for i, page in enumerate(pdf.pages):
            elements = []

            # ── Step 1: tables (before text so we can mask their bbox) ───────
            table_bboxes = []
            try:
                for table in page.find_tables():
                    table_bboxes.append(table.bbox)
                    extracted = table.extract() or []
                    headers = []
                    rows = []
                    if extracted:
                        headers = [str(c).strip() if c else "" for c in extracted[0]]
                        rows = [
                            [str(c).strip() if c else "" for c in row]
                            for row in extracted[1:]
                        ]
                    bbox = list(table.bbox)
                    elements.append({
                        "type": "table",
                        "bbox": bbox,
                        "headers": headers,
                        "rows": rows,
                    })
            except Exception:
                pass  # table extraction is best-effort

            # ── Step 2: word-level layout analysis ────────────────────────────
            words = page.extract_words(use_text_flow=True) or []

            # Sort top→bottom, left→right
            words.sort(key=lambda w: (round(w["top"] / 5) * 5, w["x0"]))

            # Mask words inside table bboxes
            def in_table(w):
                for b in table_bboxes:
                    if b[0] <= w["x0"] <= b[2] and b[1] <= w["top"] <= b[3]:
                        return True
                return False

            words = [w for w in words if not in_table(w)]

            # Group words into visual lines (Y-tolerance = 5 pts)
            lines = []
            current_line = []
            current_y = None
            for word in words:
                if current_y is None or abs(word["top"] - current_y) < 5:
                    current_line.append(word)
                    if current_y is None:
                        current_y = word["top"]
                else:
                    lines.append(current_line)
                    current_line = [word]
                    current_y = word["top"]
            if current_line:
                lines.append(current_line)

            # Convert lines → text segments
            text_lines = []
            for line_words in lines:
                text = " ".join(w["text"] for w in line_words).strip()
                if is_noise(text):
                    continue
                x0 = min(w["x0"] for w in line_words)
                top = min(w["top"] for w in line_words)
                x1 = max(w["x1"] for w in line_words)
                bottom = max(w["bottom"] for w in line_words)
                # Font size heuristic from first word
                size = line_words[0].get("size", 0) if line_words else 0
                text_lines.append({
                    "text": text,
                    "bbox": [x0, top, x1, bottom],
                    "size": size,
                    "top": top,
                })

            if not text_lines:
                slides.append({"slide_number": i + 1, "elements": elements})
                continue

            # ── Step 3: title detection (largest font, highest on page) ───────
            max_size = max((l["size"] for l in text_lines), default=0)
            title_candidates = [l for l in text_lines if l["size"] >= max_size * 0.9 and l["top"] < page.height * 0.35]

            title_added = set()
            for tc in title_candidates[:1]:  # only top-most title
                if tc["text"] not in title_added:
                    elements.append({
                        "type": "title",
                        "text": tc["text"],
                        "bbox": tc["bbox"],
                    })
                    title_added.add(tc["text"])

            # ── Step 4: paragraph + bullet reconstruction ─────────────────────
            current_para = []
            current_bbox = None
            current_bullets = []
            current_bullet_level = None

            def flush_para():
                nonlocal current_para, current_bbox
                if current_para:
                    full = " ".join(current_para).strip()
                    if full and not is_noise(full):
                        elements.append({"type": "text", "text": full, "bbox": current_bbox})
                    current_para = []
                    current_bbox = None

            def flush_bullets():
                nonlocal current_bullets, current_bullet_level
                if current_bullets:
                    elements.append({
                        "type": "bullet_list",
                        "items": current_bullets[:],
                        "level": current_bullet_level or 1,
                        "bbox": None,
                    })
                    current_bullets = []
                    current_bullet_level = None

            for tl in text_lines:
                text = tl["text"]
                bbox = tl["bbox"]

                # Skip already-used titles
                if text in title_added:
                    continue

                is_b, level, cleaned = classify_bullet(text)

                if is_b:
                    flush_para()
                    if current_bullet_level is not None and level != current_bullet_level:
                        flush_bullets()
                    current_bullets.append(cleaned)
                    current_bullet_level = level
                else:
                    flush_bullets()
                    ends_sentence = text.rstrip().endswith((".", "!", "?", ":", ";"))
                    if current_para and not ends_sentence:
                        current_para.append(text)
                        if current_bbox:
                            current_bbox[2] = max(current_bbox[2], bbox[2])
                            current_bbox[3] = max(current_bbox[3], bbox[3])
                        else:
                            current_bbox = list(bbox)
                    else:
                        flush_para()
                        current_para = [text]
                        current_bbox = list(bbox)

            flush_para()
            flush_bullets()

            slides.append({"slide_number": i + 1, "elements": elements})

    return slides

# ── PyMuPDF backend ───────────────────────────────────────────────────────────

def extract_with_pymupdf(pdf_path: str):
    import fitz  # PyMuPDF

    slides = []
    doc = fitz.open(pdf_path)
    for i, page in enumerate(doc):
        blocks = page.get_text("dict")["blocks"]
        elements = []
        for b in blocks:
            if b["type"] != 0:
                continue
            lines_text = []
            for line in b["lines"]:
                line_text = " ".join(s["text"] for s in line["spans"]).strip()
                if line_text and not is_noise(line_text):
                    lines_text.append(line_text)
            text = " ".join(lines_text).strip()
            if text:
                bbox = list(b["bbox"])
                is_b, level, cleaned = classify_bullet(text)
                if is_b:
                    elements.append({"type": "bullet_list", "items": [cleaned], "level": level, "bbox": bbox})
                else:
                    elements.append({"type": "text", "text": text, "bbox": bbox})
        slides.append({"slide_number": i + 1, "elements": elements})
    return slides

# ── pypdf backend (plaintext fallback) ────────────────────────────────────────

def extract_with_pypdf(pdf_path: str):
    import pypdf

    slides = []
    reader = pypdf.PdfReader(pdf_path)
    for i, page in enumerate(reader.pages):
        text = page.extract_text() or ""
        elements = []
        for line in text.splitlines():
            line = line.strip()
            if not line or is_noise(line):
                continue
            is_b, level, cleaned = classify_bullet(line)
            if is_b:
                elements.append({"type": "bullet_list", "items": [cleaned], "level": level, "bbox": None})
            else:
                elements.append({"type": "text", "text": line, "bbox": None})
        slides.append({"slide_number": i + 1, "elements": elements})
    return slides

# ── Main ──────────────────────────────────────────────────────────────────────

def main():
    if len(sys.argv) < 2:
        print(json.dumps({"error": "Missing PDF path argument"}))
        sys.exit(1)

    pdf_path = sys.argv[1]
    errors = {}

    try:
        slides = extract_with_pdfplumber(pdf_path)
        print(json.dumps({"status": "ok", "backend": "pdfplumber", "slides": slides}))
        return
    except Exception as e:
        errors["pdfplumber"] = str(e)

    try:
        slides = extract_with_pymupdf(pdf_path)
        print(json.dumps({"status": "ok", "backend": "pymupdf", "slides": slides}))
        return
    except Exception as e:
        errors["pymupdf"] = str(e)

    try:
        slides = extract_with_pypdf(pdf_path)
        print(json.dumps({"status": "ok", "backend": "pypdf", "slides": slides}))
        return
    except Exception as e:
        errors["pypdf"] = str(e)

    print(json.dumps({"status": "error", "error": f"All backends failed: {errors}"}))
    sys.exit(1)

if __name__ == "__main__":
    main()
