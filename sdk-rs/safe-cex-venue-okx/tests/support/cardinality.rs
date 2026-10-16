use serde_json::Value;
use std::collections::BTreeMap;

pub fn assert_groups(
    cases: &[Value],
    name: &str,
    classify: impl Fn(&Value) -> String,
    expected: &[(&str, usize)],
) {
    let mut actual = BTreeMap::new();
    for case in cases {
        *actual.entry(classify(case)).or_insert(0) += 1;
    }
    let expected: BTreeMap<_, _> = expected
        .iter()
        .map(|(key, count)| ((*key).to_owned(), *count))
        .collect();
    assert_eq!(actual, expected, "SUBCORPUS-CARDINALITY {name}");
}
