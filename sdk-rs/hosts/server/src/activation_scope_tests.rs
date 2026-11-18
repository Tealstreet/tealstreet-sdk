use super::capabilities_json_for;
use serde_json::{Value, json};

// Match Session's existing runtime test: each immutable environment has its
// own process, so parallel tests never mutate another connection's switch.
fn subprocess(test: &str, enabled: &str) -> bool {
    const CHILD: &str = "TEALSTREET_TEST_ACTIVATION_SCOPE";
    let name = format!("http::activation_scope_tests::{test}");
    if std::env::var(CHILD).as_deref() == Ok(name.as_str()) {
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &name, "--nocapture"])
        .env(CHILD, &name)
        .env("TEALSTREET_DELEGATED_ORDER_FRAMES", enabled)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "runtime={enabled}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    true
}

fn transport(venue: Option<&str>) -> Value {
    capabilities_json_for(venue)["transports"]["accountWorkerWebSocket"].clone()
}

fn assert_scope(value: &Value, available: bool, full: bool) {
    assert_eq!(
        value["delegatedActivationScopeV1"],
        json!({
            "place":available,
            "close":if available { "reduceOnly" } else { "unsupported" },
            "amend":full, "cancel":full, "cancelAll":full, "nuke":full,
        })
    );
}

#[test]
fn activation_scope_advertises_runtime_full_writes() {
    if subprocess("activation_scope_advertises_runtime_full_writes", "1") {
        return;
    }
    for venue in ["bybit", "binance", "okx", "hyperliquid"] {
        let enabled = crate::order_contract::enabled_for(venue);
        let value = transport(Some(venue));
        for flag in [
            "delegatedOrderFramesV1",
            "delegatedOrderActionsV1",
            "delegatedFullWriteActionsV1",
        ] {
            assert_eq!(value[flag], cfg!(feature = "delegated-order-frames"));
        }
        assert_eq!(value["delegatedVenueV1"], venue);
        assert_scope(&value, enabled, enabled);
    }
}

#[test]
fn activation_scope_keeps_partial_protocol_when_runtime_disabled() {
    if subprocess(
        "activation_scope_keeps_partial_protocol_when_runtime_disabled",
        "0",
    ) {
        return;
    }
    for venue in ["bybit", "binance", "okx", "hyperliquid"] {
        let value = transport(Some(venue));
        for flag in [
            "delegatedOrderFramesV1",
            "delegatedOrderActionsV1",
            "delegatedFullWriteActionsV1",
        ] {
            assert_eq!(value[flag], false);
        }
        assert_scope(&value, crate::order_contract::enabled_for(venue), false);
    }
}

#[tokio::test]
async fn activation_scope_uses_generic_venue_gate() {
    if subprocess("activation_scope_uses_generic_venue_gate", "1") {
        return;
    }
    crate::delegated::with_test_declaration(
        json!({"venues": {
            "bybit":{"remoteSigning":{"enabled":false}},
            "binance":{"remoteSigning":{"enabled":true}}
        }}),
        async {
            for venue in [None, Some("bybit"), Some("txflow"), Some("unknown")] {
                assert_scope(&transport(venue), false, false);
            }
            let binance = transport(Some("binance"));
            let enabled = cfg!(feature = "delegated-order-frames");
            assert_eq!(binance["delegatedFullWriteActionsV1"], enabled);
            assert_scope(&binance, enabled, enabled);
        },
    )
    .await;
}
