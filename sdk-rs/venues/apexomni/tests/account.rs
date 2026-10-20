mod support;
use safe_cex_engine::model::{MarginType, OrderStatus, OrderType, PositionSide};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, SocketChannel, SocketId,
};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use support::*;

const ACCOUNT: &str = r#"{"code":0,"data":{"id":"611779989544632629","l2key":"key","positions":[{"symbol":"BTC-USDT","side":"LONG","entryPrice":"100","size":"2","customImr":"0.1","unrealisedPnl":"4","curRealisedPnl":"3","liqPrice":"50"}],"contractAccount":{"makerFeeRate":"0.0002","takerFeeRate":"0.0005"}}}"#;
const EMPTY_ACCOUNT: &str = r#"{"code":0,"data":{"id":"611779989544632629","positions":[],"contractAccount":{"makerFeeRate":"0.0002","takerFeeRate":"0.0005"}}}"#;
const EMPTY_ORDERS: &str = r#"{"code":0,"data":[]}"#;
const ORDERS: &str = r#"{"code":0,"data":[{"id":"parent","symbol":"BTC-USDT","status":"OPEN","type":"LIMIT","side":"BUY","size":"2","price":"100","cumSuccessFillSize":"0.25","clientId":"caller-id","reduceOnly":false,"timeInForce":"POST_ONLY","openSlParam":{"triggerPrice":"90","triggerPriceType":"INDEX"},"openTpParams":{"triggerPrice":"110","triggerPriceType":"MARKET"}}]}"#;
const BALANCE: &str = r#"{"code":0,"data":{"totalEquityValue":"1000","availableBalance":"800","initialMargin":"100","maintenanceMargin":"20"}}"#;

fn seeded() -> (TestEngine, SocketId, Vec<HttpRequest>) {
    let mut e = Engine::new(ApexOmni::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("passphrase".into()),
        omni_key_seed: Some("11".repeat(65)),
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let mut socket = None;
    let mut http = vec![];
    for action in e.drain_actions() {
        match action {
            Action::WsConnect {
                socket: s,
                channel: Some(SocketChannel::Private),
                ..
            } => socket = Some(s),
            Action::Http(r) => http.push(r),
            _ => {}
        }
    }
    (e, socket.expect("private socket"), http)
}
fn ready() -> (TestEngine, SocketId) {
    let (mut e, socket, http) = seeded();
    e.handle(20.0, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    for r in http {
        let body = if r.url.ends_with("/v3/account") {
            ACCOUNT
        } else {
            ORDERS
        };
        response(&mut e, &r, 30.0, body);
    }
    let balance = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("/v3/account-balance"))
        .unwrap();
    e.store_mut().update_ticker("BTCUSDT", |t| t.last = 102.0);
    response(&mut e, &balance, 40.0, BALANCE);
    (e, socket)
}
struct DecodedFill {
    price: f64,
    amount: f64,
}
fn fills(e: &mut TestEngine) -> Vec<DecodedFill> {
    let Some(frame) = e.take_frame(100.0) else {
        return vec![];
    };
    let Some((_, payload)) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::FILLS)
    else {
        return vec![];
    };
    let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
    let rows = (4 + 12 * count + 7) & !7;
    (0..count)
        .map(|index| {
            let offset = rows + index * 16;
            DecodedFill {
                price: f64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap()),
                amount: f64::from_le_bytes(payload[offset + 8..offset + 16].try_into().unwrap()),
            }
        })
        .collect()
}
fn frame(kind: &str, body: &str) -> String {
    format!(r#"{{"topic":"ws_zk_accounts_v3","type":"{kind}","contents":{body}}}"#)
}
fn fill(id: &str, order: &str, size: &str) -> String {
    format!(
        r#"{{"id":"{id}","orderId":"{order}","symbol":"BTC-USDT","side":"BUY","size":"{size}","price":"102","fee":"-0.02","createdAt":1700000000123}}"#
    )
}
fn order(id: &str, status: &str, cumulative: &str) -> String {
    format!(
        r#"{{"id":"{id}","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"{status}","size":"2","price":"100","latestMatchFillPrice":"102","cumSuccessFillSize":"{cumulative}","cumSuccessFillFee":"0.02","updatedAt":1700000000123}}"#
    )
}

#[test]
fn login_precedes_immediate_subscription_and_ping_with_private_role() {
    let (mut e, socket, _) = seeded();
    e.handle(20.0, Event::WsOpened { socket });
    let messages: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::WsSend { socket: s, text } if s == socket => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 3);
    assert!(messages[0].contains(r#""op":"login""#));
    assert!(messages[0].contains(r#"\"requestPath\":\"/ws/accounts\""#));
    assert_eq!(
        messages[1],
        r#"{"op":"subscribe","args":["ws_zk_accounts_v3","ws_user_v1","ws_notify_v1"]}"#
    );
    assert_eq!(messages[2], r#"{"op":"ping"}"#);
}

#[test]
fn snapshot_commits_orders_positions_and_metadata_only_when_both_reads_succeed() {
    let (mut e, _, http) = seeded();
    let positions = http
        .iter()
        .find(|r| r.url.ends_with("/v3/account"))
        .unwrap();
    let orders = http
        .iter()
        .find(|r| r.url.ends_with("/v3/open-orders"))
        .unwrap();
    response(&mut e, positions, 20.0, ACCOUNT);
    assert!(e.store().positions().is_empty());
    assert!(!e.store().loaded().positions);
    response(&mut e, orders, 30.0, ORDERS);
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().orders().len(), 3);
    assert!(e.store().loaded().positions && e.store().loaded().orders);
    assert!(e.store().hedged());
    let long = e
        .store()
        .positions()
        .iter()
        .find(|p| p.side == PositionSide::Long)
        .unwrap();
    assert_eq!(long.id, "BTCUSDT:long");
    assert_eq!(
        (
            long.contracts,
            long.entry_price,
            long.leverage,
            long.notional
        ),
        (2.0, 100.0, 10.0, 200.0)
    );
    assert_eq!(
        (
            long.unrealized_pnl,
            long.realized_pnl,
            long.liquidation_price
        ),
        (4.0, Some(3.0), 50.0)
    );
    assert_eq!(long.margin_type, Some(MarginType::Cross));
    let setting = e.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(10.0));
    assert_eq!(setting.sell_leverage, Some(100.0));
}

#[test]
fn invalid_second_snapshot_surface_does_not_commit_first_surface() {
    let (mut e, _, http) = seeded();
    for r in http {
        response(
            &mut e,
            &r,
            20.0,
            if r.url.ends_with("/v3/account") {
                ACCOUNT
            } else {
                r#"{"code":0,"data":{}}"#
            },
        );
    }
    assert!(e.store().positions().is_empty());
    assert!(e.store().orders().is_empty());
    assert!(!e.store().loaded().positions);
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("account-balance")))
    );
}

