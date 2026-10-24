mod support;
use safe_cex_engine::model::{MarginType, OrderStatus, OrderType, PositionSide};
use safe_cex_engine::{Action, Command, Event, HttpRequest, Notice, SocketChannel, SocketId};
use safe_cex_venue_apexomni::Options;
use support::*;

/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 2_000.0;

const ACCOUNT: &str = r#"{"code":0,"data":{"id":"611779989544632629","l2key":"key","positions":[{"symbol":"BTC-USDT","side":"LONG","entryPrice":"100","size":"2","customImr":"0.1","unrealisedPnl":"4","curRealisedPnl":"3","liqPrice":"50"}],"contractAccount":{"makerFeeRate":"0.0002","takerFeeRate":"0.0005"}}}"#;
const EMPTY_ACCOUNT: &str = r#"{"code":0,"data":{"id":"611779989544632629","positions":[],"contractAccount":{"makerFeeRate":"0.0002","takerFeeRate":"0.0005"}}}"#;
const EMPTY_ORDERS: &str = r#"{"code":0,"data":[]}"#;
const ORDERS: &str = r#"{"code":0,"data":[{"id":"parent","symbol":"BTC-USDT","status":"OPEN","type":"LIMIT","side":"BUY","size":"2","price":"100","cumSuccessFillSize":"0.25","clientId":"caller-id","reduceOnly":false,"timeInForce":"POST_ONLY","openSlParam":{"triggerPrice":"90","triggerPriceType":"INDEX"},"openTpParams":{"triggerPrice":"110","triggerPriceType":"MARKET"}}]}"#;
const BALANCE: &str = r#"{"code":0,"data":{"totalEquityValue":"1000","availableBalance":"800","initialMargin":"100","maintenanceMargin":"20"}}"#;

