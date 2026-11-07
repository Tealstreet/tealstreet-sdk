use safe_cex_types::retained_json::RetainedJsonInput;

fn assert_native_survivor(discarded: &str) {
    let raw = format!(r#"{{"z":{discarded},"a":17,"z":"kept"}}"#);
    let native: RetainedJsonInput = serde_json::from_str(&raw)
        .expect("DISCARDED_DUPLICATE_CAPTURE native winner must decode before discarded values");
    let RetainedJsonInput::Object(fields) = native else {
        panic!("DISCARDED_DUPLICATE_CAPTURE native result must remain an object");
    };
    assert_eq!(fields.len(), 2, "independent surviving member count");
    assert_eq!(fields[0].0, [122], "replacement retains first key position");
    let RetainedJsonInput::Text(text) = &fields[0].1 else {
        panic!("DISCARDED_DUPLICATE_CAPTURE native winner must remain text");
    };
    assert_eq!(
        text,
        &[107, 101, 112, 116],
        "native selected text before any wire output"
    );
    assert_eq!(fields[1].0, [97]);
    let RetainedJsonInput::Lexeme(number) = &fields[1].1 else {
        panic!("unrelated native numeric lexeme must remain intact");
    };
    assert_eq!(number, "17");
}

#[test]
fn discarded_duplicate_capture_selects_survivor_before_native_decode() {
    assert_native_survivor(r#""\ud800""#);
}

#[test]
fn discarded_duplicate_capture_ignores_discarded_depth() {
    let deep = format!("{}0{}", "[".repeat(140), "]".repeat(140));
    assert_native_survivor(&deep);
}

#[test]
fn discarded_duplicate_capture_still_refuses_invalid_json_syntax() {
    for raw in [r#"{"z":invalid,"z":"kept"}"#, r#"{"z":[1,],"z":"kept"}"#] {
        assert!(serde_json::from_str::<RetainedJsonInput>(raw).is_err());
    }
}

#[test]
fn discarded_duplicate_capture_keeps_surviving_value_refusals() {
    let deep = format!("{}0{}", "[".repeat(140), "]".repeat(140));
    let raw = format!(r#"{{"z":"discarded","z":{deep}}}"#);
    assert!(serde_json::from_str::<RetainedJsonInput>(&raw).is_err());
}
