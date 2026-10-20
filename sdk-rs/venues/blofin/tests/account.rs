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
            r#"{{"arg":{{"channel":"orders"}},"data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","orderType":"limit","side":"buy","size":"10","price":"60000","createTime":"1","filledSize":"{qty}","averagePrice":"{average}","state":"{state}"}}]}}"#
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
        r#"{{"arg":{{"channel":"orders"}},"data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"{id}","orderType":"limit","side":"buy","size":"10","price":"60000","createTime":"1","filledSize":"{filled}","averagePrice":"100","state":"{state}"}}]}}"#
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
fn unmapped_owned_position_rejects_scope_but_zero_and_other_products_are_ignored() {
    for (quantity, kind, rejected) in [
        ("4", "SWAP", true),
        ("0", "SWAP", false),
        ("4", "SPOT", false),
    ] {
        let mut h = Harness::new();
        h.seed();
        let previous = h.e.store().positions().to_vec();
        let balance = h.e.store().balance().clone();
        h.e.handle(10_000., Event::TimerFired { timer: h.tick });
        h.collect();
        let known = POSITION.replace("\"positions\":\"4\"", "\"positions\":\"2\"");
        h.reply("account/positions", &format!(r#"{{"code":"0","data":[{known},{{"instId":"MISSING-USDT","instType":"{kind}","positions":"{quantity}"}}]}}"#));
        if rejected {
            assert_eq!(h.e.store().positions(), previous);
            assert_eq!(*h.e.store().balance(), balance);
            assert!(!h.pending.iter().any(|r| r.url.contains("asset/balances")));
            assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Log { message, .. }) if message.contains("owned position has unavailable market metadata"))));
        } else {
            h.reply(
                "balances",
                r#"{"code":"0","data":[{"currency":"USDT","available":"1000","frozen":"10"}]}"#,
            );
            assert_eq!(h.e.store().positions()[0].contracts, 0.002);
        }
    }
}
#[test]
fn unmapped_initial_position_does_not_claim_empty_account_loaded() {
    let mut h = Harness::new();
    h.modes();
    h.reply(
        "account/positions",
        r#"{"code":"0","data":[{"instId":"MISSING-USDT","positions":"2"}]}"#,
    );
    assert!(!h.e.store().loaded().positions);
    assert!(!h.e.store().loaded().balance);
}
#[test]
fn unmapped_working_order_rejects_authoritative_order_scope() {
    let mut h = Harness::new();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"original","orderType":"limit","side":"buy","size":"1","price":"60000","state":"live"}]}"#);
    let previous = h.e.store().orders().to_vec();
    h.e.handle(10_000., Event::TimerFired { timer: h.tick });
    h.collect();
    h.reply("orders-pending", r#"{"code":"0","data":[{"instId":"MISSING-USDT","instType":"SWAP","orderId":"external","size":"1","state":"live"}]}"#);
    assert_eq!(h.e.store().orders(), previous);
    assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Log { message, .. }) if message.contains("working order has unavailable market metadata"))));
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

