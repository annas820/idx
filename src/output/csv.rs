//! `-o csv`: the main list inside any payload as comma-separated rows.
//!
//! Rules, applied to the JSON value of `data`:
//! 1. An array of objects is the table.
//! 2. An object whose fields include arrays of objects uses the largest such
//!    array (candles, rows, items, holders, changes, periods, dividends, ...).
//! 3. Anything else becomes one row.
//!
//! Nested objects flatten to dotted columns (`valuation.pe_trailing`),
//! arrays of scalars join with `|`, arrays of objects inside a row are
//! written as JSON text. Columns keep first-seen order. Quoting follows
//! RFC 4180; numbers and booleans print as JSON does; `null` is empty.

use serde_json::{Map, Value};

fn flatten(prefix: &str, v: &Value, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&key, v, out);
            }
        }
        Value::Array(a) if a.iter().all(|x| !x.is_object() && !x.is_array()) => {
            let joined = a
                .iter()
                .map(scalar)
                .collect::<Vec<_>>()
                .join("|");
            out.push((prefix.to_string(), joined));
        }
        Value::Array(_) => out.push((prefix.to_string(), v.to_string())),
        other => out.push((prefix.to_string(), scalar(other))),
    }
}

fn scalar(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn quote(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Picks the rows per the module rules.
fn rows_of(data: &Value) -> Vec<&Value> {
    match data {
        Value::Array(a) if a.iter().any(Value::is_object) => {
            a.iter().filter(|x| x.is_object()).collect()
        }
        Value::Object(m) => {
            let best = m
                .values()
                .filter_map(|v| v.as_array())
                .filter(|a| !a.is_empty() && a.iter().all(Value::is_object))
                .max_by_key(|a| a.len());
            match best {
                Some(a) => a.iter().collect(),
                None => vec![data],
            }
        }
        other => vec![other],
    }
}

/// Renders `data` as CSV text ending in a newline. An empty list yields an
/// empty string.
pub fn render(data: &Value) -> String {
    let rows = rows_of(data);
    let mut columns: Vec<String> = Vec::new();
    let mut flat: Vec<Map<String, Value>> = Vec::with_capacity(rows.len());
    for r in rows {
        let mut pairs = Vec::new();
        flatten("", r, &mut pairs);
        let mut m = Map::new();
        for (k, v) in pairs {
            if !columns.contains(&k) {
                columns.push(k.clone());
            }
            m.insert(k, Value::String(v));
        }
        flat.push(m);
    }
    if columns.is_empty() {
        return String::new();
    }
    let mut out = columns
        .iter()
        .map(|c| quote(c))
        .collect::<Vec<_>>()
        .join(",");
    out.push('\n');
    for m in &flat {
        let line = columns
            .iter()
            .map(|c| quote(m.get(c).and_then(Value::as_str).unwrap_or("")))
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn array_of_objects_with_nesting() {
        let v = json!([
            {"symbol": "BBCA", "price": 5175.0, "valuation": {"pe": 12.9, "pb": null}, "tags": ["a", "b"]},
            {"symbol": "TLKM", "price": 1900, "valuation": {"pe": 8.1, "pb": 1.2}, "note": "x, \"y\""}
        ]);
        assert_eq!(
            render(&v),
            "symbol,price,valuation.pe,valuation.pb,tags,note\n\
             BBCA,5175.0,12.9,,a|b,\n\
             TLKM,1900,8.1,1.2,,\"x, \"\"y\"\"\"\n"
        );
    }

    #[test]
    fn object_picks_its_largest_list() {
        let v = json!({
            "symbol": "BBCA", "summary": {"candles": 2},
            "candles": [{"time": "t1", "close": 1.0}, {"time": "t2", "close": 2.0}],
            "splits": [{"date": "d"}]
        });
        assert_eq!(render(&v), "time,close\nt1,1.0\nt2,2.0\n");
        let scalar_only = json!({"symbol": "BBCA", "verdict": {"label": "fair", "reasons": ["r1", "r2"]}});
        assert_eq!(
            render(&scalar_only),
            "symbol,verdict.label,verdict.reasons\nBBCA,fair,r1|r2\n"
        );
        assert_eq!(render(&json!([])), "");
    }
}
