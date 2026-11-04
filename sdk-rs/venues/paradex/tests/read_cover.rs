//! Read covers on a real Paradex source (docs/plans/RUST_FRAME_FED_READ_COVER.md
//! venue pair 4): the open-order census commits its rows with the cover of
//! the read's own release.
mod support;
use safe_cex_engine::wire::section;
use safe_cex_engine::{Action, Event, SocketId};
use safe_cex_venue_paradex::Options;
use support::*;

const POSITION: &str = r#"{"market":"BTC-USD-PERP","status":"OPEN","side":"LONG","size":"2","average_entry_price":"100","unrealized_pnl":"4","realized_positional_pnl":"1","last_updated_at":1700000000000}"#;
const ORDER: &str = r#"{"id":"123","market":"BTC-USD-PERP","type":"LIMIT","status":"OPEN","side":"BUY","size":"2","remaining_size":"2","price":"99","instruction":"GTC","time_in_force":"GTC","created_at":1700000000000,"last_updated_at":1700000000000,"client_id":"caller-1"}"#;

fn loaded() -> (Harness, SocketId) {
    let mut h = Harness::keyed(
        Options {
            paradex_address: Some("0x0123".into()),
            subkey_address: Some("0x456".into()),
            ..Options::default()
        },
        STARK_KEY,
    );
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
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
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
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    message(&mut h, socket, r#"{"jsonrpc":"2.0","id":0,"result":{}}"#);
    (h, socket)
}

fn message(h: &mut Harness, socket: SocketId, text: &str) {
    h.e.handle(h.now, Event::WsMessage { socket, text });
    h.collect();
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

fn account_cover(h: &mut Harness) -> (Vec<u16>, Option<f64>) {
    let frame = h.e.snapshot_frame(h.now).to_vec();
    let kinds = safe_cex_engine::wire::sections(&frame)
        .map(|(kind, _)| kind)
        .collect();
    let cover = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::ORDERS_READ)
        .map(|(_, payload)| f64::from_le_bytes(payload[8..16].try_into().unwrap()))
        .filter(|cover| !cover.is_nan());
    (kinds, cover)
}

/// The repair read a stream update asks for, sent 1 s after it at `h.now`.
fn repair(h: &mut Harness, socket: SocketId) -> safe_cex_engine::HttpRequest {
    h.now += 5_000.;
    let fresh = ORDER.replace(
        "\"last_updated_at\":1700000000000",
        "\"last_updated_at\":1700000005000",
    );
    order_update(h, socket, &fresh);
    h.timer(h.now + 1_000.);
    h.take("/orders")
}

/// The cover is the census's send, not its answer, written right after ORDERS.
#[test]
fn a_census_cover_is_its_send_beside_orders() {
    let (mut h, socket) = loaded();
    let read = repair(&mut h, socket);
    let sent = h.now;
    h.now += 1_500.;
    h.respond(read, 200, &format!("{{\"results\":[{ORDER}]}}"));
    let (kinds, cover) = account_cover(&mut h);
    let orders = kinds
        .iter()
        .position(|kind| *kind == section::ORDERS)
        .unwrap();
    assert_eq!(kinds[orders + 1], section::ORDERS_READ, "{kinds:?}");
    assert_eq!(cover, Some(sent));
}

const STREAM_ORDER: &str = r#"{"id":"789","market":"BTC-USD-PERP","type":"LIMIT","status":"OPEN","side":"SELL","size":"1","remaining_size":"1","price":"120","instruction":"GTC","time_in_force":"GTC","created_at":1700000005500,"last_updated_at":1700000005500}"#;

/// H10: the stream opens an order after the census went out; the census lacks
/// it, keeps the row, and the cover stays at the census before it.
#[test]
fn a_stream_order_after_the_census_went_out_keeps_the_older_cover() {
    let (mut h, socket) = loaded();
    let before = account_cover(&mut h).1;
    assert!(before.is_some(), "the startup census proves one");
    let read = repair(&mut h, socket);
    h.now += 10.;
    order_update(&mut h, socket, STREAM_ORDER);
    h.now += 10.;
    h.respond(read, 200, &format!("{{\"results\":[{ORDER}]}}"));
    assert!(h.e.store().order("789").is_some(), "the stream row stays");
    assert_eq!(account_cover(&mut h).1, before);
}

/// A working row that decodes but cannot map proves no newer cover; the
/// control without it does.
#[test]
fn an_unmappable_working_row_proves_no_newer_cover() {
    let bad = ORDER
        .replace("\"id\":\"123\"", "\"id\":\"999\"")
        .replace("\"size\":\"2\"", "\"size\":\"x\"");
    for (rows, advances) in [(format!("{ORDER},{bad}"), false), (ORDER.to_owned(), true)] {
        let (mut h, socket) = loaded();
        let before = account_cover(&mut h).1;
        let read = repair(&mut h, socket);
        let sent = h.now;
        h.respond(read, 200, &format!("{{\"results\":[{rows}]}}"));
        assert!(h.e.store().order("999").is_none());
        let expected = if advances { Some(sent) } else { before };
        assert_eq!(account_cover(&mut h).1, expected, "{rows}");
    }
}

/// The follow-up a refused census owes waits at least 2 s from that census's
/// start and replaces the repair the stream already owed; it reads once.
#[test]
fn the_follow_up_to_a_refused_census_is_rate_bounded() {
    let (mut h, socket) = loaded();
    let read = repair(&mut h, socket);
    let started = h.now;
    h.now += 10.;
    order_update(&mut h, socket, STREAM_ORDER);
    h.now += 10.;
    let mark = h.actions.len();
    h.respond(read, 200, &format!("{{\"results\":[{ORDER}]}}"));
    assert!(
        !h.pending.iter().any(|r| r.url.contains("/orders")),
        "no census straight after the refused one"
    );
    let armed: Vec<_> = h.actions[mark..]
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(armed.len(), 1, "one follow-up: {armed:?}");
    assert_eq!(armed[0].1, started + 2_000.);
    // The repair the stream armed for 1 s after its update is gone.
    assert!(
        h.actions[mark..]
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { .. }))
    );
    h.timer(started + 2_000.);
    assert_eq!(
        h.pending
            .iter()
            .filter(|r| r.url.contains("/orders"))
            .count(),
        1
    );
}

