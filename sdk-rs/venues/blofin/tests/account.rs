// Documented BloFin REST and private-channel payload shapes.
mod support;
use safe_cex_engine::{Action, Event};
use support::*;

#[test]
fn startup_authenticates_and_loads_account_atomically() {
    let mut h = Harness::new();
    let headers = &h
        .pending
        .iter()
        .find(|r| r.url.ends_with("margin-mode"))
        .unwrap()
        .headers;
    for name in [
        "ACCESS-KEY",
        "ACCESS-NONCE",
        "ACCESS-PASSPHRASE",
        "ACCESS-TIMESTAMP",
        "ACCESS-SIGN",
    ] {
        assert!(headers.iter().any(|(k, _)| *k == name));
    }
    h.e.handle(0.0, Event::WsOpened { socket: h.private });
    h.collect();
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::WsSend{text,..}if text.contains("\"op\":\"login\"")))
    );
    h.ws(r#"{"event":"login","code":"0","msg":""}"#);
    assert_eq!(
        h.actions
            .iter()
            .filter(|a| matches!(a,Action::WsSend{text,..}if text.contains("\"subscribe\"")))
            .count(),
        4
    );
    h.seed();
    let b = h.e.store().balance();
    assert_eq!(
        (b.used, b.free, b.total, b.upnl),
        (0.72, 1009.28, 1009.5, 0.5)
    );
    let p = &h.e.store().positions()[0];
    assert_eq!(p.contracts, 0.004);
    assert_eq!(p.notional, 240.0);
    assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    assert!(
        h.e.store().loaded().orders
            && h.e.store().loaded().balance
            && h.e.store().loaded().positions
    );
}
#[test]
fn cumulative_fills_emit_execution_deltas_and_prices_once() {
    let mut h = Harness::new();
    h.seed();
    h.e.take_frame(0.0);
    let event = |qty: &str, average: &str, state: &str| {
        format!(
            r#"{{"arg":{{"channel":"orders"}},"data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","orderType":"limit","side":"buy","size":"10","price":"60000","filledSize":"{qty}","averagePrice":"{average}","state":"{state}"}}]}}"#
        )
    };
    h.ws(&event("0.1", "100", "partially_filled"));
    h.ws(&event("0.3", "120", "partially_filled"));
    h.ws(&event("0.3", "120", "partially_filled"));
    h.ws(&event("0.2", "110", "partially_filled"));
    h.ws(&event("0.5", "140", "filled"));
    h.ws(&event("0.5", "140", "filled"));
    h.ws(&event("0.3", "120", "partially_filled"));
    let fills = executions(h.e.take_frame(2.0).unwrap());
    assert_eq!(fills, [(100.0, 0.0001), (130.0, 0.0002), (170.0, 0.0002)]);
    assert_eq!(
        h.e.store().orders()[0].status,
        safe_cex_engine::model::OrderStatus::Closed
    );
}
#[test]
fn stale_rest_cannot_reopen_a_filled_order() {
    let mut h = Harness::new();
    h.modes();
    let old = h.take("orders-pending");
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","orderType":"limit","side":"buy","size":"1","filledSize":"1","averagePrice":"60000","state":"filled"}]}"#);
    h.respond(old,r#"{"code":"0","data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","orderType":"limit","side":"buy","size":"1","price":"60000","state":"live"}]}"#);
    h.reply("orders-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-tpsl-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-algo-pending", r#"{"code":"0","data":[]}"#);
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn failed_later_page_preserves_last_known_orders() {
    let mut h = Harness::new();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"original","orderType":"limit","side":"buy","size":"1","price":"60000","state":"live"}]}"#);
    h.e.handle(10_000.0, Event::TimerFired { timer: h.tick });
    h.collect();
    h.reply("orders-pending",r#"{"code":"0","data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"other","orderType":"limit","side":"buy","size":"1","state":"live"}]}"#);
    h.reply(
        "orders-pending",
        r#"{"code":"152401","msg":"Access key does not exist","data":[]}"#,
    );
    assert_eq!(h.e.store().orders().len(), 1);
    assert_eq!(h.e.store().orders()[0].id, "original");
}
#[test]
fn position_closure_keeps_settings_and_blank_liquidation_preserves_display() {
    let mut h = Harness::new();
    h.seed();
    let partial = POSITION
        .replace("\"leverage\":\"20\"", "\"leverage\":\"\"")
        .replace(
            "\"liquidationPrice\":\"26914.9\"",
            "\"liquidationPrice\":\"\"",
        );
    h.ws(&format!(
        r#"{{"arg":{{"channel":"positions"}},"data":[{partial}]}}"#
    ));
    assert_eq!(h.e.store().positions()[0].leverage, 20.0);
    assert_eq!(h.e.store().positions()[0].liquidation_price, 26914.9);
    h.ws(r#"{"arg":{"channel":"positions"},"data":[]}"#);
    assert!(h.e.store().positions().is_empty());
    assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    assert_eq!(h.e.store().balance().used, 0.0);
    assert_eq!(h.e.store().balance().upnl, 0.0);
}
#[test]
fn stale_balance_snapshot_cannot_overwrite_newer_account_push() {
    let mut h = Harness::new();
    h.modes();
    h.reply("account/positions", r#"{"code":"0","data":[]}"#);
    h.ws(r#"{"arg":{"channel":"account"},"data":{"totalEquity":"1234"}}"#);
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"available":"10","frozen":"0"}]}"#,
    );
    assert_eq!(h.e.store().balance().total, 1234.0);
}

