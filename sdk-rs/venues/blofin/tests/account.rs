// Documented BloFin REST and private-channel payload shapes.
mod support;
use safe_cex_engine::{Action, Event};
use support::*;

/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 10_000.0;

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
    h.handle(0.0, Event::WsOpened { socket: h.private });
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
    h.handle(10_000.0, Event::TimerFired { timer: h.tick });
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
    h.handle(10_000.0, Event::TimerFired { timer: h.tick });
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
    h.handle(10_000.0, Event::TimerFired { timer: h.tick });
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
    h.handle(10_000.0, Event::TimerFired { timer: h.tick });
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
// BAD-USDT is skipped as malformed (zero leverage cap); ETH-USD is deliberately
// filtered (inverse); MISSING-USDT is not listed at all.
const PARTIAL_INSTRUMENTS: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"},{"instId":"BAD-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BAD","quoteCurrency":"USDT","state":"live","contractValue":"1","minSize":"1","lotSize":"1","tickSize":"0.01","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"0"},{"instId":"ETH-USD","instType":"SWAP","contractType":"inverse","baseCurrency":"ETH","quoteCurrency":"USD","state":"live","contractValue":"1","minSize":"1","lotSize":"1","tickSize":"0.01","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"50"}]}"#;
const BALANCES: &str =
    r#"{"code":"0","data":[{"currency":"USDT","available":"1000","frozen":"10"}]}"#;
const WORKING: &str = r#"{"instId":"BTC-USDT","instType":"SWAP","orderCategory":"normal","orderId":"working","orderType":"limit","side":"buy","size":"1","price":"60000","state":"live"}"#;

#[test]
fn owned_position_on_malformed_market_rejects_scope_but_other_missing_rows_are_skipped() {
    for (inst, quantity, kind, rejected) in [
        ("BAD-USDT", "4", "SWAP", true),
        ("BAD-USDT", "0", "SWAP", false),
        ("BAD-USDT", "4", "SPOT", false),
        ("ETH-USD", "4", "SWAP", false),
        ("MISSING-USDT", "4", "SWAP", false),
    ] {
        let mut h = Harness::with_instruments(PARTIAL_INSTRUMENTS);
        h.seed();
        let previous = h.e.store().positions().to_vec();
        let balance = h.e.store().balance().clone();
        h.handle(10_000., Event::TimerFired { timer: h.tick });
        h.collect();
        let known = POSITION.replace("\"positions\":\"4\"", "\"positions\":\"2\"");
        h.reply("account/positions", &format!(r#"{{"code":"0","data":[{known},{{"instId":"{inst}","instType":"{kind}","positions":"{quantity}"}}]}}"#));
        if rejected {
            assert_eq!(h.e.store().positions(), previous);
            assert_eq!(*h.e.store().balance(), balance);
            assert!(!h.pending.iter().any(|r| r.url.contains("asset/balances")));
            assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(event)) if event.message.contains("owned position has unavailable market metadata"))));
        } else {
            h.reply("balances", BALANCES);
            assert_eq!(h.e.store().positions().len(), 1, "{inst} {kind} {quantity}");
            assert_eq!(h.e.store().positions()[0].contracts, 0.002);
        }
    }
}
#[test]
fn owned_position_and_order_on_filtered_market_are_skipped_and_the_account_loads() {
    let mut h = Harness::with_instruments(PARTIAL_INSTRUMENTS);
    h.modes();
    let filtered = POSITION
        .replace("BTC-USDT", "ETH-USD")
        .replace(
            "\"maintenanceMargin\":\"0.72\"",
            "\"maintenanceMargin\":\"1.28\"",
        )
        .replace("\"unrealizedPnl\":\"0.5\"", "\"unrealizedPnl\":\"2\"");
    h.reply(
        "account/positions",
        &format!(r#"{{"code":"0","data":[{POSITION},{filtered}]}}"#),
    );
    h.reply("balances", BALANCES);
    let filtered_order = WORKING
        .replace("BTC-USDT", "ETH-USD")
        .replace("\"working\"", "\"external\"");
    h.reply(
        "orders-pending",
        &format!(r#"{{"code":"0","data":[{WORKING},{filtered_order}]}}"#),
    );
    // The skipped row still advances the page cursor.
    let next = h.take("orders-pending");
    assert!(next.url.contains("after=external"));
    h.respond(next, r#"{"code":"0","data":[]}"#);
    h.reply("orders-tpsl-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-algo-pending", r#"{"code":"0","data":[]}"#);
    let store = h.e.store();
    let loaded = store.loaded();
    assert!(loaded.positions && loaded.balance && loaded.orders);
    assert_eq!(store.positions().len(), 1);
    assert_eq!(store.positions()[0].symbol, "BTCUSDT");
    // Account totals still count the native row outside the catalogue.
    assert_eq!(store.balance().used, 2.0);
    assert_eq!(store.balance().upnl, 2.5);
    assert_eq!(store.orders().len(), 1);
    assert_eq!(store.orders()[0].id, "working");
    for kind in ["position", "order"] {
        let needle = format!("Skipping BloFin {kind} on uncatalogued market ETH-USD");
        assert!(
            h.actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(event)) if event.message.contains(&needle))),
            "{kind}"
        );
    }
}
#[test]
fn malformed_market_initial_position_does_not_claim_empty_account_loaded() {
    let mut h = Harness::with_instruments(PARTIAL_INSTRUMENTS);
    h.modes();
    h.reply(
        "account/positions",
        r#"{"code":"0","data":[{"instId":"BAD-USDT","positions":"2"}]}"#,
    );
    assert!(!h.e.store().loaded().positions);
    assert!(!h.e.store().loaded().balance);
}
#[test]
fn working_order_on_malformed_market_rejects_authoritative_order_scope() {
    let mut h = Harness::with_instruments(PARTIAL_INSTRUMENTS);
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"original","orderType":"limit","side":"buy","size":"1","price":"60000","state":"live"}]}"#);
    let previous = h.e.store().orders().to_vec();
    h.handle(10_000., Event::TimerFired { timer: h.tick });
    h.collect();
    h.reply("orders-pending", r#"{"code":"0","data":[{"instId":"BAD-USDT","instType":"SWAP","orderId":"external","size":"1","state":"live"}]}"#);
    assert_eq!(h.e.store().orders(), previous);
    assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(event)) if event.message.contains("working order has unavailable market metadata"))));
}
#[test]
fn partial_catalogue_commits_then_rereads_without_a_second_private_start() {
    let mut h = Harness::with_instruments(PARTIAL_INSTRUMENTS);
    assert!(h.e.store().market_by_id("BAD-USDT").is_none());
    let retry = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(*at_ms, 1000.0) => Some(*timer),
            _ => None,
        })
        .expect("partial catalogue retry");
    h.seed();
    h.actions.clear();
    h.handle(1000., Event::TimerFired { timer: retry });
    h.collect_at(1000.);
    h.reply(
        "instruments",
        &PARTIAL_INSTRUMENTS.replace("\"maxLeverage\":\"0\"", "\"maxLeverage\":\"20\""),
    );
    assert!(
        h.e.store().market_by_id("BAD-USDT").is_some(),
        "repaired market"
    );
    assert!(h.e.store().market_by_id("BTC-USDT").is_some());
    assert!(
        h.pending.is_empty(),
        "no ticker reseed or private restart: {:?}",
        h.pending
    );
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
            && new_timer(&h.actions).is_none(),
        "{:?}",
        h.actions
    );
    assert!(h.e.store().loaded().positions);
}

