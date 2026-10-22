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

#[test]
fn delegated_policy_is_declared_once_and_invalid_headers_fail_the_build() {
    let output = build_support::generate(
        r#"{"default":{"canSign":[],"privateEligible":[]},"venues":{
        "v":{"remoteSigning":{"payloadRestOnly":true,"protectedHeaders":[]}},
        "protected":{"remoteSigning":{"payloadRestOnly":true,"protectedHeaders":["X-Fixed"]}}
    }}"#,
    )
    .unwrap();
    assert!(
        output.contains(
            "RemoteSigningPolicy { enabled: false, payload_supported: false, payload_rest_only: true, protected_headers: Some(&[]) }"
        )
    );
    assert!(output.contains(
        "RemoteSigningPolicy { enabled: false, payload_supported: false, payload_rest_only: true, protected_headers: Some(&[\"X-Fixed\"]) }"
    ));
    assert!(
        output
            .contains("RemoteSigningPolicy { enabled: false, payload_supported: false, payload_rest_only: false, protected_headers: None }")
    );
    for declaration in [
        r#"{"payloadRestOnly":"true"}"#,
        r#"{"protectedHeader":["X-Fixed"]}"#,
        r#"{"protectedHeaders":["Bad Name"]}"#,
        r#"{"protectedHeaders":["X-Injected\r\n"]}"#,
        r#"{"protectedHeaders":[""]}"#,
    ] {
        let json = format!(
            r#"{{"default":{{"canSign":[],"privateEligible":[]}},"venues":{{"v":{{"remoteSigning":{declaration}}}}}}}"#
        );
        assert!(build_support::generate(&json).is_err());
    }
}

#[test]
fn all_nine_aliases_share_rest_only_payload_policy_with_declared_precedence() {
    use safe_cex_engine::signing_headers::ResultHeaderPolicy;
    use safe_cex_engine::signing_policy::PrivateFeedPolicy;
    for name in [
        "binance",
        "binancepm",
        "binanceinverse",
        "binancespot",
        "bybit",
        "bybitinverse",
        "bybitspot",
        "bybitdemo",
        "hyrotrader",
    ] {
        let policy = safe_cex_venues::remote_signing_policy(name);
        assert!(policy.enabled, "{name}");
        assert_eq!(policy.payload_supported, name != "hyrotrader", "{name}");
        assert_eq!(
            policy.private_feed(true),
            PrivateFeedPolicy::RestOnly,
            "{name}"
        );
        assert_eq!(
            policy.private_feed(false),
            PrivateFeedPolicy::Streaming,
            "{name}"
        );
        let expected: &[&str] = if name.starts_with("binance") {
            &[]
        } else {
            &["X-BAPI-SIGN", "X-BAPI-API-KEY", "X-BAPI-TIMESTAMP"]
        };
        assert_eq!(
            policy.header_policy(),
            ResultHeaderPolicy::Http {
                protected: expected
            },
            "{name}"
        );
    }
    assert_eq!(
        safe_cex_venues::remote_signing_policy("unmigrated").header_policy(),
        ResultHeaderPolicy::Deny
    );
}

#[test]
fn runtime_admission_requires_dispatch_and_complete_shared_policy() {
    let generate = |bridge: &str, declaration: &str| {
        build_support::generate(&format!(
            r#"{{"default":{{"canSign":[],"privateEligible":[]}},"venues":{{"v":{{"bridge":{bridge},"remoteSigning":{declaration}}}}}}}"#
        ))
    };
    let valid =
        r#"{"enabled":true,"payloadSupported":true,"payloadRestOnly":true,"protectedHeaders":[]}"#;
    let generated = generate(r#"{"dispatchSigning":true}"#, valid).unwrap();
    assert!(generated.contains("RemoteSigningPolicy { enabled: true, payload_supported: true, payload_rest_only: true, protected_headers: Some(&[]) }"));
    assert!(generate("{}", valid).is_err());
    for incomplete in [
        r#"{"enabled":true,"payloadSupported":true,"payloadRestOnly":true}"#,
        r#"{"enabled":true,"payloadSupported":true,"protectedHeaders":[]}"#,
        r#"{"enabled":"true","protectedHeaders":[]}"#,
        r#"{"payloadSupported":"true"}"#,
    ] {
        assert!(generate(r#"{"dispatchSigning":true}"#, incomplete).is_err());
    }
}

#[test]
fn remote_wire_policy_rejects_unknown_codecs_labels_and_unbound_fixed_ids() {
    for wire in [
        r#"{"exchangeName":"bybit","extraData":"typo"}"#,
        r#"{"exchangeName":"bybit","requestId":"fixed"}"#,
        r#"{"exchangeName":"Bybit"}"#,
        r#"{"exchangeName":"bybit","typo":true}"#,
        r#"{"exchangeName":"binancespot","extraData":"binanceSpot","requestId":""}"#,
    ] {
        let json = format!(
            r#"{{"default":{{"canSign":[],"privateEligible":[]}},"venues":{{"v":{{"remoteSigning":{{"wire":{wire}}}}}}}}}"#
        );
        assert!(build_support::generate(&json).is_err(), "{wire}");
    }
    build_support::generate(include_str!(
        "../../../packages/safe-cex/src/exchanges/rust/construction-rules.json"
    ))
    .unwrap();
}