/// A refused symbol's per-order state stays as the stream left it: the
/// census's terminal claim for its order is not recorded, so a newer stream
/// update still lands.
#[test]
fn a_refused_symbols_census_rows_set_no_order_state() {
    let (mut h, socket) = loaded();
    let read = repair(&mut h, socket);
    h.now += 10.;
    order_update(&mut h, socket, STREAM_ORDER);
    h.now += 10.;
    // Newer than the stream's copy, so only the refusal keeps it from applying.
    let closed = ORDER
        .replace("\"status\":\"OPEN\"", "\"status\":\"CLOSED\"")
        .replace(
            "\"last_updated_at\":1700000000000",
            "\"last_updated_at\":1700000006000",
        );
    h.respond(read, 200, &format!("{{\"results\":[{closed}]}}"));
    assert!(
        h.e.store().order("123").is_some(),
        "the refused symbol keeps its row"
    );
    let newer = ORDER
        .replace("\"remaining_size\":\"2\"", "\"remaining_size\":\"1\"")
        .replace(
            "\"last_updated_at\":1700000000000",
            "\"last_updated_at\":1700000009000",
        );
    order_update(&mut h, socket, &newer);
    assert_eq!(h.e.store().order("123").unwrap().filled, 1.);
}

/// A stream row after the census went out refuses only its own symbol: the
/// census still commits another symbol's rows.
#[test]
fn a_stream_row_refuses_only_its_own_symbol() {
    let (mut h, socket) = loaded();
    let read = repair(&mut h, socket);
    h.now += 10.;
    order_update(&mut h, socket, STREAM_ORDER);
    h.now += 10.;
    let eth = ORDER
        .replace("\"id\":\"123\"", "\"id\":\"321\"")
        .replace("BTC-USD-PERP", "ETH-USD-PERP")
        .replace("\"price\":\"99\"", "\"price\":\"9\"");
    h.respond(read, 200, &format!("{{\"results\":[{ORDER},{eth}]}}"));
    assert!(h.e.store().order("789").is_some(), "the stream row stays");
    assert!(
        h.e.store().order("321").is_some(),
        "the other symbol commits"
    );
}