// Drive the actual account loop through slow/fast updates, a parked stale callback,
// and a resumed read. Other timers are deliberately not fired.
fn policy_poll_read<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
    endpoint: &str,
) -> (f64, Vec<safe_cex_engine::HttpRequest>) {
    use safe_cex_engine::{Action, Event};
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 60_000.0 / POLL_MS,
        },
    );
    let slow: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            Action::Http(_) | Action::ManagedHttp(_) => {
                panic!("policy updates must not issue account reads")
            }
            _ => None,
        })
        .collect();
    assert_eq!(slow.len(), 1, "exactly one recurring account poll");
    assert_eq!(slow[0].1, now + 60_000.0);
    engine.handle(
        now + 1.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let fast: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(fast.len(), 1);
    assert_eq!(fast[0].1, now + 1_501.0);
    engine.handle(
        now + 2.0,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let paused: Vec<_> = engine.drain_actions().collect();
    assert!(
        paused
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == fast[0].0))
    );
    assert!(!paused.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    engine.handle(now + 1_501.0, Event::TimerFired { timer: fast[0].0 });
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    engine.handle(
        now + 90_000.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let resumed: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].1, now + 91_500.0);
    engine.handle(
        now + 91_500.0,
        Event::TimerFired {
            timer: resumed[0].0,
        },
    );
    let reads: Vec<_> = safe_cex_engine::test_support::prepared_actions(engine, now + 91_500.0)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert!(
        reads.iter().any(|r| r.url.contains(endpoint)),
        "actual resumed account HTTP: {reads:?}"
    );
    let completed_at = now + 91_501.0;
    engine.handle(
        completed_at,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
    (completed_at, reads)
}

