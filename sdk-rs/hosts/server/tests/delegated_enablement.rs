//! Offline rollout gates; no account or venue connection is opened.

#[test]
fn compiled_order_frames_are_not_advertised_without_runtime_opt_in() {
    let capabilities = safe_cex_exchange_server::http::capabilities_json();
    assert_eq!(
        capabilities["transports"]["accountWorkerWebSocket"]["delegatedOrderFramesV1"],
        false
    );
    assert!(
        capabilities["transports"]["accountWorkerWebSocket"]
            .get("delegatedOrderLedgerV1")
            .is_none(),
        "only the router advertises its ledger"
    );
}

#[test]
fn runtime_switch_uses_the_same_executable_and_keeps_the_ledger_router_owned() {
    const EXPECT: &str = "TEALSTREET_TEST_ORDER_FRAME_CAPABILITY";
    if let Ok(expected) = std::env::var(EXPECT) {
        let capabilities = safe_cex_exchange_server::http::capabilities_json();
        let transport = &capabilities["transports"]["accountWorkerWebSocket"];
        assert_eq!(transport["delegatedOrderFramesV1"], expected == "1");
        assert!(transport.get("delegatedOrderLedgerV1").is_none());
        return;
    }
    for value in [None, Some("0"), Some("1"), Some("invalid")] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args([
            "--exact",
            "runtime_switch_uses_the_same_executable_and_keeps_the_ledger_router_owned",
            "--nocapture",
        ]);
        command.env_remove("TEALSTREET_DELEGATED_ORDER_FRAMES");
        if let Some(value) = value {
            command.env("TEALSTREET_DELEGATED_ORDER_FRAMES", value);
        }
        let expected = value == Some("1") && safe_cex_exchange_server::order_contract::enabled();
        command.env(EXPECT, if expected { "1" } else { "0" });
        let output = command.output().unwrap();
        assert!(output.status.success(), "runtime case {value:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }
}
