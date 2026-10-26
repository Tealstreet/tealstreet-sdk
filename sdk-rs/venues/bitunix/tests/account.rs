//! Documented account/position/order/TPSL envelopes and repository Bitunix contracts.
mod support;
use safe_cex_engine::model::{MarginType, OrderSide, OrderStatus};
use safe_cex_engine::{Action, Event, Notice};
use support::*;

fn observed_fills(e: &mut TestEngine) -> Vec<(u32, f64, f64)> {
    let Some(frame) = e.take_market_frame(100.0) else {
        return vec![];
    };
    let Some((_, payload)) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::FILLS)
    else {
        return vec![];
    };
    let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
    let values = (4 + count * 12).next_multiple_of(8);
    (0..count)
        .map(|i| {
            let side = 4 + count * 8 + i * 4;
            let value = values + i * 16;
            (
                u32::from_le_bytes(payload[side..side + 4].try_into().unwrap()),
                f64::from_le_bytes(payload[value..value + 8].try_into().unwrap()),
                f64::from_le_bytes(payload[value + 8..value + 16].try_into().unwrap()),
            )
        })
        .collect()
}

fn reconcile_order_snapshot(e: &mut TestEngine, at: f64, body: &str) {
    let timer = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if at_ms == at => Some(timer),
            _ => None,
        })
        .next_back()
        .expect("the private event must schedule its reconciliation");
    e.handle(at, Event::TimerFired { timer });
    let reads = requests(e);
    assert_eq!(reads.len(), 2);
    assert!(reads.iter().any(|request| request.url.contains("/tpsl/")));
    for request in reads {
        assert!(request.url.ends_with("get_pending_orders"));
        e.handle(
            at,
            Event::HttpResponse {
                request: request.id,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body: if request.url.contains("/tpsl/") {
                        TPSL
                    } else {
                        body
                    },
                },
            },
        );
    }
}

#[test]
fn reconciliation_cannot_consume_a_market_execution_awaiting_metadata() {
    let (mut e, s) = startup();
    e.handle(
        100.0,
        Event::WsMessage {
            socket: s,
            text: r#"{"ch":"order","data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"market","side":"BUY","type":"MARKET","qty":"2","price":"0","dealAmount":"0"}}"#,
        },
    );
    e.take_frame(100.0);
    e.handle(
        200.0,
        Event::WsMessage {
            socket: s,
            text: r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"market","dealAmount":"1"}}"#,
        },
    );
    assert_eq!(e.store().order("market").unwrap().filled, 1.0);
    assert!(observed_fills(&mut e).is_empty());
    reconcile_order_snapshot(
        &mut e,
        1100.0,
        r#"{"code":0,"data":{"orderList":[{"symbol":"BTCUSDT","orderId":"market","side":"BUY","orderType":"MARKET","qty":"2","price":"0","tradeQty":"1","avgPrice":"125"}]}}"#,
    );
    assert!(observed_fills(&mut e).is_empty());
    let complete = r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"market","dealAmount":"1","averagePrice":"125"}}"#;
    for at in [1201.0, 1202.0] {
        e.handle(
            at,
            Event::WsMessage {
                socket: s,
                text: complete,
            },
        );
    }
    assert_eq!(
        observed_fills(&mut e),
        [(OrderSide::Buy as u32, 125.0, 1.0)]
    );
    let row = e.store().order("market").unwrap();
    assert_eq!(
        (row.status, row.filled, row.remaining),
        (OrderStatus::Open, 1.0, 1.0)
    );
}

#[test]
fn ordinary_reconciliation_still_advances_the_execution_baseline() {
    let (mut e, s) = startup();
    e.take_frame(0.0);
    e.handle(
        200.0,
        Event::WsMessage {
            socket: s,
            text: r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one"}}"#,
        },
    );
    reconcile_order_snapshot(
        &mut e,
        1200.0,
        r#"{"code":0,"data":{"orderList":[{"symbol":"BTCUSDT","orderId":"one","side":"SELL","orderType":"LIMIT","qty":"3","price":"100","tradeQty":"2","avgPrice":"150"}]}}"#,
    );
    assert!(observed_fills(&mut e).is_empty());
    let complete = r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"3","averagePrice":"125"}}"#;
    for at in [1201.0, 1202.0] {
        e.handle(
            at,
            Event::WsMessage {
                socket: s,
                text: complete,
            },
        );
    }
    assert_eq!(
        observed_fills(&mut e),
        [(OrderSide::Sell as u32, 75.0, 1.0)]
    );
}

