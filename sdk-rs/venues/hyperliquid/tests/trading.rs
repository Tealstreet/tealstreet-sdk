//! Command contract cases derived from the official exchange endpoint shapes.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice};
use safe_cex_venue_hyperliquid::Hyperliquid;
use serde_json::Value;
use support::*;
const CLOID: &str = "0x00000000000000000000000000000001";
fn opts(text: &str) -> PlaceOrderOpts {
    serde_json::from_str(text).unwrap()
}
fn write(e: &mut Engine<Hyperliquid>) -> HttpRequest {
    e.drain_actions()
        .find_map(|a| {
            if let Action::Http(r) = a {
                (r.url.ends_with("/exchange")).then_some(r)
            } else {
                None
            }
        })
        .unwrap()
}
fn body(r: &HttpRequest) -> Value {
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}
#[test]
fn bracket_honors_caller_id_and_mints_unique_leg_ids() {
    let (mut e, _) = ready("disabled");
    e.handle(2000.0,Event::Command(Command::PlaceOrders{command:1,orders:vec![opts(&format!(r#"{{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":2,"stopLoss":80,"takeProfit":120,"clientOrderId":"{CLOID}"}}"#))]}));
    let r = write(&mut e);
    let b = body(&r);
    let a = &b["action"];
    assert_eq!(a["grouping"], "normalTpsl");
    let wires = a["orders"].as_array().unwrap();
    assert_eq!(wires.len(), 3);
    assert_eq!(wires[0]["c"], CLOID);
    assert_ne!(wires[1]["c"], wires[0]["c"]);
    assert_ne!(wires[2]["c"], wires[1]["c"]);
    assert_eq!(a["builder"]["f"], 10);
    assert_eq!(
        a["builder"]["b"],
        "0xea629e1e8534b82f704a818643f2daf38cb7fd4d"
    );
    let raw = r.body.unwrap();
    assert!(raw.find("\"t\"").unwrap() < raw.find("\"c\"").unwrap());
}
#[test]
fn invalid_or_duplicate_caller_id_rejects_entire_batch_before_a_write() {
    for id in ["0x0000000000000000000000000000000F", CLOID] {
        let (mut e, _) = ready("disabled");
        let make = |cloid: &str| {
            opts(&format!(
                r#"{{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1,"clientOrderId":"{cloid}"}}"#
            ))
        };
        e.handle(
            2000.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![make(CLOID), make(id)],
            }),
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { result: Err(_), .. })
        )));
    }
}
#[test]
fn full_position_stop_accepts_waiting_without_oid() {
    let (mut e, _) = ready("disabled");
    e.handle(2000.0,Event::Command(Command::PlaceOrders{command:1,orders:vec![opts(&format!(r#"{{"symbol":"BTC-PERP","type":"stop_market","side":"sell","price":80,"amount":2,"reduceOnly":true,"clientOrderId":"{CLOID}"}}"#))]}));
    let r = write(&mut e);
    let b = body(&r);
    assert_eq!(b["action"]["grouping"], "positionTpsl");
    assert_eq!(b["action"]["orders"][0]["s"], "0");
    assert_eq!(b["action"]["orders"][0]["p"], "95");
    reply(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}}"#,
    );
    assert!(e.drain_actions().any(
        |a| matches!(a,Action::Notice(Notice::CommandDone{result:Ok(ref ids),..})if ids=="[]")
    ));
}
#[test]
fn own_bracket_leg_is_inferred_from_cloid_before_rest_returns() {
    let (mut e, s) = ready("disabled");
    e.handle(2000.0,Event::Command(Command::PlaceOrders{command:1,orders:vec![opts(r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":2,"stopLoss":80}"#)]}));
    let r = write(&mut e);
    let b = body(&r);
    let c = b["action"]["orders"][1]["c"].as_str().unwrap();
    message(
        &mut e,
        s,
        2001.0,
        &format!(
            r#"{{"channel":"orderUpdates","data":[{{"order":{{"coin":"BTC","oid":9,"cloid":"{c}","side":"A","limitPx":"90","origSz":"2","sz":"2","reduceOnly":true}},"status":"open"}}]}}"#
        ),
    );
    let order = e.store().order("9").unwrap();
    assert_eq!(order.kind, safe_cex_engine::model::OrderType::StopLoss);
    assert_eq!(order.price, 80.0);
    assert_eq!(order.client_order_id.as_deref(), Some(c));
}
#[test]
fn lost_placement_response_reconciles_and_never_resends_write() {
    let (mut e, _) = ready("disabled");
    e.handle(
        2000.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts(
                r#"{"symbol":"BTC-PERP","type":"market","side":"buy","amount":1}"#,
            )],
        }),
    );
    let r = write(&mut e);
    assert_eq!(body(&r)["action"]["orders"][0]["p"], "105");
    e.handle(
        2001.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
    );
    assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
}
#[test]
fn amend_cancels_then_replaces_with_fresh_cloid_and_tp_to_sl_only() {
    let (mut e, s) = ready("disabled");
    message(
        &mut e,
        s,
        2000.0,
        &format!(
            r#"{{"channel":"orderUpdates","data":[{{"order":{{"coin":"BTC","oid":9,"cloid":"{CLOID}","side":"A","limitPx":"95","origSz":"2","sz":"2","reduceOnly":true,"orderType":"Take Profit Market","triggerPx":"120"}},"status":"open"}}]}}"#
        ),
    );
    e.handle(
        2001.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "9".into(),
            price: Some(80.0),
            amount: None,
        }),
    );
    let r = write(&mut e);
    assert_eq!(body(&r)["action"]["type"], "cancel");
    reply(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(2500.0, Event::TimerFired { timer });
    let r = write(&mut e);
    let b = body(&r);
    let wire = &b["action"]["orders"][0];
    assert_ne!(wire["c"], CLOID);
    assert_eq!(wire["t"]["trigger"]["tpsl"], "sl");
    assert_eq!(wire["t"]["trigger"]["triggerPx"], "80");
    reply(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}}"#,
    );
    let result = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { result: Ok(s), .. }) = a {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    let result: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(result["clientOrderId"], wire["c"]);
    assert_eq!(result["orderIds"], serde_json::json!([]));
}
#[test]
fn malformed_cancel_ack_stops_replacement_and_keeps_original() {
    let (mut e, s) = ready("disabled");
    message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"#,
    );
    e.handle(
        2001.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "9".into(),
            price: Some(85.0),
            amount: None,
        }),
    );
    let r = write(&mut e);
    reply(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"default"}}"#,
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
    );
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
    assert_eq!(
        e.store().order("9").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
}
#[test]
fn queued_leverage_uses_margin_mode_from_preceding_command() {
    let (mut e, _) = ready("disabled");
    e.handle(
        2000.0,
        Event::Command(Command::SetMarginType {
            command: 1,
            symbol: "BTC-PERP".into(),
            isolated: true,
        }),
    );
    let first = write(&mut e);
    e.handle(
        2001.0,
        Event::Command(Command::SetLeverage {
            command: 2,
            symbol: "BTC-PERP".into(),
            leverage: 5.0,
        }),
    );
    reply(
        &mut e,
        &first,
        200,
        r#"{"status":"ok","response":{"type":"default"}}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(2500.0, Event::TimerFired { timer });
    let second = write(&mut e);
    let b = body(&second);
    assert_eq!(b["action"]["isCross"], false);
    assert_eq!(b["action"]["leverage"], 5);
    assert!(!second.body.as_ref().unwrap().contains("5.0"));
}
#[test]
fn configured_builder_pair_requires_the_configured_approval_rate() {
    for (fee, expected) in [(30, true), (50, false)] {
        let (mut e, _) = ready_custom(
            "disabled",
            false,
            Some(safe_cex_venue_hyperliquid::Builder {
                b: "0x95d699952Fd9B110ef1218369def8538CCbB094B".into(),
                f: fee,
            }),
            "30",
        );
        e.handle(
            2000.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![opts(
                    r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1}"#,
                )],
            }),
        );
        let r = write(&mut e);
        let b = body(&r);
        assert_eq!(!b["action"]["builder"].is_null(), expected);
        if expected {
            assert_eq!(b["action"]["builder"]["f"], fee);
        }
    }
}
