//! Documented private JSON-RPC messages and signed auth endpoints.
mod support;
use safe_cex_engine::{Action, Command, Event, Query, SocketChannel, SocketId, wire};
use safe_cex_venue_paradex::Options;
use support::*;
const POSITION: &str = r#"{"market":"BTC-USD-PERP","status":"OPEN","side":"LONG","size":"2","average_entry_price":"100","unrealized_pnl":"4","realized_positional_pnl":"1","last_updated_at":1700000000000}"#;
const ORDER: &str = r#"{"id":"123","market":"BTC-USD-PERP","type":"LIMIT","status":"OPEN","side":"BUY","size":"2","remaining_size":"2","price":"99","instruction":"GTC","time_in_force":"GTC","created_at":1700000000000,"last_updated_at":1700000000000,"client_id":"caller-1"}"#;
fn options() -> Options {
    Options {
        key: Some(format!("0x{}1", "0".repeat(63))),
        paradex_address: Some("0x0123".into()),
        subkey_address: Some("0x456".into()),
        ..Options::default()
    }
}
fn seed() -> (Harness, SocketId) {
    let mut h = Harness::empty(options());
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply(
        "/system/config",
        r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#,
    );
    let socket = h
        .actions
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
    (h, socket)
}
fn loaded() -> (Harness, SocketId) {
    let (mut h, socket) = seed();
    h.reply("/auth/", r#"{"jwt_token":"jwt-first"}"#);
    h.reply(
        "/account/margin",
        r#"{"configs":[{"market":"BTC-USD-PERP","leverage":20}]}"#,
    );
    h.reply("/positions", &format!("{{\"results\":[{POSITION}]}}"));
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"800","initial_margin_requirement":"200"}"#,
    );
    h.reply("/orders", &format!("{{\"results\":[{ORDER}]}}"));
    h.reply(
        "/account/margin",
        r#"{"configs":[{"market":"BTC-USD-PERP","leverage":20}]}"#,
    );
    (h, socket)
}
fn message(h: &mut Harness, socket: SocketId, text: &str) {
    h.e.handle(h.now, Event::WsMessage { socket, text });
    h.collect();
}
fn authenticate(h: &mut Harness, socket: SocketId) {
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    message(h, socket, r#"{"jsonrpc":"2.0","id":0,"result":{}}"#);
}
fn fills(h: &mut Harness) -> u32 {
    let Some(frame) = h.e.take_frame(h.now) else {
        return 0;
    };
    wire::sections(frame)
        .find_map(|(kind, payload)| {
            (kind == wire::section::FILLS)
                .then(|| u32::from_le_bytes(payload[..4].try_into().unwrap()))
        })
        .unwrap_or(0)
}
#[test]
fn config_precedes_signed_auth_and_all_private_reads_carry_the_cached_bearer() {
    let (mut h, socket) = seed();
    let auth = h.take("/auth/");
    assert_eq!(auth.url, "https://api.prod.paradex.trade/v1/auth/0x456");
    assert_eq!(auth.route, Some(safe_cex_engine::RequestRoute::Direct));
    assert!(auth.body.is_none());
    for (name, value) in [
        ("PARADEX-STARKNET-ACCOUNT", "0x0123"),
        ("PARADEX-TIMESTAMP", "1700000000"),
        ("PARADEX-SIGNATURE-EXPIRATION", "1700000300"),
    ] {
        assert!(auth.headers.iter().any(|(n, v)| *n == name && v == value));
    }
    assert!(
        auth.headers
            .iter()
            .any(|(n, v)| *n == "PARADEX-STARKNET-SIGNATURE" && v.starts_with("[\""))
    );
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    assert!(!h.sent().iter().any(|s| s.contains("\"auth\"")));
    h.respond(auth, 200, r#"{"jwt_token":"jwt-first"}"#);
    let margin = h.take("/account/margin");
    assert!(
        margin
            .headers
            .iter()
            .any(|(n, v)| *n == "Authorization" && v == "Bearer jwt-first")
    );
    assert!(
        h.sent()
            .iter()
            .any(|s| s.contains("\"bearer\":\"jwt-first\""))
    );
}
#[test]
fn private_auth_tracks_the_matching_rpc_and_subscribes_each_required_channel() {
    let (mut h, socket) = loaded();
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    let before = h.sent().len();
    message(&mut h, socket, r#"{"jsonrpc":"2.0","id":999,"result":{}}"#);
    assert_eq!(h.sent().len(), before);
    message(&mut h, socket, r#"{"jsonrpc":"2.0","id":0,"result":{}}"#);
    for channel in ["orders.ALL", "positions", "balance_events", "fills.ALL"] {
        assert_eq!(h.sent().iter().filter(|s| s.contains(channel)).count(), 1);
    }
    assert_eq!(h.e.store().positions()[0].leverage, 20.);
    assert_eq!(h.e.store().balance().upnl, 4.);
    assert_eq!(
        h.e.store().orders()[0].client_order_id.as_deref(),
        Some("caller-1")
    );
}
#[test]
fn malformed_and_failed_snapshots_preserve_last_known_good_state() {
    let (mut h, _) = loaded();
    let balance = h.e.store().balance().clone();
    h.timer(h.now + 10_000.);
    let positions = h.take("/positions");
    h.respond(positions, 503, r#"{"error":"unavailable"}"#);
    let account = h.take("/account");
    h.respond(account, 200, r#"{"data":{}}"#);
    let orders = h.take("/orders");
    h.respond(orders, 200, r#"{"results":[{"id":"bad"}]}"#);
    assert_eq!(h.e.store().positions().len(), 1);
    assert_eq!(*h.e.store().balance(), balance);
    assert_eq!(h.e.store().orders().len(), 1);
}
#[test]
fn stream_closure_and_terminal_orders_fence_older_snapshots() {
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    h.timer(h.now + 10_000.);
    let positions = h.take("/positions");
    message(
        &mut h,
        socket,
        r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"positions","data":{"market":"BTC-USD-PERP","status":"CLOSED","size":"0"}}}"#,
    );
    assert!(h.e.store().positions().is_empty());
    h.respond(positions, 200, &format!("{{\"results\":[{POSITION}]}}"));
    assert!(h.e.store().positions().is_empty());
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"1000"}"#,
    );
    let orders = h.take("/orders");
    message(
        &mut h,
        socket,
        r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"orders.ALL","data":{"id":"123","market":"BTC-USD-PERP","type":"LIMIT","status":"CANCELED","side":"BUY","size":"2","remaining_size":"2","price":"99","created_at":1700000000000,"last_updated_at":1700000010000}}}"#,
    );
    h.respond(orders, 200, &format!("{{\"results\":[{ORDER}]}}"));
    assert_eq!(
        h.e.store().orders()[0].status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
}
#[test]
fn fills_are_deduplicated_by_execution_identity_across_reconnects() {
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    fills(&mut h);
    let fill = r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"fills.ALL","data":{"id":"execution-1","order_id":"123","market":"BTC-USD-PERP","side":"BUY","price":"99","size":"0.5"}}}"#;
    message(&mut h, socket, fill);
    message(&mut h, socket, fill);
    assert_eq!(fills(&mut h), 1);
    h.e.handle(h.now, Event::WsClosed { socket, code: 1006 });
    h.collect();
    h.reconnect();
    let newer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket: newer });
    h.collect();
    let id = h
        .sent()
        .iter()
        .rev()
        .find_map(|text| {
            #[derive(serde::Deserialize)]
            struct Rpc {
                method: Option<String>,
                id: Option<u64>,
            }
            let r: Rpc = serde_json::from_str(text).ok()?;
            (r.method.as_deref() == Some("auth"))
                .then_some(r.id?)
                .or(None)
        })
        .unwrap();
    message(
        &mut h,
        newer,
        &format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{}}}}"),
    );
    message(&mut h, newer, fill);
    assert_eq!(fills(&mut h), 0);
    message(&mut h, newer, &fill.replace("execution-1", "execution-2"));
    assert_eq!(fills(&mut h), 1);
}
#[test]
fn jwt_refresh_is_singleflight_and_queued_reads_fail_explicitly_on_auth_failure() {
    let (mut h, _) = loaded();
    h.now += 180_000.;
    for command in [41, 42] {
        h.command(Command::Query {
            command,
            query: Query::Fills {
                symbol: Some("BTC-USD-PERP".into()),
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: Some(2),
            },
        });
    }
    assert_eq!(
        h.pending
            .iter()
            .filter(|r| r.url.contains("/auth/"))
            .count(),
        1
    );
    assert!(!h.pending.iter().any(|r| r.url.ends_with("/fills")));
    let auth = h.take("/auth/");
    h.respond(
        auth,
        401,
        r#"{"error":"INVALID_SIGNATURE","message":"expired subkey"}"#,
    );
    assert!(h.result(41).is_err());
    assert!(h.result(42).is_err());
}
#[test]
fn private_watchdog_recovers_on_missing_auth_reply_and_ignores_late_socket_messages() {
    let (mut h, socket) = loaded();
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    h.timer(h.now + 5_000.);
    h.timer(h.now + 5_000.);
    h.timer(h.now + 5_000.);
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket:s}if *s==socket))
    );
    let before = h.e.store().orders().len();
    message(&mut h, socket, r#"{"jsonrpc":"2.0","id":0,"result":{}}"#);
    assert_eq!(h.e.store().orders().len(), before);
}