#[test]
fn login_waits_for_valid_global_modes_before_initial_snapshot() {
    let mut h = Harness::new();
    h.ws(r#"{"event":"login","code":"0","msg":""}"#);
    assert!(
        !h.pending
            .iter()
            .any(|r| r.url.contains("orders-pending") || r.url.ends_with("account/positions"))
    );
    h.reply("margin-mode", r#"{"code":"0","data":{}}"#);
    h.reply(
        "position-mode",
        r#"{"code":"0","data":{"positionMode":"unknown"}}"#,
    );
    assert!(!h.e.store().loaded().positions);
    assert!(h.pending.is_empty());
}
#[test]
fn collateral_rounds_available_and_frozen_separately() {
    let mut h = Harness::new();
    h.modes();
    h.reply("account/positions", r#"{"code":"0","data":[]}"#);
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"available":"0.005","frozen":"0.005"}]}"#,
    );
    assert_eq!(h.e.store().balance().total, 0.02);
}
#[test]
fn inline_protection_fallback_only_covers_remaining_limit_quantity() {
    let mut h = Harness::new();
    h.modes();
    h.reply("orders-pending",r#"{"code":"0","data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","orderType":"limit","side":"buy","size":"10","filledSize":"4","price":"60000","state":"partially_filled","tpTriggerPrice":"65000"}]}"#);
    h.reply("orders-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-tpsl-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-algo-pending", r#"{"code":"0","data":[]}"#);
    let leg = h.e.store().order("123__take_profit").unwrap();
    assert_eq!(leg.amount, 0.006);
    assert_eq!(leg.remaining, 0.006);
    assert_eq!(leg.extra.as_ref().unwrap().filled_in_placement_units, None);
    assert_eq!(leg.extra.as_ref().unwrap().remaining_native, Some(6.0));
}

fn executions(frame: &[u8]) -> Vec<(f64, f64)> {
    let mut offset = 16;
    while offset < frame.len() {
        let kind = u16::from_le_bytes(frame[offset..offset + 2].try_into().unwrap());
        let len = u32::from_le_bytes(frame[offset + 4..offset + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            let data = &frame[offset + 8..offset + 8 + len];
            let count = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
            let start = (4 + count * 12 + 7) & !7;
            return (0..count)
                .map(|i| {
                    let j = start + i * 16;
                    (
                        f64::from_le_bytes(data[j..j + 8].try_into().unwrap()),
                        f64::from_le_bytes(data[j + 8..j + 16].try_into().unwrap()),
                    )
                })
                .collect();
        }
        offset += 8 + len;
    }
    vec![]
}

fn order_event(id: &str, filled: &str, state: &str) -> String {
    format!(
        r#"{{"arg":{{"channel":"orders"}},"data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"{id}","orderType":"limit","side":"buy","size":"10","price":"60000","filledSize":"{filled}","averagePrice":"100","state":"{state}"}}]}}"#
    )
}

#[test]
fn first_terminal_frames_prevent_later_open_rows_for_normal_and_algo_orders() {
    let mut h = Harness::new();
    h.seed();
    for (id, state, filled) in [("filled", "filled", "4"), ("canceled", "canceled", "0")] {
        h.ws(&order_event(id, filled, state));
        h.ws(&order_event(id, "0", "live"));
        assert!(h.e.store().order(id).is_none());
    }
    let algo = |state| {
        format!(
            r#"{{"arg":{{"channel":"orders-algo"}},"data":[{{"instId":"BTC-USDT","tpslId":"900","side":"sell","size":"1","tpTriggerPrice":"65000","slTriggerPrice":"55000","state":"{state}"}}]}}"#
        )
    };
    h.ws(&algo("canceled"));
    h.ws(&algo("live"));
    assert!(h.e.store().orders().is_empty());
}

#[test]
fn regressive_partial_preserves_remaining_and_still_honors_terminal_status() {
    use safe_cex_engine::model::OrderStatus;
    let mut h = Harness::new();
    h.seed();
    h.e.take_frame(0.0);
    h.ws(&order_event("123", "4", "partially_filled"));
    h.ws(&order_event("123", "2", "partially_filled"));
    let order = h.e.store().order("123").unwrap();
    assert_eq!(order.filled, 0.004);
    assert_eq!(order.remaining, 0.006);
    assert_eq!(order.extra.as_ref().unwrap().remaining_native, Some(6.0));
    assert_eq!(
        order.extra.as_ref().unwrap().filled_in_placement_units,
        Some(4.0)
    );
    h.ws(&order_event("123", "2", "canceled"));
    assert_eq!(
        h.e.store().order("123").unwrap().status,
        OrderStatus::Canceled
    );
    assert_eq!(h.e.store().order("123").unwrap().filled, 0.004);
    h.ws(&order_event("123", "6", "filled"));
    h.ws(&order_event("123", "6", "filled"));
    assert_eq!(
        h.e.store().order("123").unwrap().status,
        OrderStatus::Closed
    );
    h.ws(&order_event("123", "6", "canceled"));
    assert_eq!(
        h.e.store().order("123").unwrap().status,
        OrderStatus::Closed
    );
    assert_eq!(
        executions(h.e.take_frame(2.0).unwrap()),
        [(100.0, 0.004), (100.0, 0.002)]
    );
}

#[test]
fn terminal_memory_survives_empty_reconciliation_and_filters_stale_snapshots() {
    let mut h = Harness::new();
    h.seed();
    h.ws(&order_event("123", "4", "partially_filled"));
    h.ws(&order_event("123", "10", "filled"));
    h.e.handle(10_000.0, Event::TimerFired { timer: h.tick });
    h.collect();
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    assert!(h.e.store().orders().is_empty());
    h.ws(&order_event("123", "0", "live"));
    assert!(h.e.store().orders().is_empty());
    h.ws(r#"{"event":"login","code":"0","msg":""}"#);
    h.reply("orders-pending",r#"{"code":"0","data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","orderType":"limit","side":"buy","size":"10","filledSize":"0","state":"live","tpTriggerPrice":"65000"}]}"#);
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    assert!(h.e.store().orders().is_empty());
}

#[test]
fn terminal_memory_is_bounded_to_the_last_500_distinct_mapped_ids() {
    let mut h = Harness::new();
    h.seed();
    for id in 0..501 {
        h.ws(&order_event(&id.to_string(), "0", "canceled"));
    }
    h.ws(&order_event("500", "0", "live"));
    h.ws(&order_event("1", "0", "live"));
    assert!(h.e.store().orders().is_empty());
    h.ws(&order_event("0", "0", "live"));
    assert_eq!(h.e.store().orders().len(), 1);
    assert_eq!(h.e.store().orders()[0].id, "0");
}

#[test]
fn regressive_reconciliation_preserves_filled_and_native_remaining() {
    let mut h = Harness::new();
    h.seed();
    h.ws(&order_event("123", "4", "partially_filled"));
    h.e.handle(10_000.0, Event::TimerFired { timer: h.tick });
    h.collect();
    h.reply("orders-pending",r#"{"code":"0","data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","orderType":"limit","side":"buy","size":"10","filledSize":"2","state":"partially_filled"}]}"#);
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    let order = h.e.store().order("123").unwrap();
    assert_eq!(order.filled, 0.004);
    assert_eq!(order.remaining, 0.006);
    assert_eq!(order.extra.as_ref().unwrap().remaining_native, Some(6.0));
}

#[test]
fn regressive_snapshot_cannot_reinsert_an_order_removed_by_reconciliation() {
    let mut h = Harness::new();
    h.seed();
    h.ws(&order_event("123", "4", "partially_filled"));
    h.e.handle(10_000.0, Event::TimerFired { timer: h.tick });
    h.collect();
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    assert!(h.e.store().orders().is_empty());
    h.ws(r#"{"event":"login","code":"0","msg":""}"#);
    h.reply("orders-pending",r#"{"code":"0","data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","orderType":"limit","side":"buy","size":"10","filledSize":"2","state":"partially_filled","tpTriggerPrice":"65000"}]}"#);
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    assert!(h.e.store().orders().is_empty());
    h.ws(&order_event("123", "2", "partially_filled"));
    assert!(h.e.store().orders().is_empty());
}

#[test]
fn pre_session_order_replay_seeds_execution_baseline_without_emitting_history() {
    let mut h = Harness::new_at(1000.0);
    h.seed();
    let event = |quantity, time| {
        order_event("123", quantity, "partially_filled").replace(
            "\"state\":",
            &format!("\"updateTime\":\"{time}\",\"state\":"),
        )
    };
    h.ws(&event("4", 900));
    assert!(executions(h.e.take_frame(1001.0).unwrap()).is_empty());
    h.ws(&event("6", 1001));
    h.ws(&event("6", 1001));
    assert_eq!(
        executions(h.e.take_frame(1002.0).unwrap()),
        [(100.0, 0.002)]
    );
    assert_eq!(h.e.store().order("123").unwrap().filled, 0.006);
}