#[test]
fn startup_rest_partial_seeds_history_but_later_rest_cannot_consume_stream_fills() {
    const START: f64 = 1_700_000_000_000.0;
    let mut h = Harness::new_at(START);
    h.modes();
    h.reply("account/positions", r#"{"code":"0","data":[]}"#);
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"currency":"USDT","available":"1000","frozen":"0"}]}"#,
    );
    let snapshot = |qty, average| {
        format!(
            r#"{{"code":"0","data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","orderType":"limit","side":"buy","size":"10","price":"100","filledSize":"{qty}","averagePrice":"{average}","state":"partially_filled"}}]}}"#
        )
    };
    h.reply("orders-pending", &snapshot(4, 100));
    h.reply("orders-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-tpsl-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-algo-pending", r#"{"code":"0","data":[]}"#);
    assert!(executions(h.e.take_frame(START).unwrap()).is_empty());
    let event = |qty, average, updated| {
        format!(
            r#"{{"arg":{{"channel":"orders"}},"data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","orderType":"limit","side":"buy","size":"10","price":"100","filledSize":"{qty}","averagePrice":"{average}","state":"partially_filled","updateTime":"{updated}"}}]}}"#
        )
    };
    let first = event(6, 120, START + 1.0);
    h.e.handle(
        START + 2.0,
        Event::WsMessage {
            socket: h.private,
            text: &first,
        },
    );
    h.collect();
    assert_eq!(
        executions(h.e.take_frame(START + 2.0).unwrap()),
        [(160.0, 0.002)]
    );

    h.e.handle(START + 10_000.0, Event::TimerFired { timer: h.tick });
    h.collect();
    h.reply("orders-pending", &snapshot(8, 130));
    h.reply("orders-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-tpsl-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-algo-pending", r#"{"code":"0","data":[]}"#);
    assert_eq!(h.e.store().order("123").unwrap().filled, 0.008);
    let late = event(8, 130, START + 11_000.0);
    for _ in 0..2 {
        h.e.handle(
            START + 12_000.0,
            Event::WsMessage {
                socket: h.private,
                text: &late,
            },
        );
        h.collect();
    }
    assert_eq!(
        executions(h.e.take_frame(START + 12_000.0).unwrap()),
        [(160.0, 0.002)]
    );
}

// Startup reporting waits for a complete REST execution baseline. Order-state
// revisions may reject that snapshot without invalidating its cumulative proof.
fn startup_reply_at(h: &mut Harness, path: &str, body: &str, now: f64) {
    let request = h.take(path);
    h.e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    h.collect();
}
fn startup_execution(
    id: &str,
    qty: u32,
    average: u32,
    created: f64,
    updated: f64,
    state: &str,
) -> String {
    format!(
        r#"{{"arg":{{"channel":"orders"}},"data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"{id}","orderType":"limit","side":"buy","size":"10","price":"100","filledSize":"{qty}","averagePrice":"{average}","state":"{state}","createTime":"{created}","updateTime":"{updated}"}}]}}"#
    )
}
fn startup_orders(qty: u32, average: u32) -> String {
    format!(
        r#"{{"code":"0","data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","orderType":"limit","side":"buy","size":"10","price":"100","filledSize":"{qty}","averagePrice":"{average}","state":"partially_filled"}}]}}"#
    )
}
fn finish_startup_orders(h: &mut Harness, now: f64) {
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        startup_reply_at(h, path, r#"{"code":"0","data":[]}"#, now);
    }
}
#[test]
fn startup_ws_before_rest_emits_only_the_proven_suffix_at_its_execution_price() {
    const START: f64 = 1_700_000_000_000.0;
    for terminal in [false, true] {
        let mut h = Harness::new_at(START);
        h.modes();
        let first = startup_execution(
            "123",
            6,
            120,
            START - 100.0,
            START + 1.0,
            if terminal {
                "filled"
            } else {
                "partially_filled"
            },
        );
        h.e.handle(
            START + 2.0,
            Event::WsMessage {
                socket: h.private,
                text: &first,
            },
        );
        h.collect();
        assert!(executions(h.e.take_frame(START + 2.0).unwrap()).is_empty());
        startup_reply_at(
            &mut h,
            "orders-pending",
            &startup_orders(4, 100),
            START + 3.0,
        );
        finish_startup_orders(&mut h, START + 4.0);
        assert_eq!(
            executions(h.e.take_frame(START + 4.0).unwrap()),
            [(160.0, 0.002)]
        );
        h.e.handle(
            START + 5.0,
            Event::WsMessage {
                socket: h.private,
                text: &first,
            },
        );
        h.collect();
        assert!(
            h.e.take_frame(START + 5.0)
                .is_none_or(|f| executions(f).is_empty())
        );
    }
}
#[test]
fn startup_rest_already_covering_live_progress_establishes_a_quiet_reporting_baseline() {
    const START: f64 = 1_700_000_000_000.0;
    let mut h = Harness::new_at(START);
    h.modes();
    let first = startup_execution(
        "123",
        6,
        120,
        START - 100.0,
        START + 1.0,
        "partially_filled",
    );
    h.e.handle(
        START + 2.0,
        Event::WsMessage {
            socket: h.private,
            text: &first,
        },
    );
    h.collect();
    assert!(executions(h.e.take_frame(START + 2.0).unwrap()).is_empty());
    startup_reply_at(
        &mut h,
        "orders-pending",
        &startup_orders(6, 120),
        START + 3.0,
    );
    finish_startup_orders(&mut h, START + 4.0);
    assert!(
        h.e.take_frame(START + 4.0)
            .is_none_or(|f| executions(f).is_empty())
    );
    let live = startup_execution(
        "123",
        8,
        130,
        START - 100.0,
        START + 5.0,
        "partially_filled",
    );
    h.e.handle(
        START + 6.0,
        Event::WsMessage {
            socket: h.private,
            text: &live,
        },
    );
    h.collect();
    assert_eq!(
        executions(h.e.take_frame(START + 6.0).unwrap()),
        [(160.0, 0.002)]
    );
}
#[test]
fn startup_unknown_terminal_prefix_is_quiet_but_proven_new_filled_order_is_live() {
    const START: f64 = 1_700_000_000_000.0;
    let mut h = Harness::new_at(START);
    h.modes();
    let old = startup_execution("old", 6, 120, START - 100.0, START + 1.0, "filled");
    h.e.handle(
        START + 2.0,
        Event::WsMessage {
            socket: h.private,
            text: &old,
        },
    );
    h.collect();
    assert!(executions(h.e.take_frame(START + 2.0).unwrap()).is_empty());
    let new = startup_execution("new", 2, 160, START + 1.0, START + 2.0, "filled");
    h.e.handle(
        START + 3.0,
        Event::WsMessage {
            socket: h.private,
            text: &new,
        },
    );
    h.collect();
    assert_eq!(
        executions(h.e.take_frame(START + 3.0).unwrap()),
        [(160.0, 0.002)]
    );
    finish_startup_orders(&mut h, START + 4.0);
    assert!(
        h.e.take_frame(START + 4.0)
            .is_none_or(|f| executions(f).is_empty())
    );
    for text in [&old, &new] {
        h.e.handle(
            START + 5.0,
            Event::WsMessage {
                socket: h.private,
                text,
            },
        );
        h.collect();
    }
    assert!(
        h.e.take_frame(START + 5.0)
            .is_none_or(|f| executions(f).is_empty())
    );
}

#[test]
fn failed_startup_page_cannot_release_early_executions_or_seed_a_partial_census() {
    const START: f64 = 1_700_000_000_000.0;
    let mut h = Harness::new_at(START);
    h.modes();
    let first = startup_execution(
        "123",
        6,
        120,
        START - 100.0,
        START + 1.0,
        "partially_filled",
    );
    h.e.handle(
        START + 2.0,
        Event::WsMessage {
            socket: h.private,
            text: &first,
        },
    );
    h.collect();
    assert!(executions(h.e.take_frame(START + 2.0).unwrap()).is_empty());
    startup_reply_at(
        &mut h,
        "orders-pending",
        &startup_orders(4, 100),
        START + 3.0,
    );
    startup_reply_at(
        &mut h,
        "orders-pending",
        r#"{"code":"500","data":[]}"#,
        START + 4.0,
    );
    assert!(
        h.e.take_frame(START + 4.0)
            .is_none_or(|f| executions(f).is_empty())
    );
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == START + 1004.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.e.handle(START + 1004.0, Event::TimerFired { timer });
    h.collect();
    startup_reply_at(
        &mut h,
        "orders-pending",
        &startup_orders(4, 100),
        START + 1005.0,
    );
    finish_startup_orders(&mut h, START + 1006.0);
    assert_eq!(
        executions(h.e.take_frame(START + 1006.0).unwrap()),
        [(160.0, 0.002)]
    );
}
#[test]
fn startup_missing_quote_basis_does_not_invent_a_suffix_price() {
    const START: f64 = 1_700_000_000_000.0;
    let mut h = Harness::new_at(START);
    h.modes();
    let first = startup_execution(
        "123",
        6,
        120,
        START - 100.0,
        START + 1.0,
        "partially_filled",
    );
    h.e.handle(
        START + 2.0,
        Event::WsMessage {
            socket: h.private,
            text: &first,
        },
    );
    h.collect();
    assert!(executions(h.e.take_frame(START + 2.0).unwrap()).is_empty());
    startup_reply_at(&mut h, "orders-pending", &startup_orders(4, 0), START + 3.0);
    finish_startup_orders(&mut h, START + 4.0);
    assert!(
        h.e.take_frame(START + 4.0)
            .is_none_or(|f| executions(f).is_empty())
    );
    let live = startup_execution(
        "123",
        8,
        130,
        START - 100.0,
        START + 5.0,
        "partially_filled",
    );
    h.e.handle(
        START + 6.0,
        Event::WsMessage {
            socket: h.private,
            text: &live,
        },
    );
    h.collect();
    assert_eq!(
        executions(h.e.take_frame(START + 6.0).unwrap()),
        [(160.0, 0.002)]
    );
}
#[test]
fn startup_multiple_buffered_executions_keep_their_individual_suffix_prices() {
    const START: f64 = 1_700_000_000_000.0;
    let mut h = Harness::new_at(START);
    h.modes();
    for (qty, avg, time) in [(6, 120, START + 1.0), (8, 130, START + 2.0)] {
        let event = startup_execution("123", qty, avg, START - 100.0, time, "partially_filled");
        h.e.handle(
            time,
            Event::WsMessage {
                socket: h.private,
                text: &event,
            },
        );
        h.collect();
    }
    assert!(executions(h.e.take_frame(START + 2.0).unwrap()).is_empty());
    startup_reply_at(
        &mut h,
        "orders-pending",
        &startup_orders(4, 100),
        START + 3.0,
    );
    finish_startup_orders(&mut h, START + 4.0);
    assert_eq!(
        executions(h.e.take_frame(START + 4.0).unwrap()),
        [(160.0, 0.002), (160.0, 0.002)]
    );
}
#[test]
fn startup_bounded_tracker_never_replays_an_evicted_unknown_prefix() {
    const START: f64 = 1_700_000_000_000.0;
    let mut h = Harness::new_at(START);
    h.modes();
    for id in 0..502 {
        let event = startup_execution(
            &id.to_string(),
            6,
            120,
            START - 100.0,
            START + 1.0,
            "filled",
        );
        h.e.handle(
            START + 2.0,
            Event::WsMessage {
                socket: h.private,
                text: &event,
            },
        );
        h.collect();
    }
    assert!(executions(h.e.take_frame(START + 2.0).unwrap()).is_empty());
    finish_startup_orders(&mut h, START + 3.0);
    assert!(
        h.e.take_frame(START + 3.0)
            .is_none_or(|f| executions(f).is_empty())
    );
}

#[test]
fn delayed_unknown_startup_terminal_seeds_once_then_post_ready_suffix_is_live() {
    const START: f64 = 1_700_000_000_000.0;
    let mut h = Harness::new_at(START);
    h.modes();
    finish_startup_orders(&mut h, START + 10.0);
    h.e.take_frame(START + 10.0);
    let late = startup_execution(
        "123",
        6,
        120,
        START - 100.0,
        START + 1.0,
        "partially_filled",
    );
    h.e.handle(
        START + 11.0,
        Event::WsMessage {
            socket: h.private,
            text: &late,
        },
    );
    h.collect();
    assert!(
        h.e.take_frame(START + 11.0)
            .is_none_or(|f| executions(f).is_empty())
    );
    let live = startup_execution(
        "123",
        8,
        130,
        START - 100.0,
        START + 12.0,
        "partially_filled",
    );
    h.e.handle(
        START + 13.0,
        Event::WsMessage {
            socket: h.private,
            text: &live,
        },
    );
    h.collect();
    assert_eq!(
        executions(h.e.take_frame(START + 13.0).unwrap()),
        [(160.0, 0.002)]
    );
}
#[test]
fn proven_new_order_reporting_survives_rest_ahead_during_startup() {
    const START: f64 = 1_700_000_000_000.0;
    let mut h = Harness::new_at(START);
    h.modes();
    let first = startup_execution("123", 2, 100, START + 1.0, START + 2.0, "partially_filled");
    h.e.handle(
        START + 3.0,
        Event::WsMessage {
            socket: h.private,
            text: &first,
        },
    );
    h.collect();
    assert_eq!(
        executions(h.e.take_frame(START + 3.0).unwrap()),
        [(100.0, 0.002)]
    );
    startup_reply_at(
        &mut h,
        "orders-pending",
        &startup_orders(4, 110),
        START + 4.0,
    );
    finish_startup_orders(&mut h, START + 5.0);
    assert!(
        h.e.take_frame(START + 5.0)
            .is_none_or(|f| executions(f).is_empty())
    );
    let live = startup_execution("123", 4, 110, START + 1.0, START + 6.0, "partially_filled");
    h.e.handle(
        START + 7.0,
        Event::WsMessage {
            socket: h.private,
            text: &live,
        },
    );
    h.collect();
    assert_eq!(
        executions(h.e.take_frame(START + 7.0).unwrap()),
        [(120.0, 0.002)]
    );
}

#[test]
fn first_old_order_after_empty_startup_census_never_assumes_a_zero_prefix() {
    const START: f64 = 1_700_000_000_000.0;
    for state in ["partially_filled", "filled"] {
        let mut h = Harness::new_at(START);
        h.modes();
        finish_startup_orders(&mut h, START + 10.0);
        h.e.take_frame(START + 10.0);
        let late = startup_execution("123", 6, 120, START - 100.0, START + 12.0, state);
        h.e.handle(
            START + 13.0,
            Event::WsMessage {
                socket: h.private,
                text: &late,
            },
        );
        h.collect();
        assert!(
            h.e.take_frame(START + 13.0)
                .is_none_or(|f| executions(f).is_empty())
        );
        assert!(h.actions.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Log{level:safe_cex_engine::LogLevel::Error,message})if message.contains("execution prefix unavailable"))));
        h.e.handle(
            START + 14.0,
            Event::WsMessage {
                socket: h.private,
                text: &late,
            },
        );
        h.collect();
        assert!(
            h.e.take_frame(START + 14.0)
                .is_none_or(|f| executions(f).is_empty())
        );
        if state == "partially_filled" {
            let live = startup_execution("123", 8, 130, START - 100.0, START + 15.0, state);
            h.e.handle(
                START + 16.0,
                Event::WsMessage {
                    socket: h.private,
                    text: &live,
                },
            );
            h.collect();
            assert_eq!(
                executions(h.e.take_frame(START + 16.0).unwrap()),
                [(160.0, 0.002)]
            );
        }
    }
}

