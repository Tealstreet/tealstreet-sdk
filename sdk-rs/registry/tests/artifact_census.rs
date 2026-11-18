#[path = "../build_support.rs"]
mod build_support;

const POLICY: &str =
    include_str!("../../../packages/safe-cex/src/exchanges/rust/construction-rules.json");

#[test]
fn complete_production_policy_retains_generated_output() {
    assert!(build_support::generate(POLICY).is_ok());
}

#[test]
fn duplicated_production_venue_cannot_erase_its_spot_policy() {
    let end = POLICY.rfind("\n  }").unwrap();
    for name in ["krakenspot", "kraken\\u0073pot"] {
        let source = format!(
            "{},\n    \"{name}\": {{}}{}",
            &POLICY[..end],
            &POLICY[end..]
        );
        let error = build_support::generate(&source)
            .map(|_| ())
            .expect_err("duplicate venue must refuse");
        assert!(
            error.contains("duplicate") && error.contains("krakenspot"),
            "{error}"
        );
    }
}

#[test]
fn deleted_production_venue_cannot_shrink_the_build_census() {
    let mut source: serde_json::Value = serde_json::from_str(POLICY).unwrap();
    source["venues"]
        .as_object_mut()
        .unwrap()
        .remove("krakenspot");
    let error = build_support::generate(&source.to_string())
        .map(|_| ())
        .expect_err("missing policy must refuse");
    assert!(
        error.contains("missing") && error.contains("krakenspot"),
        "{error}"
    );
}

#[test]
fn unknown_production_venue_cannot_extend_the_build_census() {
    let mut source: serde_json::Value = serde_json::from_str(POLICY).unwrap();
    source["venues"]["unreviewedvenue"] = serde_json::json!({});
    let error = build_support::generate(&source.to_string())
        .map(|_| ())
        .expect_err("unknown policy must refuse");
    assert!(
        error.contains("unexpected") && error.contains("unreviewedvenue"),
        "{error}"
    );
}
