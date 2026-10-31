//! Official account/account_all/account_all_orders/trades schemas, with source races.
mod support;
use safe_cex_engine::model::{MarginType, OrderSide, OrderStatus, OrderType, PositionSide};
use safe_cex_engine::{Action, Event};
use support::*;
/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 2_000.0;

#[test]
fn account_bootstrap_resolves_identity_and_separates_settings() {
    let (e, _) = ready();
    assert!(!e.store().hedged());
    assert_eq!(e.store().positions().len(), 1);
    let p = &e.store().positions()[0];
    assert_eq!(p.symbol, "ETH");
    assert_eq!(p.contracts, 2.0);
    assert_eq!(e.store().setting("ETH").unwrap().leverage, 10.0);
    assert_eq!(e.store().setting("BTC").unwrap().leverage, 20.0);
    assert_eq!(e.store().balance().free, 900.0);
    assert_eq!(e.store().balance().total, 1000.0);
    assert_eq!(e.store().orders()[0].id, "42");
}
#[test]
fn private_stream_subscribes_to_base_and_authenticated_orders() {
    let (mut e, s) = ready();
    e.handle(NOW + 1.0, Event::WsOpened { socket: s });
    let sent = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(sent.iter().any(|t| t.contains("account_all/718895")));
    assert!(
        sent.iter()
            .any(|t| t.contains("account_all_orders/718895") && t.contains("auth"))
    );
}
#[test]
fn stale_account_snapshot_preserves_newer_position_close() {
    let (mut e, s) = ready();
    e.handle(NOW + 1.0, Event::WsOpened { socket: s });
    let held = requests(&mut e);
    message(
        &mut e,
        s,
        NOW + 2.0,
        r#"{"type":"update/account_all","channel":"account_all:718895","positions":{"1":{"market_id":1,"position":"0","sign":1}}}"#,
    );
    for r in held {
        if r.url.contains("/account?") {
            reply_at(&mut e, NOW + 3.0, &r, 200, ACCOUNT);
        } else {
            reply_at(&mut e, NOW + 3.0, &r, 200, OPEN);
        }
    }
    assert!(e.store().positions().is_empty());
}
#[test]
fn a_zero_size_stream_row_closes_both_long_and_short_net_positions() {
    let (mut e, s) = ready();
    message(
        &mut e,
        s,
        NOW + 1.0,
        r#"{"channel":"account_all:718895","positions":{"1":{"position":"-3","avg_entry_price":"100","margin_mode":1,"initial_margin_fraction":2000}}}"#,
    );
    assert_eq!(e.store().positions()[0].side, PositionSide::Short);
    assert_eq!(
        e.store().setting("ETH").unwrap().margin_type,
        Some(MarginType::Isolated)
    );
    message(
        &mut e,
        s,
        NOW + 2.0,
        r#"{"channel":"account_all:718895","positions":{"1":{"position":"0"}}}"#,
    );
    assert!(e.store().positions().is_empty());
}
#[test]
fn stale_order_snapshot_cannot_resurrect_a_stream_cancel() {
    let (mut e, s) = ready();
    e.handle(NOW + 1.0, Event::WsOpened { socket: s });
    let held = requests(&mut e);
    message(
        &mut e,
        s,
        NOW + 2.0,
        r#"{"channel":"account_all_orders:718895","orders":{"1":[{"order_index":"42","market_id":1,"status":"canceled","initial_base_amount":"2","price":"90"}]}}"#,
    );
    for r in held {
        reply_at(
            &mut e,
            NOW + 3.0,
            &r,
            200,
            if r.url.contains("ActiveOrders") {
                OPEN
            } else {
                ACCOUNT
            },
        );
    }
    assert!(!e.store().orders().iter().any(|o| o.id == "42"));
    message(
        &mut e,
        s,
        NOW + 40_000.0,
        r#"{"channel":"account_all_orders:718895","orders":{"1":[{"order_index":"42","market_id":1,"status":"open","initial_base_amount":"2","price":"90"}]}}"#,
    );
    assert!(!e.store().orders().iter().any(|o| o.id == "42"));
}
#[test]
fn filled_order_and_duplicate_execution_emit_exactly_one_fill() {
    let (mut e, s) = ready();
    e.take_frame(NOW);
    let now = NOW + 6000.0;
    let timestamp = now as u64;
    let frame = format!(
        r#"{{"type":"update/account_all","channel":"account_all:718895","orders":{{"1":[{{"order_index":"42","market_id":1,"status":"filled","initial_base_amount":"2","filled_base_amount":"2","remaining_base_amount":"0","price":"90","updated_at":{timestamp}}}]}},"trades":{{"1":[{{"trade_id":"99","ask_id":"50","bid_id":"42","ask_account_id":"9","bid_account_id":"718895","price":"90","size":"0.25","timestamp":{timestamp}}}]}}}}"#
    );
    for _ in 0..2 {
        message(&mut e, s, now, &frame);
    }
    assert_eq!(
        e.store()
            .orders()
            .iter()
            .find(|o| o.id == "42")
            .unwrap()
            .status,
        OrderStatus::Closed
    );
    assert_eq!(fill_count(e.take_frame(now).unwrap()), 1);
}
#[test]
fn reconnect_history_is_not_a_new_execution() {
    let (mut e, s) = ready();
    e.take_frame(NOW);
    let now = NOW + 6000.0;
    let frame = format!(
        r#"{{"type":"subscribed/account_all","channel":"account_all:718895","trades":{{"1":[{{"trade_id":"99","order_id":"42","side":"buy","price":"90","size":"0.25","timestamp":{}}}]}}}}"#,
        now as u64
    );
    message(&mut e, s, now, &frame);
    let update = frame.replace("subscribed/account_all", "update/account_all");
    message(&mut e, s, now + 1.0, &update);
    assert_eq!(e.take_frame(now + 1.0).map_or(0, fill_count), 0);
}
#[test]
fn failed_scoped_orders_keep_every_last_known_good_order() {
    let (mut e, s) = ready();
    e.handle(NOW + 1.0, Event::WsOpened { socket: s });
    let held = requests(&mut e);
    let all = held
        .into_iter()
        .find(|r| r.url.contains("ActiveOrders"))
        .unwrap();
    reply_at(
        &mut e,
        NOW + 2.0,
        &all,
        400,
        r#"{"code":400,"message":"invalid market parameter"}"#,
    );
    let first = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("ActiveOrders"))
        .unwrap();
    reply_at(
        &mut e,
        NOW + 3.0,
        &first,
        200,
        r#"{"code":200,"orders":[]}"#,
    );
    let second = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("ActiveOrders"))
        .unwrap();
    reply_at(
        &mut e,
        NOW + 4.0,
        &second,
        503,
        r#"{"code":503,"message":"temporarily unavailable"}"#,
    );
    assert_eq!(e.store().orders().len(), 1);
    assert_eq!(e.store().orders()[0].id, "42");
}
#[test]
fn failed_account_reads_preserve_balance_positions_and_settings() {
    let (mut e, s) = ready();
    let balance = e.store().balance().clone();
    let positions = e.store().positions().to_vec();
    e.handle(NOW + 1.0, Event::WsOpened { socket: s });
    for r in requests(&mut e) {
        reply_at(&mut e, NOW + 2.0, &r, 503, "unavailable");
    }
    assert_eq!(*e.store().balance(), balance);
    assert_eq!(e.store().positions(), positions);
    assert_eq!(e.store().setting("ETH").unwrap().leverage, 10.0);
}
#[test]
fn conditional_order_metadata_stays_sticky_across_sparse_updates() {
    let (mut e, s) = ready();
    message(
        &mut e,
        s,
        NOW + 1.0,
        r#"{"channel":"account_all_orders:718895","orders":{"1":[{"order_index":"43","market_id":1,"type":2,"initial_base_amount":"1","price":"80","trigger_price":"85","reduce_only":true,"is_ask":true,"time_in_force":0,"status":"open"}]}}"#,
    );
    message(
        &mut e,
        s,
        NOW + 2.0,
        r#"{"channel":"account_all_orders:718895","orders":{"1":[{"order_index":"43","market_id":1,"type":0,"initial_base_amount":"1","price":"85","status":"open"}]}}"#,
    );
    let order = e.store().orders().iter().find(|o| o.id == "43").unwrap();
    assert_eq!(order.kind, OrderType::StopLoss);
    assert_eq!(order.side, OrderSide::Sell);
    assert!(order.reduce_only);
}
#[test]
fn server_ping_is_answered_on_the_private_socket() {
    let (mut e, s) = ready();
    message(&mut e, s, NOW + 1.0, r#"{"type":"ping"}"#);
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsSend{socket,text}if socket==s&&text.contains("pong")))
    );
}
#[test]
fn missing_secret_keeps_public_mode_without_any_private_reads() {
    let mut opts = options();
    opts.secret = None;
    let (mut e, _) = ready_with(opts);
    assert!(e.store().positions().is_empty());
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a,Action::Http(r)if r.url.contains("/account")))
    );
}
#[test]
fn successful_but_missing_or_malformed_private_lists_never_clear_state() {
    for body in [
        r#"{"code":200}"#,
        r#"{"code":"500","orders":[]}"#,
        r#"{"code":{},"orders":[]}"#,
    ] {
        let (mut e, s) = ready();
        e.handle(NOW + 1.0, Event::WsOpened { socket: s });
        let r = requests(&mut e)
            .into_iter()
            .find(|r| r.url.contains("ActiveOrders"))
            .unwrap();
        reply_at(&mut e, NOW + 2.0, &r, 200, body);
        assert_eq!(e.store().orders().len(), 1, "{body}");
    }
}
#[test]
fn wrong_principal_or_incomplete_account_snapshots_preserve_state() {
    for body in [
        r#"{"code":200,"accounts":[{"index":9,"total_asset_value":"0","available_balance":"0","positions":[]}]}"#,
        r#"{"code":200,"accounts":[{"index":718895}]}"#,
    ] {
        let (mut e, s) = ready();
        e.handle(NOW + 1.0, Event::WsOpened { socket: s });
        let r = requests(&mut e)
            .into_iter()
            .find(|r| r.url.contains("/account?"))
            .unwrap();
        reply_at(&mut e, NOW + 2.0, &r, 200, body);
        assert_eq!(e.store().positions().len(), 1);
        assert_eq!(e.store().balance().free, 900.0);
    }
}
#[test]
fn invalid_fill_cannot_consume_a_later_valid_execution_and_market_scopes_do_not_collide() {
    let (mut e, s) = ready();
    e.take_frame(NOW);
    let now = NOW + 6000.0;
    let timestamp = now as u64;
    let bad = format!(
        r#"{{"channel":"account_all:718895","trades":{{"1":[{{"trade_id":"99","order_id":"42","side":"buy","price":"0","size":"0.25","timestamp":{timestamp}}}]}}}}"#
    );
    message(&mut e, s, now, &bad);
    let good = bad.replace("\"price\":\"0\"", "\"price\":\"90\"");
    message(&mut e, s, now + 1.0, &good);
    let second = good.replace("\"1\":[", "\"2\":[");
    message(&mut e, s, now + 2.0, &second);
    let frame = e.take_frame(now + 2.0).unwrap();
    assert_eq!(fill_count(frame), 2);
    let mut at = 16;
    while at + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let size = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            let side = at + 8 + 4 + 2 * 4 * 2;
            assert_eq!(
                u32::from_le_bytes(frame[side..side + 4].try_into().unwrap()),
                OrderSide::Buy as u32
            );
            assert_eq!(
                u32::from_le_bytes(frame[side + 4..side + 8].try_into().unwrap()),
                OrderSide::Buy as u32
            );
            break;
        }
        at += 8 + size;
    }
}
#[test]
fn api_key_zero_signs_and_invalid_private_index_refuses_at_setup() {
    let mut opts = options();
    opts.api_key_index = Some("0".into());
    let (e, _) = ready_with(opts);
    assert_eq!(e.store().positions().len(), 1);
    let mut opts = options();
    opts.api_key_index = Some("255".into());
    assert!(matches!(
        try_engine(opts),
        Err(safe_cex_engine::signing::SignError::InvalidRequest)
    ));
}

