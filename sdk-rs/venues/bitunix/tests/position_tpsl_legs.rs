//! AGT-71: Bitunix holds ONE position TP/SL order per position, carrying both legs under one id
//! ("Each position can only have one Position TP/SL Order"). Placing one leg without the other
//! replaces the order, so the existing leg must ride along. The REST pending list carries no
//! `type`, so a refresh must not forget that a known id is the position TP/SL.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, SocketId};
use support::*;

const NO_TPSL: &str = r#"{"code":0,"data":[]}"#;
const SL_ONLY: &str = r#"{"code":0,"data":[{"symbol":"BTCUSDT","id":"pos-tpsl","positionId":"position-1","slPrice":"100000","slQty":"0.01","slStopType":"MARK_PRICE"}]}"#;
const BOTH: &str = r#"{"code":0,"data":[{"symbol":"BTCUSDT","id":"pos-tpsl","positionId":"position-1","tpPrice":"90000","slPrice":"100000","tpQty":"0.01","slQty":"0.01","tpStopType":"MARK_PRICE","slStopType":"MARK_PRICE"}]}"#;
const WS_SL: &str = r#"{"ch":"tpsl","data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"pos-tpsl","positionId":"position-1","side":"Sell","positionMode":"HEDGE","status":"NEW","type":"POSITION_TPSL","slStopType":"MARK_PRICE","slPrice":"100000"}}"#;

fn protection(kind: OrderType, price: f64) -> PlaceOrderOpts {
    let mut o: PlaceOrderOpts = serde_json::from_str(&format!(
        r#"{{"symbol":"BTCUSDT","type":"limit","side":"buy","price":{price},"amount":0}}"#
    ))
    .unwrap();
    o.kind = kind;
    o
}

/// Answers every read with `tpsl` for the TP/SL list and returns the writes sent.
fn drive(e: &mut TestEngine, mut actions: Vec<Action>, tpsl: &str) -> Vec<HttpRequest> {
    let mut sent = Vec::new();
    for _ in 0..50 {
        let reads: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) if r.method == Method::Get => Some(r.clone()),
                _ => None,
            })
            .collect();
        sent.extend(actions.iter().filter_map(|a| match a {
            Action::Http(r) if r.method != Method::Get => Some(r.clone()),
            _ => None,
        }));
        if reads.is_empty() {
            break;
        }
        actions = Vec::new();
        for r in reads {
            let answer = if r.url.contains("/tpsl/") {
                tpsl
            } else if r.url.contains("get_pending_positions") {
                POSITIONS
            } else if r.url.contains("get_pending_orders") {
                r#"{"code":0,"data":{"orderList":[]}}"#
            } else {
                ACCOUNT
            };
            reply(e, r.id, answer);
            actions.extend(prepared_now(e));
        }
    }
    sent
}

fn place(e: &mut TestEngine, command: u32, o: PlaceOrderOpts, tpsl: &str) -> HttpRequest {
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command,
            orders: vec![o],
        }),
    );
    let actions = prepared_now(e);
    drive(e, actions, tpsl)
        .into_iter()
        .find(|r| r.url.contains("/tpsl/position/place_order"))
        .expect("a position TP/SL write")
}

/// A full SL is placed and acknowledged; `ws` says whether its POSITION_TPSL push arrives.
fn with_full_stop(ws: bool) -> (TestEngine, SocketId) {
    let (mut e, socket) = startup();
    let actions = prepared_now(&mut e);
    drive(&mut e, actions, NO_TPSL);
    let write = place(&mut e, 2, protection(OrderType::StopLoss, 100000.0), NO_TPSL);
    reply(&mut e, write.id, r#"{"code":0,"data":{"orderId":"pos-tpsl"}}"#);
    let actions = prepared_now(&mut e);
    drive(&mut e, actions, SL_ONLY);
    if ws {
        event(&mut e, socket, WS_SL);
        let actions = prepared_now(&mut e);
        drive(&mut e, actions, SL_ONLY);
    }
    (e, socket)
}

#[test]
fn adding_a_full_tp_keeps_the_full_stop_after_a_typeless_rest_refresh() {
    for ws in [true, false] {
        let (mut e, _) = with_full_stop(ws);
        let write = place(&mut e, 3, protection(OrderType::TakeProfit, 90000.0), SL_ONLY);
        let p: serde_json::Value = serde_json::from_str(write.body.as_deref().unwrap()).unwrap();
        assert_eq!(p["tpPrice"], "90000", "ws={ws} {p}");
        assert_eq!(p["slPrice"], "100000", "ws={ws}: the stop must ride along {p}");
        assert_eq!(p["positionId"], "position-1");
    }
}

#[test]
fn a_position_tpsl_with_both_legs_lists_two_orders() {
    let (mut e, _) = with_full_stop(false);
    let write = place(&mut e, 3, protection(OrderType::TakeProfit, 90000.0), SL_ONLY);
    reply(&mut e, write.id, r#"{"code":0,"data":{"orderId":"pos-tpsl"}}"#);
    let actions = prepared_now(&mut e);
    drive(&mut e, actions, BOTH);
    let legs: Vec<_> = e
        .store()
        .orders()
        .iter()
        .filter(|o| o.status == OrderStatus::Open && o.id.starts_with("pos-tpsl"))
        .map(|o| (o.id.clone(), o.kind, o.price))
        .collect();
    assert_eq!(legs.len(), 2, "{legs:?}");
    assert!(legs.iter().any(|(_, k, p)| *k == OrderType::StopLoss && *p == 100000.0), "{legs:?}");
    assert!(legs.iter().any(|(_, k, p)| *k == OrderType::TakeProfit && *p == 90000.0), "{legs:?}");
    assert_ne!(legs[0].0, legs[1].0);
}