fn versioned(row: &str, id: &str, updated: f64) -> String {
    row.replace("\"id\":\"123\"", &format!("\"id\":\"{id}\""))
        .replace(
            "\"last_updated_at\":1700000000000",
            &format!("\"last_updated_at\":{updated:.0}"),
        )
}
#[test]
fn terminal_seen_only_in_census_blocks_older_open_stream() {
    let (mut h, socket) = loaded();
    let read = repair(&mut h, socket);
    let closed =
        versioned(ORDER, "999", h.now).replace("\"status\":\"OPEN\"", "\"status\":\"CLOSED\"");
    h.respond(read, 200, &format!(r#"{{"results":[{closed}]}}"#));
    assert!(
        h.e.store().order("999").is_none(),
        "terminal row is not published as open"
    );
    let older = versioned(ORDER, "999", h.now - 500.);
    h.now += 1.;
    order_update(&mut h, socket, &older);
    assert!(
        !h.e.store()
            .order("999")
            .is_some_and(|o| o.status == safe_cex_engine::model::OrderStatus::Open),
        "accepted terminal census evidence must prevent resurrection"
    );
}
#[test]
fn stream_terminal_working_control_blocks_older_open() {
    let (mut h, socket) = loaded();
    h.now += 6000.;
    let closed =
        versioned(ORDER, "999", h.now).replace("\"status\":\"OPEN\"", "\"status\":\"CLOSED\"");
    order_update(&mut h, socket, &closed);
    let older = versioned(ORDER, "999", h.now - 500.);
    h.now += 1.;
    order_update(&mut h, socket, &older);
    assert!(
        !h.e.store()
            .order("999")
            .is_some_and(|o| o.status == safe_cex_engine::model::OrderStatus::Open)
    );
}
#[test]
fn same_clock_refusal_keeps_native_amendment_evidence() {
    let (mut h, socket) = loaded();
    let read = repair(&mut h, socket);
    let fresh = versioned(ORDER, "123", h.now)
        .replace("\"price\":\"99\"", "\"flags\":[],\"price\":\"101\"");
    order_update(&mut h, socket, &fresh);
    let kept = h.e.store().order("123").unwrap().clone();
    let stale =
        versioned(ORDER, "123", h.now + 1000.).replace("\"price\":\"99\"", "\"price\":\"111\"");
    h.respond(read, 200, &format!(r#"{{"results":[{stale}]}}"#));
    assert_eq!(h.e.store().order("123"), Some(&kept));
    h.command(safe_cex_engine::Command::AmendOrder {
        command: 901,
        id: "123".into(),
        price: None,
        amount: Some(3.),
    });
    assert!(
        !h.failures
            .iter()
            .any(|e| e.outcome == Some(safe_cex_engine::Outcome::NotSent)),
        "amendment diagnostic: {:?}",
        h.result(901)
    );
    let amend = h.take("/orders/123");
    let body: serde_json::Value = serde_json::from_str(amend.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["price"], "101");
    assert_eq!(body["size"], "3");
}
#[test]
fn census_around_write_boundaries_cannot_lift_lost_write_fence() {
    for before_write in [true, false] {
        let (mut h, socket) = loaded();
        let previous = account_cover(&mut h).1;
        let read = before_write.then(|| repair(&mut h, socket));
        let order=serde_json::from_str(r#"{"symbol":"BTC-USD-PERP","type":"limit","side":"buy","amount":1,"price":100,"clientOrderId":"probe-902"}"#).unwrap();
        h.command(safe_cex_engine::Command::PlaceOrders {
            command: 902,
            orders: vec![order],
        });
        let write = h.take("/orders");
        assert_eq!(write.method, safe_cex_engine::Method::Post);
        let read = read.unwrap_or_else(|| repair(&mut h, socket));
        h.e.handle(
            h.now,
            Event::HttpFailed {
                request: write.id,
                error: "lost physical write response",
            },
        );
        h.collect();
        h.now += 1.;
        h.respond(read, 200, &format!(r#"{{"results":[{ORDER}]}}"#));
        assert_eq!(
            account_cover(&mut h).1,
            previous,
            "write boundary invalidates read token ({before_write})"
        );
        let order=serde_json::from_str(r#"{"symbol":"BTC-USD-PERP","type":"limit","side":"buy","amount":1,"price":100,"clientOrderId":"probe-903"}"#).unwrap();
        h.command(safe_cex_engine::Command::PlaceOrders {
            command: 903,
            orders: vec![order],
        });
        assert!(
            h.result(903).is_err(),
            "old census must not lift lost-write fence"
        );
        assert!(
            h.pending
                .iter()
                .all(|r| r.method == safe_cex_engine::Method::Get)
        );
    }
}
#[test]
fn unidentified_and_unsupported_rows_prove_no_cover() {
    for bad in [
        r#"{"market":"BTC-USD-PERP","type":"LIMIT","status":"OPEN"}"#.to_owned(),
        ORDER.replace("LIMIT", "UNSUPPORTED_TYPE"),
    ] {
        let (mut h, socket) = loaded();
        let previous = account_cover(&mut h).1;
        let read = repair(&mut h, socket);
        let kept = h.e.store().order("123").unwrap().clone();
        h.respond(read, 200, &format!(r#"{{"results":[{bad}]}}"#));
        assert_eq!(h.e.store().order("123"), Some(&kept));
        assert_eq!(account_cover(&mut h).1, previous);
    }
}
#[test]
fn retired_old_census_cannot_replace_fresh_commit() {
    let (mut h, socket) = loaded();
    let old = repair(&mut h, socket);
    let old_copy = old.clone();
    h.respond(old, 200, &format!(r#"{{"results":[{ORDER}]}}"#));
    let newer = repair(&mut h, socket);
    let fresh = versioned(ORDER, "321", h.now);
    h.respond(newer, 200, &format!(r#"{{"results":[{fresh}]}}"#));
    let rows = h.e.store().orders().to_vec();
    let cover = account_cover(&mut h).1;
    h.now += 1.;
    h.respond(old_copy, 200, r#"{"results":[]}"#);
    assert_eq!(h.e.store().orders(), rows);
    assert_eq!(account_cover(&mut h).1, cover);
}
