//! Offline rollout gates; no account or venue connection is opened.

#[test]
fn compiled_order_frames_are_not_advertised_without_runtime_opt_in() {
    const PROBE: &str = "TEALSTREET_TEST_ORDER_FRAME_DEFAULT_OFF";
    if std::env::var(PROBE).as_deref() != Ok("1") {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "compiled_order_frames_are_not_advertised_without_runtime_opt_in",
                "--nocapture",
            ])
            .env("TEALSTREET_DELEGATED_ORDER_FRAMES", "0")
            .env(PROBE, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "runtime advertisement input 0:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        return;
    }
    let capabilities = safe_cex_exchange_server::http::capabilities_json();
    assert_eq!(
        capabilities["transports"]["accountWorkerWebSocket"]["delegatedOrderFramesV1"],
        false
    );
    assert_eq!(
        capabilities["transports"]["accountWorkerWebSocket"]["delegatedOrderActionsV1"],
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
        let capabilities = safe_cex_exchange_server::http::capabilities_json_for(Some("binance"));
        let transport = &capabilities["transports"]["accountWorkerWebSocket"];
        assert_eq!(transport["delegatedOrderFramesV1"], expected == "1");
        assert_eq!(transport["delegatedOrderActionsV1"], expected == "1");
        assert!(transport.get("delegatedOrderActionLedgerV1").is_none());
        assert!(transport.get("delegatedOrderLedgerV1").is_none());
        let supported = safe_cex_exchange_server::order_contract::enabled();
        assert_eq!(transport["delegatedActivationScopeV1"]["place"], supported);
        assert_eq!(
            transport["delegatedActivationScopeV1"]["close"],
            if supported {
                "reduceOnly"
            } else {
                "unsupported"
            }
        );
        for action in ["amend", "cancel", "cancelAll", "nuke"] {
            assert_eq!(transport["delegatedActivationScopeV1"][action], false);
        }
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