fn packet(template: u16, block: u16) -> Vec<u8> {
    let mut data = Vec::new();
    for value in [block, template, 1, 2] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.resize(8 + usize::from(block), 0);
    data
}
fn integer(data: &mut [u8], at: usize, value: i64) {
    data[8 + at..16 + at].copy_from_slice(&value.to_le_bytes());
}
fn text(data: &mut Vec<u8>, value: &str) {
    data.push(value.len().try_into().unwrap());
    data.extend_from_slice(value.as_bytes());
}
fn binary(h: &mut Harness, socket: SocketId, data: &[u8]) {
    h.e.handle(h.now, Event::WsBinary { socket, data });
    h.collect();
}
#[test]
fn binary_private_events_preserve_amendment_decimals_and_fill_identity() {
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    fills(&mut h);
    let mut order = packet(20, 128);
    order[8 + 16] = 3;
    order[8 + 17] = 1;
    order[8 + 18] = 1;
    order[8 + 19] = 3;
    integer(&mut order, 20, 9_912_345_678);
    integer(&mut order, 28, i64::MIN);
    integer(&mut order, 36, 200_000_000);
    integer(&mut order, 44, 150_000_000);
    integer(&mut order, 60, 1_700_000_000_000_000);
    integer(&mut order, 68, 1_700_000_000_001_000);
    for value in ["123", "caller-1", "BTC-USD-PERP", "", "", ""] {
        text(&mut order, value);
    }
    binary(&mut h, socket, &order);
    assert_eq!(h.e.store().orders()[0].price, 99.12345678);
    assert_eq!(
        h.e.store().orders()[0].time_in_force,
        Some(safe_cex_engine::model::TimeInForce::PostOnly)
    );
    assert_eq!(h.e.store().orders()[0].remaining, 1.5);
    let mut fill = packet(21, 116);
    fill[8 + 17] = 1;
    integer(&mut fill, 19, 9_912_345_678);
    integer(&mut fill, 27, 50_000_000);
    for value in [
        "sbe-fill-1",
        "123",
        "caller-1",
        "trade-1",
        "BTC-USD-PERP",
        "USDC",
    ] {
        text(&mut fill, value);
    }
    binary(&mut h, socket, &fill);
    binary(&mut h, socket, &fill);
    assert_eq!(fills(&mut h), 1);
    let mut cancel = order.clone();
    cancel[8 + 16] = 4;
    binary(&mut h, socket, &cancel);
    assert_eq!(
        h.e.store().orders()[0].status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
}
#[test]
fn binary_positions_and_account_fence_snapshots_and_reject_truncation() {
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    let mut position = packet(22, 154);
    position[8 + 16] = 1;
    position[8 + 153] = 1;
    for (at, value) in [
        (17, 300_000_000),
        (25, 10_000_000_000),
        (33, 1_200_000_000),
        (41, 500_000_000),
        (57, i64::MIN),
        (65, 2_000_000_000),
        (73, 1_700_000_000_000_000),
    ] {
        integer(&mut position, at, value);
    }
    for value in ["BTC-USD-PERP", "fill-last"] {
        text(&mut position, value);
    }
    binary(&mut h, socket, &position);
    assert_eq!(h.e.store().positions()[0].contracts, 3.);
    assert_eq!(h.e.store().balance().upnl, 12.);
    let mut account = packet(23, 121);
    for (at, value) in [
        (16, 120_000_000_000),
        (24, 90_000_000_000),
        (32, 30_000_000_000),
    ] {
        integer(&mut account, at, value);
    }
    text(&mut account, "USDC");
    binary(&mut h, socket, &account);
    assert_eq!(h.e.store().balance().total, 1200.);
    assert_eq!(h.e.store().balance().upnl, 12.);
    let original = h.e.store().balance().clone();
    let mut truncated = account.clone();
    truncated.truncate(20);
    binary(&mut h, socket, &truncated);
    assert_eq!(*h.e.store().balance(), original);
    position[8 + 153] = 2;
    integer(&mut position, 17, 0);
    binary(&mut h, socket, &position);
    assert!(h.e.store().positions().is_empty());
}
#[test]
fn delayed_jwt_response_cannot_dispatch_private_requests_after_its_expiry() {
    let (mut h, _) = seed();
    let auth = h.take("/auth/");
    h.now += 300_000.;
    h.respond(auth, 200, r#"{"jwt_token":"already-expired"}"#);
    assert!(!h.pending.iter().any(|r| r.url.ends_with("/account/margin")));
    assert_eq!(
        h.pending
            .iter()
            .filter(|r| r.url.contains("/auth/"))
            .count(),
        1
    );
    h.reply("/auth/", r#"{"jwt_token":"fresh"}"#);
    assert!(
        h.take("/account/margin")
            .headers
            .iter()
            .any(|(n, v)| *n == "Authorization" && v == "Bearer fresh")
    );
}

#[test]
fn positions_cannot_claim_that_an_unread_balance_is_loaded() {
    let (mut h, _) = seed();
    h.reply("/auth/", r#"{"jwt_token":"jwt-first"}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    h.reply("/positions", &format!("{{\"results\":[{POSITION}]}}"));
    assert!(h.e.store().loaded().positions);
    assert!(!h.e.store().loaded().balance);
    let request = h.take("/account");
    h.respond(request, 503, r#"{"error":"unavailable"}"#);
    assert!(!h.e.store().loaded().balance);
}
#[test]
fn pongs_cannot_keep_a_socket_with_missing_subscription_acknowledgements_alive() {
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    for _ in 0..3 {
        message(&mut h, socket, r#"{"type":"pong"}"#);
        h.timer(h.now + 5_000.);
    }
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket:s}if *s==socket))
    );
}

#[test]
fn auth_requires_a_documented_object_and_subscription_acks_require_the_expected_channel() {
    for invalid in [
        "false",
        "[]",
        "\"ok\"",
        "{\"authenticated\":true}",
        "{\"node_id\":null}",
        "{\"node_id\":0}",
        "{\"node_id\":\"\"}",
    ] {
        let (mut h, socket) = loaded();
        h.e.handle(h.now, Event::WsOpened { socket });
        h.collect();
        message(
            &mut h,
            socket,
            &format!("{{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{invalid}}}"),
        );
        assert!(
            !h.sent().iter().any(|s| s.contains("orders.ALL")),
            "{invalid}"
        );
    }
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    for id in 1..=5 {
        message(
            &mut h,
            socket,
            &format!("{{\"jsonrpc\":\"2.0\",\"id\":{id}}}"),
        );
    }
    for id in 1..=5 {
        message(
            &mut h,
            socket,
            &format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"channel\":\"wrong\"}}}}"),
        );
    }
    for _ in 0..3 {
        message(&mut h, socket, r#"{"type":"pong"}"#);
        h.timer(h.now + 5_000.);
    }
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket:s}if *s==socket))
    );
}
#[test]
fn old_position_events_cannot_reopen_a_newer_closed_position() {
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    h.now += 5_000.;
    message(
        &mut h,
        socket,
        r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"positions","data":{"market":"BTC-USD-PERP","status":"CLOSED","size":"0","last_updated_at":1700000005000}}}"#,
    );
    message(
        &mut h,
        socket,
        &format!(
            "{{\"jsonrpc\":\"2.0\",\"method\":\"subscription\",\"params\":{{\"channel\":\"positions\",\"data\":{POSITION}}}}}"
        ),
    );
    assert!(h.e.store().positions().is_empty());
}

