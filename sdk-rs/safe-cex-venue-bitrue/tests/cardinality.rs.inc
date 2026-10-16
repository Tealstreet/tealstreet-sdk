std::thread_local! { static NATIVE_COMPARISONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
// Frozen counts were captured from unchanged actual-source corpora. They are
// source census evidence, not independently generated expected field values.
fn array_cardinalities(value: &serde_json::Value) -> Vec<usize> {
    fn walk(value: &serde_json::Value, counts: &mut Vec<usize>) {
        match value {
            serde_json::Value::Array(items) => {
                counts.push(items.len());
                for item in items { walk(item, counts); }
            }
            serde_json::Value::Object(fields) => {
                let mut keys: Vec<_> = fields.keys().collect();
                keys.sort();
                for key in keys { walk(&fields[key], counts); }
            }
            _ => {}
        }
    }
    let mut counts = Vec::new();
    walk(value, &mut counts);
    counts
}
fn source_cardinality(value: &serde_json::Value, target: &str) {
    NATIVE_COMPARISONS.with(|count| count.set(0));
    let ledger: serde_json::Value = serde_json::from_str(include_str!("cardinality-counts.json")).unwrap();
    let expected: Vec<usize> = serde_json::from_value(ledger[target]["counts"].clone()).unwrap();
    assert_eq!(array_cardinalities(value), expected, "BITRUE-SOURCE-CARDINALITY total/per-case: {target}");
}
fn native_cardinality(expected: &serde_json::Value, actual: &serde_json::Value, target: &str) {
    NATIVE_COMPARISONS.with(|count| count.set(count.get() + 1));
    // Independent of the frozen source ledger. Full native equality follows.
    assert_eq!(array_cardinalities(actual), array_cardinalities(expected), "BITRUE-NATIVE-CARDINALITY expected/native: {target}");
}

fn native_comparisons(expected: usize, target: &str) {
    NATIVE_COMPARISONS.with(|count| assert_eq!(count.get(), expected, "BITRUE-NATIVE-CARDINALITY executed comparisons: {target}"));
}