#[test]
fn partial_metadata_recovery_keeps_the_highest_deferred_execution_pending() {
    let (mut e, s) = startup();
    e.take_frame(0.0);
    e.handle(
        100.0,
        Event::WsMessage {
            socket: s,
            text: r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"unknown","dealAmount":"2"}}"#,
        },
    );
    assert!(observed_fills(&mut e).is_empty());
    e.handle(
        200.0,
        Event::WsMessage {
            socket: s,
            text: r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"unknown","dealAmount":"1","side":"BUY","averagePrice":"125"}}"#,
        },
    );
    assert_eq!(
        observed_fills(&mut e),
        [(OrderSide::Buy as u32, 125.0, 1.0)]
    );
    reconcile_order_snapshot(
        &mut e,
        1100.0,
        r#"{"code":0,"data":{"orderList":[{"symbol":"BTCUSDT","orderId":"unknown","side":"BUY","orderType":"MARKET","qty":"3","price":"0","tradeQty":"2","avgPrice":"125"}]}}"#,
    );
    let complete = r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"unknown","dealAmount":"2","averagePrice":"125"}}"#;
    for at in [1201.0, 1202.0] {
        e.handle(
            at,
            Event::WsMessage {
                socket: s,
                text: complete,
            },
        );
    }
    assert_eq!(
        observed_fills(&mut e),
        [(OrderSide::Buy as u32, 125.0, 1.0)]
    );
}

#[test]
fn sparse_buy_updates_and_closes_use_reconciled_parent_execution_fields() {
    for kind in ["UPDATE", "CLOSE"] {
        let (mut e, s) = startup();
        e.store_mut()
            .update_order("one", |row| row.side = OrderSide::Buy);
        e.take_frame(0.0);
        let text = format!(
            r#"{{"ch":"order","data":{{"event":"{kind}","symbol":"BTCUSDT","orderId":"one","dealAmount":"2","orderStatus":"FILLED"}}}}"#
        );
        event(&mut e, s, &text);
        event(&mut e, s, &text);
        assert_eq!(
            observed_fills(&mut e),
            [(OrderSide::Buy as u32, 100.0, 1.0)],
            "{kind}",
        );
        let row = e.store().order("one").unwrap();
        assert_eq!(
            (row.side, row.price, row.filled),
            (OrderSide::Buy, 100.0, 2.0)
        );
        assert_eq!(
            row.status,
            if kind == "CLOSE" {
                OrderStatus::Closed
            } else {
                OrderStatus::Open
            }
        );
    }
}

#[test]
fn explicit_execution_price_and_cumulative_average_override_parent_fallback() {
    let (mut e, s) = startup();
    e.store_mut()
        .update_order("one", |row| row.side = OrderSide::Buy);
    e.take_frame(0.0);
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"2","price":"112"}}"#,
    );
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","dealAmount":"3","averagePrice":"110","price":"999","orderStatus":"FILLED"}}"#,
    );
    assert_eq!(
        observed_fills(&mut e),
        [
            (OrderSide::Buy as u32, 112.0, 1.0),
            (OrderSide::Buy as u32, 118.0, 1.0),
        ]
    );
}

#[test]
fn late_sparse_buy_executions_advance_terminals_once_without_rewinding_state() {
    let (mut e, s) = startup();
    e.store_mut()
        .update_order("one", |row| row.side = OrderSide::Buy);
    e.take_frame(0.0);
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","dealAmount":"1","orderStatus":"CANCELED","mtime":"2025-04-05T16:05:42Z"}}"#,
    );
    let updated = e.store().order("one").unwrap().last_update;
    let update = r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"2","mtime":"2025-04-05T16:05:41Z"}}"#;
    event(&mut e, s, update);
    event(&mut e, s, update);
    assert_eq!(
        e.store().order("one").unwrap().status,
        OrderStatus::Canceled
    );
    let close = r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","dealAmount":"3","averagePrice":"110","orderStatus":"FILLED","mtime":"2025-04-05T16:05:41Z"}}"#;
    event(&mut e, s, close);
    event(&mut e, s, close);
    assert_eq!(
        observed_fills(&mut e),
        [
            (OrderSide::Buy as u32, 100.0, 1.0),
            (OrderSide::Buy as u32, 130.0, 1.0),
        ]
    );
    let row = e.store().order("one").unwrap();
    assert_eq!(
        (row.status, row.filled, row.remaining),
        (OrderStatus::Closed, 3.0, 0.0)
    );
    assert_eq!((row.price, row.last_update), (100.0, updated));
}

#[test]
fn unknown_sparse_executions_wait_for_metadata_without_consuming_the_watermark() {
    for fields in [r#", "price":"120""#, r#", "side":"BUY""#, ""] {
        let (mut e, s) = startup();
        e.take_frame(0.0);
        event(
            &mut e,
            s,
            &format!(
                r#"{{"ch":"order","data":{{"event":"UPDATE","symbol":"BTCUSDT","orderId":"unknown","dealAmount":"2"{fields}}}}}"#
            ),
        );
        assert!(observed_fills(&mut e).is_empty(), "{fields}");
        let complete = r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"unknown","dealAmount":"2","side":"BUY","averagePrice":"125"}}"#;
        event(&mut e, s, complete);
        event(&mut e, s, complete);
        assert_eq!(
            observed_fills(&mut e),
            [(OrderSide::Buy as u32, 125.0, 2.0)],
            "{fields}",
        );
    }
}