#[test]
fn a_new_repair_read_cannot_reopen_a_position_with_older_server_data() {
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    h.now += 5_000.;
    message(
        &mut h,
        socket,
        r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"positions","data":{"market":"BTC-USD-PERP","status":"CLOSED","size":"0","last_updated_at":1700000005000}}}"#,
    );
    h.timer(h.now + 500.);
    let request = h.take("/positions");
    h.respond(request, 200, &format!("{{\"results\":[{POSITION}]}}"));
    assert!(h.e.store().positions().is_empty());
}

#[test]
fn http_200_object_business_errors_cannot_replace_a_good_snapshot_with_empty_data() {
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    message(
        &mut h,
        socket,
        &format!(
            "{{\"jsonrpc\":\"2.0\",\"method\":\"subscription\",\"params\":{{\"channel\":\"positions\",\"data\":{POSITION}}}}}"
        ),
    );
    h.timer(h.now + 500.);
    let request = h.take("/positions");
    h.respond(
        request,
        200,
        r#"{"error":{"code":"UNAVAILABLE"},"results":[]}"#,
    );
    assert_eq!(h.e.store().positions().len(), 1);
}

#[test]
fn mixed_instrument_snapshots_and_streams_only_emit_registered_perpetual_markets() {
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    message(
        &mut h,
        socket,
        r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"positions","data":[{"market":"BTC-USD-250101-100000-C","status":"OPEN","size":null},{"market":"BTC-USD-PERP","status":"OPEN","side":"LONG","size":"3","average_entry_price":"100","unrealized_pnl":"6","last_updated_at":1700000000001}]}}"#,
    );
    assert_eq!(h.e.store().positions().len(), 1);
    assert_eq!(h.e.store().positions()[0].contracts, 3.);
    assert_eq!(h.e.store().balance().total, 1000.);
    message(
        &mut h,
        socket,
        r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"orders.ALL","data":{"id":"option","market":"BTC-USD-250101-100000-C","type":"LIMIT","status":"OPEN","side":"BUY","price":"1","size":"1","remaining_size":"1"}}}"#,
    );
    assert!(!h.e.store().orders().iter().any(|o| o.id == "option"));
    fills(&mut h);
    message(
        &mut h,
        socket,
        r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"fills.ALL","data":{"id":"option-fill","order_id":"option","market":"BTC-USD-250101-100000-C","side":"BUY","price":"1","size":"1"}}}"#,
    );
    assert_eq!(fills(&mut h), 0);
    h.timer(h.now + 500.);
    let request = h.take("/positions");
    h.respond(
        request,
        200,
        &format!(
            "{{\"results\":[{POSITION},{{\"market\":\"BTC-USD-250101-100000-C\",\"size\":null}}]}}"
        ),
    );
    assert_eq!(h.e.store().positions().len(), 1);
    assert_eq!(h.e.store().positions()[0].symbol, "BTC-USD-PERP");
}