#[test]
fn post_ready_missing_quote_gap_is_visible_and_later_proven_suffix_continues_once() {
    const START: f64 = 1_700_000_000_000.0;
    let mut h = Harness::new_at(START);
    h.modes();
    startup_reply_at(&mut h, "orders-pending", &startup_orders(4, 0), START + 1.0);
    finish_startup_orders(&mut h, START + 2.0);
    h.e.take_frame(START + 2.0);
    let gap = startup_execution(
        "123",
        6,
        120,
        START - 100.0,
        START + 3.0,
        "partially_filled",
    );
    h.e.handle(
        START + 4.0,
        Event::WsMessage {
            socket: h.private,
            text: &gap,
        },
    );
    h.collect();
    assert!(
        h.e.take_frame(START + 4.0)
            .is_none_or(|f| executions(f).is_empty())
    );
    assert!(h.actions.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Log{level:safe_cex_engine::LogLevel::Error,message})if message.contains("execution price unavailable for order 123")&&message.contains("missing cumulative quote baseline"))));
    h.e.handle(
        START + 5.0,
        Event::WsMessage {
            socket: h.private,
            text: &gap,
        },
    );
    h.collect();
    assert!(
        h.e.take_frame(START + 5.0)
            .is_none_or(|f| executions(f).is_empty())
    );
    let live = startup_execution(
        "123",
        8,
        130,
        START - 100.0,
        START + 6.0,
        "partially_filled",
    );
    for now in [START + 7.0, START + 8.0] {
        h.e.handle(
            now,
            Event::WsMessage {
                socket: h.private,
                text: &live,
            },
        );
        h.collect();
    }
    assert_eq!(
        executions(h.e.take_frame(START + 8.0).unwrap()),
        [(160.0, 0.002)]
    );
}