#[test]
fn an_unknown_sparse_create_cannot_supply_a_fabricated_side_to_later_fills() {
    let (mut e, s) = startup();
    e.take_frame(0.0);
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"unknown","qty":"3","price":"120","dealAmount":"0"}}"#,
    );
    assert!(e.store().order("unknown").is_none());
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"unknown","dealAmount":"1","price":"120"}}"#,
    );
    assert!(observed_fills(&mut e).is_empty());
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"unknown","dealAmount":"1","price":"120","side":"BUY"}}"#,
    );
    assert_eq!(
        observed_fills(&mut e),
        [(OrderSide::Buy as u32, 120.0, 1.0)]
    );
}

fn recover_sparse_market_close(kind: &str, timestamp: &str) {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"market","side":"BUY","type":"MARKET","qty":"2","price":"0","dealAmount":"0","mtime":"2025-04-05T16:05:40Z"}}"#,
    );
    e.take_frame(0.0);
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"market","dealAmount":"2","orderStatus":"FILLED","mtime":"2025-04-05T16:05:42Z"}}"#,
    );
    let row = e.store().order("market").unwrap();
    let updated = row.last_update;
    assert_eq!(
        (row.status, row.filled, row.remaining, row.price),
        (OrderStatus::Closed, 2.0, 0.0, 0.0)
    );
    assert!(observed_fills(&mut e).is_empty());
    let recovery = format!(
        r#"{{"ch":"order","data":{{"event":"{kind}","symbol":"BTCUSDT","orderId":"market","dealAmount":"2","side":"BUY","averagePrice":"125","orderStatus":"FILLED"{timestamp}}}}}"#
    );
    event(&mut e, s, &recovery);
    event(&mut e, s, &recovery);
    assert_eq!(
        observed_fills(&mut e),
        [(OrderSide::Buy as u32, 125.0, 2.0)]
    );
    let row = e.store().order("market").unwrap();
    assert_eq!(
        (row.status, row.filled, row.remaining, row.price),
        (OrderStatus::Closed, 2.0, 0.0, 0.0)
    );
    assert_eq!(row.last_update, updated);
}

#[test]
fn complete_update_recovers_a_sparse_market_close_at_the_same_cumulative_quantity() {
    recover_sparse_market_close("UPDATE", "");
}

#[test]
fn older_complete_close_recovers_execution_metadata_without_rewinding_the_order() {
    recover_sparse_market_close("CLOSE", r#", "mtime":"2025-04-05T16:05:41Z""#);
}

#[test]
fn a_known_sparse_create_merges_execution_quantity_without_replacing_parent_identity() {
    let (mut e, s) = startup();
    e.store_mut()
        .update_order("one", |row| row.side = OrderSide::Buy);
    let parent = e.store().order("one").unwrap().clone();
    e.take_frame(0.0);
    let create = r#"{"ch":"order","data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"one","qty":"4","price":"101","dealAmount":"2","mtime":"2025-04-05T16:05:42Z"}}"#;
    event(&mut e, s, create);
    event(&mut e, s, create);
    assert_eq!(
        observed_fills(&mut e),
        [(OrderSide::Buy as u32, 101.0, 1.0)]
    );
    let row = e.store().order("one").unwrap();
    assert_eq!(
        (row.status, row.amount, row.filled, row.remaining, row.price),
        (OrderStatus::Open, 4.0, 2.0, 2.0, 101.0)
    );
    assert_eq!((row.kind, row.side), (parent.kind, parent.side));
    assert_eq!(row.client_order_id, parent.client_order_id);
    assert_eq!(row.time_in_force, parent.time_in_force);
    assert_eq!(e.store().order("one__tp").unwrap().price, 80.0);
    assert_eq!(e.store().order("one__sl").unwrap().price, 110.0);
}

#[test]
fn reordered_updates_cannot_regress_fills_prices_or_native_protection() {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","qty":"3","dealAmount":"2","price":"105","tpPrice":"81","mtime":"2025-04-05T16:05:40Z"}}"#,
    );
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","qty":"3","dealAmount":"1","price":"90","tpPrice":"70","mtime":"2025-04-05T16:05:39Z"}}"#,
    );
    let row = e.store().order("one").unwrap();
    assert_eq!((row.filled, row.remaining, row.price), (2.0, 1.0, 105.0));
    e.handle(
        100.0,
        Event::Command(safe_cex_engine::Command::AmendOrder {
            command: 900,
            id: "one".into(),
            price: Some(106.0),
            amount: None,
        }),
    );
    let writes = requests(&mut e);
    let body = writes.last().unwrap().body.as_deref().unwrap();
    assert!(body.contains(r#""tpPrice":"81""#), "{body}");
}

#[test]
fn stale_terminal_cancel_cannot_downgrade_a_full_fill() {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","orderStatus":"FILLED","dealAmount":"3","mtime":"2025-04-05T16:05:42Z"}}"#,
    );
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","orderStatus":"CANCELED","dealAmount":"1","mtime":"2025-04-05T16:05:41Z"}}"#,
    );
    let row = e.store().order("one").unwrap();
    assert_eq!(
        (row.status, row.filled, row.remaining),
        (OrderStatus::Closed, 3.0, 0.0)
    );
}

