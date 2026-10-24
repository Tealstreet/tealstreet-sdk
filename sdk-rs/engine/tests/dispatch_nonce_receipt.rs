use safe_cex_engine::signing::SignReceipt;

#[test]
fn canonical_decimal_nonce_round_trips_without_a_float() {
    for value in [0, 1, 9_007_199_254_740_993, u64::MAX] {
        let wire = format!(r#"{{"nonce":"{value}"}}"#);
        let receipt: SignReceipt = serde_json::from_str(&wire).unwrap();
        assert_eq!(receipt.nonce, Some(value));
        assert_eq!(receipt.stamp_ms, None);
        assert_eq!(serde_json::to_string(&receipt).unwrap(), wire);
    }
}
#[test]
fn noncanonical_and_nonstring_nonce_values_are_refused() {
    for value in [
        r#""""#,
        r#""00""#,
        r#""01""#,
        r#""+1""#,
        r#""-1""#,
        r#"" 1""#,
        r#""1 ""#,
        r#""1.0""#,
        r#""1e3""#,
        r#""١""#,
        r#""18446744073709551616""#,
        "1",
        "null",
        "true",
        "[]",
        "{}",
    ] {
        assert!(
            serde_json::from_str::<SignReceipt>(&format!("{{\"nonce\":{value}}}")).is_err(),
            "{value}"
        );
    }
}
#[test]
fn nonce_is_independent_of_safe_clock_receipts() {
    let receipt = SignReceipt {
        nonce: Some(u64::MAX),
        stamp_ms: Some(1000.),
        expiry_ms: Some(2000.),
    };
    assert!(receipt.validate().is_ok());
    let wire = serde_json::to_string(&receipt).unwrap();
    assert_eq!(serde_json::from_str::<SignReceipt>(&wire).unwrap(), receipt);
    assert!(
        SignReceipt {
            stamp_ms: Some(1000.5),
            ..receipt
        }
        .validate()
        .is_err()
    );
}
#[test]
fn omitted_nonce_keeps_the_original_wire_record() {
    let receipt: SignReceipt = serde_json::from_str(r#"{"stampMs":1000.0}"#).unwrap();
    assert_eq!(receipt.nonce, None);
    assert_eq!(
        serde_json::to_string(&receipt).unwrap(),
        r#"{"stampMs":1000.0}"#
    );
}