#[test]
fn documented_auth_node_id_and_idle_binary_heartbeats_need_no_text_ping() {
    let (mut h, socket) = loaded();
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    message(
        &mut h,
        socket,
        r#"{"jsonrpc":"2.0","id":0,"result":{"node_id":"a1b2c3d4e5f6g7h8"}}"#,
    );
    for (index, channel) in [
        "orders.ALL",
        "positions",
        "balance_events",
        "account",
        "fills.ALL",
    ]
    .into_iter()
    .enumerate()
    {
        message(
            &mut h,
            socket,
            &format!(
                r#"{{"jsonrpc":"2.0","id":{},"result":{{"channel":"{channel}"}}}}"#,
                index + 1
            ),
        );
    }
    assert!(h.sent().iter().any(|text| text.contains("orders.ALL")));
    let start = h.now;
    let mut watchdog = h.timer_at(start + 5_000.);
    let mut heartbeat = vec![16, 0, 40, 0, 1, 0, 2, 0];
    heartbeat.extend_from_slice(&[0; 16]);
    for i in 1..=5 {
        h.now = start + f64::from(i) * 20_000.;
        h.e.handle(
            h.now,
            Event::WsBinary {
                socket,
                data: &heartbeat,
            },
        );
        h.e.handle(h.now, Event::TimerFired { timer: watchdog });
        h.collect();
        watchdog = h.timer_at(h.now + 5_000.);
    }
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
    );
    assert!(!h.sent().iter().any(|text| text.contains("ping")));
    h.now += 90_000.;
    h.e.handle(h.now, Event::TimerFired { timer: watchdog });
    h.collect();
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
    );
}

