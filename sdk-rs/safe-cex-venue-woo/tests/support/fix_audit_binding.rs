use crate::binding_test_harness as harness;
use safe_cex_types::ExchangeName;

const INACTIVE_V3_ROWS: &[(u8, &str)] = &[
    (1, "WooV3PreparedRequest"),
    (2, "prepareWooV3Request"),
    (6, "validateTarget"),
    (7, "decimalToNumber"),
    (8, "mapWooV3AccountBalance"),
    (9, "WooV3CancelAllAfterResponse"),
    (10, "WooV3OrderBookRecoveryCoordinator.pushUpdate"),
    (12, "mapWooV3Candles"),
    (13, "WooV3SetLeverageInput"),
    (14, "WOO_V3_ACCOUNT_WRITE_PARITY"),
    (15, "requirePerpetualSymbol"),
    (16, "positionProvesNoExposure"),
    (22, "WOO_V3_WS_CHANNELS"),
    (23, "WOO_V3_WS_LIMITS"),
    (24, "WOO_V3_WS_PUBLIC_TOPIC_KINDS"),
    (25, "WOO_V3_WS_PRIVATE_TOPICS"),
    (26, "WOO_V3_WS_BLOCKERS"),
    (27, "WooV3ListenKey"),
    (28, "buildWooV3PublicTopic"),
    (29, "buildWooV3PrivateTopic"),
    (30, "parseWooV3WsTopic"),
    (31, "WOO_V3_WS_COMMAND_ID_CONTRACT_STATUS"),
    (32, "WooV3WsCommandAck"),
    (33, "WooV3WsPing"),
    (34, "WooV3WsPong"),
    (35, "WooV3WsOrderBookUpdate"),
    (36, "WooV3WsTicker"),
    (37, "WooV3WsKline"),
    (38, "WooV3WsPrice"),
    (39, "WooV3WsFundingRate"),
    (40, "WooV3WsOpenInterest"),
    (41, "WooV3WsPublicPayload"),
    (42, "WooV3WsAccount"),
    (43, "WooV3WsBalanceRow"),
    (44, "WooV3WsExecutionReport"),
    (45, "decodePrivatePayload"),
    (46, "advanceWooV3OrderBookSequence"),
    (47, "WooV3AlgoOrderMapping"),
    (48, "mapRegularOrderType"),
    (52, "WooV3AccountInfo"),
    (53, "decodeMeta"),
    (54, "WooV3WsBlocker"),
    (55, "WooV3AlgoOrder"),
    (56, "normalizeUpdate"),
    (57, "WooV3SetLeverageBuildResult"),
    (58, "WooV3AccountMutationResult"),
    (59, "WooV3WsPrivatePayload"),
];

#[test]
fn assigned_v3_rows_have_no_constructed_runtime_registration() {
    let h = harness::harness(ExchangeName::Woo, harness::options(), |_| {});
    let this = &h.exchange.rt.this;
    assert!(this.get("fetchMarkets").unwrap().is_callable());
    assert!(
        this.get("publicWebsocket")
            .unwrap()
            .get("ping")
            .unwrap()
            .is_callable()
    );
    assert!(
        this.get("privateWebsocket")
            .unwrap()
            .get("ping")
            .unwrap()
            .is_callable()
    );
    assert_eq!(INACTIVE_V3_ROWS.len(), 47);
    for (id, name) in INACTIVE_V3_ROWS {
        for key in [
            *name,
            name.split('.').next().unwrap(),
            name.rsplit('.').next().unwrap(),
        ] {
            assert!(
                this.get(key).unwrap().is_undefined(),
                "row{id} {name}: unexpected member {key}"
            );
            assert!(
                h.exchange
                    .rt
                    .shared
                    .globals
                    .get(key)
                    .unwrap()
                    .is_undefined(),
                "row{id} {name}: unexpected runtime global {key}"
            );
        }
    }
    assert!(h.http.requests.borrow().is_empty());
    assert!(h.runtime.sockets.borrow().is_empty());
    assert!(h.runtime.reports.borrow().is_empty());
}

#[test]
fn maintained_class_and_import_graph_keep_v3_out_of_runtime() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-class-oracle.cjs"
        ))
        .arg("--fix-audit-boundary")
        .output()
        .expect("maintained whole class boundary");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["constructed"], 2);
    assert_eq!(result["v3Imports"], 0);
    assert_eq!(result["gates"], 8);
    assert!(result["legacyModules"].as_u64().unwrap() >= 4);
}

#[test]
fn schedule_cancel_keeps_the_inherited_refusal_and_no_timer_route() {
    let h = harness::harness(ExchangeName::Woo, harness::options(), |http| {
        http.ok("DELETE", "/v3/orders/pending", serde_json::json!({}));
    });
    for time in [None, Some(None), Some(Some(0.0)), Some(Some(f64::NAN))] {
        let error = h.drive(h.venue().schedule_cancel(time)).unwrap_err();
        assert_eq!(error.name, "Error");
        assert_eq!(error.message, "Not implemented");
        assert!(
            h.http.requests.borrow().is_empty(),
            "no dead-man-switch HTTP"
        );
        assert!(h.runtime.sockets.borrow().is_empty(), "no timer socket");
        assert!(h.runtime.reports.borrow().is_empty(), "no extra diagnostic");
    }
    h.runtime.advance(900_001.0);
    assert!(
        h.http.requests.borrow().is_empty(),
        "no deferred cancellation timer"
    );
    h.drive(h.venue().cancel_all_orders()).unwrap();
    let calls = h.http.calls("/v3/orders/pending");
    assert_eq!(calls.len(), 1, "the live cancellation route still runs");
    assert_eq!(calls[0].method, "DELETE");
    assert!(calls[0].headers["x-api-signature"].as_str().is_some());
}

#[test]
fn maintained_woo_inherits_the_exact_base_schedule_cancel_rejection() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-class-oracle.cjs"
        ))
        .arg("--schedule-cancel")
        .output()
        .expect("maintained WOO inherited method");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["name"], "Error");
    assert_eq!(result["message"], "Not implemented");
    assert_eq!(result["cases"], 4);
}