#[test]
fn missing_or_zero_creation_time_does_not_prove_an_unknown_order_is_new() {
    const START: f64 = 1_700_000_000_000.0;
    for omitted in [false, true] {
        let mut h = Harness::new_at(START);
        h.modes();
        finish_startup_orders(&mut h, START + 1.0);
        h.e.take_frame(START + 1.0);
        let mut event = startup_execution("123", 6, 120, 0.0, START + 2.0, "filled");
        if omitted {
            event = event.replace("\"createTime\":\"0\",", "");
        }
        h.e.handle(
            START + 3.0,
            Event::WsMessage {
                socket: h.private,
                text: &event,
            },
        );
        h.collect();
        assert!(
            h.e.take_frame(START + 3.0)
                .is_none_or(|f| executions(f).is_empty())
        );
        assert!(h.actions.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Log{level:safe_cex_engine::LogLevel::Error,message})if message.contains("execution prefix unavailable"))));
    }
}
#[test]
fn explicit_unfilled_socket_order_proves_zero_prefix_without_creation_time() {
    const START: f64 = 1_700_000_000_000.0;
    let mut h = Harness::new_at(START);
    h.modes();
    finish_startup_orders(&mut h, START + 1.0);
    h.e.take_frame(START + 1.0);
    let unfilled = startup_execution("123", 0, 0, 0.0, START + 2.0, "live");
    h.e.handle(
        START + 3.0,
        Event::WsMessage {
            socket: h.private,
            text: &unfilled,
        },
    );
    h.collect();
    let live = startup_execution("123", 2, 160, 0.0, START + 4.0, "filled");
    h.e.handle(
        START + 5.0,
        Event::WsMessage {
            socket: h.private,
            text: &live,
        },
    );
    h.collect();
    assert_eq!(
        executions(h.e.take_frame(START + 5.0).unwrap()),
        [(160.0, 0.002)]
    );
}