#[test]
fn balance_uses_last_price_and_keeps_previous_value_on_missing_payload() {
    let (e, _) = ready();
    let b = e.store().balance();
    assert_eq!((b.total, b.upnl, b.free, b.used), (996.0, 4.0, 900.0, 20.0));
    let (mut e, _, http) = seeded();
    for r in http {
        let body = if r.url.ends_with("/v3/account") {
            ACCOUNT
        } else {
            EMPTY_ORDERS
        };
        response(&mut e, &r, 20.0, body);
    }
    let r = requests(&mut e).remove(0);
    e.store_mut().set_balance(safe_cex_engine::model::Balance {
        total: 996.0,
        upnl: 4.0,
        free: 900.0,
        used: 20.0,
    });
    response(&mut e, &r, 30.0, r#"{"code":0}"#);
    assert_eq!(e.store().balance().total, 996.0);
}

#[test]
fn both_bracket_spellings_map_virtual_children_and_caller_id_verbatim() {
    let (e, _) = ready();
    let parent = e.store().order("parent").unwrap();
    assert_eq!(parent.client_order_id.as_deref(), Some("caller-id"));
    assert_eq!((parent.filled, parent.remaining), (0.25, 1.75));
    let stop = e.store().order("parent__stop_loss").unwrap();
    let take = e.store().order("parent__take_profit").unwrap();
    assert_eq!(
        (stop.kind, stop.price, stop.filled, stop.remaining),
        (OrderType::StopLoss, 90.0, 0.0, 2.0)
    );
    assert_eq!((take.kind, take.price), (OrderType::TakeProfit, 110.0));
    assert_eq!(stop.parent_id.as_deref(), Some("parent"));
}

#[test]
fn documented_empty_optional_average_and_bracket_prices_are_not_malformed_orders() {
    let (mut e, socket) = ready();
    ws(
        &mut e,
        socket,
        100.0,
        &frame(
            "delta",
            r#"{"orders":[{"id":"blank-fields","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"OPEN","size":"1","price":"100","averagePrice":"","openSlParams":{"triggerPrice":"","triggerPriceType":"UNKNOWN_PRICE_TYPE"},"openTpParams":{"triggerPrice":"","triggerPriceType":"UNKNOWN_PRICE_TYPE"}}]}"#,
        ),
    );
    assert!(e.store().order("blank-fields").is_some());
    assert!(e.store().order("blank-fields__stop_loss").is_none());
}

#[test]
fn duplicate_fill_is_emitted_once_and_same_second_distinct_ids_are_not_collapsed() {
    let (mut e, socket) = ready();
    let first = fill("exec-1", "entry", "0.5");
    let second = fill("exec-2", "entry", "0.5");
    let text = frame("delta", &format!(r#"{{"fills":[{first},{second}]}}"#));
    ws(&mut e, socket, 100.0, &text);
    assert_eq!(
        fills(&mut e).iter().map(|f| f.amount).collect::<Vec<_>>(),
        vec![0.5, 0.5]
    );
    ws(&mut e, socket, 110.0, &text);
    assert!(fills(&mut e).is_empty());
}

#[test]
fn snapshot_fills_seed_dedup_without_live_notifications() {
    let (mut e, socket) = ready();
    let data = format!(r#"{{"fills":[{}]}}"#, fill("historic", "old", "1"));
    ws(&mut e, socket, 100.0, &frame("snapshot", &data));
    assert!(fills(&mut e).is_empty());
    ws(&mut e, socket, 110.0, &frame("delta", &data));
    assert!(fills(&mut e).is_empty());
}

#[test]
fn per_execution_rows_and_cumulative_order_in_same_frame_do_not_duplicate_fill() {
    let (mut e, socket) = ready();
    let text = frame(
        "delta",
        &format!(
            r#"{{"fills":[{}],"orders":[{}]}}"#,
            fill("exec", "entry", "2"),
            order("entry", "FILLED", "2")
        ),
    );
    ws(&mut e, socket, 100.0, &text);
    let actual = fills(&mut e);
    assert_eq!(actual.len(), 1);
    assert_eq!((actual[0].price, actual[0].amount), (102.0, 2.0));
    assert_eq!(
        e.store().order("entry").unwrap().status,
        OrderStatus::Closed
    );
    ws(&mut e, socket, 110.0, &text);
    assert!(fills(&mut e).is_empty());
}

#[test]
fn cumulative_order_cache_never_invents_notifications_and_late_executions_emit_full_quantity() {
    let (mut e, socket) = ready();
    ws(
        &mut e,
        socket,
        100.0,
        &frame(
            "delta",
            &format!(r#"{{"orders":[{}]}}"#, order("entry", "OPEN", "0.5")),
        ),
    );
    assert!(fills(&mut e).is_empty());
    ws(
        &mut e,
        socket,
        110.0,
        &frame(
            "delta",
            &format!(r#"{{"orders":[{}]}}"#, order("entry", "FILLED", "2")),
        ),
    );
    assert!(fills(&mut e).is_empty());
    ws(
        &mut e,
        socket,
        120.0,
        &frame(
            "delta",
            &format!(r#"{{"orders":[{}]}}"#, order("entry", "FILLED", "2")),
        ),
    );
    assert!(fills(&mut e).is_empty());
    ws(
        &mut e,
        socket,
        130.0,
        &frame(
            "delta",
            &format!(
                r#"{{"fills":[{},{}]}}"#,
                fill("late-1", "entry", "1"),
                fill("late-2", "entry", "1")
            ),
        ),
    );
    assert_eq!(
        fills(&mut e)
            .iter()
            .map(|fill| fill.amount)
            .collect::<Vec<_>>(),
        vec![1.0, 1.0]
    );
}

#[test]
fn canceled_parent_removes_virtual_siblings_and_delayed_rest_cannot_reopen() {
    let (mut e, socket) = ready();
    // Order heal begins before the terminal event.
    ws(&mut e, socket, 100.0, &frame("delta", r#"{"orders":[]}"#));
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1100.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    let stale = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        1110.0,
        &frame(
            "delta",
            &format!(r#"{{"orders":[{}]}}"#, order("parent", "CANCELED", "0.25")),
        ),
    );
    assert!(e.store().orders().is_empty());
    response(&mut e, &stale, 20_000.0, ORDERS);
    assert!(e.store().orders().is_empty());
}

#[test]
fn final_parent_closes_known_virtual_siblings_when_terminal_payload_omits_them() {
    let (mut e, socket) = ready();
    ws(
        &mut e,
        socket,
        100.0,
        &frame(
            "delta",
            &format!(r#"{{"orders":[{}]}}"#, order("parent", "FILLED", "2")),
        ),
    );
    assert_eq!(
        e.store().order("parent__stop_loss").unwrap().status,
        OrderStatus::Closed
    );
    assert_eq!(
        e.store().order("parent__take_profit").unwrap().status,
        OrderStatus::Closed
    );
    assert!(fills(&mut e).is_empty());
}

#[test]
fn newer_websocket_position_survives_snapshot_already_in_flight() {
    let (mut e, socket, http) = seeded();
    e.handle(20.0, Event::WsOpened { socket });
    ws(
        &mut e,
        socket,
        30.0,
        &frame(
            "delta",
            r#"{"positions":[{"symbol":"BTC-USDT","side":"LONG","size":"3","entryPrice":"101","customInitialMarginRate":"0.2"}]}"#,
        ),
    );
    for r in http {
        response(
            &mut e,
            &r,
            2000.0,
            if r.url.ends_with("/v3/account") {
                EMPTY_ACCOUNT
            } else {
                EMPTY_ORDERS
            },
        );
    }
    let position = e.store().position("BTCUSDT:long").unwrap();
    assert_eq!(
        (position.contracts, position.entry_price, position.leverage),
        (3.0, 101.0, 5.0)
    );
}

#[test]
fn flat_positions_leave_the_open_store_but_keep_per_side_leverage_settings() {
    let (mut e, socket) = ready();
    ws(
        &mut e,
        socket,
        100.0,
        &frame(
            "delta",
            r#"{"positions":[{"symbol":"BTC-USDT","side":"LONG","size":"0","entryPrice":"0","customImr":"0"}]}"#,
        ),
    );
    assert!(e.store().positions().is_empty());
    let setting = e.store().setting("BTCUSDT").unwrap();
    assert_eq!(
        (setting.buy_leverage, setting.sell_leverage),
        (Some(10.0), Some(100.0))
    );
    ws(
        &mut e,
        socket,
        110.0,
        &frame(
            "delta",
            r#"{"positions":[{"symbol":"BTC-USDT","side":"SHORT","size":"0","entryPrice":"0","customImr":"0.05"}]}"#,
        ),
    );
    assert!(e.store().positions().is_empty());
    let setting = e.store().setting("BTCUSDT").unwrap();
    assert_eq!(
        (setting.buy_leverage, setting.sell_leverage),
        (Some(10.0), Some(20.0))
    );
    ws(
        &mut e,
        socket,
        10_000.0,
        &frame(
            "delta",
            r#"{"positions":[{"symbol":"BTC-USDT","side":"LONG","size":"1","entryPrice":"101"}]}"#,
        ),
    );
    assert_eq!(e.store().position("BTCUSDT:long").unwrap().leverage, 10.0);
}

#[test]
fn a_late_rest_snapshot_cannot_reopen_a_position_removed_by_websocket_zero() {
    let (mut e, socket) = ready();
    ws(
        &mut e,
        socket,
        100.0,
        &frame("delta", r#"{"positions":[]}"#),
    );
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1100.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    let stale = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        1110.0,
        &frame(
            "delta",
            r#"{"positions":[{"symbol":"BTC-USDT","side":"LONG","size":"0","entryPrice":"0"}]}"#,
        ),
    );
    assert!(e.store().positions().is_empty());
    response(&mut e, &stale, 20_000.0, ACCOUNT);
    assert!(e.store().positions().is_empty());
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().buy_leverage,
        Some(10.0)
    );
}

#[test]
fn websocket_realized_pnl_alias_preserves_financial_value_and_current_field_precedence() {
    let (mut e, socket) = ready();
    ws(
        &mut e,
        socket,
        100.0,
        &frame(
            "delta",
            r#"{"positions":[{"symbol":"BTC-USDT","side":"LONG","size":"2","entryPrice":"100","realizedPnl":"-0.010238"}]}"#,
        ),
    );
    assert_eq!(
        e.store().position("BTCUSDT:long").unwrap().realized_pnl,
        Some(-0.010238)
    );
    ws(
        &mut e,
        socket,
        110.0,
        &frame(
            "delta",
            r#"{"positions":[{"symbol":"BTC-USDT","side":"LONG","size":"2","entryPrice":"100","curRealisedPnl":"0","realizedPnl":"999"}]}"#,
        ),
    );
    assert_eq!(
        e.store().position("BTCUSDT:long").unwrap().realized_pnl,
        Some(0.0)
    );
}

#[test]
fn unknown_order_market_does_not_abort_later_known_order() {
    let (mut e, socket) = ready();
    let unknown = order("unknown", "OPEN", "0").replace("BTC-USDT", "UNKNOWN");
    ws(
        &mut e,
        socket,
        100.0,
        &frame(
            "delta",
            &format!(
                r#"{{"orders":[{unknown},{}]}}"#,
                order("known", "OPEN", "0")
            ),
        ),
    );
    assert!(e.store().order("known").is_some());
    assert!(e.store().order("unknown").is_none());
}

#[test]
fn server_ping_echoes_args_and_reconnect_fences_old_socket() {
    let (mut e, socket) = ready();
    ws(
        &mut e,
        socket,
        100.0,
        r#"{"op":"ping","args":["server-token",7]}"#,
    );
    assert!(e.drain_actions().any(|a| matches!(a,Action::WsSend{text,..} if text == r#"{"op":"pong","args":["server-token",7]}"#)));
    e.handle(200.0, Event::WsClosed { socket, code: 1006 });
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1200.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1200.0, Event::TimerFired { timer });
    let replacement = e
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => Some(socket),
            _ => None,
        })
        .unwrap();
    assert_ne!(socket, replacement);
    ws(
        &mut e,
        socket,
        1300.0,
        &frame(
            "delta",
            r#"{"positions":[{"symbol":"BTC-USDT","side":"LONG","size":"999","entryPrice":"1"}]}"#,
        ),
    );
    assert_eq!(e.store().position("BTCUSDT:long").unwrap().contracts, 2.0);
}

#[test]
fn private_pong_reports_rounded_one_way_latency() {
    let (mut e, socket) = ready();
    ws(&mut e, socket, 101.0, r#"{"op":"pong"}"#);
    let frame = e.take_frame(101.0).unwrap();
    let (_, status) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::STATUS)
        .unwrap();
    assert_eq!(f64::from_le_bytes(status[8..16].try_into().unwrap()), 41.0);
}

#[test]
fn invalid_omni_seed_fails_private_start_without_hiding_public_markets() {
    let mut e = Engine::new(ApexOmni::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("passphrase".into()),
        omni_key_seed: Some("bad".into()),
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    assert_eq!(e.store().markets().len(), 1);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::WsConnect {
            channel: Some(SocketChannel::Private),
            ..
        } | Action::Http(_)
    )));
    assert!(actions.iter().any(|a| matches!(a,Action::Notice(Notice::Log{message,..}) if message.contains("Invalid OmniKey seed"))));
}

#[test]
fn missing_passphrase_does_not_open_a_permanently_unauthenticated_socket() {
    let mut e = Engine::new(ApexOmni::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        omni_key_seed: Some("11".repeat(65)),
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::WsConnect {
            channel: Some(SocketChannel::Private),
            ..
        } | Action::Http(_)
    )));
    assert!(actions.iter().any(
        |a| matches!(a,Action::Notice(Notice::Log{message,..}) if message.contains("passphrase"))
    ));
}

#[test]
fn silent_private_handshake_is_closed_and_reconnected_without_open_event() {
    let mut e = Engine::new(ApexOmni::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("passphrase".into()),
        omni_key_seed: Some("11".repeat(65)),
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let actions: Vec<_> = e.drain_actions().collect();
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
        .unwrap();
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 5010.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(30_010.0, Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket:s} if *s == socket))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::SetTimer{at_ms,..} if *at_ms == 31_010.0))
    );
}

#[test]
fn account_identity_uses_documented_id_and_does_not_replace_account_store() {
    let (mut e, _) = ready();
    e.handle(100.0, Event::Command(Command::FetchAccount { command: 91 }));
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("/v3/account"));
    response(
        &mut e,
        &r,
        110.0,
        r#"{"code":0,"data":{"id":"611779989544632629"}}"#,
    );
    let result = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 91,
                result,
            }) => Some(result),
            _ => None,
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        result,
        r#"{"userId":"611779989544632629","affiliateId":null}"#
    );
    assert_eq!(e.store().orders().len(), 3);
    assert_eq!(e.store().positions().len(), 1);
}

#[test]
fn acknowledged_leverage_survives_older_snapshot_and_cached_position_heal() {
    let (mut e, socket) = ready();
    ws(
        &mut e,
        socket,
        100.0,
        &frame("delta", r#"{"positions":[]}"#),
    );
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1100.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    let old_positions = requests(&mut e).remove(0);
    e.handle(
        1200.0,
        Event::Command(Command::SetLeverage {
            command: 92,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
        }),
    );
    let write = requests(&mut e).remove(0);
    assert!(write.url.ends_with("/v3/set-initial-margin-rate"));
    response(&mut e, &write, 1210.0, r#"{"code":0,"data":{}}"#);
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    response(&mut e, &old_positions, 10_000.0, ACCOUNT);
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    assert_eq!(e.store().position("BTCUSDT:long").unwrap().leverage, 20.0);
}
