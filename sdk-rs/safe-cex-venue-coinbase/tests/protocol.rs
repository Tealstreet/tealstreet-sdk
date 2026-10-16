use safe_cex_venue_coinbase::protocol::*;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/protocol.cjs"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
#[test]
fn protocol_coinbase_starbase_current_cutover_evidence() {
    assert_eq!(
        json!(COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE),
        oracle()["coinbase.starbase-cutover.ts"]["COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE"],
        "COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE"
    );
}
#[test]
fn protocol_coinbase_starbase_http_shadow_activation_enabled() {
    assert_eq!(
        json!(COINBASE_STARBASE_HTTP_SHADOW_ACTIVATION_ENABLED),
        oracle()["coinbase.starbase-cutover.ts"]["COINBASE_STARBASE_HTTP_SHADOW_ACTIVATION_ENABLED"],
        "COINBASE_STARBASE_HTTP_SHADOW_ACTIVATION_ENABLED"
    );
}
#[test]
fn protocol_coinbase_starbase_websocket_activation_enabled() {
    assert_eq!(
        json!(COINBASE_STARBASE_WEBSOCKET_ACTIVATION_ENABLED),
        oracle()["coinbase.starbase-cutover.ts"]["COINBASE_STARBASE_WEBSOCKET_ACTIVATION_ENABLED"],
        "COINBASE_STARBASE_WEBSOCKET_ACTIVATION_ENABLED"
    );
}
#[test]
fn protocol_coinbase_starbase_runtime_activation_enabled() {
    assert_eq!(
        json!(COINBASE_STARBASE_RUNTIME_ACTIVATION_ENABLED),
        oracle()["coinbase.starbase-cutover.ts"]["COINBASE_STARBASE_RUNTIME_ACTIVATION_ENABLED"],
        "COINBASE_STARBASE_RUNTIME_ACTIVATION_ENABLED"
    );
}
#[test]
fn protocol_coinbase_starbase_base_amount_source() {
    assert_eq!(
        json!(COINBASE_STARBASE_BASE_AMOUNT_SOURCE),
        oracle()["coinbase.starbase-orders.ts"]["COINBASE_STARBASE_BASE_AMOUNT_SOURCE"],
        "COINBASE_STARBASE_BASE_AMOUNT_SOURCE"
    );
}
#[test]
fn protocol_coinbase_starbase_public_frame_classifier_activation_enabled() {
    assert_eq!(
        json!(COINBASE_STARBASE_PUBLIC_FRAME_CLASSIFIER_ACTIVATION_ENABLED),
        oracle()["coinbase.starbase-public-frames.ts"]["COINBASE_STARBASE_PUBLIC_FRAME_CLASSIFIER_ACTIVATION_ENABLED"],
        "COINBASE_STARBASE_PUBLIC_FRAME_CLASSIFIER_ACTIVATION_ENABLED"
    );
}
#[test]
fn protocol_coinbase_starbase_public_frame_capture_sha256() {
    assert_eq!(
        json!(COINBASE_STARBASE_PUBLIC_FRAME_CAPTURE_SHA256),
        oracle()["coinbase.starbase-public-frames.ts"]["COINBASE_STARBASE_PUBLIC_FRAME_CAPTURE_SHA256"],
        "COINBASE_STARBASE_PUBLIC_FRAME_CAPTURE_SHA256"
    );
}
#[test]
fn protocol_coinbase_starbase_chart_resolutions() {
    assert_eq!(
        json!(COINBASE_STARBASE_CHART_RESOLUTIONS),
        oracle()["coinbase.starbase-public.ts"]["COINBASE_STARBASE_CHART_RESOLUTIONS"],
        "COINBASE_STARBASE_CHART_RESOLUTIONS"
    );
}
#[test]
fn protocol_coinbase_starbase_timeframe_resolution() {
    assert_eq!(
        json!(COINBASE_STARBASE_TIMEFRAME_RESOLUTION),
        oracle()["coinbase.starbase-public.ts"]["COINBASE_STARBASE_TIMEFRAME_RESOLUTION"],
        "COINBASE_STARBASE_TIMEFRAME_RESOLUTION"
    );
}
#[test]
fn protocol_coinbase_starbase_websocket_only_session_methods() {
    assert_eq!(
        json!(COINBASE_STARBASE_WEBSOCKET_ONLY_SESSION_METHODS),
        oracle()["coinbase.starbase-session.ts"]["COINBASE_STARBASE_WEBSOCKET_ONLY_SESSION_METHODS"],
        "COINBASE_STARBASE_WEBSOCKET_ONLY_SESSION_METHODS"
    );
}
#[test]
fn protocol_coinbase_starbase_endpoints() {
    assert_eq!(
        json!(COINBASE_STARBASE_ENDPOINTS),
        oracle()["coinbase.starbase.ts"]["COINBASE_STARBASE_ENDPOINTS"],
        "COINBASE_STARBASE_ENDPOINTS"
    );
}
#[test]
fn protocol_coinbase_starbase_auth_refresh_after_ms() {
    assert_eq!(
        json!(COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS),
        oracle()["coinbase.starbase.ts"]["COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS"],
        "COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS"
    );
}
#[test]
fn protocol_export_set() {
    let expected: Value = serde_json::from_str(r#"{"coinbase.starbase-cutover.ts": ["COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE", "COINBASE_STARBASE_HTTP_SHADOW_ACTIVATION_ENABLED", "COINBASE_STARBASE_WEBSOCKET_ACTIVATION_ENABLED", "COINBASE_STARBASE_RUNTIME_ACTIVATION_ENABLED"], "coinbase.starbase-orders.ts": ["COINBASE_STARBASE_BASE_AMOUNT_SOURCE"], "coinbase.starbase-private.ts": [], "coinbase.starbase-public-frames.ts": ["COINBASE_STARBASE_PUBLIC_FRAME_CLASSIFIER_ACTIVATION_ENABLED", "COINBASE_STARBASE_PUBLIC_FRAME_CAPTURE_SHA256"], "coinbase.starbase-public.ts": ["COINBASE_STARBASE_CHART_RESOLUTIONS", "COINBASE_STARBASE_TIMEFRAME_RESOLUTION"], "coinbase.starbase-session.ts": ["COINBASE_STARBASE_WEBSOCKET_ONLY_SESSION_METHODS"], "coinbase.starbase-ws-session.ts": [], "coinbase.starbase.ts": ["COINBASE_STARBASE_ENDPOINTS", "COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS"]}"#).unwrap();
    let actual: serde_json::Map<String, Value> = oracle()
        .as_object()
        .unwrap()
        .iter()
        .map(|(f, v)| {
            (
                f.clone(),
                json!(v.as_object().unwrap().keys().collect::<Vec<_>>()),
            )
        })
        .collect();
    assert_eq!(
        Value::Object(actual),
        expected,
        "Starbase constant export set"
    );
}
