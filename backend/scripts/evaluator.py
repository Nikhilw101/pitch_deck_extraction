import json
import argparse
from typing import Dict, Any, List, Set

def load_json(filepath: str) -> Dict[str, Any]:
    with open(filepath, 'r', encoding='utf-8') as f:
        return json.load(f)

def extract_flat_fields(data: Dict[str, Any], prefix="") -> Dict[str, str]:
    """Flatten nested JSON into dot-separated keys, lowercased values."""
    fields = {}
    for k, v in data.items():
        key = f"{prefix}.{k}" if prefix else k
        
        # Handle our new wrapped format {"value": ..., "source_text": ...}
        if isinstance(v, dict):
            if "value" in v:
                val = v["value"]
                fields[key.lower()] = str(val).lower().strip()
                continue
            
            # Recursive flatten
            nested = extract_flat_fields(v, prefix=key)
            fields.update(nested)
        elif isinstance(v, list):
            fields[key.lower()] = str(v).lower().strip()
        else:
            fields[key.lower()] = str(v).lower().strip()
            
    return fields

def evaluate_extracted_vs_gold(extracted_path: str, gold_path: str):
    """Compare extracted JSON against a gold standard JSON."""
    try:
        extracted = load_json(extracted_path)
        gold = load_json(gold_path)
    except Exception as e:
        print(f"Error loading files: {e}")
        return

    # Assuming extracted is the full output format, find the deck sections
    sections = extracted.get("sections", [])
    
    # We will flatten both into a single dict for easy comparison
    ext_fields = {}
    for sec in sections:
        sec_name = sec.get("section_name", "Unknown").lower()
        data = sec.get("data", {})
        flat = extract_flat_fields(data, prefix=sec_name)
        ext_fields.update(flat)
        
    gold_fields = extract_flat_fields(gold)
    
    gold_keys = set(gold_fields.keys())
    ext_keys = set(ext_fields.keys())
    
    true_positives = 0
    false_positives = 0
    false_negatives = 0
    
    matched_details = []
    error_details = []
    
    # Check what we found
    for key in ext_keys:
        if key in gold_keys:
            if ext_fields[key] == gold_fields[key] or ext_fields[key] in gold_fields[key] or gold_fields[key] in ext_fields[key]:
                true_positives += 1
                matched_details.append((key, ext_fields[key]))
            else:
                # Same key, wrong value (Counts as FP and FN technically, or just a mismatch)
                false_positives += 1
                error_details.append(f"MISMATCH on '{key}': Expected '{gold_fields[key]}', Got '{ext_fields[key]}'")
        else:
            # We extracted something not in gold standard
            false_positives += 1
            error_details.append(f"HALLUCINATION: Found '{key}'={ext_fields[key]} not in gold standard")
            
    # Check what we missed
    for key in gold_keys:
        if key not in ext_keys:
            false_negatives += 1
            error_details.append(f"MISSING: Failed to extract '{key}'={gold_fields[key]}")

    precision = true_positives / (true_positives + false_positives) if (true_positives + false_positives) > 0 else 0
    recall = true_positives / (true_positives + false_negatives) if (true_positives + false_negatives) > 0 else 0
    f1 = 2 * (precision * recall) / (precision + recall) if (precision + recall) > 0 else 0

    print("="*50)
    print("EVALUATION RESULTS")
    print("="*50)
    print(f"True Positives:  {true_positives}")
    print(f"False Positives: {false_positives}")
    print(f"False Negatives: {false_negatives}")
    print("-" * 50)
    print(f"Precision:       {precision:.2f}")
    print(f"Recall:          {recall:.2f}")
    print(f"F1 Score:        {f1:.2f}")
    print("="*50)
    
    if error_details:
        print("\nERRORS:")
        for err in error_details:
            print(f" - {err}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Evaluate extracted pitch deck data against gold standard.")
    parser.add_argument("extracted", help="Path to the JSON output from the extractor")
    parser.add_argument("gold", help="Path to the gold standard JSON file")
    
    args = parser.parse_args()
    evaluate_extracted_vs_gold(args.extracted, args.gold)