#[test]
fn sparse_position_aggregate_does_not_close_existing_exposure() {
    let (mut e, s) = ready();
    message(
        &mut e,
        s,
        NOW + 1.0,
        r#"{"channel":"account_all:718895","positions":{"1":{"market_id":1,"open_order_count":3}}}"#,
    );
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].contracts, 2.0);
    assert_eq!(e.store().setting("ETH").unwrap().leverage, 10.0);
    assert_eq!(e.store().positions()[0].leverage, 10.0);
    message(
        &mut e,
        s,
        NOW + 2.0,
        r#"{"channel":"account_all:718895","positions":{"1":{"market_id":1,"initial_margin_fraction":2000,"margin_mode":1}}}"#,
    );
    assert_eq!(e.store().setting("ETH").unwrap().leverage, 5.0);
    assert_eq!(e.store().positions()[0].leverage, 5.0);
    assert_eq!(e.store().positions()[0].contracts, 2.0);
    assert_eq!(
        e.store().positions()[0].margin_type,
        Some(MarginType::Isolated)
    );
}
#[test]
fn live_execution_during_initial_five_seconds_is_reported_once() {
    let (mut e, s) = ready();
    e.take_frame(NOW);
    let now = NOW + 1.0;
    let frame = format!(
        r#"{{"type":"update/account_all","channel":"account_all:718895","trades":{{"1":[{{"trade_id":"99","order_id":"42","side":"buy","price":"90","size":"0.25","timestamp":{}}}]}}}}"#,
        now as u64
    );
    for _ in 0..2 {
        message(&mut e, s, now, &frame);
    }
    assert_eq!(fill_count(e.take_frame(now).unwrap()), 1);
}
#[test]
fn native_hyphenated_types_tif_and_cancel_reasons_preserve_lifecycle() {
    let (mut e, s) = ready();
    for (id, kind) in [
        ("51", "stop-loss"),
        ("52", "stop-loss-limit"),
        ("53", "take-profit"),
        ("54", "take-profit-limit"),
    ] {
        let frame = format!(
            r#"{{"channel":"account_all_orders:718895","orders":{{"1":[{{"order_index":"{id}","market_id":1,"type":"{kind}","time_in_force":"post-only","status":"open","initial_base_amount":"1","price":"120"}}]}}}}"#
        );
        message(&mut e, s, NOW + 1.0, &frame);
        let order = e.store().orders().iter().find(|o| o.id == id).unwrap();
        assert_eq!(
            order.kind,
            if kind.starts_with("stop") {
                OrderType::StopLoss
            } else {
                OrderType::TakeProfit
            }
        );
        assert_eq!(
            order.time_in_force,
            Some(safe_cex_engine::model::TimeInForce::PostOnly)
        );
        let cancel = frame.replace("\"status\":\"open\"", "\"status\":\"canceled-post-only\"");
        message(&mut e, s, NOW + 2.0, &cancel);
        assert!(!e.store().orders().iter().any(|o| o.id == id));
        message(&mut e, s, NOW + 3.0, &frame);
        assert!(!e.store().orders().iter().any(|o| o.id == id));
    }
}
#[test]
fn transaction_time_microseconds_prevents_late_position_and_order_regression() {
    let (mut e, s) = ready();
    let newer = ((NOW + 2000.0) * 1000.0) as u64;
    let older = ((NOW + 1000.0) * 1000.0) as u64;
    message(
        &mut e,
        s,
        NOW + 2000.0,
        &format!(
            r#"{{"channel":"account_all:718895","transaction_time":{newer},"positions":{{"1":{{"market_id":1,"position":"0"}}}}}}"#
        ),
    );
    message(
        &mut e,
        s,
        NOW + 3000.0,
        &format!(
            r#"{{"channel":"account_all:718895","transaction_time":{older},"positions":{{"1":{{"market_id":1,"position":"3","avg_entry_price":"100"}}}}}}"#
        ),
    );
    assert!(e.store().positions().is_empty());
    let row = |time, filled| {
        format!(
            r#"{{"channel":"account_all_orders:718895","transaction_time":{time},"orders":{{"1":[{{"order_index":"42","market_id":1,"status":"open","initial_base_amount":"2","filled_base_amount":"{filled}","price":"90"}}]}}}}"#
        )
    };
    message(&mut e, s, NOW + 4000.0, &row(newer, 1));
    message(&mut e, s, NOW + 5000.0, &row(older, 0));
    assert_eq!(
        e.store()
            .orders()
            .iter()
            .find(|o| o.id == "42")
            .unwrap()
            .filled,
        1.0
    );
}
#[test]
fn active_order_stream_does_not_starve_the_serialized_rest_balance() {
    let (mut e, s) = ready();
    e.handle(NOW + 1.0, Event::WsOpened { socket: s });
    let snapshot = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("/account?"))
        .unwrap();
    message(
        &mut e,
        s,
        NOW + 2.0,
        r#"{"channel":"account_all_orders:718895","orders":{"1":[{"order_index":"42","market_id":1,"status":"open","initial_base_amount":"2","filled_base_amount":"0.5","price":"90"}]}}"#,
    );
    reply_at(
        &mut e,
        NOW + 3.0,
        &snapshot,
        200,
        &ACCOUNT.replace("\"900\"", "\"850\""),
    );
    assert_eq!(e.store().balance().free, 850.0);
    assert_eq!(
        e.store()
            .orders()
            .iter()
            .find(|o| o.id == "42")
            .unwrap()
            .filled,
        0.5
    );
}
#[test]
fn direct_account_fill_arrays_accept_both_fill_and_trade_ids_without_duplicate_fields() {
    let (mut e, s) = ready();
    e.take_frame(NOW);
    let frame = format!(
        r#"{{"channel":"account_all:718895","data":{{"fills":[{{"fillId":"f99","tradeId":"99","orderId":"42","symbol":"ETH","side":"BUY","price":"90","size":"0.05","timestamp":{}}}]}}}}"#,
        NOW as u64
    );
    for _ in 0..2 {
        message(&mut e, s, NOW, &frame);
    }
    assert_eq!(fill_count(e.take_frame(NOW).unwrap()), 1);
}
#[test]
fn initial_and_reconnecting_private_handshakes_have_watchdogs_before_open() {
    use safe_cex_engine::SocketChannel;
    let mut timers = Timers::new();
    let mut e = support::engine(options());
    e.start(NOW);
    let meta = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    reply(&mut e, &meta, 200, META);
    let lookup = drain(&mut e, &mut timers)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/account?") => Some(r),
            _ => None,
        })
        .unwrap();
    reply(&mut e, &lookup, 200, ACCOUNT);
    let mut connected_at = NOW;
    let mut actions = drain(&mut e, &mut timers);
    for _ in 0..2 {
        let socket = actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Private),
                    ..
                } => Some(*socket),
                _ => None,
            })
            .expect("private connection");
        let closed = |actions: &[Action]| {
            actions
                .iter()
                .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
        };
        assert!(!closed(&advance(
            &mut e,
            &mut timers,
            connected_at + 179_999.0
        )));
        assert!(closed(&advance(
            &mut e,
            &mut timers,
            connected_at + 180_000.0
        )));
        connected_at += 180_000.0;
        actions = Vec::new();
        while !actions.iter().any(|a| {
            matches!(
                a,
                Action::WsConnect {
                    channel: Some(SocketChannel::Private),
                    ..
                }
            )
        }) {
            connected_at += 1.0;
            assert!(connected_at < NOW + 400_000.0, "private reconnect");
            actions = advance(&mut e, &mut timers, connected_at);
        }
    }
}
#[test]
fn unmapped_owned_position_is_skipped_and_holds_balance_but_zero_rows_remain_legitimate() {
    for quantity in ["2", "0"] {
        let (mut e, socket) = ready();
        let positions = e.store().positions().to_vec();
        let balance = e.store().balance().clone();
        e.handle(NOW + 1., Event::WsOpened { socket });
        let request = requests(&mut e)
            .into_iter()
            .find(|r| r.url.contains("/account?"))
            .unwrap();
        let body = ACCOUNT
            .replace(
                "\"positions\":[",
                &format!(r#""positions":[{{"market_id":999,"position":"{quantity}"}},"#),
            )
            .replace(
                "\"available_balance\":\"900\"",
                "\"available_balance\":\"850\"",
            );
        reply_at(&mut e, NOW + 2., &request, 200, &body);
        if quantity == "2" {
            assert_eq!(e.store().positions().len(), 1);
            assert_eq!(e.store().positions()[0].contracts, positions[0].contracts);
            assert_eq!(*e.store().balance(), balance);
            assert!(e.drain_actions().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Warn, message, .. })) if message == "Lighter skipped position 999 on an unknown market")));
        } else {
            assert_eq!(e.store().positions().len(), positions.len());
            assert_eq!(e.store().positions()[0].contracts, positions[0].contracts);
            assert_eq!(e.store().positions()[0].symbol, positions[0].symbol);
            assert_eq!(e.store().balance().free, 850.);
        }
    }
}
#[test]
fn unmapped_working_order_is_skipped_with_a_warning_and_terminal_unknown_rows_are_ignored() {
    for status in ["open", "canceled", "filled"] {
        let (mut e, socket) = ready();
        let previous = e.store().orders().to_vec();
        e.handle(NOW + 1., Event::WsOpened { socket });
        let request = requests(&mut e)
            .into_iter()
            .find(|r| r.url.contains("accountActiveOrders"))
            .unwrap();
        let body = format!(
            r#"{{"code":200,"orders":[{{"order_index":"999","market_id":999,"status":"{status}","remaining_base_amount":"2"}}]}}"#
        );
        reply_at(&mut e, NOW + 2., &request, 200, &body);
        // The snapshot commits either way; its omission of 42 removes it.
        assert_eq!(previous.len(), 1);
        assert!(e.store().orders().is_empty());
        assert_eq!(
            e.drain_actions().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Warn, message, .. })) if message == "Lighter skipped order 999 on an unknown market")),
            status == "open"
        );
    }
}
#[test]
fn unmapped_initial_position_values_the_balance_only_with_its_native_pnl() {
    for pnl in [true, false] {
        let mut e = support::engine(options());
        e.start(NOW);
        let mut bad = ACCOUNT.replace("\"market_id\":1", "\"market_id\":999");
        if !pnl {
            bad = bad.replace(r#""unrealized_pnl":"0","#, "");
        }
        let mut warning = false;
        for _ in 0..20 {
            let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
            if actions.is_empty() {
                break;
            }
            for action in actions {
                match action {
                    Action::Http(r) => {
                        let body =
                            if r.url.contains("orderBookDetails") || r.url.contains("orderBooks") {
                                META
                            } else if r.url.contains("exchangeStats") {
                                r#"{"code":200,"order_book_stats":[]}"#
                            } else if r.url.contains("accountActiveOrders") {
                                OPEN
                            } else if r.url.contains("/account?") {
                                &bad
                            } else {
                                panic!("unexpected request {}", r.url)
                            };
                        reply(&mut e, &r, 200, body);
                    }
                    Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                        level: safe_cex_engine::Level::Warn,
                        message,
                        ..
                    })) if message == "Lighter skipped position 999 on an unknown market" => {
                        warning = true
                    }
                    _ => {}
                }
            }
        }
        assert!(warning);
        assert!(e.store().loaded().positions);
        assert!(e.store().positions().is_empty());
        assert_eq!(e.store().loaded().balance, pnl);
        if pnl {
            assert_eq!(e.store().balance().total, 1000.0);
            assert_eq!(e.store().balance().free, 900.0);
        }
    }
}