fn order_update(h: &mut Harness, socket: SocketId, row: &str) {
    message(
        h,
        socket,
        &format!(
            r#"{{"jsonrpc":"2.0","method":"subscription","params":{{"channel":"orders.ALL","data":{row}}}}}"#
        ),
    );
}

#[test]
fn a_new_order_repair_cannot_rewind_a_newer_or_more_filled_order() {
    for stale_stamp in [1_700_000_000_000_u64, 1_700_000_006_000] {
        let (mut h, socket) = loaded();
        authenticate(&mut h, socket);
        h.now += 5_000.;
        let fresh = ORDER
            .replace("\"remaining_size\":\"2\"", "\"remaining_size\":\"1\"")
            .replace(
                "\"last_updated_at\":1700000000000",
                "\"last_updated_at\":1700000005000",
            );
        order_update(&mut h, socket, &fresh);
        assert_eq!(h.e.store().order("123").unwrap().filled, 1.);
        h.timer(h.now + 1_000.);
        let repair = h.take("/orders");
        let stale = ORDER.replace(
            "\"last_updated_at\":1700000000000",
            &format!("\"last_updated_at\":{stale_stamp}"),
        );
        let other = ORDER.replace("\"id\":\"123\"", "\"id\":\"456\"");
        h.respond(repair, 200, &format!(r#"{{"results":[{stale},{other}]}}"#));
        assert_eq!(h.e.store().order("123").unwrap().filled, 1.);
        assert_eq!(h.e.store().order("123").unwrap().remaining, 1.);
        assert!(h.e.store().order("456").is_some());
    }
}

#[test]
fn stream_orders_cannot_lose_fills_at_equal_newer_or_missing_server_timestamps() {
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    h.now += 5_000.;
    let fresh = ORDER
        .replace("\"remaining_size\":\"2\"", "\"remaining_size\":\"1\"")
        .replace(
            "\"last_updated_at\":1700000000000",
            "\"last_updated_at\":1700000005000",
        );
    order_update(&mut h, socket, &fresh);
    for stamp in ["1700000005000", "1700000006000", "0"] {
        h.now += 1.;
        let stale = ORDER.replace(
            "1700000000000,\"client_id\"",
            &format!("{stamp},\"client_id\""),
        );
        order_update(&mut h, socket, &stale);
        assert_eq!(
            h.e.store().order("123").unwrap().filled,
            1.,
            "stamp {stamp}"
        );
    }
    // A newer timestamp alone cannot turn back the server's execution count.
    let valid = fresh
        .replace("1700000005000", "1700000005500")
        .replace("\"price\":\"99\"", "\"price\":\"100\"");
    order_update(&mut h, socket, &valid);
    assert_eq!(h.e.store().order("123").unwrap().price, 100.);
}

#[test]
fn transient_order_omission_keeps_fill_floor_but_allows_same_version_readback() {
    let (mut h, socket) = loaded();
    let poll = h.now + 10_000.;
    authenticate(&mut h, socket);
    h.now += 5_000.;
    let fresh = ORDER
        .replace("\"remaining_size\":\"2\"", "\"remaining_size\":\"1\"")
        .replace(
            "\"last_updated_at\":1700000000000",
            "\"last_updated_at\":1700000005000",
        );
    order_update(&mut h, socket, &fresh);
    h.timer(h.now + 1_000.);
    h.reply("/orders", r#"{"results":[]}"#);
    assert!(h.e.store().orders().is_empty());
    order_update(&mut h, socket, ORDER);
    assert!(h.e.store().orders().is_empty());
    // A lagging empty replica is not proof of a permanent terminal status.
    // Later HTTP presence at the same valid server version restores the order.
    h.timer(poll);
    h.reply("/positions", &format!(r#"{{"results":[{POSITION}]}}"#));
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"800"}"#,
    );
    h.reply("/orders", &format!(r#"{{"results":[{fresh}]}}"#));
    assert_eq!(h.e.store().order("123").unwrap().filled, 1.);
}

#[test]
fn duplicate_order_ids_reject_a_snapshot_before_any_store_or_watermark_mutation() {
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    h.now += 5_000.;
    let fresh = ORDER
        .replace("\"remaining_size\":\"2\"", "\"remaining_size\":\"1\"")
        .replace(
            "\"last_updated_at\":1700000000000",
            "\"last_updated_at\":1700000005000",
        );
    order_update(&mut h, socket, &fresh);
    h.timer(h.now + 1_000.);
    let updated = fresh
        .replace("1700000005000", "1700000010000")
        .replace("\"price\":\"99\"", "\"price\":\"100\"");
    h.reply("/orders", &format!(r#"{{"results":[{updated},{ORDER}]}}"#));
    assert_eq!(h.e.store().orders().len(), 1);
    assert_eq!(h.e.store().order("123").unwrap().price, 99.);
    assert_eq!(h.e.store().order("123").unwrap().filled, 1.);
    let valid = fresh
        .replace("1700000005000", "1700000005500")
        .replace("\"price\":\"99\"", "\"price\":\"101\"");
    order_update(&mut h, socket, &valid);
    assert_eq!(h.e.store().order("123").unwrap().price, 101.);
}

#[test]
fn amend_invalidation_keeps_fill_watermarks_and_exact_decimal_size_changes_remain_valid() {
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    h.now += 5_000.;
    let fresh = ORDER
        .replace("\"size\":\"2\"", "\"size\":\"0.2\"")
        .replace("\"remaining_size\":\"2\"", "\"remaining_size\":\"0.1\"")
        .replace(
            "\"instruction\":\"GTC\"",
            "\"instruction\":\"GTC\",\"flags\":[]",
        )
        .replace(
            "\"last_updated_at\":1700000000000",
            "\"last_updated_at\":1700000005000",
        );
    order_update(&mut h, socket, &fresh);
    h.command(Command::AmendOrder {
        command: 91,
        id: "123".into(),
        price: Some(100.),
        amount: Some(0.3),
    });
    let amend = h.take("/orders/123");
    h.respond(amend, 200, r#"{"id":"123"}"#);
    order_update(&mut h, socket, ORDER);
    assert_eq!(h.e.store().order("123").unwrap().amount, 0.2);
    let changed = fresh
        .replace("\"size\":\"0.2\"", "\"size\":\"0.3\"")
        .replace("\"remaining_size\":\"0.1\"", "\"remaining_size\":\"0.2\"")
        .replace("\"price\":\"99\"", "\"price\":\"100\"")
        .replace("1700000005000", "1700000006000");
    order_update(&mut h, socket, &changed);
    let order = h.e.store().order("123").unwrap();
    assert_eq!(order.amount, 0.3);
    assert_eq!(order.price, 100.);
    assert!((order.filled - 0.1).abs() < 1e-12);
    h.command(Command::AmendOrder {
        command: 92,
        id: "123".into(),
        price: None,
        amount: Some(0.4),
    });
    let request = h.take("/orders/123");
    assert!(
        request
            .body
            .as_deref()
            .unwrap()
            .contains("\"price\":\"100\"")
    );
}

#[path = "support/socket.rs"]
mod socket_checks;
#[test]
fn private_supervisor_bounds_handshake_and_preserves_backoff_after_open() {
    let (mut h, socket) = loaded();
    authenticate(&mut h, socket);
    socket_checks::flaps(h.e, socket, h.now);
}
