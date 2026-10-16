// Census is captured from the actual TS corpus, independently of native output.
// It pins source totals, per-case array lengths, and identities; no paired zip.
fn source_census(value: &serde_json::Value) -> serde_json::Value {
    fn walk(v: &serde_json::Value, path: String, counts: &mut serde_json::Map<String, serde_json::Value>) {
        match v {
            serde_json::Value::Array(a) => {
                counts.insert(path.clone(), serde_json::json!(a.len()));
                for (index, item) in a.iter().enumerate() {
                    walk(item, format!("{path}/{index}"), counts);
                }
            }
            serde_json::Value::Object(o) => {
                for (key, item) in o { walk(item, format!("{path}/{key}"), counts); }
            }
            _ => {}
        }
    }
    let mut counts = serde_json::Map::new();
    walk(value, String::new(), &mut counts);
    let mut identities = serde_json::Map::new();
    for (key, collection) in value.as_object().unwrap() {
        if let Some(items) = collection.as_array() {
            identities.insert(key.clone(), serde_json::Value::Array(items.iter().map(|item| {
                let mut id = serde_json::Map::new();
                for field in ["name", "method", "scenario", "key", "field", "profile", "arg"] {
                    if let Some(v) = item.get(field) { id.insert(field.into(), v.clone()); }
                }
                if let Some(expected) = item.get("expected").and_then(serde_json::Value::as_object) {
                    let mut keys = expected.keys().collect::<Vec<_>>();
                    keys.sort();
                    id.insert("expectedKeys".into(), serde_json::json!(keys));
                }
                serde_json::Value::Object(id)
            }).collect()));
        }
    }
    serde_json::json!({"counts":counts, "identities":identities})
}
fn assert_source_census(value: &serde_json::Value, pinned: &str) {
    let expected: serde_json::Value = serde_json::from_str(pinned).unwrap();
    assert_eq!(source_census(value), expected, "SOURCE-CARDINALITY total and per-case counts and identities");
}
// This walks EXPECTED arrays and indexes native only AFTER independent length
// equality. It does not consult the pinned source census or zip two iterators.
fn assert_native_cardinality(native: &serde_json::Value, expected: &serde_json::Value, path: &str) {
    match expected {
        serde_json::Value::Array(items) => {
            let actual = native.as_array().unwrap_or_else(|| panic!("NATIVE-CARDINALITY expected array at {path}"));
            assert_eq!(actual.len(), items.len(), "NATIVE-CARDINALITY expected/native length at {path}");
            for (index, value) in items.iter().enumerate() {
                assert_native_cardinality(&actual[index], value, &format!("{path}/{index}"));
            }
        }
        serde_json::Value::Object(o) => {
            for (key, value) in o { assert_native_cardinality(&native[key], value, &format!("{path}/{key}")); }
        }
        _ => {}
    }
}

#[test]
fn source_cardinality_rejects_empty_subset_and_subcorpus() {
    let mut rejected = 0;
    for kind in ["empty", "subset", "subcorpus"] {
        let mut corrupted = ORACLE.clone();
        match kind {
            "empty" => corrupted["cases"].as_array_mut().unwrap().clear(),
            "subset" => { corrupted["cases"].as_array_mut().unwrap().pop(); },
            "subcorpus" => {
                if corrupted["cases"][0].get("expected").is_some() {
                    corrupted["cases"][0]["expected"] = serde_json::json!({});
                } else {
                    corrupted["aliases"].as_array_mut().unwrap().clear();
                }
            }
            _ => unreachable!(),
        }
        let failure = std::panic::catch_unwind(|| assert_source_census(&corrupted, SOURCE_CENSUS))
            .expect_err("SOURCE-CONTROL corrupted corpus must fail");
        let message = failure.downcast_ref::<String>().map(String::as_str)
            .or_else(|| failure.downcast_ref::<&str>().copied()).unwrap_or("");
        assert!(message.contains("SOURCE-CARDINALITY"), "SOURCE-CONTROL must fail named assertion");
        rejected += 1;
    }
    assert_eq!(rejected, 3, "SOURCE-CONTROL executed comparison count");
}