fn seeded() -> (TestEngine, SocketId, Vec<HttpRequest>) {
    let mut e = private_engine(Options::default(), &Material::default());
    e.start(0.0);
    let r = requests(&mut e, 0.0).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let mut socket = None;
    let mut http = vec![];
    for action in e.prepared(10.0) {
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
    ready_with_account(ACCOUNT)
}
fn ready_with_account(account: &str) -> (TestEngine, SocketId) {
    let (mut e, socket, http) = seeded();
    e.handle(20.0, Event::WsOpened { socket });
    e.prepared(20.0).for_each(drop);
    for r in http {
        let body = if r.url.ends_with("/v3/account") {
            account
        } else {
            ORDERS
        };
        response(&mut e, &r, 30.0, body);
    }
    let balance = requests(&mut e, 30.0)
        .into_iter()
        .find(|r| r.url.ends_with("/v3/account-balance"))
        .unwrap();
    e.store_mut().update_ticker("BTCUSDT", |t| t.last = 102.0);
    response(&mut e, &balance, 40.0, BALANCE);
    (e, socket)
}
fn hedged_account(reverse: bool) -> String {
    let long =
        r#"{"symbol":"BTC-USDT","side":"LONG","entryPrice":"100","size":"2","customImr":"0.1"}"#;
    let short =
        r#"{"symbol":"BTC-USDT","side":"SHORT","entryPrice":"110","size":"1","customImr":"0.1"}"#;
    let rows = if reverse {
        format!("{short},{long}")
    } else {
        format!("{long},{short}")
    };
    format!(
        r#"{{"code":0,"data":{{"id":"611779989544632629","positions":[{rows}],"contractAccount":{{"makerFeeRate":"0.0002","takerFeeRate":"0.0005"}}}}}}"#
    )
}
fn position_ids(e: &TestEngine) -> Vec<&str> {
    e.store()
        .positions()
        .iter()
        .map(|position| position.id.as_str())
        .collect()
}
fn position_update(e: &mut TestEngine, socket: SocketId, at: f64, side: &str, size: &str) {
    ws(
        e,
        socket,
        at,
        &frame(
            "delta",
            &format!(
                r#"{{"positions":[{{"symbol":"BTC-USDT","side":"{side}","entryPrice":"100","size":"{size}","customImr":"0.1"}}]}}"#
            ),
        ),
    );
}
fn begin_position_heal(e: &mut TestEngine, socket: SocketId, at: f64) -> HttpRequest {
    ws(e, socket, at, &frame("delta", r#"{"positions":[]}"#));
    let timer = e.prepared(at)
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if at_ms == at + 1000.0 => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(at + 1000.0, Event::TimerFired { timer });
    requests(e)
        .into_iter()
        .find(|request| request.url.ends_with("/v3/account"))
        .unwrap()
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
    let messages: Vec<_> = e.prepared(20.0)
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
        !e.prepared(20.0)
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
    let r = requests(&mut e, 20.0).remove(0);
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
    let timer = e.prepared(100.0)
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1100.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    let stale = requests(&mut e, 1100.0).remove(0);
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
    let timer = e.prepared(100.0)
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1100.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    let stale = requests(&mut e, 1100.0).remove(0);
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
fn hedged_position_order_survives_side_updates_and_guarded_rest_healing() {
    let (mut e, socket) = ready_with_account(&hedged_account(false));
    let expected = ["BTCUSDT:long", "BTCUSDT:short"];
    assert_eq!(position_ids(&e), expected);
    position_update(&mut e, socket, 100.0, "LONG", "3");
    assert_eq!(position_ids(&e), expected);
    position_update(&mut e, socket, 110.0, "SHORT", "4");
    assert_eq!(position_ids(&e), expected);
    let stale = begin_position_heal(&mut e, socket, 120.0);
    position_update(&mut e, socket, 1130.0, "LONG", "5");
    position_update(&mut e, socket, 1140.0, "SHORT", "6");
    response(&mut e, &stale, 3000.0, &hedged_account(false));
    assert_eq!(position_ids(&e), expected);
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .map(|position| position.contracts)
            .collect::<Vec<_>>(),
        [5.0, 6.0]
    );
}

#[test]
fn guarded_snapshot_omissions_keep_live_store_order_and_closed_sides_stay_removed() {
    let (mut e, socket) = ready_with_account(&hedged_account(true));
    let expected = ["BTCUSDT:short", "BTCUSDT:long"];
    let stale = begin_position_heal(&mut e, socket, 100.0);
    position_update(&mut e, socket, 1110.0, "LONG", "3");
    position_update(&mut e, socket, 1120.0, "SHORT", "4");
    response(&mut e, &stale, 3000.0, EMPTY_ACCOUNT);
    assert_eq!(position_ids(&e), expected);
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .map(|position| position.contracts)
            .collect::<Vec<_>>(),
        [4.0, 3.0]
    );
    let stale = begin_position_heal(&mut e, socket, 3100.0);
    position_update(&mut e, socket, 4110.0, "LONG", "0");
    position_update(&mut e, socket, 4120.0, "SHORT", "5");
    response(&mut e, &stale, 6000.0, &hedged_account(false));
    assert_eq!(position_ids(&e), ["BTCUSDT:short"]);
    assert_eq!(e.store().positions()[0].contracts, 5.0);
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().buy_leverage,
        Some(10.0)
    );
}

#[test]
fn guarded_rest_rows_keep_the_provider_order_when_both_sides_are_present() {
    let (mut e, socket) = ready_with_account(&hedged_account(false));
    let stale = begin_position_heal(&mut e, socket, 100.0);
    position_update(&mut e, socket, 1110.0, "LONG", "3");
    position_update(&mut e, socket, 1120.0, "SHORT", "4");
    response(&mut e, &stale, 3000.0, &hedged_account(true));
    assert_eq!(position_ids(&e), ["BTCUSDT:short", "BTCUSDT:long"]);
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .map(|position| position.contracts)
            .collect::<Vec<_>>(),
        [4.0, 3.0]
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
    assert!(e.prepared(100.0).any(|a| matches!(a,Action::WsSend{text,..} if text == r#"{"op":"pong","args":["server-token",7]}"#)));
    e.handle(200.0, Event::WsClosed { socket, code: 1006 });
    let timer = e.prepared(200.0)
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (950.0..=1200.0).contains(&at_ms) => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1200.0, Event::TimerFired { timer });
    let replacement = e.prepared(1200.0)
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
fn missing_passphrase_does_not_open_a_permanently_unauthenticated_socket() {
    let mut e = private_engine(
        Options {
            ..Default::default()
        },
        &Material {
            key: "key",
            signing: "secret",
            passphrase: None,
            seed: Some(("11".repeat(65)).to_string()),
        },
    );
    e.start(0.0);
    let r = requests(&mut e, 0.0).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let actions: Vec<_> = e.prepared(10.0).collect();
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::WsConnect {
            channel: Some(SocketChannel::Private),
            ..
        } | Action::Http(_)
    )));
    assert!(actions.iter().any(
        |a| matches!(a,Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("passphrase"))
    ));
}

#[test]
fn silent_private_handshake_is_closed_and_reconnected_without_open_event() {
    let mut e = private_engine(
        Options {
            ..Default::default()
        },
        &Material {
            key: "key",
            signing: "secret",
            passphrase: Some("passphrase"),
            seed: Some(("11".repeat(65)).to_string()),
        },
    );
    e.start(0.0);
    let r = requests(&mut e, 0.0).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let actions: Vec<_> = e.prepared(10.0).collect();
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
    let private = actions
        .iter()
        .position(|a| matches!(a, Action::WsConnect { socket: s, .. } if *s == socket))
        .unwrap();
    let timer = actions[..private]
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 5010.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(30_010.0, Event::TimerFired { timer });
    let actions: Vec<_> = e.prepared(30_010.0).collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket:s} if *s == socket))
    );
    assert!(
        actions.iter().any(
            |a| matches!(a,Action::SetTimer{at_ms,..} if (30_760.0..=31_010.0).contains(at_ms))
        )
    );
}

