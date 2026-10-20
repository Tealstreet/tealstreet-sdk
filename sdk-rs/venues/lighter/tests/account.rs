//! Official account/account_all/account_all_orders/trades schemas, with source races.
mod support;
use safe_cex_engine::model::{MarginType, OrderSide, OrderStatus, OrderType, PositionSide};
use safe_cex_engine::{Action, Event};
use support::*;
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
    assert_eq!(fill_count(e.take_frame(now + 1.0).unwrap()), 0);
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
fn api_key_zero_is_valid_and_255_is_the_unsupported_sentinel() {
    let mut opts = options();
    opts.api_key_index = Some("0".into());
    let (e, _) = ready_with(opts);
    assert_eq!(e.store().positions().len(), 1);
    let mut opts = options();
    opts.api_key_index = Some("255".into());
    let (e, _) = ready_with(opts);
    assert!(e.store().positions().is_empty());
    assert_eq!(e.store().balance().free, 0.0);
}