#[test]
fn unpriceable_live_quote_preserves_the_proven_basis_visibly_and_next_quote_recovers() {
    const START: f64 = 1_700_000_000_000.0;
    for (known_average, invalid_average) in [(100, 0), (100, 50), (150, 100)] {
        let mut h = Harness::new_at(START);
        h.modes();
        startup_reply_at(
            &mut h,
            "orders-pending",
            &startup_orders(4, known_average),
            START + 1.0,
        );
        finish_startup_orders(&mut h, START + 2.0);
        h.e.take_frame(START + 2.0);
        h.actions.clear();
        let invalid = startup_execution(
            "123",
            6,
            invalid_average,
            START - 100.0,
            START + 3.0,
            "partially_filled",
        );
        h.e.handle(
            START + 4.0,
            Event::WsMessage {
                socket: h.private,
                text: &invalid,
            },
        );
        h.collect();
        assert!(
            h.e.take_frame(START + 4.0)
                .is_none_or(|frame| executions(frame).is_empty())
        );
        assert!(h.actions.iter().any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::Log { level:safe_cex_engine::LogLevel::Error, message }) if message.contains("execution price unavailable for order 123"))), "invalid average {invalid_average} must visibly reject the current quote and preserve the proven basis");
        let seed = startup_execution(
            "123",
            8,
            130,
            START - 100.0,
            START + 5.0,
            "partially_filled",
        );
        h.e.handle(
            START + 6.0,
            Event::WsMessage {
                socket: h.private,
                text: &seed,
            },
        );
        h.collect();
        assert_eq!(
            executions(h.e.take_frame(START + 6.0).unwrap()),
            [(260.0 - f64::from(known_average), 0.004)],
            "the next valid quote recovers the interval from the last proven prefix"
        );
        let suffix = startup_execution("123", 10, 140, START - 100.0, START + 7.0, "filled");
        for now in [START + 8.0, START + 9.0] {
            h.e.handle(
                now,
                Event::WsMessage {
                    socket: h.private,
                    text: &suffix,
                },
            );
            h.collect();
        }
        assert_eq!(
            executions(h.e.take_frame(START + 9.0).unwrap()),
            [(180.0, 0.002)]
        );
    }
}