#[test]
fn periodic_account_policy_changes_preserve_inflight_reads_and_fence_stale_timers() {
    let (mut e, _) = ready();
    let now = NOW + 1000.;
    e.drain_actions().for_each(drop);
    e.handle(
        now,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let stale: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::ClearTimer { timer } => Some(timer),
            _ => None,
        })
        .collect();
    assert_eq!(stale.len(), 1, "one recurring account registration");
    let mut obsolete = stale;
    let mut current = None;
    for interval in [100_000_f64, 1_500.] {
        e.handle(
            now,
            Event::PollScaleChanged {
                scale: interval / POLL_MS,
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].1, now + interval.max(0.0));
        if let Some((timer, _)) = current.replace(timers[0]) {
            obsolete.push(timer);
        }
    }
    for timer in obsolete {
        e.handle(now, Event::TimerFired { timer });
        assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
    }
    let (timer, due) = current.unwrap();
    e.handle(due, Event::TimerFired { timer });
    let mut pending: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, due)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert!(
        pending.iter().any(|r| r.url.contains("/account?")),
        "actual native account request"
    );
    // Pause while REST is in flight. Completion may finish the current read
    // chain, but must not recreate an active recurring host timer.
    e.handle(
        due,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    e.drain_actions().for_each(drop);
    for _ in 0..12 {
        if pending.is_empty() {
            break;
        }
        for r in std::mem::take(&mut pending) {
            let body = if r.url.contains("accountActiveOrders") {
                OPEN
            } else {
                ACCOUNT
            };
            e.handle(
                due + 1.,
                Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
            pending.extend(e.drain_actions().filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            }));
        }
    }
    assert!(pending.is_empty(), "all fixture account reads complete");
    e.handle(
        due + 2.,
        Event::PollScaleChanged {
            scale: 10_000. / POLL_MS,
        },
    );
    let resumed: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(
        resumed.len(),
        1,
        "completion retains one paused registration"
    );
    assert_eq!(resumed[0].1, due + 2. + 10_000_f64.max(0.0));
}