#[test]
fn advancing_late_terminal_fill_survives_cancel_without_rewinding_time() {
    let (mut e, s) = startup();
    e.take_frame(0.0);
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","orderStatus":"CANCELED","dealAmount":"2","price":"105","tpPrice":"81","mtime":"2025-04-05T16:05:42Z"}}"#,
    );
    let time = e.store().order("one").unwrap().last_update;
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"2.5","price":"95","tpPrice":"75","mtime":"2025-04-05T16:05:41Z"}}"#,
    );
    let row = e.store().order("one").unwrap();
    assert_eq!(
        (row.status, row.filled, row.remaining, row.price),
        (OrderStatus::Canceled, 2.5, 0.0, 105.0)
    );
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"2.5","price":"95","mtime":"2025-04-05T16:05:41Z"}}"#,
    );
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","orderStatus":"FILLED","dealAmount":"3","qty":"4","price":"90","tpPrice":"70","mtime":"2025-04-05T16:05:41Z"}}"#,
    );
    let row = e.store().order("one").unwrap();
    assert_eq!(
        (row.status, row.filled, row.remaining),
        (OrderStatus::Closed, 3.0, 0.0)
    );
    assert_eq!(row.last_update, time);
    assert_eq!((row.amount, row.price), (3.0, 105.0));
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"3.25","price":"95","mtime":"2025-04-05T16:05:41Z"}}"#,
    );
    let row = e.store().order("one").unwrap();
    assert_eq!(
        (
            row.status,
            row.filled,
            row.remaining,
            row.price,
            row.last_update
        ),
        (OrderStatus::Closed, 3.25, 0.0, 105.0, time)
    );
    let frame = e.take_market_frame(100.0).unwrap();
    let fills: Vec<_> = safe_cex_engine::wire::sections(frame)
        .filter(|(kind, _)| *kind == 11)
        .collect();
    assert_eq!(fills.len(), 1);
    // REST baseline 1 -> canceled 2 -> late update 2.5 -> late close 3 -> 3.25.
    // Replaying the intermediate update must not emit a fifth execution.
    assert_eq!(u32::from_le_bytes(fills[0].1[..4].try_into().unwrap()), 4);
}

#[test]
fn missing_timestamps_keep_cumulative_fills_and_terminal_state_monotonic() {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"2","price":"105","tpPrice":"81","slPrice":"111"}}"#,
    );
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"1","price":"90","tpPrice":"70","slPrice":"120"}}"#,
    );
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"0","qty":"4","price":"90","tpPrice":"70","slPrice":"120"}}"#,
    );
    assert_eq!(e.store().order("one").unwrap().filled, 2.0);
    assert_eq!(
        (
            e.store().order("one").unwrap().amount,
            e.store().order("one").unwrap().price
        ),
        (3.0, 105.0)
    );
    // Omission is a legitimate sparse amend, unlike an explicit zero cumulative.
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","price":"106","tpPrice":"82"}}"#,
    );
    e.handle(
        100.0,
        Event::Command(safe_cex_engine::Command::AmendOrder {
            command: 902,
            id: "one".into(),
            price: Some(106.0),
            amount: None,
        }),
    );
    let writes = requests(&mut e);
    let body = writes.last().unwrap().body.as_deref().unwrap();
    assert!(
        body.contains(r#""tpPrice":"82""#) && body.contains(r#""slPrice":"111""#),
        "{body}"
    );
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","orderStatus":"CANCELED","dealAmount":"2"}}"#,
    );
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"2.5"}}"#,
    );
    let row = e.store().order("one").unwrap();
    assert_eq!(
        (row.status, row.filled, row.remaining),
        (OrderStatus::Canceled, 2.5, 0.0)
    );
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","orderStatus":"FILLED","dealAmount":"3"}}"#,
    );
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","orderStatus":"CANCELED","dealAmount":"3"}}"#,
    );
    let row = e.store().order("one").unwrap();
    assert_eq!(
        (row.status, row.filled, row.remaining),
        (OrderStatus::Closed, 3.0, 0.0)
    );
}