#[test]
fn account_identity_uses_documented_id_and_does_not_replace_account_store() {
    let (mut e, _) = ready();
    e.handle(100.0, Event::Command(Command::FetchAccount { command: 91 }));
    let r = requests(&mut e, 100.0).remove(0);
    assert!(r.url.ends_with("/v3/account"));
    response(
        &mut e,
        &r,
        110.0,
        r#"{"code":0,"data":{"id":"611779989544632629"}}"#,
    );
    let result = e.prepared(110.0)
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 91,
                result,
            }) => Some(result),
            _ => None,
        })
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
    let timer = e.prepared(100.0)
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1100.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    let old_positions = requests(&mut e, 1100.0).remove(0);
    e.handle(
        1200.0,
        Event::Command(Command::SetLeverage {
            command: 92,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
            side: None,
        }),
    );
    let write = prepared_requests(&mut e, 1200.).remove(0);
    assert!(write.url.ends_with("/v3/set-initial-margin-rate"));
    response(&mut e, &write, 1210.0, r#"{"code":0,"data":{}}"#);
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    response(&mut e, &old_positions, 10_000.0, ACCOUNT);
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    assert_eq!(e.store().position("BTCUSDT:long").unwrap().leverage, 20.0);
}

#[test]
fn missing_or_blank_position_symbol_rejects_rest_heal_without_clearing_state() {
    for symbol in [None, Some(""), Some(" ")] {
        let (mut e, socket) = ready();
        let request = begin_position_heal(&mut e, socket, 100.0);
        let symbol_field = symbol
            .map(|s| format!(r#""symbol":"{s}","#))
            .unwrap_or_default();
        let body = ACCOUNT.replace(r#""symbol":"BTC-USDT","#, &symbol_field);
        response(&mut e, &request, 3000.0, &body);
        assert_eq!(e.store().position("BTCUSDT:long").unwrap().contracts, 2.0);
    }
}

#[test]
fn missing_or_blank_order_identifiers_reject_rest_heal_without_clearing_state() {
    for (field, value) in [
        ("symbol", None),
        ("symbol", Some("")),
        ("symbol", Some(" ")),
        ("id", None),
        ("id", Some("")),
        ("id", Some(" ")),
    ] {
        let (mut e, socket) = ready();
        ws(&mut e, socket, 100.0, &frame("delta", r#"{"orders":[]}"#));
        let timer = e.prepared(100.0)
            .find_map(|a| match a {
                Action::SetTimer {
                    timer,
                    at_ms: 1100.0,
                } => Some(timer),
                _ => None,
            })
            .unwrap();
        e.handle(1100.0, Event::TimerFired { timer });
        let request = requests(&mut e, 1100.0).remove(0);
        let original = if field == "symbol" {
            r#""symbol":"BTC-USDT","#
        } else {
            r#""id":"parent","#
        };
        let replacement = value
            .map(|v| format!(r#""{field}":"{v}","#))
            .unwrap_or_default();
        response(
            &mut e,
            &request,
            3000.0,
            &ORDERS.replace(original, &replacement),
        );
        assert_eq!(e.store().orders().len(), 3);
        assert_eq!(e.store().order("parent").unwrap().status, OrderStatus::Open);
    }
}

#[test]
fn malformed_terminal_ws_identity_cannot_remove_or_tombstone_a_live_order() {
    let (mut e, socket) = ready();
    let malformed = order("parent", "CANCELED", "0").replace("BTC-USDT", " ");
    ws(
        &mut e,
        socket,
        100.0,
        &frame("delta", &format!(r#"{{"orders":[{malformed}]}}"#)),
    );
    assert_eq!(e.store().orders().len(), 3);
    let valid = order("parent", "OPEN", "0.5");
    ws(
        &mut e,
        socket,
        110.0,
        &frame("delta", &format!(r#"{{"orders":[{valid}]}}"#)),
    );
    assert_eq!(e.store().order("parent").unwrap().filled, 0.5);
}

#[test]
fn account_identity_can_be_read_before_start_without_an_omni_seed() {
    let mut engine = private_engine(
        Options {
            ..Default::default()
        },
        &Material {
            key: "key",
            signing: "secret",
            passphrase: Some("passphrase"),
            seed: None,
        },
    );
    engine.handle(
        1700000000123.0,
        Event::Command(Command::FetchAccount { command: 77 }),
    );
    let actions = engine.prepared(1700000000123.0).collect::<Vec<_>>();
    assert_eq!(
        actions.len(),
        1,
        "identity must not start sockets or timers"
    );
    let Action::Http(request) = &actions[0] else {
        panic!("identity must issue exactly one signed account read");
    };
    assert_eq!(request.method, safe_cex_engine::Method::Get);
    assert_eq!(request.url, "https://omni.apex.exchange/api/v3/account");
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| *name == "APEX-API-KEY" && value == "key")
    );
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| *name == "APEX-SIGNATURE" && !value.is_empty())
    );
    response(
        &mut engine,
        request,
        1700000000130.0,
        r#"{"code":0,"data":{"id":"611779989544632629"}}"#,
    );
    let actions = engine.prepared(1700000000130.0).collect::<Vec<_>>();
    assert_eq!(actions.len(), 1);
    assert!(
        matches!(&actions[0], Action::Notice(Notice::CommandDone { command: 77, result: body }) if body == r#"{"userId":"611779989544632629","affiliateId":null}"#)
    );
    let loaded = engine.store().loaded();
    assert!(
        !loaded.markets
            && !loaded.tickers
            && !loaded.positions
            && !loaded.orders
            && !loaded.balance
    );
    assert!(engine.store().positions().is_empty());
    assert!(engine.store().orders().is_empty());
}

#[test]
fn periodic_account_policy_changes_preserve_inflight_reads_and_fence_stale_timers() {
    let (mut e, _) = ready();
    let now = 1000.;
    e.drain_actions().for_each(drop);
    e.handle(
        now,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let stale: Vec<_> = e.prepared(now)
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
        let actions: Vec<_> = e.prepared(now).collect();
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
        assert!(!e.prepared(now).any(|a| matches!(a, Action::Http(_))));
    }
    let (timer, due) = current.unwrap();
    e.handle(due, Event::TimerFired { timer });
    let mut pending: Vec<_> = e.prepared(due)
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert!(
        pending.iter().any(|r| r.url.ends_with("/v3/account")),
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
    e.prepared(due).for_each(drop);
    for _ in 0..12 {
        if pending.is_empty() {
            break;
        }
        for r in std::mem::take(&mut pending) {
            let body = if r.url.ends_with("/v3/account") {
                ACCOUNT
            } else if r.url.ends_with("/v3/account-balance") {
                BALANCE
            } else {
                ORDERS
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
            pending.extend(e.prepared(due + 1.).filter_map(|a| match a {
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
    let resumed: Vec<_> = e.prepared(due + 2.)
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
fn rest_and_stream_remaining_preserve_native_decimal_precision() {
    for (amount, filled, remaining) in [
        (0.3, 0.2, 0.1),
        (0.00000003, 0.00000001, 0.00000002),
        (0.3, 0.3, 0.0),
        (0.0037, 0.0011, 0.0026),
    ] {
        for streamed in [false, true] {
            let row = serde_json::json!({"id":"decimal","symbol":"BTC-USDT","status":"OPEN","type":"LIMIT","side":"BUY","size":amount.to_string(),"price":"100","cumSuccessFillSize":filled.to_string()});
            let body = serde_json::json!({"code":0,"data":[row.clone()]}).to_string();
            let (mut e, socket, reads) = seeded();
            for request in reads {
                let data = if request.url.ends_with("/v3/account") {
                    ACCOUNT
                } else if streamed {
                    EMPTY_ORDERS
                } else {
                    &body
                };
                response(&mut e, &request, 30.0, data);
            }
            if streamed {
                ws(&mut e, socket, 40.0, &serde_json::json!({"topic":"ws_zk_accounts_v3","type":"delta","contents":{"orders":[row]}}).to_string());
            }
            let order = e.store().order("decimal").unwrap();
            assert_eq!(
                order.remaining, remaining,
                "amount={amount} filled={filled} streamed={streamed}"
            );
        }
    }
}