#[test]
fn delayed_live_execution_is_not_lost_after_five_seconds() {
    let (mut e, s) = ready();
    e.take_frame(NOW);
    let event = format!(
        r#"{{"type":"update/account_all","channel":"account_all:718895","trades":{{"1":[{{"trade_id":"late-99","order_id":"42","side":"buy","price":"90","size":"0.25","timestamp":{}}}]}}}}"#,
        (NOW + 1.0) as u64
    );
    message(&mut e, s, NOW + 10_000.0, &event);
    message(&mut e, s, NOW + 10_001.0, &event);
    assert_eq!(fill_count(e.take_frame(NOW + 10_001.0).unwrap()), 1);
    let history = event
        .replace("update/account_all", "subscribed/account_all")
        .replace("late-99", "history-100");
    message(&mut e, s, NOW + 10_002.0, &history);
    message(
        &mut e,
        s,
        NOW + 10_003.0,
        &history.replace("subscribed/account_all", "update/account_all"),
    );
    assert_eq!(e.take_frame(NOW + 10_003.0).map_or(0, fill_count), 0);
}
#[test]
fn fill_stamped_ahead_of_the_host_clock_is_still_emitted() {
    let (mut e, s) = ready();
    e.take_frame(NOW);
    let frame = format!(
        r#"{{"type":"update/account_all","channel":"account_all:718895","trades":{{"1":[{{"trade_id":"ahead-1","ask_id":"50","bid_id":"42","ask_account_id":"9","bid_account_id":"718895","price":"90","size":"0.25","timestamp":{}}}]}}}}"#,
        (NOW + 65_000.0) as u64
    );
    message(&mut e, s, NOW + 5000.0, &frame);
    assert_eq!(fill_count(e.take_frame(NOW + 5000.0).unwrap()), 1);
}