#[test]
fn stale_terminal_tpsl_close_preserves_both_virtual_siblings() {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"tpsl","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"protect","orderStatus":"FILLED","tpPrice":"91000","slPrice":"101000"}}"#,
    );
    event(
        &mut e,
        s,
        r#"{"ch":"tpsl","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"protect","orderStatus":"CANCELED","tpPrice":"70","slPrice":"120"}}"#,
    );
    for (id, price) in [
        ("protect__position-1__tp", 91000.0),
        ("protect__position-1__sl", 101000.0),
    ] {
        let row = e.store().order(id).unwrap();
        assert_eq!((row.status, row.price), (OrderStatus::Closed, price));
    }
}
#[test]
fn startup_keeps_reported_net_pnl_and_handles_null_tpsl_and_account_arrays() {
    let (e, _) = startup();
    let p = &e.store().positions()[0];
    assert_eq!(p.realized_pnl, Some(-3.5));
    assert_eq!(p.contracts, 0.01);
    assert_eq!(p.entry_price, 95000.0);
    assert_eq!(e.store().balance().total, 1003.0);
    assert_eq!(e.store().balance().upnl, 12.0);
    assert!(e.store().hedged());
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().margin_type,
        Some(MarginType::Isolated)
    );
    assert_eq!(
        e.store().order("protect__position-1__tp").unwrap().side,
        OrderSide::Buy
    );
    assert!(e.store().order("protect__position-1__sl").is_some());
}
#[test]
fn parent_updates_do_not_corrupt_virtual_trigger_prices() {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","side":"Sell","price":"105","qty":"3","dealAmount":"2","averagePrice":"100","mtime":"2025-04-05T16:05:39.369077000Z"}}"#,
    );
    assert_eq!(e.store().order("one").unwrap().price, 105.0);
    assert_eq!(e.store().order("one").unwrap().last_update, 1743869139369.0);
    assert_eq!(e.store().order("one__tp").unwrap().price, 80.0);
    assert_eq!(e.store().order("one__sl").unwrap().price, 110.0);
}
#[test]
fn terminal_events_cannot_be_undone_by_delayed_creates_or_cached_rest() {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","orderStatus":"FILLED","dealAmount":"3"}}"#,
    );
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"one","orderType":"LIMIT","side":"SELL","qty":"3","price":"100"}}"#,
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Closed);
    let timer = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                if at_ms == 1100.0 { Some(timer) } else { None }
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    let reads = requests(&mut e);
    for r in reads {
        reply(
            &mut e,
            r.id,
            if r.url.contains("/tpsl/") {
                TPSL
            } else {
                ORDERS
            },
        );
    }
    assert!(
        !e.store()
            .orders()
            .iter()
            .any(|o| o.id == "one" && o.status == OrderStatus::Open)
    );
}
#[test]
fn signature_headers_cover_sorted_query_and_exact_body() {
    let mut e = support::engine(support::Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Default::default()
    });
    e.start(0.0);
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    reply(&mut e, r.id, PAIRS);
    let r = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("marginCoin"))
        .unwrap();
    let h = |name| {
        r.headers
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap()
            .1
            .as_str()
    };
    let digest = safe_cex_crypto::hex(&safe_cex_crypto::sha256(
        format!("{}{}keymarginCoinUSDT", h("nonce"), h("timestamp")).as_bytes(),
    ));
    assert_eq!(
        h("sign"),
        safe_cex_crypto::hex(&safe_cex_crypto::sha256(
            format!("{digest}secret").as_bytes()
        ))
    );
    assert!(r.body.is_none());
    assert!(!e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
}

