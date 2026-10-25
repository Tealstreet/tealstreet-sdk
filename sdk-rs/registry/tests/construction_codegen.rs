#[path = "../build_support.rs"]
mod build_support;

#[test]
fn two_families_declare_owned_presentation_without_enabling_runtime_admission() {
    for (name, header) in [
        ("okx", "OK-ACCESS-PASSPHRASE"),
        ("bitget", "ACCESS-PASSPHRASE"),
    ] {
        let json = format!(
            r#"{{"default":{{"canSign":[],"privateEligible":[]}},"venues":{{"{name}":{{"remoteSigning":{{"signatureEncoding":"base64","passphraseHeader":"{header}"}}}}}}}}"#
        );
        let output = build_support::generate(&json).unwrap();
        assert!(output.contains("enabled: false"));
        assert!(output.contains("signature_encoding: SignatureEncoding::Base64"));
        assert!(output.contains(&format!("passphrase_header: Some(\"{header}\")")));
    }
    for declaration in [
        r#"{"signatureEncoding":"hex-base64"}"#,
        r#"{"signatureEncoding":true}"#,
        r#"{"passphraseHeader":""}"#,
        r#"{"passphraseHeader":"Bad Name"}"#,
        r#"{"passphraseHeader":"X-Injected\r\n"}"#,
        r#"{"passphraseHeader":false}"#,
    ] {
        let json = format!(
            r#"{{"default":{{"canSign":[],"privateEligible":[]}},"venues":{{"v":{{"remoteSigning":{declaration}}}}}}}"#
        );
        assert!(build_support::generate(&json).is_err(), "{declaration}");
    }
    let long = "x".repeat(257);
    assert!(build_support::generate(&format!(r#"{{"default":{{"canSign":[],"privateEligible":[]}},"venues":{{"v":{{"remoteSigning":{{"passphraseHeader":"{long}"}}}}}}}}"#)).is_err());
}

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
            "RemoteSigningPolicy { enabled: false, payload_supported: false, payload_rest_only: true, protected_headers: Some(&[]), signature_encoding: SignatureEncoding::Hex, passphrase_header: None }"
        )
    );
    assert!(output.contains(
        "RemoteSigningPolicy { enabled: false, payload_supported: false, payload_rest_only: true, protected_headers: Some(&[\"X-Fixed\"]), signature_encoding: SignatureEncoding::Hex, passphrase_header: None }"
    ));
    assert!(
        output
            .contains("RemoteSigningPolicy { enabled: false, payload_supported: false, payload_rest_only: false, protected_headers: None, signature_encoding: SignatureEncoding::Hex, passphrase_header: None }")
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
    assert!(generated.contains("RemoteSigningPolicy { enabled: true, payload_supported: true, payload_rest_only: true, protected_headers: Some(&[]), signature_encoding: SignatureEncoding::Hex, passphrase_header: None }"));
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

#[test]
fn seconds_timestamp_wire_requires_exactly_one_declared_method_path_suffix() {
    let rules = |wire: &str| {
        format!(
            r#"{{"default":{{"canSign":[],"privateEligible":[]}},"venues":{{"v":{{"remoteSigning":{{"wire":{wire}}}}}}}}}"#
        )
    };
    for family in ["/users/self/verify", "/user/verify"] {
        build_support::generate(&rules(&format!(
            r#"{{"exchangeName":"v","extraData":"secondsTimestamp","secondsSuffix":{{"method":"GET","path":"{family}"}}}}"#
        )))
        .unwrap();
    }
    for wire in [
        r#"{"exchangeName":"v","extraData":"secondsTimestamp"}"#,
        r#"{"exchangeName":"v","secondsSuffix":{"method":"GET","path":"/user/verify"}}"#,
        r#"{"exchangeName":"v","extraData":"bybitExpiry","secondsSuffix":{"method":"GET","path":"/user/verify"}}"#,
        r#"{"exchangeName":"v","extraData":"secondsTimestamp","secondsSuffix":{"method":"get","path":"/user/verify"}}"#,
        r#"{"exchangeName":"v","extraData":"secondsTimestamp","secondsSuffix":{"method":"","path":"/user/verify"}}"#,
        r#"{"exchangeName":"v","extraData":"secondsTimestamp","secondsSuffix":{"method":"GET","path":"user/verify"}}"#,
        r#"{"exchangeName":"v","extraData":"secondsTimestamp","secondsSuffix":{"method":"GET","path":"/user verify"}}"#,
        r#"{"exchangeName":"v","extraData":"secondsTimestamp","secondsSuffix":{"method":"GET","path":"/user/verify\r\n"}}"#,
        r#"{"exchangeName":"v","extraData":"secondsTimestamp","secondsSuffix":{"method":"GET","path":"/user/verify","typo":1}}"#,
        r#"{"exchangeName":"v","extraData":"okxTimestamp"}"#,
    ] {
        assert!(build_support::generate(&rules(wire)).is_err(), "{wire}");
    }
}

#[test]
fn hosted_material_policy_is_strict_and_does_not_change_native_output() {
    let declaration = r#"{"privateFields":["key","secret","passphrase","starkPrivateKey","polymarket.apiSecret","polymarket.apiPassphrase"],"keyPublicWithPayloadSigner":true}"#;
    let table = |policy: &str| {
        format!(
            r#"{{"default":{{"canSign":[],"privateEligible":[]}},"venues":{{"test":{{{policy}}}}}}}"#
        )
    };
    let ordinary = build_support::generate(&table("")).unwrap();
    let declared =
        build_support::generate(&table(&format!(r#""hostedMaterial":{declaration}"#))).unwrap();
    assert_eq!(declared, ordinary);
    let raw = include_str!("../../../packages/safe-cex/src/exchanges/rust/construction-rules.json");
    let mut plain: serde_json::Value = serde_json::from_str(raw).unwrap();
    for row in plain["venues"].as_object_mut().unwrap().values_mut() {
        row.as_object_mut().unwrap().remove("hostedMaterial");
    }
    assert_eq!(
        build_support::generate(raw).unwrap(),
        build_support::generate(&plain.to_string()).unwrap()
    );
    for policy in [
        r#"{"privateFields":["secrett"]}"#,
        r#"{"privateFields":[]}"#,
        r#"{"privateFields":["secret"],"keyPublicWithPayloadSigner":true}"#,
        r#"{"privateFields":["key"],"unexpected":true}"#,
    ] {
        assert!(build_support::generate(&table(&format!(r#""hostedMaterial":{policy}"#))).is_err());
    }
}
