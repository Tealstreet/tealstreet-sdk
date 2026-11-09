#[path = "../build_support.rs"]
mod build_support;

#[test]
fn hosted_delegated_flags_do_not_change_any_local_construction_bytes() {
    let rules: serde_json::Value = serde_json::from_str(include_str!(
        "../../../packages/safe-cex/src/exchanges/rust/construction-rules.json"
    ))
    .unwrap();
    let mut without = rules.clone();
    for rule in without["venues"].as_object_mut().unwrap().values_mut() {
        rule.as_object_mut().unwrap().remove("delegated");
    }
    assert_eq!(
        build_support::generate(&rules.to_string()).unwrap(),
        build_support::generate(&without.to_string()).unwrap()
    );
}

#[test]
fn delegated_declaration_requires_a_boolean_and_rejects_unknown_fields() {
    for delegated in [
        r#"{"enabled":"true"}"#,
        r#"{"enabled":true,"unexpected":true}"#,
    ] {
        let input = serde_json::json!({
            "default": {"canSign": [], "privateEligible": []},
            "venues": {"bybit": {"delegated": serde_json::from_str::<serde_json::Value>(delegated).unwrap()}},
        }).to_string();
        assert!(build_support::generate(&input).is_err());
    }
}
