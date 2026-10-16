//! Helpers for walking retained JSON response envelopes, before REST mapping.
use serde_json::Value;
use std::collections::HashSet;

/// Stock JSON objects are records; null, arrays and primitives are not.
pub fn is_record(value: Option<&Value>) -> bool {
    value.is_some_and(Value::is_object)
}

/// Source preorder, named envelope edges only, with a maximum accepted depth of 4.
/// Values are borrowed so callers keep numeric ingress and object identity intact.
pub fn get_response_candidates(payload: Option<&Value>) -> Vec<&Value> {
    fn visit<'a>(
        value: Option<&'a Value>,
        depth: usize,
        candidates: &mut Vec<&'a Value>,
        seen: &mut HashSet<*const Value>,
    ) {
        let Some(value) = value else { return };
        if value.is_null() || depth > 4 {
            return;
        }
        if (value.is_object() || value.is_array()) && !seen.insert(std::ptr::from_ref(value)) {
            return;
        }
        candidates.push(value);
        if !is_record(Some(value)) {
            return;
        }
        for field in [
            "data",
            "result",
            "payload",
            "positionList",
            "positions",
            "list",
            "rows",
        ] {
            visit(value.get(field), depth + 1, candidates, seen);
        }
    }
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    visit(payload, 0, &mut candidates, &mut seen);
    candidates
}
