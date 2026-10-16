//! Finding witnesses describe current behavior; they are not parity-pass claims.
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_venue_cryptocom::signing_value::SigningValue;
use safe_cex_venue_cryptocom::{
    api::is_public_endpoint,
    signing::{
        generate_authenticated_request, generate_signature, generate_ws_auth_message,
        params_to_string,
    },
};
use serde_json::{Value, json};
use std::{
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
#[test]
fn native_values_precede_source_divergence_observations() {
    // All decoded native assertions precede starting the source/wire probe.
    assert!(
        !is_public_endpoint("/audit/custom"),
        "AUDIT-NATIVE-INITIAL-PUBLIC"
    );
    assert!(
        is_public_endpoint("/public/get-book"),
        "AUDIT-NATIVE-PUBLIC-BOOK"
    );
    let invalid = serde_json::from_str::<Value>(r#"{"\ud800":1,"\ue000":2}"#);
    assert!(invalid.is_err(), "AUDIT-NATIVE-WHOLE-PARAM-REFUSAL");
    let params = json!({"\u{fffd}":1,"\u{e000}":2});
    assert_eq!(
        params_to_string(Some(&params), None).unwrap(),
        "\u{e000}2\u{fffd}1",
        "AUDIT-NATIVE-KEY-SORT"
    );
    let signature = generate_signature(
        "private/create-order",
        17.0,
        "api",
        "secret",
        &params,
        1700123456789.0,
    )
    .unwrap();
    assert_eq!(
        signature, "5abd9e4ce8dc59b69e8b4dc90f69fdfdb3b5e8baab11602b17d4ea254306899f",
        "AUDIT-NATIVE-HMAC"
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let mut clock = VirtualClock::default();
    clock.now = {
        let calls = calls.clone();
        Arc::new(move || {
            calls.fetch_add(1, Ordering::SeqCst);
            1000.0
        })
    };
    let identity_params = SigningValue::from_value(params.clone());
    let result = generate_authenticated_request(
        "\u{fffd}",
        identity_params.clone(),
        "\u{fffd}",
        "secret",
        Some(97.0),
        &clock,
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "AUDIT-NATIVE-EAGER-REQUEST"
    );
    let request = result.unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "AUDIT-NATIVE-REQUEST-CLOCK"
    );
    assert!(
        request.params.same_reference(&identity_params),
        "AUDIT-NATIVE-RETAINED-IDENTITY"
    );
    assert_eq!(request.method, "\u{fffd}");
    assert_eq!(request.api_key, "\u{fffd}");
    assert_eq!(request.id.to_bits(), 1000.0f64.to_bits());
    assert_eq!(request.nonce.to_bits(), 97.0f64.to_bits());
    let result = generate_ws_auth_message("\u{fffd}", "secret", &clock);
    assert_eq!(calls.load(Ordering::SeqCst), 3, "AUDIT-NATIVE-EAGER-WS");
    let message = result.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3, "AUDIT-NATIVE-WS-CLOCK");
    assert_eq!(message.api_key, "\u{fffd}");
    assert_eq!(message.method, "public/auth");
    // Source modules observe the current, distinct behavior before serializing.
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/divergence_audit_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "AUDIT-SOURCE-EXEC:{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["header"]["placements"], 16);
    assert_eq!(source["header"]["booleanObservations"], 64);
    assert_eq!(
        source["mutable"],
        json!({"customBefore":false,"customAfter":true,"bookBefore":true,"bookAfter":false})
    );
    assert_eq!(
        source["originalDigest"],
        "54a13805c342de266ffb2de9573517bca55450a208af3734dc6e8dd140e41826"
    );
    assert_eq!(source["replacementDigest"], signature);
    for field in ["requestMethodUnits", "requestKeyUnits", "wsKeyUnits"] {
        assert_eq!(source[field], json!([55296]));
    }
    assert_eq!(source["clockCalls"], 3);
    assert_eq!(source["paramsAlias"], true);
    assert_eq!(source["mutatedCounter"], 7);
}
