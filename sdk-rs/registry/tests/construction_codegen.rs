#[path = "../build_support.rs"]
mod build_support;

#[test]
fn distinguishes_fallback_from_empty_conditions_and_escapes_strings() {
    let output = build_support::generate(
        r#"{"default":{"canSign":[["key"]],"privateEligible":[["canSign"]]},"venues":{
        "z":{"canSign":null,"privateEligible":[],"explicitPrivateError":"quote\"\nline"},
        "a":{"canSign":[[]]}}}"#,
    )
    .unwrap();
    assert!(output.contains("can_sign: Some(&[&[Input::Key]])"));
    assert!(output.contains("can_sign: None, private_eligible: Some(&[])"));
    assert!(output.contains("can_sign: Some(&[&[]]), private_eligible: None"));
    assert!(output.contains(r#"Some("quote\"\nline")"#));
    assert!(output.find("\"a\"").unwrap() < output.find("\"z\"").unwrap());
}

#[test]
fn invalid_or_incomplete_shared_rules_fail_the_build() {
    for json in [
        r#"{"default":{},"venues":{}}"#,
        r#"{"default":{"canSign":null,"privateEligible":[]},"venues":{}}"#,
        r#"{"default":{"canSign":[["unknown"]],"privateEligible":[]},"venues":{}}"#,
        r#"{"default":{"canSign":[],"privateEligible":[],"typo":true},"venues":{}}"#,
    ] {
        assert!(build_support::generate(json).is_err(), "{json}");
    }
}