fn balance_request(
    e: &mut TestEngine,
    s: safe_cex_engine::SocketId,
    at: f64,
) -> safe_cex_engine::RequestId {
    e.handle(
        at,
        Event::WsMessage {
            socket: s,
            text: r#"{"ch":"balance","data":{"available":"900"}}"#,
        },
    );
    fire_balance_repair(e, at + 1000.0)
}
fn fire_balance_repair(e: &mut TestEngine, at: f64) -> safe_cex_engine::RequestId {
    let (timer, due) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms >= at - 1000.0 && at_ms <= at => {
                Some((timer, at_ms))
            }
            _ => None,
        })
        .expect("actual first balance repair deadline");
    e.handle(due, Event::TimerFired { timer });
    requests(e)
        .into_iter()
        .find(|r| r.url.contains("/futures/account?"))
        .unwrap()
        .id
}
fn fire_read_retry(e: &mut TestEngine, now: f64) -> safe_cex_engine::RequestId {
    let (timer, at) = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms >= now + 750.0 && at_ms <= now + 1000.0 => {
                Some((timer, at_ms))
            }
            _ => None,
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("actual first failed-read retry");
    e.handle(at, Event::TimerFired { timer });
    requests(e).remove(0).id
}
#[test]
fn older_replaced_balance_and_pre_mode_ack_reads_cannot_restore_old_state() {
    use safe_cex_engine::{Command, Response};
    let (mut e, s) = startup();
    let a = balance_request(&mut e, s, 100.0);
    let b = balance_request(&mut e, s, 10100.0);
    let body = r#"{"code":0,"data":{"data":[{"marginCoin":"USDT","available":"700","margin":"10","crossUnrealizedPNL":"0","isolationUnrealizedPNL":"0","unrealizedPNL":"999"}]}}"#;
    e.handle(
        11101.0,
        Event::HttpResponse {
            request: b,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    assert_eq!(e.store().balance().free, 700.0);
    assert_eq!(e.store().balance().upnl, 0.0);
    e.handle(
        11102.0,
        Event::HttpResponse {
            request: a,
            response: Response {
                status: 200,
                headers: "",
                body: ACCOUNT,
            },
        },
    );
    assert_eq!(e.store().balance().free, 700.0);
    let old = balance_request(&mut e, s, 11200.0);
    e.store_mut().set_positions(vec![]);
    e.handle(
        12201.0,
        Event::Command(Command::SetPositionMode {
            command: 92,
            hedged: false,
            symbol: None,
        }),
    );
    let write = requests(&mut e).remove(0);
    e.handle(
        12202.0,
        Event::HttpResponse {
            request: write.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":0,"data":{}}"#,
            },
        },
    );
    e.drain_actions().for_each(drop);
    e.handle(
        12203.0,
        Event::HttpResponse {
            request: old,
            response: Response {
                status: 200,
                headers: "",
                body: ACCOUNT,
            },
        },
    );
    assert_eq!(e.store().balance().free, 700.0);
    assert!(
        !e.store().hedged(),
        "old balance cannot undo accepted mode ACK"
    );
    let read = fire_balance_repair(&mut e, 13203.0);
    e.handle(
        13204.0,
        Event::HttpResponse {
            request: read,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":10001,"msg":"request too frequently","data":[]}"#,
            },
        },
    );
    assert_eq!(e.store().balance().free, 700.0);
    let read = fire_read_retry(&mut e, 13204.0);
    reply(&mut e, read, r#"{"code":0,"data":{"unexpected":1}}"#);
    assert_eq!(e.store().balance().free, 700.0);
}
#[test]
fn private_heartbeat_preserves_legacy_silent_sessions() {
    let (mut e, s) = startup();
    e.handle(
        1.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    let (retry, at) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .unwrap();
    e.handle(at, Event::TimerFired { timer: retry });
    let actions: Vec<_> = e.drain_actions().collect();
    let socket = actions
        .iter()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(*socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(at, Event::WsOpened { socket });
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(50_000.0, Event::TimerFired { timer });
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a,Action::WsClose {socket:s} if s==socket))
    );
}

#[test]
fn incremental_fills_use_rest_baseline_and_dedupe_repeated_updates() {
    let (mut e, s) = startup();
    e.take_frame(0.0);
    let update = r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","side":"Sell","qty":"3","dealAmount":"2","averagePrice":"150"}}"#;
    event(&mut e, s, update);
    event(&mut e, s, update);
    let bytes = e.take_market_frame(100.0).unwrap();
    let mut at = 16;
    let mut found = 0;
    while at < bytes.len() {
        let kind = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let payload = &bytes[at + 8..at + 8 + size];
        if kind == 11 {
            assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 1);
            assert_eq!(
                f64::from_le_bytes(payload[16..24].try_into().unwrap()),
                200.0
            );
            assert_eq!(f64::from_le_bytes(payload[24..32].try_into().unwrap()), 1.0);
            found += 1;
        }
        at += 8 + size;
    }
    assert_eq!(found, 1);
}

#[test]
fn malformed_order_rows_preserve_previous_snapshot() {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"1"}}"#,
    );
    let timer = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                if at_ms == 1100.0 { Some(timer) } else { None }
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    for r in requests(&mut e) {
        reply(
            &mut e,
            r.id,
            if r.url.contains("/tpsl/") {
                TPSL
            } else {
                r#"{"code":0,"data":{"orderList":[{}]}}"#
            },
        );
    }
    assert!(e.store().order("one").is_some());
    assert!(e.store().order("").is_none());
}