const BASELINE: &str =
    r#"{"type":"subscribed/account_all","channel":"account_all:718895","account":{}}"#;
fn trade_row(id: &str, at: f64) -> String {
    format!(
        r#"{{"trade_id":"{id}","market_id":1,"ask_id":"50","bid_id":"42","ask_account_id":"9","bid_account_id":"718895","price":"90","size":"0.25","timestamp":{}}}"#,
        at as u64
    )
}
/// Drops the private socket at `NOW + 1000` and reopens it, returning the
/// new socket and the requests the reopen issued.
fn reconnect(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_lighter::Lighter>,
    s: safe_cex_engine::SocketId,
) -> (safe_cex_engine::SocketId, Vec<safe_cex_engine::HttpRequest>) {
    e.drain_actions().for_each(drop);
    e.handle(
        NOW + 1000.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .expect("reconnect timer");
    e.handle(NOW + 2000.0, Event::TimerFired { timer });
    let socket = e
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .expect("reconnect");
    e.handle(NOW + 3000.0, Event::WsOpened { socket });
    (socket, requests(e))
}
#[test]
fn fills_made_while_the_private_socket_was_down_are_backfilled_once() {
    let (mut e, s) = ready();
    message(&mut e, s, NOW + 500.0, BASELINE);
    e.take_frame(NOW + 500.0);
    let (socket, reads) = reconnect(&mut e, s);
    let backfill = reads
        .iter()
        .find(|r| r.url.contains("/api/v1/trades?"))
        .expect("fill backfill read");
    assert!(backfill.url.contains("account_index=718895"));
    assert!(backfill.headers.iter().any(|(k, _)| *k == "authorization"));
    let body = format!(
        r#"{{"code":200,"trades":[{},{},{},{}]}}"#,
        trade_row("gap-2", NOW + 1600.0),
        trade_row("gap-1", NOW + 1500.0),
        trade_row("before-session", NOW - 60_000.0),
        // Microsecond stamps are normalized, not read as far-future milliseconds.
        trade_row("before-session-us", (NOW - 60_000.0) * 1000.0),
    );
    reply_at(&mut e, NOW + 3100.0, backfill, 200, &body);
    assert_eq!(fill_count(e.take_frame(NOW + 3100.0).unwrap()), 2);
    // The reconnect snapshot repeats one backfilled fill and carries one more.
    let snapshot = format!(
        r#"{{"type":"subscribed/account_all","channel":"account_all:718895","trades":{{"1":[{},{},{}]}}}}"#,
        trade_row("gap-2", NOW + 1600.0),
        trade_row("gap-3", NOW + 1700.0),
        trade_row("older-history", NOW - 60_000.0),
    );
    message(&mut e, socket, NOW + 3200.0, &snapshot);
    assert_eq!(fill_count(e.take_frame(NOW + 3200.0).unwrap()), 1);
    reply_at(&mut e, NOW + 3300.0, backfill, 200, &body);
    assert_eq!(e.take_frame(NOW + 3300.0).map_or(0, fill_count), 0);
}
#[test]
fn a_full_backfill_page_inside_the_outage_reads_the_next_page() {
    let (mut e, s) = ready();
    message(&mut e, s, NOW + 500.0, BASELINE);
    let (_, reads) = reconnect(&mut e, s);
    let backfill = reads
        .iter()
        .find(|r| r.url.contains("/api/v1/trades?"))
        .unwrap();
    let rows = (0..100)
        .map(|i| trade_row(&format!("t{i}"), NOW + 2000.0 - i as f64))
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(r#"{{"code":200,"next_cursor":"page-2","trades":[{rows}]}}"#);
    reply_at(&mut e, NOW + 3100.0, backfill, 200, &body);
    assert_eq!(fill_count(e.take_frame(NOW + 3100.0).unwrap()), 100);
    let next = requests(&mut e);
    assert!(
        next.iter()
            .any(|r| r.url.contains("/api/v1/trades?") && r.url.contains("cursor=page-2"))
    );
}
#[test]
fn a_drop_before_the_baseline_snapshot_replays_no_history() {
    let (mut e, s) = ready();
    let (socket, reads) = reconnect(&mut e, s);
    assert!(!reads.iter().any(|r| r.url.contains("/api/v1/trades?")));
    let snapshot = format!(
        r#"{{"type":"subscribed/account_all","channel":"account_all:718895","trades":{{"1":[{}]}}}}"#,
        trade_row("pre-session", NOW - 5000.0),
    );
    message(&mut e, socket, NOW + 3200.0, &snapshot);
    assert_eq!(e.take_frame(NOW + 3200.0).map_or(0, fill_count), 0);
}
#[test]
fn fills_during_a_quiet_but_pinging_stretch_are_backfilled_once() {
    let (mut e, socket, mut timers) = ready_tracked(options());
    e.handle(NOW, Event::WsOpened { socket });
    drain(&mut e, &mut timers);
    message(&mut e, socket, NOW + 500.0, BASELINE);
    e.take_frame(NOW + 500.0);
    let mut closed_at = None;
    for step in 1..=20 {
        let now = NOW + f64::from(step) * 10_000.0;
        let mut actions = advance(&mut e, &mut timers, now - 1.0);
        message(&mut e, socket, now, r#"{"type":"pong"}"#);
        actions.extend(advance(&mut e, &mut timers, now));
        if actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
        {
            closed_at = Some(now);
            break;
        }
    }
    let at = closed_at.expect("stale stream reconnects") + 1_000.0;
    let next = advance(&mut e, &mut timers, at)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(at + 100.0, Event::WsOpened { socket: next });
    let reads = requests(&mut e);
    let backfill = reads
        .iter()
        .find(|r| r.url.contains("/api/v1/trades?"))
        .expect("fill backfill read");
    let quiet = [
        trade_row("quiet-1", NOW + 30_000.0),
        trade_row("quiet-2", NOW + 60_000.0),
    ];
    let body = format!(r#"{{"code":200,"trades":[{},{}]}}"#, quiet[1], quiet[0]);
    reply_at(&mut e, at + 200.0, backfill, 200, &body);
    assert_eq!(fill_count(e.take_frame(at + 200.0).unwrap()), 2);
    let snapshot = format!(
        r#"{{"type":"subscribed/account_all","channel":"account_all:718895","trades":{{"1":[{},{}]}}}}"#,
        quiet[0], quiet[1]
    );
    message(&mut e, next, at + 300.0, &snapshot);
    assert_eq!(e.take_frame(at + 300.0).map_or(0, fill_count), 0);
}
#[test]
fn a_rejected_backfill_retries_once_and_then_emits() {
    let (mut e, s) = ready();
    message(&mut e, s, NOW + 500.0, BASELINE);
    e.take_frame(NOW + 500.0);
    let (_, reads) = reconnect(&mut e, s);
    let first = reads
        .iter()
        .find(|r| r.url.contains("/api/v1/trades?"))
        .unwrap();
    reply_at(
        &mut e,
        NOW + 3100.0,
        first,
        200,
        r#"{"code":429,"message":"too many requests"}"#,
    );
    let retry = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == NOW + 5100.0 => Some(timer),
            _ => None,
        })
        .expect("backfill retry timer");
    e.take_frame(NOW + 3100.0);
    e.handle(NOW + 5100.0, Event::TimerFired { timer: retry });
    let reads = requests(&mut e);
    let second = reads
        .iter()
        .find(|r| r.url.contains("/api/v1/trades?"))
        .expect("backfill retried");
    let body = format!(
        r#"{{"code":200,"trades":[{}]}}"#,
        trade_row("gap-1", NOW + 1500.0)
    );
    reply_at(&mut e, NOW + 5200.0, second, 200, &body);
    assert_eq!(fill_count(e.take_frame(NOW + 5200.0).unwrap()), 1);
}
#[test]
fn a_socket_drop_after_pings_without_payloads_backfills_from_the_last_payload() {
    let (mut e, s) = ready();
    message(&mut e, s, NOW + 500.0, BASELINE);
    message(&mut e, s, NOW + 89_000.0, r#"{"type":"pong"}"#);
    e.drain_actions().for_each(drop);
    e.handle(
        NOW + 90_000.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 91_000.0, Event::TimerFired { timer });
    let socket = e
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 91_100.0, Event::WsOpened { socket });
    let reads = requests(&mut e);
    let backfill = reads
        .iter()
        .find(|r| r.url.contains("/api/v1/trades?"))
        .unwrap();
    let body = format!(
        r#"{{"code":200,"trades":[{}]}}"#,
        trade_row("quiet-1", NOW + 30_000.0)
    );
    reply_at(&mut e, NOW + 91_200.0, backfill, 200, &body);
    assert_eq!(fill_count(e.take_frame(NOW + 91_200.0).unwrap()), 1);
}
#[test]
fn bad_snapshot_rows_keep_their_stored_copies_until_a_valid_snapshot_omits_them() {
    let (mut e, s) = ready();
    let order = |id: u32, market: u32, kind: &str, price: &str| {
        format!(
            r#"{{"order_index":"{id}","market_id":{market},"status":"open","type":"{kind}","time_in_force":1,"is_ask":false,"initial_base_amount":"2","filled_base_amount":"0","remaining_base_amount":"2","price":"{price}"}}"#
        )
    };
    let position = |market: u32, size: &str| {
        format!(
            r#"{{"market_id":{market},"position":"{size}","sign":1,"avg_entry_price":"100","unrealized_pnl":"0","initial_margin_fraction":1000,"margin_mode":0,"liquidation_price":"50"}}"#
        )
    };
    let resync = |e: &mut safe_cex_engine::Engine<_>,
                  now: f64,
                  orders: &[String],
                  positions: &[String]| {
        e.handle(now, Event::WsOpened { socket: s });
        let orders = format!(r#"{{"code":200,"orders":[{}]}}"#, orders.join(","));
        let account = format!(
            r#"{{"code":200,"accounts":[{{"index":718895,"collateral":"1000","total_asset_value":"1000","available_balance":"900","positions":[{}]}}]}}"#,
            positions.join(",")
        );
        for r in requests(e) {
            let body = if r.url.contains("ActiveOrders") {
                &orders
            } else {
                &account
            };
            reply_at(e, now, &r, 200, body);
        }
        e.drain_actions()
            .filter_map(|a| match a {
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    message,
                    ..
                })) => Some(message),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    resync(
        &mut e,
        NOW + 1.0,
        &[
            order(42, 1, "limit", "90"),
            order(43, 2, "limit", "80"),
            order(44, 1, "limit", "70"),
            order(46, 1, "limit", "60"),
        ],
        &[position(1, "2"), position(2, "1")],
    );
    assert_eq!(e.store().orders().len(), 4);
    assert_eq!(e.store().positions().len(), 2);
    let logs = resync(
        &mut e,
        NOW + 2.0,
        &[
            order(42, 1, "limit", "91"),
            order(43, 2, "limit", "bad"),
            order(44, 1, "mystery", "71"),
        ],
        &[position(1, "3"), position(2, "bad")],
    );
    let price = |e: &safe_cex_engine::Engine<_>, id: &str| e.store().order(id).map(|o| o.price);
    let size = |e: &safe_cex_engine::Engine<_>, symbol: &str| {
        e.store()
            .positions()
            .iter()
            .find(|p| p.symbol == symbol)
            .map(|p| p.contracts)
    };
    assert_eq!(price(&e, "42"), Some(91.0));
    assert_eq!(price(&e, "43"), Some(80.0));
    assert_eq!(
        e.store().order("44").map(|o| (o.price, o.kind)),
        Some((70.0, OrderType::Limit))
    );
    assert_eq!(price(&e, "46"), None);
    assert_eq!(size(&e, "ETH"), Some(3.0));
    assert_eq!(size(&e, "BTC"), Some(1.0));
    for expected in [
        "Lighter skipped unmappable order 43",
        "Lighter skipped unmappable order 44",
        "Lighter skipped malformed position BTC",
    ] {
        assert!(logs.iter().any(|m| m == expected), "{expected}: {logs:?}");
    }
    assert!(
        !logs
            .iter()
            .any(|m| m.contains("bad") || m.contains("mystery"))
    );
    resync(
        &mut e,
        NOW + 3.0,
        &[order(42, 1, "limit", "91")],
        &[position(1, "3")],
    );
    assert_eq!(e.store().orders().len(), 1);
    assert_eq!(size(&e, "BTC"), None);
    assert_eq!(e.store().positions().len(), 1);
}
#[test]
fn unknown_snapshot_order_type_is_never_invented_as_a_limit() {
    let (mut e, s) = ready();
    e.handle(NOW + 1.0, Event::WsOpened { socket: s });
    let r = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("ActiveOrders"))
        .unwrap();
    reply_at(
        &mut e,
        NOW + 2.0,
        &r,
        200,
        &OPEN
            .replace(r#""order_index":"42""#, r#""order_index":"47""#)
            .replace(r#""type":"limit""#, r#""type":"mystery""#),
    );
    assert!(e.store().order("47").is_none());
    assert!(e.store().order("42").is_none());
}
#[test]
fn spot_rows_in_private_snapshots_are_skipped_silently_and_perp_rows_commit() {
    let meta = META.replace(
        r#""order_book_details":["#,
        r#""order_book_details":[{"market_id":2048,"symbol":"ETH/USDC","status":"active","market_type":"spot","supported_size_decimals":3,"supported_price_decimals":2,"min_base_amount":"0.001","min_quote_amount":"1"},"#,
    );
    let (mut e, s) = ready_with_meta(options(), &meta);
    assert!(e.store().market("ETH/USDC").is_none());
    e.handle(NOW + 1.0, Event::WsOpened { socket: s });
    let spot_order = r#"{"order_index":"77","market_id":2048,"status":"open","type":"limit","time_in_force":1,"is_ask":false,"initial_base_amount":"1","remaining_base_amount":"1","price":"10"}"#;
    for r in requests(&mut e) {
        let body = if r.url.contains("ActiveOrders") {
            OPEN.replace(r#""orders":["#, &format!(r#""orders":[{spot_order},"#))
        } else {
            ACCOUNT
                .replace(
                    r#""positions":["#,
                    r#""positions":[{"market_id":2048,"position":"5","sign":1},"#,
                )
                .replace(
                    r#""available_balance":"900""#,
                    r#""available_balance":"850""#,
                )
        };
        reply_at(&mut e, NOW + 2.0, &r, 200, &body);
    }
    let logs = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                message,
                ..
            })) => Some(message),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        !logs
            .iter()
            .any(|m| m.contains("skipped") || m.contains("failed")),
        "{logs:?}"
    );
    assert_eq!(
        e.store()
            .orders()
            .iter()
            .map(|o| o.id.as_str())
            .collect::<Vec<_>>(),
        ["42"]
    );
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().balance().free, 850.0);
}
#[test]
fn twap_and_liquidation_orders_map_alike_from_the_stream_and_rest() {
    for kind in [
        r#""twap""#,
        r#""twap-sub""#,
        r#""liquidation""#,
        "6",
        "7",
        "8",
    ] {
        let (mut e, s) = ready();
        let row = |id: u32, price: &str| {
            format!(
                r#"{{"order_index":"{id}","market_id":1,"status":"open","type":{kind},"time_in_force":1,"is_ask":false,"initial_base_amount":"2","filled_base_amount":"0","remaining_base_amount":"2","price":"{price}"}}"#
            )
        };
        message(
            &mut e,
            s,
            NOW + 1.0,
            &format!(
                r#"{{"channel":"account_all_orders:718895","orders":{{"1":[{}]}}}}"#,
                row(60, "80")
            ),
        );
        assert_eq!(
            e.store().order("60").map(|o| o.kind),
            Some(OrderType::Limit),
            "{kind}"
        );
        e.handle(NOW + 2.0, Event::WsOpened { socket: s });
        let r = requests(&mut e)
            .into_iter()
            .find(|r| r.url.contains("ActiveOrders"))
            .unwrap();
        reply_at(
            &mut e,
            NOW + 3.0,
            &r,
            200,
            &format!(
                r#"{{"code":200,"orders":[{},{}]}}"#,
                row(60, "81"),
                row(61, "82")
            ),
        );
        for (id, price) in [("60", 81.0), ("61", 82.0)] {
            assert_eq!(
                e.store().order(id).map(|o| (o.kind, o.price)),
                Some((OrderType::Limit, price)),
                "{kind} {id}"
            );
        }
    }
}
#[test]
fn skipped_position_row_with_only_a_market_id_keeps_the_stored_copy() {
    let (mut e, socket) = ready();
    let mut market = e.store().market("ETH").unwrap().clone();
    market.id = "999".into();
    market.symbol = "SOL".into();
    let mut markets = e.store().markets().to_vec();
    markets.push(market);
    e.store_mut().set_markets(markets);
    let mut sol = e.store().position("ETH").unwrap().clone();
    sol.id = "SOL".into();
    sol.symbol = "SOL".into();
    sol.contracts = 5.0;
    let mut positions = e.store().positions().to_vec();
    positions.push(sol);
    e.store_mut().set_positions(positions);
    e.handle(NOW + 1., Event::WsOpened { socket });
    let request = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("/account?"))
        .unwrap();
    let body = ACCOUNT.replace(
        "\"positions\":[",
        r#""positions":[{"market_id":999,"position":"5"},"#,
    );
    reply_at(&mut e, NOW + 2., &request, 200, &body);
    assert!(e.drain_actions().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message == "Lighter skipped position SOL on an unknown market")));
    assert_eq!(e.store().position("SOL").map(|p| p.contracts), Some(5.0));
    assert_eq!(e.store().position("ETH").map(|p| p.contracts), Some(2.0));
}
#[test]
fn skipped_position_rows_keep_the_balance_updating() {
    for (row, eth) in [
        // Malformed owned row: its stored copy stays and carries its PnL.
        (r#"{"market_id":1,"position":"bad"}"#, Some(2.0)),
        // Flat row that fails mapping: a valid closure, not a skip.
        (
            r#"{"market_id":1,"position":"0","avg_entry_price":"bad"}"#,
            None,
        ),
    ] {
        let (mut e, socket) = ready();
        assert_eq!(e.store().balance().free, 900.0);
        e.handle(NOW + 1., Event::WsOpened { socket });
        let request = requests(&mut e)
            .into_iter()
            .find(|r| r.url.contains("/account?"))
            .unwrap();
        let body = format!(
            r#"{{"code":200,"accounts":[{{"index":718895,"collateral":"1100","total_asset_value":"1100","available_balance":"850","positions":[{row}]}}]}}"#
        );
        reply_at(&mut e, NOW + 2., &request, 200, &body);
        assert_eq!(e.store().position("ETH").map(|p| p.contracts), eth, "{row}");
        let balance = e.store().balance();
        assert_eq!(
            (balance.total, balance.free, balance.used),
            (1100.0, 850.0, 250.0),
            "{row}"
        );
    }
}

#[test]
fn exhausted_backfill_is_account_scoped_without_a_query_owner() {
    use safe_cex_engine::log::code;
    use safe_cex_engine::{Level, Notice, Value};
    let (mut e, s) = ready();
    e.set_log_level(Level::Error);
    message(&mut e, s, NOW + 500.0, BASELINE);
    e.take_frame(NOW + 500.0);
    let (_, reads) = reconnect(&mut e, s);
    let first = reads
        .iter()
        .find(|r| r.url.contains("/api/v1/trades?"))
        .unwrap();
    reply_at(
        &mut e,
        NOW + 3100.0,
        first,
        200,
        r#"{"code":429,"message":"too many requests"}"#,
    );
    let retry = e
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if at_ms == NOW + 5100.0 => Some(timer),
            _ => None,
        })
        .expect("one existing backfill retry");
    e.handle(NOW + 5100.0, Event::TimerFired { timer: retry });
    let reads = requests(&mut e);
    let second = reads
        .iter()
        .find(|r| r.url.contains("/api/v1/trades?"))
        .unwrap();
    reply_at(
        &mut e,
        NOW + 5200.0,
        second,
        200,
        r#"{"code":429,"message":"too many requests"}"#,
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let events = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::Event(event)) if event.code == code::HISTORY_INCOMPLETE => {
                Some(event)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].level, Level::Warn);
    assert!(events[0].message.starts_with("Lighter fill backfill failed; fills during the outage may be missing until the next reconnect:"));
    assert_eq!(
        events[0].fields,
        vec![("scope", Value::Str("account/background".into()))]
    );
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
            | Action::SetTimer { .. }
    )));
    assert_eq!(e.take_frame(NOW + 5200.0).map_or(0, fill_count), 0);
}