#[test]
fn decreasing_quote_cannot_price_the_next_execution_from_the_rejected_value() {
    const START: f64 = 1_700_000_000_000.0;
    let mut h = Harness::new_at(START);
    h.modes();
    startup_reply_at(
        &mut h,
        "orders-pending",
        &startup_orders(4, 100),
        START + 1.0,
    );
    finish_startup_orders(&mut h, START + 2.0);
    h.e.take_frame(START + 2.0);
    for (qty, average, offset) in [(6, 50, 3.0), (8, 130, 5.0)] {
        let event = startup_execution(
            "123",
            qty,
            average,
            START - 100.0,
            START + offset,
            "partially_filled",
        );
        h.e.handle(
            START + offset + 1.0,
            Event::WsMessage {
                socket: h.private,
                text: &event,
            },
        );
        h.collect();
    }
    let fills =
        h.e.take_frame(START + 6.0)
            .map(executions)
            .unwrap_or_default();
    assert_eq!(
        fills,
        [(160.0, 0.004)],
        "the valid quote must recover the full interval from the last proven prefix"
    );
}

fn unknown_startup_quote_recovery(evict: bool) {
    const START: f64 = 1_700_000_000_000.0;
    for (known_average, invalid_average) in [(100, 50), (150, 100)] {
        let mut h = Harness::new_at(START);
        h.modes();
        for (qty, average, offset) in [(4, known_average, 1.0), (6, invalid_average, 2.0)] {
            let event =
                startup_execution("123", qty, average, START - 100.0, START + offset, "filled");
            h.e.handle(
                START + offset + 1.0,
                Event::WsMessage {
                    socket: h.private,
                    text: &event,
                },
            );
            h.collect();
        }
        if evict {
            // Duplicate another ID to evict both target observations without
            // expiring the target from the independent 500-order tracker.
            let noise = startup_execution("noise", 1, 1, START - 100.0, START + 3.0, "filled");
            for index in 0..501 {
                h.e.handle(
                    START + 4.0 + f64::from(index),
                    Event::WsMessage {
                        socket: h.private,
                        text: &noise,
                    },
                );
                h.collect();
            }
        }
        finish_startup_orders(&mut h, START + 1000.0);
        assert!(
            h.e.take_frame(START + 1000.0)
                .is_none_or(|frame| executions(frame).is_empty())
        );
        let live = startup_execution("123", 8, 130, START - 100.0, START + 1001.0, "filled");
        for now in [START + 1002.0, START + 1003.0] {
            h.e.handle(
                now,
                Event::WsMessage {
                    socket: h.private,
                    text: &live,
                },
            );
            h.collect();
        }
        let fills =
            h.e.take_frame(START + 1003.0)
                .map(executions)
                .unwrap_or_default();
        assert_eq!(
            fills,
            [(260.0 - f64::from(known_average), 0.004)],
            "startup/eviction must retain the last proven cumulative quote (evict={evict})"
        );
        assert!(h.actions.iter().any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::Log { level:safe_cex_engine::LogLevel::Error, message }) if message.contains("execution price unavailable for order 123"))));
    }
}
#[test]
fn startup_unknown_quote_cannot_decrease_through_an_empty_census() {
    unknown_startup_quote_recovery(false);
}
#[test]
fn startup_unknown_quote_cannot_decrease_through_bounded_queue_eviction() {
    unknown_startup_quote_recovery(true);
}