#[test]
fn plain_orders_carry_no_trigger_type_and_virtual_legs_no_client_id() {
    let (e, _) = startup();
    let parent = e.store().order("one").unwrap();
    assert_eq!(parent.trigger_type, None);
    assert_eq!(parent.client_order_id.as_deref(), Some("web"));
    for leg in ["one__tp", "one__sl"] {
        let row = e.store().order(leg).unwrap();
        assert_eq!(row.client_order_id, None, "{leg}");
        assert!(row.trigger_type.is_some(), "{leg}");
    }
}

#[test]
fn account_tick_reads_balance_every_500ms() {
    let (mut e, mut timers) = started_with_reads_answered();
    let reads = fire_account_tick(&mut e, &mut timers, 500.0);
    assert!(reads.iter().any(is_balance_read));
    assert!(timers.iter().any(|(_, at)| *at == 1000.0));
}

#[test]
fn candle_interest_reads_symbol_leverage_and_margin_mode_once() {
    use safe_cex_engine::model::Interval;
    let (mut e, _) = startup();
    for interval in [Interval::M1, Interval::M5] {
        e.handle(
            1.0,
            Event::Command(safe_cex_engine::Command::SubscribeCandles {
                symbol: "BTCUSDT".into(),
                interval,
            }),
        );
    }
    let reads: Vec<_> = requests(&mut e)
        .into_iter()
        .filter(|r| r.url.contains("get_leverage_margin_mode"))
        .collect();
    assert_eq!(reads.len(), 1);
    let read = &reads[0];
    assert_eq!(read.method, safe_cex_engine::Method::Get);
    assert!(
        read.url.ends_with(
            "/api/v1/futures/account/get_leverage_margin_mode?marginCoin=USDT&symbol=BTCUSDT"
        ),
        "{}",
        read.url
    );
    assert!(read.headers.iter().any(|(name, _)| *name == "sign"));
    reply(
        &mut e,
        read.id,
        r#"{"code":0,"data":{"symbol":"BTCUSDT","marginCoin":"USDT","leverage":50,"marginMode":"CROSS"}}"#,
    );
    let setting = e.store().setting("BTCUSDT").unwrap();
    assert_eq!(
        (setting.leverage, setting.margin_type),
        (50.0, Some(MarginType::Cross))
    );
    for p in e
        .store()
        .positions()
        .iter()
        .filter(|p| p.symbol == "BTCUSDT")
    {
        assert_eq!((p.leverage, p.margin_type), (50.0, Some(MarginType::Cross)));
    }
}

#[test]
fn live_fill_price_and_amount_drop_binary_noise() {
    let (mut e, s) = startup();
    e.take_frame(0.0);
    // REST baseline is 1 @ 100; (1.4 * 1000.1 - 100) / (1.4 - 1) is 3250.3500000000004 / 0.3999999999999999 raw.
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","side":"Sell","qty":"3","dealAmount":"1.4","averagePrice":"1000.1"}}"#,
    );
    assert_eq!(
        observed_fills(&mut e),
        [(OrderSide::Sell as u32, 3250.35, 0.4)]
    );
}

/// Starts without sockets, answering every startup read; returns the timers set.
fn started_with_reads_answered() -> (TestEngine, Vec<(safe_cex_engine::TimerId, f64)>) {
    let mut e = support::engine(support::Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Default::default()
    });
    e.start(0.0);
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    reply(&mut e, r.id, PAIRS);
    let mut timers = vec![];
    loop {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
        if actions.is_empty() {
            return (e, timers);
        }
        for a in actions {
            match a {
                Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)),
                Action::Http(r) => {
                    let body = if r.url.ends_with("/tickers") {
                        TICKERS
                    } else if r.url.contains("get_pending_positions") {
                        POSITIONS
                    } else if r.url.contains("/tpsl/") {
                        TPSL
                    } else if r.url.contains("get_pending_orders") {
                        ORDERS
                    } else {
                        ACCOUNT
                    };
                    reply(&mut e, r.id, body);
                }
                _ => {}
            }
        }
    }
}

fn is_balance_read(r: &safe_cex_engine::HttpRequest) -> bool {
    r.url.contains("/api/v1/futures/account?marginCoin=USDT")
}

/// Fires the account tick scheduled at `at`, returning the HTTP reads it issued.
fn fire_account_tick(
    e: &mut TestEngine,
    timers: &mut Vec<(safe_cex_engine::TimerId, f64)>,
    at: f64,
) -> Vec<safe_cex_engine::HttpRequest> {
    let (timer, _) = timers
        .iter()
        .rev()
        .find(|(_, at_ms)| *at_ms == at)
        .copied()
        .expect("account tick scheduled");
    e.handle(at, Event::TimerFired { timer });
    e.drain_actions()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            Action::SetTimer { timer, at_ms } => {
                timers.push((timer, at_ms));
                None
            }
            _ => None,
        })
        .collect()
}

