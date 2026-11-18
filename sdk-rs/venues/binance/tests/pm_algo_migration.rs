//! PM Algo Service REST and stream contracts; no live transport.
mod support;
use safe_cex_engine::model::{OrderType, PlaceOrderOpts};
use safe_cex_engine::{Command, Event, Method};
use support::{Harness, Options};

const ALGO: &str = r#"[{"algoId":123,"clientAlgoId":"stop","symbol":"BTCUSDT","orderType":"STOP_MARKET","side":"SELL","quantity":"0.01","triggerPrice":"90","price":"0","reduceOnly":true,"workingType":"MARK_PRICE","closePosition":false}]"#;
const LEGACY: &str = r#"[{"strategyId":123,"newClientStrategyId":"stop","symbol":"BTCUSDT","strategyType":"STOP_MARKET","side":"SELL","origQty":"0.01","stopPrice":"90","price":"0","reduceOnly":true,"workingType":"MARK_PRICE"}]"#;
fn starting() -> Harness {
    starting_with(false)
}
fn starting_with(deferred: bool) -> Harness {
    let options = Options {
        portfolio_margin: true,
        key: Some("fixture-key".into()),
        secret: Some("fixture-secret".into()),
        ..Options::default()
    };
    let mut h = if deferred {
        Harness::started_deferred(options)
    } else {
        Harness::started(options)
    };
    h.load_public();
    h.respond("/papi/v1/listenKey", 200, r#"{"listenKey":"fixture"}"#);
    h.respond(
        "/papi/v1/um/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/papi/v1/um/positionRisk",200,r#"[{"entryPrice":"100","leverage":"20","markPrice":"100","positionAmt":"0.01","notional":"1","symbol":"BTCUSDT","unRealizedProfit":"0","liquidationPrice":"50","positionSide":"BOTH"}]"#);
    h.respond("/papi/v1/balance",200,r#"[{"asset":"USDT","totalWalletBalance":"1000","crossMarginFree":"900","umUnrealizedPNL":"0"}]"#);
    h
}
fn ready() -> Harness {
    ready_with(false)
}
fn ready_with(deferred: bool) -> Harness {
    let mut h = starting_with(deferred);
    h.respond("/papi/v1/um/openOrders", 200, "[]");
    // Let baseline probes reach the command too; assertions below require new wire.
    if h.has_request("/papi/v1/um/algo/openAlgoOrders") {
        h.respond("/papi/v1/um/algo/openAlgoOrders", 200, ALGO);
    } else {
        h.respond("/papi/v1/um/conditional/openOrders", 200, LEGACY);
    }
    let socket = h.socket("/pm/ws").unwrap();
    h.event(Event::WsOpened { socket });
    h.answer_open_position_read();
    h.requests.clear();
    h
}
fn amend(h: &mut Harness) {
    h.event(Event::Command(Command::AmendOrder {
        command: 7,
        id: "stop".into(),
        price: Some(85.),
        amount: None,
    }));
}
fn cancel(h: &mut Harness) -> safe_cex_engine::HttpRequest {
    let i = h
        .requests
        .iter()
        .position(|r| r.method == Method::Delete)
        .expect("cancel");
    h.requests.remove(i)
}
fn posts(h: &Harness) -> usize {
    h.requests
        .iter()
        .filter(|r| r.method == Method::Post && r.url.contains("/papi/v1/um/"))
        .count()
}
fn proof(status: &str) -> serde_json::Value {
    serde_json::json!({"algoId":123,"clientAlgoId":"stop","symbol":"BTCUSDT","algoStatus":status,"actualOrderId":"","actualPrice":"0.00000","triggerTime":0})
}
fn order(kind: OrderType) -> PlaceOrderOpts {
    serde_json::from_value(serde_json::json!({"symbol":"BTCUSDT","type":kind,"side":"buy","amount":0.01,"price":100,"clientOrderId":"ordinary"})).unwrap()
}

#[test]
fn open_snapshot_uses_algo_endpoint_and_algo_fields() {
    let mut h = starting();
    h.respond("/papi/v1/um/openOrders", 200, "[]");
    if h.has_request("/papi/v1/um/conditional/openOrders") {
        // The retired route returns an object, which used to block the whole snapshot.
        h.respond(
            "/papi/v1/um/conditional/openOrders",
            200,
            r#"{"code":-1000,"msg":"retired endpoint"}"#,
        );
    } else {
        h.respond("/papi/v1/um/algo/openAlgoOrders", 200, ALGO);
    }
    assert!(
        h.engine.store().loaded().orders,
        "a retired-route object cannot block PM's complete order snapshot"
    );
    let row = h.engine.store().order("stop").unwrap();
    assert_eq!(
        (row.amount, row.price, row.kind),
        (0.01, 90., OrderType::StopLoss)
    );
}
#[test]
fn pm_ao_events_bind_the_client_and_native_algo_without_fabricating_fills() {
    let mut h = ready();
    h.message("/pm/ws",r#"{"e":"ALGO_UPDATE","fs":"UM","ao":{"caid":"stream-stop","aid":456,"s":"BTCUSDT","S":"SELL","o":"STOP_MARKET","X":"NEW","q":"0.01","tp":"90","R":true,"wt":"MARK_PRICE","aq":"0"}}"#);
    assert!(h.engine.store().order("stream-stop").is_some());
    h.event(Event::Command(Command::CancelOrders {
        command: 8,
        ids: vec!["stream-stop".into()],
    }));
    let request = cancel(&mut h);
    assert!(request.url.contains("/papi/v1/um/algo/order?"));
    assert!(request.url.contains("algoId=456"));
}
#[test]
fn complete_ack_alone_never_replaces_then_exact_never_triggered_proof_does() {
    let mut h = ready();
    amend(&mut h);
    let request = cancel(&mut h);
    assert!(request.url.contains("/papi/v1/um/algo/order?"));
    assert!(request.url.contains("algoId=123"));
    h.reply(request.id, 200, "", r#"{"complete":true}"#);
    assert_eq!(posts(&h), 0);
    let query = h.take("/papi/v1/um/algo/algoOrder");
    assert_eq!(query.method, Method::Get);
    assert!(query.url.contains("algoId=123"));
    h.reply(query.id, 200, "", &proof("CANCELED").to_string());
    let replacement = h.take("/papi/v1/um/algo/order");
    assert_eq!(replacement.method, Method::Post);
    assert!(replacement.url.contains("algoType=CONDITIONAL"));
    assert!(replacement.url.contains("type=STOP_MARKET"));
    assert!(replacement.url.contains("triggerPrice=85"));
    assert!(!replacement.url.contains("strategy"));
    assert!(!replacement.url.contains("stopPrice"));
}
#[test]
fn incomplete_or_foreign_cancel_proof_is_unknown_and_never_creates() {
    for mutation in [
        "pending",
        "native",
        "client",
        "symbol",
        "child",
        "price",
        "time",
        "missing-child",
        "missing-price",
        "missing-time",
        "nan",
    ] {
        let mut h = ready();
        amend(&mut h);
        let request = cancel(&mut h);
        h.reply(request.id, 200, "", r#"{"complete":true}"#);
        let query = h.take("/papi/v1/um/algo/algoOrder");
        let mut p = proof("CANCELED");
        match mutation {
            "pending" => p["algoStatus"] = serde_json::json!("NEW"),
            "native" => p["algoId"] = serde_json::json!(124),
            "client" => p["clientAlgoId"] = serde_json::json!("foreign"),
            "symbol" => p["symbol"] = serde_json::json!("ETHUSDT"),
            "child" => p["actualOrderId"] = serde_json::json!("789"),
            "price" => p["actualPrice"] = serde_json::json!("1"),
            "time" => p["triggerTime"] = serde_json::json!(1),
            "missing-child" | "missing-price" | "missing-time" => {
                p.as_object_mut().unwrap().remove(match mutation {
                    "missing-child" => "actualOrderId",
                    "missing-price" => "actualPrice",
                    _ => "triggerTime",
                });
            }
            "nan" => p["actualPrice"] = serde_json::json!("NaN"),
            _ => unreachable!(),
        };
        h.reply(query.id, 200, "", &p.to_string());
        assert_eq!(posts(&h), 0, "{mutation}");
        assert_eq!(
            h.failures
                .iter()
                .find(|(id, _)| *id == 7)
                .unwrap()
                .1
                .outcome,
            Some(safe_cex_engine::error::Outcome::Unknown),
            "{mutation}"
        );
    }
}
#[test]
fn ordinary_pm_limit_and_market_wire_match_master() {
    let mut actual = Vec::new();
    for kind in [OrderType::Limit, OrderType::Market] {
        let mut h = ready();
        h.event(Event::Command(Command::PlaceOrders {
            command: 9,
            orders: vec![order(kind)],
        }));
        let r = h.take("/papi/v1/um/order");
        actual.push(serde_json::json!({"url":r.url,"body":r.body,"headers":r.headers}));
    }
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/pm-ordinary-master.json")).unwrap();
    assert_eq!(serde_json::json!(actual), expected);
}

fn assert_unknown_no_create(h: &Harness, case: &str) {
    assert_eq!(posts(h), 0, "{case}");
    assert_eq!(
        h.failures
            .iter()
            .find(|(id, _)| *id == 7)
            .expect(case)
            .1
            .outcome,
        Some(safe_cex_engine::Outcome::Unknown),
        "{case}"
    );
    assert!(!matches!(h.result(7), Some(Ok(_))), "{case}");
}
#[test]
fn cancel_verification_http_transport_and_timeout_failures_stay_unknown() {
    for case in [
        "429",
        "500",
        "malformed",
        "network",
        "timeout",
        "sign-timeout",
    ] {
        let mut h = ready_with(case == "sign-timeout");
        amend(&mut h);
        let cancel = cancel(&mut h);
        if case == "sign-timeout" {
            h.hold_signatures(true);
        }
        h.reply(cancel.id, 200, "", r#"{"complete":true}"#);
        if case == "sign-timeout" {
            h.advance(20_000.);
        } else {
            let query = h.take("/papi/v1/um/algo/algoOrder");
            match case {
                "429" => h.reply(
                    query.id,
                    429,
                    "",
                    r#"{"code":-1003,"msg":"Too many requests"}"#,
                ),
                "500" => h.reply(query.id, 500, "", "server error"),
                "malformed" => h.reply(query.id, 200, "", "{"),
                "network" => h.event(Event::HttpFailed {
                    request: query.id,
                    error: "network failed",
                }),
                "timeout" => h.advance(20_000.),
                _ => unreachable!(),
            }
        }
        assert_unknown_no_create(&h, case);
    }
}
#[test]
fn malformed_or_negative_complete_ack_never_admits_query_or_create() {
    for body in [
        "{}",
        r#"{"complete":false}"#,
        r#"{"complete":"true"}"#,
        "[]",
    ] {
        let mut h = ready();
        amend(&mut h);
        let cancel = cancel(&mut h);
        h.reply(cancel.id, 200, "", body);
        assert_unknown_no_create(&h, body);
        assert!(!h.has_request("/papi/v1/um/algo/algoOrder"));
    }
}
#[test]
fn filled_or_rebound_target_cannot_be_replaced_by_a_late_valid_proof() {
    for rebound in [false, true] {
        let mut h = ready();
        amend(&mut h);
        let cancel = cancel(&mut h);
        h.reply(cancel.id, 200, "", r#"{"complete":true}"#);
        let query = h.take("/papi/v1/um/algo/algoOrder");
        if rebound {
            h.message("/pm/ws", r#"{"e":"ALGO_UPDATE","fs":"UM","ao":{"caid":"stop","aid":999,"s":"BTCUSDT","S":"SELL","o":"STOP_MARKET","X":"NEW","q":"0.01","tp":"90","R":true,"aq":"0"}}"#);
        } else {
            let mut rows = h.engine.store().orders().to_vec();
            rows[0].filled = 0.005;
            h.engine.store_mut().set_orders(rows);
        }
        h.reply(query.id, 200, "", &proof("CANCELED").to_string());
        assert_unknown_no_create(&h, if rebound { "rebound" } else { "filled" });
    }
}

#[test]
fn a_triggered_child_event_is_not_zero_execution_cancel_proof() {
    let mut h = ready();
    amend(&mut h);
    let cancel = cancel(&mut h);
    h.reply(cancel.id, 200, "", r#"{"complete":true}"#);
    let query = h.take("/papi/v1/um/algo/algoOrder");
    h.message("/pm/ws", r#"{"e":"ALGO_UPDATE","fs":"UM","ao":{"caid":"stop","aid":123,"s":"BTCUSDT","S":"SELL","o":"STOP_MARKET","X":"CANCELED","q":"0.01","tp":"90","R":true,"aq":"0.005"}}"#);
    assert_eq!(
        h.engine.store().order("stop").unwrap().filled,
        0.0,
        "no invented parent fills"
    );
    h.reply(query.id, 200, "", &proof("CANCELED").to_string());
    assert_unknown_no_create(&h, "contradictory child execution");
}

#[test]
fn pm_trailing_callback_rate_is_validated_before_cancel_and_can_be_put_back_exactly() {
    for original_rate in ["1.5", "11"] {
        for target in [90.0, 89.0, 99.9] {
            let mut h = starting();
            h.respond("/papi/v1/um/openOrders", 200, "[]");
            h.respond("/papi/v1/um/algo/openAlgoOrders", 200, &format!(r#"[{{"algoId":124,"clientAlgoId":"trail","symbol":"BTCUSDT","orderType":"TRAILING_STOP_MARKET","side":"SELL","quantity":"0.01","price":"0","reduceOnly":true,"callbackRate":"{original_rate}","activatePrice":"103"}}]"#));
            let socket = h.socket("/pm/ws").unwrap();
            h.event(Event::WsOpened { socket });
            if h.has_request("/papi/v1/um/positionRisk") {
                h.respond("/papi/v1/um/positionRisk", 200, r#"[{"entryPrice":"100","leverage":"20","markPrice":"100","positionAmt":"0.01","notional":"1","symbol":"BTCUSDT","unRealizedProfit":"0","liquidationPrice":"50","positionSide":"BOTH"}]"#);
            }
            h.requests.clear();
            h.event(Event::Command(Command::AmendOrder {
                command: 10,
                id: "trail".into(),
                price: Some(target),
                amount: None,
            }));
            let allowed = original_rate == "1.5" && target != 89.0;
            assert_eq!(
                h.requests.iter().any(|r| r.method == Method::Delete),
                allowed,
                "rate={original_rate} target={target}: {:?}",
                h.done
            );
            if !allowed {
                assert!(h.requests.iter().all(|r| r.method == Method::Get));
                assert!(h.result(10).unwrap().is_err());
            }
        }
    }
}

#[test]
fn verification_cannot_start_when_its_window_would_expire_with_the_mutation_root() {
    let mut h = starting();
    h.respond("/papi/v1/um/openOrders", 200, "[]");
    let rows = (0..4)
        .map(|i| {
            serde_json::json!({
                "algoId":123+i,"clientAlgoId":format!("stop{i}"),"symbol":"BTCUSDT",
                "orderType":"STOP_MARKET","side":"SELL","quantity":"0.01","triggerPrice":"90",
            })
        })
        .collect::<Vec<_>>();
    h.respond(
        "/papi/v1/um/algo/openAlgoOrders",
        200,
        &serde_json::to_string(&rows).unwrap(),
    );
    let socket = h.socket("/pm/ws").unwrap();
    h.event(Event::WsOpened { socket });
    h.answer_open_position_read();
    h.requests.clear();
    h.event(Event::Command(Command::CancelOrders {
        command: 7,
        ids: (0..4).map(|i| format!("stop{i}")).collect(),
    }));
    for i in 0..4 {
        let cancel = cancel(&mut h);
        h.advance(16_000.);
        h.reply(cancel.id, 200, "", r#"{"complete":true}"#);
        if i < 3 {
            let query = h.take("/papi/v1/um/algo/algoOrder");
            h.advance(16_000.);
            let mut p = proof("CANCELED");
            p["algoId"] = serde_json::json!(123 + i);
            p["clientAlgoId"] = serde_json::json!(format!("stop{i}"));
            h.reply(query.id, 200, "", &p.to_string());
        }
    }
    assert!(!h.has_request("/papi/v1/um/algo/algoOrder"));
    assert_unknown_no_create(&h, "bounded proof read");
}

#[test]
fn pm_uses_only_its_um_ao_stream_shape() {
    let mut h = ready();
    let row = h.engine.store().order("stop").unwrap().clone();
    h.message("/pm/ws", r#"{"e":"ALGO_UPDATE","fs":"CM","ao":{"caid":"stop","aid":123,"s":"BTCUSDT","S":"SELL","o":"STOP_MARKET","X":"CANCELED","aq":"0"}}"#);
    h.message("/pm/ws", r#"{"e":"ALGO_UPDATE","o":{"caid":"stop","aid":123,"s":"BTCUSDT","S":"SELL","o":"STOP_MARKET","X":"CANCELED","aq":"0"}}"#);
    assert_eq!(h.engine.store().order("stop"), Some(&row));
}

#[test]
fn unknown_algo_cancel_is_not_terminal_proof_even_for_a_cancel_only_command() {
    for amending in [false, true] {
        let mut h = ready();
        let row = h.engine.store().order("stop").unwrap().clone();
        if amending {
            amend(&mut h);
        } else {
            h.event(Event::Command(Command::CancelOrders {
                command: 7,
                ids: vec!["stop".into()],
            }));
        }
        let cancel = cancel(&mut h);
        h.reply(
            cancel.id,
            400,
            "",
            r#"{"code":-2011,"msg":"Unknown order sent."}"#,
        );
        assert_unknown_no_create(&h, "unknown cancel");
        assert_eq!(
            h.result(7).unwrap().as_ref().unwrap_err(),
            "Binance algo cancellation could not be confirmed; check Open Orders before re-placing. The outcome is unknown. Check Open Orders"
        );
        assert_eq!(h.engine.store().order("stop"), Some(&row));
        assert!(!h.has_request("/papi/v1/um/algo/algoOrder"));
    }
}
#[test]
fn an_algo_without_captured_native_identity_is_refused_before_cancel() {
    let mut h = starting();
    h.respond("/papi/v1/um/openOrders", 200, "[]");
    let rows = ALGO.replace("\"algoId\":123,", "");
    h.respond("/papi/v1/um/algo/openAlgoOrders", 200, &rows);
    h.requests.clear();
    h.event(Event::Command(Command::CancelOrders {
        command: 7,
        ids: vec!["stop".into()],
    }));
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
    assert_eq!(
        h.failures
            .iter()
            .find(|(id, _)| *id == 7)
            .unwrap()
            .1
            .outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
}

#[test]
fn canceled_stream_requires_explicit_zero_actual_quantity() {
    for quantity in [None, Some("bad"), Some("NaN"), Some("-1"), Some("0")] {
        let mut h = ready();
        amend(&mut h);
        let cancel = cancel(&mut h);
        h.reply(cancel.id, 200, "", r#"{"complete":true}"#);
        let query = h.take("/papi/v1/um/algo/algoOrder");
        let mut event = serde_json::json!({"e":"ALGO_UPDATE","fs":"UM","ao":{
            "caid":"stop","aid":123,"s":"BTCUSDT","S":"SELL","o":"STOP_MARKET",
            "X":"CANCELED","q":"0.01","tp":"90","R":true,
        }});
        if let Some(quantity) = quantity {
            event["ao"]["aq"] = serde_json::json!(quantity);
        }
        h.message("/pm/ws", &event.to_string());
        let row = h.engine.store().order("stop").unwrap();
        assert_eq!(
            row.status,
            if quantity == Some("0") {
                safe_cex_engine::model::OrderStatus::Canceled
            } else {
                safe_cex_engine::model::OrderStatus::Open
            },
            "aq={quantity:?}"
        );
        assert_eq!(row.filled, 0.0);
        assert_eq!(
            posts(&h),
            0,
            "a stream observation never bypasses the targeted proof read"
        );
        h.reply(query.id, 200, "", &proof("NEW").to_string());
        assert_unknown_no_create(&h, "stream alone is not cancellation proof");
    }
}