fn policy_poll_after_completion<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
) {
    use safe_cex_engine::{Action, Event};
    // A response may schedule an ordinary repair/deadline, but the parked poll
    // itself must still be registered and adopt this newest policy.
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 30_000.0 / POLL_MS,
        },
    );
    let times: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { at_ms, .. } => Some(at_ms),
            _ => None,
        })
        .collect();
    assert_eq!(times, vec![now + 30_000.0]);
}

#[test]
fn recurring_account_poll_obeys_live_policy_and_completion() {
    let mut h = Harness::new();
    h.seed();
    let engine = &mut h.e;
    let (now, mut reads) = policy_poll_read(engine, 0.0, "account/positions");
    for _ in 0..12 {
        if reads.is_empty() {
            break;
        }
        for r in reads {
            let body = if r.url.contains("balances") {
                r#"{"code":"0","data":[{"currency":"USDT","available":"1000","frozen":"10"}]}"#
            } else {
                r#"{"code":"0","data":[]}"#
            };
            engine.handle(
                now,
                safe_cex_engine::Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
        }
        reads = engine
            .drain_actions()
            .filter_map(|a| match a {
                safe_cex_engine::Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
    }
    assert!(reads.is_empty(), "account cycle must complete");
    policy_poll_after_completion(engine, now);
}

#[test]
fn failure_poll_floor_survives_fast_policy_and_pause_resume() {
    let mut h = Harness::new();
    h.seed();
    let engine = &mut h.e;
    let now = 0.0;
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let timer = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == now + 1_500.0 => Some(timer),
            _ => None,
        })
        .expect("fast account poll");
    engine.handle(now + 1_500.0, Event::TimerFired { timer });
    let output = safe_cex_engine::test_support::prepared_actions(engine, now + 1_500.0);
    let reads: Vec<_> = output
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(reads.len(), 2, "balance and positions fail authentication");
    // Keep any next periodic timer for a loop that rearms before HTTP completes.
    for r in reads {
        engine.handle(
            now + 1_500.0,
            Event::HttpResponse {
                request: r.id,
                response: safe_cex_engine::Response {
                    status: 401,
                    headers: "",
                    body: r#"{"code":"152401","msg":"invalid key"}"#,
                },
            },
        );
    }
    // BloFin determines its next auth backoff on the following poll tick.
    let next = output
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == now + 3_000.0 => Some(*timer),
            _ => None,
        })
        .expect("next account tick");
    engine.handle(now + 3_000.0, Event::TimerFired { timer: next });
    engine.drain_actions().for_each(drop);
    // The jittered failure floor is fixed once and must survive every policy change.
    let mut floor = None;
    for (offset, interval_ms) in [(1.0, 1_000.0), (2.0, f64::INFINITY), (3.0, 1_200.0)] {
        engine.handle(
            now + 3_000.0 + offset,
            Event::PollScaleChanged {
                scale: interval_ms / POLL_MS,
            },
        );
        let times: Vec<_> = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::SetTimer { at_ms, .. } => Some(at_ms),
                _ => None,
            })
            .collect();
        if interval_ms.is_infinite() {
            assert!(times.is_empty());
        } else {
            let at = *times.first().expect("one rearmed poll");
            assert_eq!(times, vec![at]);
            let delay = at - (now + 3_000.0);
            assert!(
                (0.75 * 40_000.0..=40_000.0).contains(&delay),
                "jittered failure backoff {delay}"
            );
            assert_eq!(*floor.get_or_insert(at), at);
        }
    }
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
    h.handle(
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

    h.handle(START + 10_000.0, Event::TimerFired { timer: h.tick });
    h.collect();
    h.reply("orders-pending", &snapshot(8, 130));
    h.reply("orders-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-tpsl-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-algo-pending", r#"{"code":"0","data":[]}"#);
    assert_eq!(h.e.store().order("123").unwrap().filled, 0.008);
    let late = event(8, 130, START + 11_000.0);
    for _ in 0..2 {
        h.handle(
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
    h.handle(
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
        h.handle(
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
        h.handle(
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
    h.handle(
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
    h.handle(
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
    h.handle(
        START + 2.0,
        Event::WsMessage {
            socket: h.private,
            text: &old,
        },
    );
    h.collect();
    assert!(executions(h.e.take_frame(START + 2.0).unwrap()).is_empty());
    let new = startup_execution("new", 2, 160, START + 1.0, START + 2.0, "filled");
    h.handle(
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
        h.handle(
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
    h.handle(
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
    h.handle(START + 1004.0, Event::TimerFired { timer });
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
    h.handle(
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
    h.handle(
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
        h.handle(
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
        h.handle(
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
    h.handle(
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
    h.handle(
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
    h.handle(
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
    h.handle(
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
        h.handle(
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
        assert!(h.actions.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(event)) if event.level == safe_cex_engine::Level::Error && event.message.contains("execution prefix unavailable"))));
        h.handle(
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
            h.handle(
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
    h.handle(
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
    assert!(h.actions.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(event)) if event.level == safe_cex_engine::Level::Error && event.message.contains("execution price unavailable for order 123")&& event.message.contains("missing cumulative quote baseline"))));
    h.handle(
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
        h.handle(
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
        h.handle(
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
        assert!(h.actions.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(event)) if event.level == safe_cex_engine::Level::Error && event.message.contains("execution prefix unavailable"))));
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
    h.handle(
        START + 3.0,
        Event::WsMessage {
            socket: h.private,
            text: &unfilled,
        },
    );
    h.collect();
    let live = startup_execution("123", 2, 160, 0.0, START + 4.0, "filled");
    h.handle(
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
        h.handle(
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
        assert!(h.actions.iter().any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::Event(event)) if event.level == safe_cex_engine::Level::Error && event.message.contains("execution price unavailable for order 123"))), "invalid average {invalid_average} must visibly reject the current quote and preserve the proven basis");
        let seed = startup_execution(
            "123",
            8,
            130,
            START - 100.0,
            START + 5.0,
            "partially_filled",
        );
        h.handle(
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
            h.handle(
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
        h.handle(
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
            h.handle(
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
                h.handle(
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
            h.handle(
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
        assert!(h.actions.iter().any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::Event(event)) if event.level == safe_cex_engine::Level::Error && event.message.contains("execution price unavailable for order 123"))));
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
        h.handle(
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
            Action::Notice(safe_cex_engine::Notice::Event(event)) if event.level == safe_cex_engine::Level::Error
        )));
        let live = startup_execution(
            "123",
            8,
            130,
            START - 100.0,
            START + 4.0,
            "partially_filled",
        );
        h.handle(
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
        assert!(h.actions.iter().any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::Event(event)) if event.level == safe_cex_engine::Level::Error && event.message.contains("missing cumulative quote baseline"))));
        let suffix = startup_execution("123", 10, 140, START - 100.0, START + 6.0, "filled");
        for now in [START + 7.0, START + 8.0] {
            h.handle(
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
    h.handle(1_700_000_000_123.875, Event::WsOpened { socket: h.private });
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
        Action::Notice(safe_cex_engine::Notice::Event(event))
            if event.message.contains("Malformed BloFin order row"))));
}
fn new_timer(actions: &[Action]) -> Option<(safe_cex_engine::TimerId, f64)> {
    actions.iter().enumerate().find_map(|(index, action)| match action {
        Action::SetTimer { timer, at_ms }
            if !actions[index + 1..].iter().any(|action| matches!(action, Action::ClearTimer { timer: cleared } if cleared == timer)) => Some((*timer, *at_ms)),
        _ => None,
    })
}
#[test]
fn refresh_that_omits_a_held_market_keeps_it_with_its_position_and_order() {
    let mut h = Harness::with_instruments(PARTIAL_INSTRUMENTS);
    let (retry, at) = new_timer(&h.actions)
        .filter(|(_, at)| jittered(*at, 1000.0))
        .expect("partial catalogue retry");
    h.seed();
    h.ws(&format!(
        r#"{{"arg":{{"channel":"orders"}},"data":[{WORKING}]}}"#
    ));
    assert!(h.e.store().order("working").is_some());
    let listed = h.e.store().market_by_id("BTC-USDT").cloned().unwrap();
    h.handle(at, Event::TimerFired { timer: retry });
    h.collect_at(at);
    // BTC-USDT drops out of the list while BAD-USDT stays malformed.
    h.reply(
        "instruments",
        &PARTIAL_INSTRUMENTS.replacen("BTC-USDT", "SOL-USDT", 1),
    );
    assert_eq!(h.e.store().market_by_id("BTC-USDT"), Some(&listed));
    assert!(h.e.store().market_by_id("SOL-USDT").is_some());
    h.handle(10_000., Event::TimerFired { timer: h.tick });
    h.collect();
    h.reply(
        "account/positions",
        &format!(r#"{{"code":"0","data":[{POSITION}]}}"#),
    );
    h.reply("balances", BALANCES);
    h.reply(
        "orders-pending",
        &format!(r#"{{"code":"0","data":[{WORKING}]}}"#),
    );
    while h.pending.iter().any(|r| r.url.contains("orders-pending")) {
        h.reply("orders-pending", r#"{"code":"0","data":[]}"#);
    }
    h.reply("orders-tpsl-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-algo-pending", r#"{"code":"0","data":[]}"#);
    let store = h.e.store();
    assert!(store.loaded().positions && store.loaded().balance && store.loaded().orders);
    assert_eq!(store.positions().len(), 1);
    assert_eq!(store.positions()[0].symbol, "BTCUSDT");
    assert!(store.order("working").is_some());
}
#[test]
fn persistently_malformed_catalogue_stops_rereading_after_five_refreshes() {
    let mut h = Harness::with_instruments(PARTIAL_INSTRUMENTS);
    let mut retry = new_timer(&h.actions).filter(|(_, at)| jittered(*at, 1000.0));
    let mut rereads = 0;
    while let Some((timer, at)) = retry.take() {
        h.handle(at, Event::TimerFired { timer });
        h.collect_at(at);
        h.actions.clear();
        h.reply("instruments", PARTIAL_INSTRUMENTS);
        rereads += 1;
        retry = new_timer(&h.actions);
        assert!(rereads <= 5, "re-read is bounded");
    }
    assert_eq!(rereads, 5);
    assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(event)) if event.message.contains("stops re-reading"))));
}

/// Retry delays are jittered into `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}

#[test]
fn auth_failure_ticks_are_jittered_while_healthy_ticks_keep_their_cadence() {
    let mut h = Harness::new();
    h.seed();
    let mut now = 10_000.0;
    let mut tick = h.tick;
    let fire = |h: &mut Harness, timer, now: f64| {
        h.actions.clear();
        h.handle(now, Event::TimerFired { timer });
        h.collect();
        // The next tick is armed after its reads, whose own 20s windows come first.
        h.actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms - now >= 7_500.0 => {
                    Some((*timer, *at_ms - now))
                }
                _ => None,
            })
            .expect("next tick")
    };
    let (next, delay) = fire(&mut h, tick, now);
    assert_eq!(delay, 10_000.0, "healthy polling cadence is exact");
    h.reply(
        "orders-pending",
        r#"{"code":"152401","msg":"Access key does not exist","data":[]}"#,
    );
    let mut delays = Vec::new();
    (tick, now) = (next, now + delay);
    for _ in 0..6 {
        let (next, delay) = fire(&mut h, tick, now);
        delays.push(delay);
        (tick, now) = (next, now + delay);
    }
    assert!(delays.iter().all(|d| jittered(*d, 20_000.0)), "{delays:?}");
    assert!(delays.iter().any(|d| *d < 20_000.0), "{delays:?}");
}

/// Fires the account poll at `at`, answers its reads, and returns their logs.
fn poll(h: &mut Harness, at: f64, positions: &str, orders: &str) -> Vec<String> {
    poll_lists(h, at, positions, orders, "")
}

/// `poll`, with `tpsl` as the first TP/SL list page.
fn poll_lists(h: &mut Harness, at: f64, positions: &str, orders: &str, tpsl: &str) -> Vec<String> {
    h.actions.clear();
    h.handle(at, Event::TimerFired { timer: h.tick });
    h.collect();
    h.tick = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at + 10_000.0 => Some(*timer),
            _ => None,
        })
        .expect("next poll");
    h.reply(
        "account/positions",
        &format!(r#"{{"code":"0","data":[{positions}]}}"#),
    );
    if h.pending.iter().any(|r| r.url.contains("balances")) {
        h.reply(
            "balances",
            r#"{"code":"0","data":[{"currency":"USDT","available":"1000","frozen":"10"}]}"#,
        );
    }
    h.reply(
        "orders-pending",
        &format!(r#"{{"code":"0","data":[{orders}]}}"#),
    );
    if h.pending.iter().any(|r| r.url.contains("orders-pending")) {
        h.reply("orders-pending", r#"{"code":"0","data":[]}"#);
    }
    if !tpsl.is_empty() {
        h.reply(
            "orders-tpsl-pending",
            &format!(r#"{{"code":"0","data":[{tpsl}]}}"#),
        );
    }
    for path in ["orders-tpsl-pending", "orders-algo-pending"] {
        if h.pending.iter().any(|r| r.url.contains(path)) {
            h.reply(path, r#"{"code":"0","data":[]}"#);
        }
    }
    h.actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(event)) => Some(event.message.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn one_malformed_rest_row_keeps_its_stored_copy_while_valid_peers_commit() {
    let instruments = INSTRUMENT.replace(
        "]}",
        &format!(
            ",{}]}}",
            INSTRUMENT[INSTRUMENT.find("[{").unwrap() + 1..INSTRUMENT.rfind("}]").unwrap() + 1]
                .replace("BTC", "ETH")
        ),
    );
    let mut h = Harness::with_instruments(&instruments);
    h.seed();
    let eth = POSITION.replace("BTC-USDT", "ETH-USDT");
    let order = |id: &str, price: &str| {
        format!(
            r#"{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"{id}","side":"buy","size":"1","price":"{price}","state":"live"}}"#
        )
    };
    poll(
        &mut h,
        10_000.0,
        &format!("{POSITION},{eth}"),
        &format!("{},{}", order("a", "60000"), order("b", "61000")),
    );
    let stored_position = h.e.store().position("ETHUSDT").unwrap().clone();
    let stored_order = h.e.store().order("b").unwrap().clone();
    let balance = h.e.store().balance().clone();

    let btc = POSITION.replace("\"positions\":\"4\"", "\"positions\":\"5\"");
    let bad_eth = eth.replace("\"positions\":\"4\"", "\"positions\":null");
    let bad_order = order("b", "61000").replace("\"size\":\"1\"", "\"size\":null");
    for round in 0..2 {
        let logs = poll(
            &mut h,
            20_000.0 + 10_000.0 * f64::from(round),
            &format!("{btc},{bad_eth}"),
            &format!("{},{bad_order}", order("a", "60001")),
        );
        let store = h.e.store();
        assert_eq!(store.position("BTCUSDT").unwrap().contracts, 0.005);
        assert_eq!(store.position("ETHUSDT"), Some(&stored_position));
        assert_eq!(store.order("a").unwrap().price, 60001.0);
        assert_eq!(store.order("b"), Some(&stored_order));
        // Used margin and UPnL sum the rows: the balance waits for a clean read.
        assert_eq!(*store.balance(), balance);
        let skips: Vec<_> = logs.iter().filter(|l| l.contains("skipped")).collect();
        let expected: &[&str] = if round == 0 {
            &[
                "BloFin skipped malformed position row ETH-USDT",
                "BloFin skipped malformed order row b",
            ]
        } else {
            &[]
        };
        assert_eq!(skips, expected, "{logs:?}");
        assert!(!logs.iter().any(|l| l.contains("null") || l.contains('{')));
    }

    poll(&mut h, 40_000.0, &btc, &order("a", "60001"));
    let store = h.e.store();
    assert!(store.position("ETHUSDT").is_none());
    assert_eq!(store.positions().len(), 1);
    assert!(store.order("b").is_none());
    assert_eq!(store.orders().len(), 1);
}

#[test]
fn kept_attached_tpsl_leg_is_not_duplicated_by_its_parent() {
    let mut h = Harness::new();
    h.seed();
    let parent = r#"{"instId":"BTC-USDT","orderCategory":"normal","orderId":"X","orderType":"limit","side":"buy","size":"1","price":"60000","state":"live","attachAlgoOrders":[{"tpTriggerPrice":"65000","tpOrderPrice":"-1"}]}"#;
    let tpsl = r#"{"instId":"BTC-USDT","tpslId":"T1","orderId":"X","side":"buy","size":"1","tpTriggerPrice":"65000","tpOrderPrice":"-1","state":"live"}"#;
    let legs = |h: &Harness| {
        let mut ids: Vec<_> =
            h.e.store()
                .orders()
                .iter()
                .filter(|o| o.kind == safe_cex_engine::model::OrderType::TakeProfit)
                .map(|o| o.id.clone())
                .collect();
        ids.sort();
        ids
    };
    poll_lists(&mut h, 10_000.0, POSITION, parent, tpsl);
    assert_eq!(legs(&h), ["T1__take_profit"]);
    let bad = tpsl.replace("\"size\":\"1\"", "\"size\":null");
    let logs = poll_lists(&mut h, 20_000.0, POSITION, parent, &bad);
    assert!(
        logs.iter()
            .any(|l| l == "BloFin skipped malformed order row T1"),
        "{logs:?}"
    );
    assert_eq!(legs(&h), ["T1__take_profit"]);
    poll_lists(&mut h, 30_000.0, POSITION, parent, "");
    assert_eq!(legs(&h), ["X__take_profit"]);
}

#[test]
fn row_report_order_aliases_rearm_on_decode_even_when_the_read_cannot_commit() {
    let mut h = Harness::new();
    h.seed();
    let bad = ["order-alias", "tpsl-alias", "algo-alias"]
        .map(|id| format!(r#"{{"instId":"BTC-USDT","orderId":"{id}","size":null}}"#))
        .join(",");
    let skips = |logs: Vec<String>| {
        logs.into_iter()
            .filter(|message| message.starts_with("BloFin skipped malformed order row"))
            .collect::<Vec<_>>()
    };
    let expected = ["order-alias", "tpsl-alias", "algo-alias"]
        .map(|id| format!("BloFin skipped malformed order row {id}"));
    assert_eq!(skips(poll(&mut h, 10_000.0, POSITION, &bad)), expected);
    assert!(skips(poll(&mut h, 20_000.0, POSITION, &bad)).is_empty());

    // Decoding rearms all aliases before market mapping. The unidentified next
    // row fails this read, preventing the clean accepted-read clear as a shortcut.
    let decoded = r#"{"instId":"FILTERED-USD","orderId":"order-alias","tpslId":"tpsl-alias","algoId":"algo-alias","side":"buy","size":"1","price":"100","state":"live"}"#;
    poll(
        &mut h,
        30_000.0,
        POSITION,
        &format!("{decoded},{{\"size\":null}}"),
    );
    assert_eq!(skips(poll(&mut h, 40_000.0, POSITION, &bad)), expected);
    assert!(h.e.store().orders().is_empty());
}

#[test]
fn row_report_position_decode_rearms_before_a_held_snapshot_and_keeps_empty_identity() {
    let mut h = Harness::new();
    h.seed();
    let bad = POSITION.replace("\"positions\":\"4\"", "\"positions\":null");
    let messages = |logs: Vec<String>| {
        logs.into_iter()
            .filter(|message| message.starts_with("BloFin skipped malformed position row"))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        messages(poll(&mut h, 10_000.0, &bad, "")),
        ["BloFin skipped malformed position row BTC-USDT"]
    );
    assert!(messages(poll(&mut h, 20_000.0, &bad, "")).is_empty());
    let held = format!("{POSITION},{{\"positions\":null}}");
    assert_eq!(
        messages(poll(&mut h, 30_000.0, &held, "")),
        ["BloFin skipped malformed position row without an instrument"]
    );
    assert_eq!(
        messages(poll(&mut h, 40_000.0, &bad, "")),
        ["BloFin skipped malformed position row BTC-USDT"]
    );
    assert_eq!(h.e.store().position("BTCUSDT").unwrap().contracts, 0.004);
}

#[test]
fn row_report_position_decode_rearms_only_its_key_before_a_later_failed_row() {
    let mut h = Harness::with_instruments(PARTIAL_INSTRUMENTS);
    h.seed();
    let bad = POSITION.replace("\"positions\":\"4\"", "\"positions\":null");
    let anonymous = r#"{"positions":null}"#;
    let messages = |logs: Vec<String>| {
        logs.into_iter()
            .filter(|message| message.starts_with("BloFin skipped malformed position row"))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        messages(poll(&mut h, 10_000.0, &format!("{bad},{anonymous}"), "")).len(),
        2
    );
    // This malformed-catalogue row fails the whole read after decoding BTC.
    // No balance/commit clear can explain rearming BTC or erase anonymous memory.
    let failed = format!(r#"{POSITION},{{"instId":"BAD-USDT","positions":null}},{anonymous}"#);
    assert!(messages(poll(&mut h, 20_000.0, &failed, "")).is_empty());
    assert_eq!(
        messages(poll(&mut h, 30_000.0, &format!("{bad},{anonymous}"), "")),
        ["BloFin skipped malformed position row BTC-USDT"]
    );
}

#[test]
fn row_report_once_does_not_upgrade_after_the_retained_position_is_removed() {
    let mut h = Harness::new();
    h.seed();
    let bad = POSITION.replace("\"positions\":\"4\"", "\"positions\":null");
    poll(&mut h, 10_000.0, &bad, "");
    let reports: Vec<_> = h
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.message == "BloFin skipped malformed position row BTC-USDT" =>
            {
                Some(event.level)
            }
            _ => None,
        })
        .collect();
    assert_eq!(reports, [safe_cex_engine::Level::Warn]);
    h.ws(r#"{"arg":{"channel":"positions"},"data":[]}"#);
    assert!(h.e.store().positions().is_empty());
    let logs = poll(&mut h, 20_000.0, &bad, "");
    assert!(
        !logs
            .iter()
            .any(|message| message == "BloFin skipped malformed position row BTC-USDT"),
        "native Once remains quiet even when the next skip would be Error"
    );
}

fn prefix_overflow_event(quantity: &str, average: &str) -> String {
    order_event("prefix-overflow", quantity, "partially_filled").replace(
        "\"averagePrice\":\"100\"",
        &format!("\"averagePrice\":\"{average}\""),
    )
}

fn known_live_prefix_for_price_overflow() -> Harness {
    let mut h = Harness::new();
    h.seed();
    h.e.take_frame(0.0);
    h.ws(&prefix_overflow_event("1", "1"));
    assert_eq!(executions(h.e.take_frame(1.0).unwrap()), [(1.0, 0.001)]);
    h
}

#[test]
fn incremental_price_overflow_emits_no_invalid_financial_receipt() {
    let mut h = known_live_prefix_for_price_overflow();
    // The cumulative quote is finite (1.5e308), but /0.5 overflows.
    h.ws(&prefix_overflow_event("1.5", "1e308"));
    let fills = h.e.take_frame(1.0).map(executions).unwrap_or_default();
    assert!(
        fills.is_empty(),
        "invalid incremental price must be rejected before delivery: {fills:?}"
    );
}

#[test]
fn incremental_price_overflow_keeps_the_same_quantity_correction_eligible() {
    let mut h = known_live_prefix_for_price_overflow();
    h.ws(&prefix_overflow_event("1.5", "1e308"));
    // Drain any old-code invalid receipt to isolate consumed eligibility.
    h.e.take_frame(1.0);
    let corrected = prefix_overflow_event("1.5", "100");
    h.ws(&corrected);
    h.ws(&corrected);
    let fills = h.e.take_frame(1.0).map(executions).unwrap_or_default();
    assert_eq!(
        fills,
        [(298.0, 0.0005)],
        "corrected suffix uses the last proven q1/value1 exactly once"
    );
}