#[test]
fn unanswered_balance_read_is_not_superseded_by_later_ticks() {
    let (mut e, mut timers) = started_with_reads_answered();
    let pending = fire_account_tick(&mut e, &mut timers, 500.0)
        .into_iter()
        .find(is_balance_read)
        .expect("first tick reads balance")
        .id;
    for at in [1000.0, 1500.0, 2000.0] {
        let reads = fire_account_tick(&mut e, &mut timers, at);
        assert!(
            !reads.iter().any(is_balance_read),
            "tick at {at} superseded the unanswered balance read"
        );
    }
    reply(&mut e, pending, ACCOUNT);
    let reads = fire_account_tick(&mut e, &mut timers, 2500.0);
    assert!(
        reads.iter().any(is_balance_read),
        "an answered read must resume polling"
    );
}

#[test]
fn balance_read_lost_without_any_answer_is_abandoned_after_ten_seconds() {
    let (mut e, mut timers) = started_with_reads_answered();
    fire_account_tick(&mut e, &mut timers, 500.0)
        .into_iter()
        .find(is_balance_read)
        .expect("first tick reads balance");
    let mut at = 1000.0;
    while at < 10_500.0 {
        assert!(
            !fire_account_tick(&mut e, &mut timers, at)
                .iter()
                .any(is_balance_read)
        );
        at += 500.0;
    }
    assert!(
        fire_account_tick(&mut e, &mut timers, 10_500.0)
            .iter()
            .any(is_balance_read),
        "a lost read must not stall balance polling forever"
    );
}

fn subscribe_candles_settings_read(
    e: &mut TestEngine,
    interval: safe_cex_engine::model::Interval,
) -> Option<safe_cex_engine::RequestId> {
    e.handle(
        1.0,
        Event::Command(safe_cex_engine::Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval,
        }),
    );
    requests(e)
        .into_iter()
        .find(|r| r.url.contains("get_leverage_margin_mode"))
        .map(|r| r.id)
}

#[test]
fn failed_or_malformed_symbol_settings_read_retries_without_interest_bypassing_backoff() {
    use safe_cex_engine::model::Interval;
    let (mut e, _) = startup();
    let read = subscribe_candles_settings_read(&mut e, Interval::M1).expect("first read");
    e.handle(
        2.0,
        Event::HttpResponse {
            request: read,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: r#"{"code":0,"data":{"leverage":0}}"#,
            },
        },
    );
    let pending: Vec<_> = e.drain_actions().collect();
    assert!(
        subscribe_candles_settings_read(&mut e, Interval::M5).is_none(),
        "interest must not bypass failed-read backoff"
    );
    let (timer, at) = pending
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms >= 752.0 && *at_ms <= 1002.0 => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("malformed settings retry");
    e.handle(at, Event::TimerFired { timer });
    let read = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("get_leverage_margin_mode"))
        .unwrap()
        .id;
    e.handle(
        at + 1.0,
        Event::HttpFailed {
            request: read,
            error: "network down",
        },
    );
    let pending: Vec<_> = e.drain_actions().collect();
    assert!(
        subscribe_candles_settings_read(&mut e, Interval::M15).is_none(),
        "interest must not bypass second failure's backoff"
    );
    let (timer, due) = pending
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms >= at + 1501.0 && *at_ms <= at + 2001.0 => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("second settings retry");
    e.handle(due, Event::TimerFired { timer });
    let read = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("get_leverage_margin_mode"))
        .unwrap()
        .id;
    reply(
        &mut e,
        read,
        r#"{"code":0,"data":{"leverage":50,"marginMode":"CROSS"}}"#,
    );
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 50.0);
    assert!(subscribe_candles_settings_read(&mut e, Interval::H1).is_none());
}

#[test]
fn symbol_settings_reply_after_a_settings_ack_cannot_restore_old_values() {
    use safe_cex_engine::model::Interval;
    let (mut e, _) = startup();
    let read = subscribe_candles_settings_read(&mut e, Interval::M1).expect("settings read");
    e.handle(
        1.0,
        Event::Command(safe_cex_engine::Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
            side: None,
        }),
    );
    let write = requests(&mut e).remove(0);
    reply(&mut e, write.id, r#"{"code":0,"data":{}}"#);
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 30.0);
    reply(
        &mut e,
        read,
        r#"{"code":0,"data":{"leverage":50,"marginMode":"CROSS"}}"#,
    );
    let setting = e.store().setting("BTCUSDT").unwrap();
    assert_eq!(
        (setting.leverage, setting.margin_type),
        (30.0, Some(MarginType::Isolated))
    );
    assert!(
        e.store()
            .positions()
            .iter()
            .filter(|p| p.symbol == "BTCUSDT")
            .all(|p| p.leverage == 30.0 && p.margin_type == Some(MarginType::Isolated))
    );
}