#[test]
fn unpriceable_pre_session_quote_advances_only_the_quiet_historical_quantity_floor() {
    const START: f64 = 1_700_000_000_000.0;
    for (known_average, invalid_average) in [(100, 0), (100, 50), (150, 100)] {
        let mut h = Harness::new_at(START);
        h.modes();
        startup_reply_at(
            &mut h,
            "orders-pending",
            &startup_orders(4, known_average),
            START + 1.0,
        );
        finish_startup_orders(&mut h, START + 2.0);
        h.e.take_frame(START + 2.0);
        h.actions.clear();
        let old = startup_execution(
            "123",
            6,
            invalid_average,
            START - 100.0,
            START - 1.0,
            "partially_filled",
        );
        h.e.handle(
            START + 3.0,
            Event::WsMessage {
                socket: h.private,
                text: &old,
            },
        );
        h.collect();
        assert!(
            h.e.take_frame(START + 3.0)
                .is_none_or(|frame| executions(frame).is_empty())
        );
        assert!(!h.actions.iter().any(|action| matches!(
            action,
            Action::Notice(safe_cex_engine::Notice::Log {
                level: safe_cex_engine::LogLevel::Error,
                ..
            })
        )));
        let live = startup_execution(
            "123",
            8,
            130,
            START - 100.0,
            START + 4.0,
            "partially_filled",
        );
        h.e.handle(
            START + 5.0,
            Event::WsMessage {
                socket: h.private,
                text: &live,
            },
        );
        h.collect();
        assert!(
            h.e.take_frame(START + 5.0)
                .is_none_or(|frame| executions(frame).is_empty()),
            "a missing historical quote cannot replay historical quantity4..6 as live"
        );
        assert!(h.actions.iter().any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::Log { level:safe_cex_engine::LogLevel::Error, message }) if message.contains("missing cumulative quote baseline"))));
        let suffix = startup_execution("123", 10, 140, START - 100.0, START + 6.0, "filled");
        for now in [START + 7.0, START + 8.0] {
            h.e.handle(
                now,
                Event::WsMessage {
                    socket: h.private,
                    text: &suffix,
                },
            );
            h.collect();
        }
        assert_eq!(
            executions(h.e.take_frame(START + 8.0).unwrap()),
            [(180.0, 0.002)]
        );
    }
}

