#[path = "support/retained_input_observations.rs"]
mod retained_input_observations;
use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_types::object_map::JsObjectMap;
use safe_cex_venue_woo::{
    v3_account_writes::{
        WooV3AccountMutationTarget, WooV3AccountPositionMode, WooV3AccountWriteError,
    },
    v3_contracts::{WooV3Endpoint, WooV3HttpMethod},
    v3_position_mode::*,
    v3_private_orders::{WooV3AlgoOrder, WooV3GatewayAlgoQuantity},
};
use serde_json::Value;
fn source() -> Value {
    let o = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/position-mode-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "MODE source oracle {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}
fn input(e: &Value) -> Option<JsNumberInput> {
    e["input"].as_str().map(|s| js_from_str(s).unwrap())
}
fn number(a: f64, e: &Value, label: &str) {
    assert_eq!(
        a.to_bits(),
        e["$number"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            .to_bits(),
        "MODE native {label}"
    );
}
fn mode(a: WooV3AccountPositionMode) -> &'static str {
    match a {
        WooV3AccountPositionMode::OneWay => "ONE_WAY",
        WooV3AccountPositionMode::HedgeMode => "HEDGE_MODE",
    }
}
fn captured(a: &JsNumberInput, e: &Value) {
    if e.get("$number").is_some() {
        number(a.clone().decode::<f64>().unwrap(), e, "captured number");
    } else {
        match e {
            Value::Null => assert!(
                a.clone().decode::<Option<bool>>().unwrap().is_none(),
                "MODE native null"
            ),
            Value::Bool(v) => assert_eq!(
                a.clone().decode::<bool>().unwrap(),
                *v,
                "MODE native captured bool"
            ),
            Value::String(v) => assert_eq!(
                a.clone().decode::<String>().unwrap(),
                *v,
                "MODE native captured string"
            ),
            Value::Array(v) => {
                let a = a.clone().decode::<Vec<JsNumberInput>>().unwrap();
                assert_eq!(a.len(), v.len(), "MODE native captured array length");
                for (a, e) in a.iter().zip(v) {
                    captured(a, e);
                }
            }
            Value::Object(_) => map(
                &a.clone().decode::<JsObjectMap<JsNumberInput>>().unwrap(),
                e,
            ),
            _ => panic!("source number marker required"),
        }
    }
}
fn map(a: &JsObjectMap<JsNumberInput>, e: &Value) {
    let v = e.as_object().unwrap();
    assert_eq!(a.len(), v.len(), "MODE native captured key count");
    for (k, e) in v {
        captured(
            a.get(k)
                .unwrap_or_else(|| panic!("MODE native missing key {k}")),
            e,
        );
    }
}
fn optional_number(a: Option<f64>, e: &Value, k: &str) {
    assert_eq!(
        a.is_some(),
        e.get(k).is_some(),
        "MODE native optional {k} presence"
    );
    if let Some(v) = a {
        number(v, &e[k], k);
    }
}
#[test]
fn position_evidence_choices_are_native_before_source() {
    use safe_cex_venue_woo::v3_account_writes::{
        WOO_V3_ACCOUNT_WRITE_PARITY, require_perpetual_symbol,
    };
    use serde_json::json;
    fn capture(value: Value) -> JsNumberInput {
        js_from_str(&value.to_string()).unwrap()
    }
    fn base() -> Value {
        json!({"targetPositionMode":"HEDGE_MODE","evidence":{
            "account":{"source":"decoded-v3-account-info","reconciliationId":"audit-23","accountMode":"MARGIN","positionMode":"ONE_WAY"},
            "positions":{"source":"decoded-v3-futures-positions-complete-page","reconciliationId":"audit-23","complete":true,"rows":[{"symbol":"PERP_BTC_USDT","positionSide":"BOTH","holding":"-00.","pendingLongQty":".000","pendingShortQty":"0","settlePrice":"1","averageOpenPrice":"2","pnl24H":"3","fee24H":"4","markPrice":"5","estLiqPrice":"6","adlQuantile":7,"timestamp":11}]},
            "regularOrders":{"source":"decoded-v3-pending-regular-orders-complete-pages","reconciliationId":"audit-23","complete":true,"rows":[]},
            "algoOrders":{"source":"decoded-v3-pending-algo-orders-complete-pages","reconciliationId":"audit-23","complete":true,"rows":[]}
        }})
    }
    fn blocker(value: Value, expected: &str, current: Option<WooV3AccountPositionMode>) {
        let WooV3SetPositionModeBuildResult::Blocked {
            blocker,
            current_position_mode,
            target_position_mode,
        } = build_woo_v3_set_position_mode_request(Some(&capture(value))).unwrap()
        else {
            panic!("WOO-MODE-CHOICE native expected blocker")
        };
        assert_eq!(
            blocker, expected,
            "WOO-MODE-CHOICE native blocker precedence"
        );
        assert_eq!(
            current_position_mode, current,
            "WOO-MODE-CHOICE native optional current mode"
        );
        assert!(matches!(
            target_position_mode,
            WooV3AccountPositionMode::HedgeMode
        ));
    }
    assert_eq!(
        require_perpetual_symbol("PERP_01_USDT").unwrap(),
        "PERP_01_USDT"
    );
    assert_eq!(
        require_perpetual_symbol("PERP_btc_USDT").unwrap_err(),
        "WOO V3 symbol must use PERP_BASE_QUOTE form"
    );
    let array = js_from_str::<String>(r#"["PERP_BTC_USDT"]"#);
    assert!(
        array.is_err(),
        "WOO-MODE-CHOICE native typed symbol refusal"
    );
    let positions_source = "decoded-v3-futures-positions-complete-page";
    let snapshot = capture(
        json!({"source":positions_source,"reconciliationId":"audit-23","complete":true,"rows":[null]}),
    );
    let mut retained =
        validate_snapshot_evidence(Some(&snapshot), positions_source, "audit").unwrap();
    assert_eq!(retained.reconciliation_id, "audit-23");
    assert_eq!(retained.rows.len(), 1);
    assert!(
        retained.rows[0]
            .clone()
            .decode::<Option<bool>>()
            .unwrap()
            .is_none(),
        "WOO-MODE-CHOICE native untyped null row retained"
    );
    retained.rows.push(capture(json!({"value":37})));
    assert_eq!(retained.rows.len(), 2);
    assert_eq!(
        validate_snapshot_evidence(Some(&snapshot), positions_source, "audit")
            .unwrap()
            .rows
            .len(),
        1,
        "WOO-MODE-CHOICE native snapshot independence"
    );
    let invalid = capture(
        json!({"source":positions_source,"reconciliationId":"audit-23","complete":1,"rows":[]}),
    );
    assert!(validate_snapshot_evidence(Some(&invalid), positions_source, "audit").is_none());
    for text in ["0", "-00.", "-.00", ".000"] {
        assert!(
            is_exact_decimal_zero(Some(&capture(json!(text)))),
            "WOO-MODE-CHOICE native admitted zero {text}"
        );
    }
    for text in [".", "-.", "0e0", "1e-9999", "0\n", "1"] {
        assert!(
            !is_exact_decimal_zero(Some(&capture(json!(text)))),
            "WOO-MODE-CHOICE native rejected zero {text}"
        );
    }
    assert!(!is_exact_decimal_zero(Some(&capture(json!(0)))));
    assert!(position_proves_no_exposure(Some(&capture(
        base()["evidence"]["positions"]["rows"][0].clone()
    ))));
    assert!(!position_proves_no_exposure(Some(&capture(
        json!({"holding":"0","pendingLongQty":"0","pendingShortQty":"1e-9999"})
    ))));
    let captured = capture(base());
    let typed: WooV3SetPositionModeInput = captured.clone().decode().unwrap();
    assert!(matches!(
        typed.target_position_mode,
        WooV3AccountPositionMode::HedgeMode
    ));
    assert_eq!(typed.evidence.account.reconciliation_id, "audit-23");
    assert_eq!(typed.evidence.account.source, "decoded-v3-account-info");
    assert_eq!(typed.evidence.account.account_mode, "MARGIN");
    assert!(matches!(
        typed.evidence.account.position_mode,
        WooV3AccountPositionMode::OneWay
    ));
    for (id, source, complete, expected_source) in [
        (
            &typed.evidence.positions.reconciliation_id,
            typed.evidence.positions.source.as_str(),
            typed.evidence.positions.complete,
            "decoded-v3-futures-positions-complete-page",
        ),
        (
            &typed.evidence.regular_orders.reconciliation_id,
            typed.evidence.regular_orders.source.as_str(),
            typed.evidence.regular_orders.complete,
            "decoded-v3-pending-regular-orders-complete-pages",
        ),
        (
            &typed.evidence.algo_orders.reconciliation_id,
            typed.evidence.algo_orders.source.as_str(),
            typed.evidence.algo_orders.complete,
            "decoded-v3-pending-algo-orders-complete-pages",
        ),
    ] {
        assert_eq!(id, "audit-23");
        assert_eq!(
            source, expected_source,
            "WOO-MODE-CHOICE native exact snapshot source"
        );
        assert!(complete);
    }
    assert_eq!(typed.evidence.positions.rows.len(), 1);
    assert_eq!(typed.evidence.positions.rows[0].holding, "-00.");
    assert!(
        typed.evidence.regular_orders.rows.is_empty() && typed.evidence.algo_orders.rows.is_empty()
    );
    let mut wider = typed.clone();
    wider.evidence.positions.complete = false;
    wider.evidence.positions.source = "locally-wider".into();
    assert!(!wider.evidence.positions.complete);
    assert_eq!(wider.evidence.positions.source, "locally-wider");
    assert_eq!(typed.evidence.positions.source, positions_source);
    let mut invalid_type = base();
    invalid_type["evidence"]["positions"]["complete"] = json!(false);
    assert!(
        capture(invalid_type)
            .decode::<WooV3SetPositionModeInput>()
            .is_err()
    );
    let WooV3SetPositionModeBuildResult::Ready {
        request,
        current_position_mode,
        target_position_mode,
        mut precondition_evidence,
    } = build_woo_v3_set_position_mode_request(Some(&captured)).unwrap()
    else {
        panic!("WOO-MODE-CHOICE native ready variant")
    };
    assert!(matches!(
        current_position_mode,
        WooV3AccountPositionMode::OneWay
    ));
    assert!(matches!(
        target_position_mode,
        WooV3AccountPositionMode::HedgeMode
    ));
    assert_eq!(request.method, WooV3HttpMethod::Put);
    assert_eq!(request.path, WooV3Endpoint::FuturesPositionMode);
    assert!(request.query.is_none());
    let body = request.body.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(
        body.get("positionMode")
            .unwrap()
            .clone()
            .decode::<String>()
            .unwrap(),
        "HEDGE_MODE",
        "WOO-MODE-CHOICE native ready target body"
    );
    assert_eq!(
        precondition_evidence,
        "complete-reconciled-v3-positions-zero-and-pending-orders-empty"
    );
    precondition_evidence.push_str("-local");
    assert!(precondition_evidence.ends_with("-local"));
    let mut incomplete = base();
    incomplete["evidence"]["account"] = Value::Null;
    blocker(
        incomplete,
        "account-state-evidence-incomplete-or-mixed",
        None,
    );
    let mut conflict = base();
    conflict["evidence"]["account"]["accountMode"] = json!("SPOT");
    conflict["evidence"]["positions"]["reconciliationId"] = json!("different");
    blocker(
        conflict,
        "account-state-evidence-incomplete-or-mixed",
        Some(WooV3AccountPositionMode::OneWay),
    );
    let mut exposed = base();
    exposed["evidence"]["positions"]["rows"][0]["holding"] = json!("1");
    exposed["evidence"]["regularOrders"]["rows"] = json!([null]);
    blocker(
        exposed,
        "open-positions-must-be-closed",
        Some(WooV3AccountPositionMode::OneWay),
    );
    let mut sparse_projection = base();
    sparse_projection["evidence"]["positions"]["rows"] = json!([null]);
    blocker(
        sparse_projection,
        "open-positions-must-be-closed",
        Some(WooV3AccountPositionMode::OneWay),
    );
    let raw = base().to_string().replace("audit-23", r"\ud800");
    for section in ["account", "positions", "regularOrders", "algoOrders"] {
        assert_eq!(
            retained_input_observations::text_at(&raw, &["evidence", section, "reconciliationId"]),
            vec![0xd800],
            "WOO-MODE-CHOICE native retained ID {section}"
        );
    }
    assert!(js_from_str::<WooV3SetPositionModeInput>(&raw).is_err());
    assert!(
        js_from_str::<WooV3CompletePositionModeEvidence>(
            &base()["evidence"]
                .to_string()
                .replace("audit-23", r"\ud800")
        )
        .is_err()
    );
    let raw_snapshot = r#"{"source":"decoded-v3-futures-positions-complete-page","reconciliationId":"\ud800","complete":true,"rows":[]}"#;
    assert_eq!(
        retained_input_observations::text_at(raw_snapshot, &["reconciliationId"]),
        vec![0xd800],
        "WOO-MODE-CHOICE native retained snapshot ID"
    );
    let paired = base().to_string().replace("audit-23", r"\ud800\udc00");
    let decoded = js_from_str::<WooV3SetPositionModeInput>(&paired).unwrap();
    assert_eq!(decoded.evidence.account.reconciliation_id, "\u{10000}");
    let ascii = base().to_string().replace("audit-23", r"\\ud800");
    let decoded = js_from_str::<WooV3SetPositionModeInput>(&ascii).unwrap();
    assert_eq!(decoded.evidence.account.reconciliation_id, r"\ud800");
    let parity = WOO_V3_ACCOUNT_WRITE_PARITY;
    assert!(parity.request_and_response_fixtures);
    assert!(!parity.runtime_imported);
    assert!(!parity.credentialed_demo_lifecycle);
    assert!(parity.readback_required);
    assert_eq!(
        parity.leverage_readback,
        "strict-one-way-both-or-hedge-long-short"
    );
    assert_eq!(
        parity.default_margin_mode,
        "excluded-official-response-unusable"
    );
    assert_eq!(
        parity.cross_one_way_leverage,
        "descriptor-ready-request-state-only"
    );
    assert_eq!(
        parity.cross_hedge_leverage,
        "blocked-resulting-leg-scope-requires-demo-evidence"
    );
    assert_eq!(
        parity.isolated_leverage,
        "blocked-outside-safe-cex-cross-margin-contract"
    );
    assert_eq!(
        parity.position_mode_preconditions,
        "complete-reconciled-v3-positions-zero-and-pending-orders-empty"
    );
    assert_eq!(parity.mutation_response_state, "request-target-only");
    let mut local = parity;
    local.runtime_imported = true;
    assert!(local.runtime_imported);
    let fresh = WOO_V3_ACCOUNT_WRITE_PARITY;
    assert!(!fresh.runtime_imported);
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/position-mode-oracle.cjs"
        ))
        .arg("--choice-audit")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "WOO-MODE-CHOICE source observations: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let observations: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(observations["groups"], 8);
    assert_eq!(observations["loneIdRows"], 4);
    assert_eq!(observations["revisitedRows"], 1);
}
#[test]
fn native_evidence_helpers() {
    let s = source();
    let rows = s["zeros"].as_array().unwrap();
    assert_eq!(rows.len(), 27, "MODE zero floor");
    for e in rows {
        assert_eq!(
            is_exact_decimal_zero(input(e).as_ref()),
            e["ok"].as_bool().unwrap(),
            "MODE native exact decimal zero"
        );
    }
    let rows = s["positions"].as_array().unwrap();
    assert_eq!(rows.len(), 21, "MODE exposure floor");
    for e in rows {
        assert_eq!(
            position_proves_no_exposure(input(e).as_ref()),
            e["ok"].as_bool().unwrap(),
            "MODE native exposure predicate"
        );
    }
    let rows = s["snapshots"].as_array().unwrap();
    assert_eq!(rows.len(), 36, "MODE snapshot floor");
    for e in rows {
        let name = e["name"].as_str().unwrap();
        let expected = match name {
            "positions" => "decoded-v3-futures-positions-complete-page",
            "regularOrders" => "decoded-v3-pending-regular-orders-complete-pages",
            "algoOrders" => "decoded-v3-pending-algo-orders-complete-pages",
            _ => panic!("unknown source"),
        };
        assert_eq!(s["sources"][name], expected);
        let a = validate_snapshot_evidence(input(e).as_ref(), expected, "probe");
        if e.get("absent").is_some() {
            assert!(a.is_none(), "MODE native snapshot refusal");
        } else {
            let a = a.expect("MODE native snapshot accepted");
            let x = &e["ok"];
            assert_eq!(
                a.reconciliation_id.as_str(),
                x["reconciliationId"].as_str().unwrap(),
                "MODE native snapshot reconciliation ID"
            );
            let xs = x["rows"].as_array().unwrap();
            assert_eq!(a.rows.len(), xs.len(), "MODE native snapshot rows count");
            for (a, e) in a.rows.iter().zip(xs) {
                captured(a, e);
            }
        }
    }
}
#[test]
fn native_position_mode_builders() {
    let s = source();
    let rows = s["builds"].as_array().unwrap();
    assert_eq!(rows.len(), 40, "MODE build floor");
    for e in rows {
        let r = build_woo_v3_set_position_mode_request(input(e).as_ref());
        if let Some(x) = e.get("error") {
            assert_eq!(x["name"], "Error");
            assert_eq!(
                r.expect_err("MODE native validation refusal"),
                x["message"].as_str().unwrap(),
                "MODE native first validation error"
            );
            continue;
        }
        let x = &e["ok"];
        match r.expect("MODE native build success") {
            WooV3SetPositionModeBuildResult::Ready {
                request,
                current_position_mode,
                target_position_mode,
                precondition_evidence,
            } => {
                assert_eq!(x["ready"], true, "MODE native ready variant");
                assert_eq!(
                    mode(current_position_mode),
                    x["currentPositionMode"].as_str().unwrap(),
                    "MODE native current position mode"
                );
                assert_eq!(
                    mode(target_position_mode),
                    x["targetPositionMode"].as_str().unwrap(),
                    "MODE native target position mode"
                );
                assert_eq!(
                    request.method,
                    WooV3HttpMethod::Put,
                    "MODE native request method"
                );
                assert_eq!(x["request"]["method"], "PUT");
                assert_eq!(
                    request.path,
                    WooV3Endpoint::FuturesPositionMode,
                    "MODE native request path"
                );
                assert_eq!(x["request"]["path"], "/v3/futures/positionMode");
                assert!(request.query.is_none(), "MODE native query absence");
                map(
                    request.body.as_ref().expect("MODE native body"),
                    &x["request"]["body"],
                );
                assert_eq!(
                    precondition_evidence.as_str(),
                    x["preconditionEvidence"].as_str().unwrap(),
                    "MODE native precondition evidence"
                );
            }
            WooV3SetPositionModeBuildResult::Blocked {
                blocker,
                current_position_mode,
                target_position_mode,
            } => {
                assert_eq!(x["ready"], false, "MODE native blocked variant");
                assert_eq!(
                    blocker.as_str(),
                    x["blocker"].as_str().unwrap(),
                    "MODE native first blocker"
                );
                assert_eq!(
                    current_position_mode.is_some(),
                    x.get("currentPositionMode").is_some(),
                    "MODE native optional current mode presence"
                );
                if let Some(v) = current_position_mode {
                    assert_eq!(
                        mode(v),
                        x["currentPositionMode"].as_str().unwrap(),
                        "MODE native blocked current mode"
                    );
                }
                assert_eq!(
                    mode(target_position_mode),
                    x["targetPositionMode"].as_str().unwrap(),
                    "MODE native blocked target mode"
                );
            }
        }
    }
}
fn evidence(a: &WooV3CompletePositionModeEvidence, e: &Value) {
    let x = &e["account"];
    assert_eq!(
        a.account.source.as_str(),
        x["source"].as_str().unwrap(),
        "MODE native account source"
    );
    assert_eq!(
        a.account.reconciliation_id.as_str(),
        x["reconciliationId"].as_str().unwrap(),
        "MODE native account ID"
    );
    assert_eq!(
        a.account.account_mode.as_str(),
        x["accountMode"].as_str().unwrap(),
        "MODE native account mode"
    );
    assert_eq!(
        mode(a.account.position_mode),
        x["positionMode"].as_str().unwrap(),
        "MODE native declaration account position mode"
    );
    for (source, id, complete, name, len) in [
        (
            &a.positions.source,
            &a.positions.reconciliation_id,
            a.positions.complete,
            "positions",
            a.positions.rows.len(),
        ),
        (
            &a.regular_orders.source,
            &a.regular_orders.reconciliation_id,
            a.regular_orders.complete,
            "regularOrders",
            a.regular_orders.rows.len(),
        ),
        (
            &a.algo_orders.source,
            &a.algo_orders.reconciliation_id,
            a.algo_orders.complete,
            "algoOrders",
            a.algo_orders.rows.len(),
        ),
    ] {
        let x = &e[name];
        assert_eq!(
            source.as_str(),
            x["source"].as_str().unwrap(),
            "MODE native declaration snapshot source"
        );
        assert_eq!(
            id.as_str(),
            x["reconciliationId"].as_str().unwrap(),
            "MODE native declaration snapshot ID"
        );
        assert_eq!(
            complete,
            x["complete"].as_bool().unwrap(),
            "MODE native declaration complete literal"
        );
        assert_eq!(
            len,
            x["rows"].as_array().unwrap().len(),
            "MODE native declaration row count"
        );
    }
    for (p, x) in a
        .positions
        .rows
        .iter()
        .zip(e["positions"]["rows"].as_array().unwrap())
    {
        assert_eq!(
            p.symbol.as_str(),
            x["symbol"].as_str().unwrap(),
            "MODE native position identity"
        );
        assert_eq!(p.holding.as_str(), x["holding"].as_str().unwrap());
        assert_eq!(
            p.pending_long_qty.as_str(),
            x["pendingLongQty"].as_str().unwrap()
        );
        assert_eq!(
            p.pending_short_qty.as_str(),
            x["pendingShortQty"].as_str().unwrap()
        );
        number(p.adl_quantile, &x["adlQuantile"], "position adl");
        number(p.timestamp, &x["timestamp"], "position timestamp");
    }
    for (p, x) in a
        .regular_orders
        .rows
        .iter()
        .zip(e["regularOrders"]["rows"].as_array().unwrap())
    {
        assert_eq!(
            p.symbol.as_str(),
            x["symbol"].as_str().unwrap(),
            "MODE native regular identity"
        );
        assert_eq!(p.order_tag.as_str(), x["orderTag"].as_str().unwrap());
        number(p.order_id, &x["orderId"], "regular order ID");
        number(p.client_order_id, &x["clientOrderId"], "regular client ID");
        number(p.created_time, &x["createdTime"], "regular created time");
        number(p.total_rebate, &x["totalRebate"], "regular rebate");
        optional_number(p.leverage, x, "leverage");
        optional_number(p.bid_ask_level, x, "bidAskLevel");
    }
    for (p, x) in a
        .algo_orders
        .rows
        .iter()
        .zip(e["algoOrders"]["rows"].as_array().unwrap())
    {
        algo(p, x);
    }
}
fn algo(a: &WooV3AlgoOrder, e: &Value) {
    assert_eq!(
        a.symbol.as_str(),
        e["symbol"].as_str().unwrap(),
        "MODE native algo identity"
    );
    assert_eq!(
        a.algo_order_tag.as_str(),
        e["algoOrderTag"].as_str().unwrap()
    );
    for (v, k) in [
        (a.algo_order_id, "algoOrderId"),
        (a.client_algo_order_id, "clientAlgoOrderId"),
        (a.root_algo_order_id, "rootAlgoOrderId"),
        (a.parent_algo_order_id, "parentAlgoOrderId"),
        (a.created_time, "createdTime"),
        (a.updated_time, "updatedTime"),
        (a.total_rebate, "totalRebate"),
    ] {
        number(v, &e[k], k);
    }
    match &a.quantity {
        WooV3GatewayAlgoQuantity::Number(v) => number(*v, &e["quantity"], "algo quantity"),
        WooV3GatewayAlgoQuantity::Decimal(v) => assert_eq!(
            v.as_str(),
            e["quantity"].as_str().unwrap(),
            "MODE native decimal quantity variant"
        ),
    };
    optional_number(a.leverage, e, "leverage");
    let rows = e["childOrders"].as_array().unwrap();
    assert_eq!(a.child_orders.len(), rows.len(), "MODE native child count");
    for (a, e) in a.child_orders.iter().zip(rows) {
        algo(a, e);
    }
}
#[test]
fn native_mode_declarations() {
    let s = source();
    let rows = s["declarations"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "MODE declaration floor");
    for e in rows {
        let v = input(e).unwrap();
        let x = &e["expected"];
        if e["type"] == "evidence" {
            let a = v
                .decode::<WooV3CompletePositionModeEvidence>()
                .expect("MODE native evidence decode");
            evidence(&a, x);
        } else {
            let a = v
                .decode::<WooV3SetPositionModeInput>()
                .expect("MODE native input decode");
            assert_eq!(
                mode(a.target_position_mode),
                x["targetPositionMode"].as_str().unwrap(),
                "MODE native input target mode"
            );
            evidence(&a.evidence, &x["evidence"]);
        }
    }
    let rows = s["refusals"].as_array().unwrap();
    assert_eq!(rows.len(), 11, "MODE declaration refusal floor");
    for e in rows {
        let v = input(e).unwrap();
        let bad = if e["type"] == "evidence" {
            v.decode::<WooV3CompletePositionModeEvidence>().is_err()
        } else {
            v.decode::<WooV3SetPositionModeInput>().is_err()
        };
        assert!(bad, "MODE native evidence declaration refusal");
    }
}
#[test]
fn native_mode_responses() {
    let s = source();
    let rows = s["responses"].as_array().unwrap();
    assert_eq!(rows.len(), 16, "MODE response floor");
    let base = input(&s["builds"][0]).unwrap();
    let WooV3SetPositionModeBuildResult::Ready { request, .. } =
        build_woo_v3_set_position_mode_request(Some(&base)).unwrap()
    else {
        panic!("MODE native ready request")
    };
    for e in rows {
        let mut req = request.clone();
        if let Some(o) = e.get("overrides") {
            if o.get("method").is_some() {
                req.method = WooV3HttpMethod::Get;
            }
            if o.get("path").is_some() {
                req.path = WooV3Endpoint::FuturesLeverage;
            }
        }
        let target = if e["target"] == "ONE_WAY" {
            WooV3AccountPositionMode::OneWay
        } else {
            WooV3AccountPositionMode::HedgeMode
        };
        let r = decode_woo_v3_set_position_mode_response(input(e).as_ref(), &req, target);
        if let Some(x) = e.get("error") {
            match r.expect_err("MODE native response refusal") {
                WooV3AccountWriteError::Validation(m) => {
                    assert_eq!(x["name"], "Error");
                    assert_eq!(
                        m.as_str(),
                        x["message"].as_str().unwrap(),
                        "MODE native response first error"
                    );
                }
                WooV3AccountWriteError::Gateway(g) => {
                    assert_eq!(
                        g.name.as_str(),
                        x["name"].as_str().unwrap(),
                        "MODE native gateway name"
                    );
                    assert_eq!(
                        g.message.as_str(),
                        x["message"].as_str().unwrap(),
                        "MODE native gateway message"
                    );
                    assert_eq!(
                        g.contract_status.as_str(),
                        x["contractStatus"].as_str().unwrap(),
                        "MODE native gateway contract"
                    );
                    optional_number(g.code, x, "code");
                    map(&g.gateway_response, &x["gatewayResponse"]);
                }
            }
            continue;
        }
        let a = r.expect("MODE native response success");
        let x = &e["ok"];
        assert_eq!(
            a.accepted,
            x["accepted"].as_bool().unwrap(),
            "MODE native accepted"
        );
        number(a.timestamp, &x["timestamp"], "response timestamp");
        let WooV3AccountMutationTarget::PositionMode { position_mode } = a.target else {
            panic!("MODE native mutation target variant")
        };
        assert_eq!(x["target"]["family"], "position-mode");
        assert_eq!(
            mode(position_mode),
            x["target"]["positionMode"].as_str().unwrap(),
            "MODE native response target mode"
        );
        assert_eq!(
            a.state_evidence.as_str(),
            x["stateEvidence"].as_str().unwrap(),
            "MODE native response evidence"
        );
    }
}
