//! Makes the generated tool input schemas portable across MCP harnesses.
//!
//! schemars emits draft 2020-12 with `"$schema"`, `"format": "uint32"` and
//! nullable types as `"type": ["string", "null"]`. Claude accepts all of
//! that, but Gemini's function declarations and OpenAI's strict tool mode
//! reject type arrays and unknown `format`s, and some clients choke on a
//! `$schema` key. Every tool is already optional-by-omission (the field is
//! simply left out of `required`), so dropping the `null` alternative loses
//! nothing.

use serde_json::Value;

/// Rewrites `schema` in place into the common subset every harness reads.
pub fn portable(schema: &mut Value) {
    let Value::Object(map) = schema else {
        if let Value::Array(items) = schema {
            for v in items {
                portable(v);
            }
        }
        return;
    };
    map.remove("$schema");
    map.remove("format");
    if let Some(Value::Array(types)) = map.get("type") {
        let kept: Vec<Value> = types
            .iter()
            .filter(|t| t.as_str() != Some("null"))
            .cloned()
            .collect();
        let replacement = match kept.len() {
            0 => Value::String("string".into()),
            1 => kept.into_iter().next().expect("one element"),
            _ => Value::Array(kept),
        };
        map.insert("type".into(), replacement);
    }
    for (_, v) in map.iter_mut() {
        portable(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strips_nullable_types_formats_and_schema_key() {
        let mut v = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "limit": {"type": ["integer", "null"], "format": "uint32", "minimum": 0},
                "symbol": {"type": "string"},
                "sources": {"type": ["array", "null"], "items": {"type": "string"}},
                "nested": {"type": "object", "properties": {"x": {"type": ["number", "null"], "format": "double"}}}
            },
            "required": ["symbol"]
        });
        portable(&mut v);
        assert_eq!(
            v,
            json!({
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 0},
                    "symbol": {"type": "string"},
                    "sources": {"type": "array", "items": {"type": "string"}},
                    "nested": {"type": "object", "properties": {"x": {"type": "number"}}}
                },
                "required": ["symbol"]
            })
        );
    }
}