#[test]
fn websocket_login_timestamp_is_an_integer_matching_signed_bytes() {
    use base64::Engine as _;
    use safe_cex_crypto::{Encoding, hmac_sha256};
    let mut h = Harness::new();
    h.e.handle(1_700_000_000_123.875, Event::WsOpened { socket: h.private });
    h.collect();
    let login = h
        .actions
        .iter()
        .find_map(|action| match action {
            Action::WsSend { text, .. } if text.contains("\"op\":\"login\"") => Some(text),
            _ => None,
        })
        .unwrap();
    let login: serde_json::Value = serde_json::from_str(login).unwrap();
    let args = &login["args"][0];
    let timestamp = args["timestamp"]
        .as_u64()
        .expect("timestamp must serialize as a JSON integer");
    assert_eq!(timestamp, 1_700_000_000_123);
    let signed = format!(
        "/users/self/verifyGET{timestamp}{}",
        args["nonce"].as_str().unwrap()
    );
    let expected = base64::engine::general_purpose::STANDARD.encode(hmac_sha256(
        b"secret",
        signed.as_bytes(),
        Encoding::Hex,
    ));
    assert_eq!(args["sign"], expected);
}

#[test]
fn null_filled_size_cannot_close_a_known_order_and_valid_siblings_continue() {
    let mut h = Harness::new();
    h.seed();
    h.ws(&order_event("123", "0", "live"));
    let bad = r#"{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","orderType":"limit","side":"buy","size":"10","price":"60000","filledSize":null,"averagePrice":"100","state":"filled"}"#;
    let sibling = r#"{"instId":"BTC-USDT","orderCategory":"normal","orderId":"456","orderType":"limit","side":"buy","size":"10","price":"60000","filledSize":"0","averagePrice":"0","state":"live"}"#;
    h.ws(&format!(
        r#"{{"arg":{{"channel":"orders"}},"data":[{bad},{sibling}]}}"#
    ));
    assert_eq!(
        h.e.store().order("123").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
    assert!(h.e.store().order("456").is_some());
    assert!(h.actions.iter().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Log { message, .. })
            if message.contains("Malformed BloFin order row"))));
}
