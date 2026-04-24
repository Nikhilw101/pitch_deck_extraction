use crate::models::extraction_model::Element;

pub fn refine_table(elem: &mut Element) {
    if let Element::Table {
        headers,
        rows,
        bbox: _,
    } = elem
    {
        // 1. Split merged cells in rows
        for row in rows.iter_mut() {
            if row.len() == 1 {
                let text = &row[0];
                // Split by tabs or multiple spaces (>= 2)
                let parts: Vec<String> = text
                    .split('\t')
                    .flat_map(|s| s.split("  "))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                if parts.len() > 1 {
                    *row = parts;
                }
            }
        }

        // 2. Simple heuristic to detect headers if they are empty
        if headers.is_empty() && !rows.is_empty() {
            let first_row = &rows[0];
            let is_header = first_row
                .iter()
                .all(|c| !c.trim().is_empty() && !c.chars().any(|ch| ch.is_numeric()));

            if is_header && first_row.len() > 1 {
                let new_headers = first_row.clone();
                rows.remove(0);
                *headers = new_headers;
            }
        }
    }
}