/// Legacy notifies from the stream only, for an event under 5s old and more
/// than 5s after connecting, and never for the user's own cancel.
#[test]
fn a_rejected_order_reports_its_reason_once_from_the_stream() {
    let (mut e, s) = ready();
    let row = |id: &str, status: &str, extra: &str, time: f64| {
        format!(
            r#"{{"channel":"account_all_orders:718895","timestamp":{},"orders":{{"1":[{{"order_index":"{id}","market_id":1,"type":"limit","time_in_force":"post-only","status":"{status}","initial_base_amount":"1","price":"120"{extra}}}]}}}}"#,
            time as u64
        )
    };
    let notices = |e: &mut safe_cex_engine::Engine<safe_cex_venue_lighter::Lighter>| {
        e.drain_actions()
            .filter_map(|a| match a {
                Action::Notice(safe_cex_engine::Notice::Event(event))
                    if event.code == "order.rejected" =>
                {
                    Some((event.level, event.message))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let cases = [
        // Within 5s of connecting: not reported, as legacy.
        ("60", "canceled-post-only", "", NOW + 1_000.0, None),
        (
            "61",
            "canceled-post-only",
            "",
            NOW + 10_000.0,
            Some("canceled-post-only"),
        ),
        (
            "62",
            "canceled-reduce-only",
            "",
            NOW + 11_000.0,
            Some("canceled-reduce-only"),
        ),
        (
            "63",
            "canceled",
            r#","cancel_reason":"MARGIN""#,
            NOW + 12_000.0,
            Some("MARGIN"),
        ),
        (
            "64",
            "canceled",
            r#","cancelReason":"USER_CANCELED""#,
            NOW + 13_000.0,
            None,
        ),
        ("65", "canceled", "", NOW + 14_000.0, None),
        (
            "66",
            "filled",
            r#","cancel_reason":"MARGIN""#,
            NOW + 15_000.0,
            None,
        ),
        ("67", "open", "", NOW + 16_000.0, None),
    ];
    for (id, status, extra, now, expected) in cases {
        notices(&mut e);
        message(&mut e, s, now, &row(id, status, extra, now));
        let seen = notices(&mut e);
        assert_eq!(
            seen,
            expected
                .map(|m| vec![(safe_cex_engine::Level::Error, m.to_string())])
                .unwrap_or_default(),
            "{id} {status}"
        );
    }
    // The same terminal row again, and a stale event, report nothing.
    let now = NOW + 17_000.0;
    message(&mut e, s, now, &row("61", "canceled-post-only", "", now));
    assert!(notices(&mut e).is_empty());
    message(
        &mut e,
        s,
        now,
        &row("68", "canceled-post-only", "", now - 6_000.0),
    );
    assert!(notices(&mut e).is_empty());
}
