//! Frame-fed Bitget: the page engine loads only public contracts, opens no
//! socket, takes account state and VENUE_STATE from a data source's frames,
//! and sends each write as its planned requests and nothing else.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Command, Event, HttpRequest, Method};
use support::*;

fn page() -> Harness {
    let mut h = Harness::started_configure(
        FixtureOptions {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            ..FixtureOptions::default()
        },
        false,
        |engine| engine.set_frame_fed().unwrap(),
    );
    h.respond("/contracts", 200, CONTRACTS);
    h
}

/// A page engine fed a classic data source's snapshot (open order "1").
fn fed() -> Harness {
    let mut source = Harness::classic();
    let frame = source.engine.snapshot_frame(source.now).to_vec();
    let mut h = page();
    h.engine.apply_frame(h.now, &frame, true).unwrap();
    h.collect();
    h
}

/// Fires every timer due in the next ten seconds and returns what was sent.
fn drain(h: &mut Harness) -> Vec<HttpRequest> {
    let until = h.now + 10_000.0;
    loop {
        let mut due: Vec<_> = h
            .timers
            .iter()
            .copied()
            .filter(|(_, at)| *at <= until)
            .collect();
        if due.is_empty() {
            break;
        }
        due.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (timer, at) = due[0];
        h.timers.retain(|(t, _)| *t != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
    }
    std::mem::take(&mut h.requests)
}

fn place(h: &mut Harness) {
    let order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90,"amount":0.01}"#,
    )
    .unwrap();
    h.command(Command::PlaceOrders {
        command: 7,
        orders: vec![order],
    });
}

#[test]
fn frame_fed_start_reads_only_public_contracts_and_opens_no_socket() {
    let mut h = page();
    assert!(h.sockets.is_empty(), "{:?}", h.sockets);
    assert!(
        h.requests.is_empty(),
        "no tickers, settings or account reads: {:?}",
        h.requests
    );
    assert!(h.engine.store().loaded().markets);
    assert!(drain(&mut h).is_empty(), "no polls");
}

#[test]
fn a_source_snapshot_carries_the_account_type_so_a_placement_is_one_request() {
    let mut h = fed();
    assert!(h.engine.store().order("1").is_some());
    assert!(h.requests.is_empty(), "{:?}", h.requests);
    place(&mut h);
    let sent = std::mem::take(&mut h.requests);
    assert_eq!(sent.len(), 1, "no settings read before the write: {sent:?}");
    assert_eq!(sent[0].method, Method::Post);
    assert!(sent[0].url.contains("/order/"), "{}", sent[0].url);
    h.reply(
        sent[0].id,
        200,
        r#"{"code":"00000","data":{"successList":[{"orderId":"9","clientOid":"c"}],"failureList":[]}}"#,
    );
    assert!(drain(&mut h).is_empty(), "no post-write read or reconnect");
    assert!(h.sockets.is_empty());
}

#[test]
fn a_cancel_of_a_source_order_is_one_request_and_no_read() {
    let mut h = fed();
    h.command(Command::CancelOrders {
        command: 8,
        ids: vec!["1".into()],
    });
    let sent = std::mem::take(&mut h.requests);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].method, Method::Post);
    assert!(sent[0].url.contains("cancel"), "{}", sent[0].url);
    assert!(
        sent[0]
            .body
            .as_deref()
            .unwrap()
            .contains(r#""orderId":"1""#),
        "{:?}",
        sent[0].body
    );
    h.reply(
        sent[0].id,
        200,
        r#"{"code":"00000","data":{"successList":[{"orderId":"1"}],"failureList":[]}}"#,
    );
    assert!(drain(&mut h).is_empty());
}

#[test]
fn an_unknown_placement_outcome_is_never_re_sent_or_read_back() {
    let mut h = fed();
    place(&mut h);
    let sent = std::mem::take(&mut h.requests);
    assert_eq!(sent.len(), 1);
    h.reply(sent[0].id, 502, "bad gateway");
    assert!(drain(&mut h).is_empty(), "no resend and no reconcile read");
}

#[test]
fn writes_wait_for_the_source_account_type() {
    // Before any frame the page engine does not know classic from UTA.
    let mut h = page();
    place(&mut h);
    assert!(h.requests.is_empty(), "{:?}", h.requests);
    let mut source = Harness::classic();
    let frame = source.engine.snapshot_frame(source.now).to_vec();
    h.engine.apply_frame(h.now, &frame, true).unwrap();
    h.collect();
    assert_eq!(h.requests.len(), 1, "{:?}", h.requests);
}

/// A UTA data source, as `tests/trading.rs`, with no open orders.
fn uta_source() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    h.respond(
        "/current-position",
        200,
        &POSITION
            .replace("\"data\":[", "\"data\":{\"list\":[")
            .replace("]}", "]}}"),
    );
    h.respond(
        "/account/assets",
        200,
        r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"900","unrealisedPnl":"0"}}"#,
    );
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/unfilled-strategy-orders"))
    {
        h.respond(
            "/unfilled-strategy-orders",
            200,
            r#"{"code":"00000","data":[]}"#,
        );
    }
    h.login();
    h
}

#[test]
fn a_uta_source_routes_the_page_engines_placement_to_the_uta_api() {
    let mut source = uta_source();
    let frame = source.engine.snapshot_frame(source.now).to_vec();
    let mut h = page();
    h.engine.apply_frame(h.now, &frame, true).unwrap();
    h.collect();
    assert_eq!(
        h.engine.store().hedged(),
        source.engine.store().hedged(),
        "hedge mode arrives as STATUS"
    );
    place(&mut h);
    let sent = std::mem::take(&mut h.requests);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(
        sent[0].url.ends_with("/api/v3/trade/place-order"),
        "{}",
        sent[0].url
    );
    h.reply(
        sent[0].id,
        200,
        r#"{"code":"00000","data":{"orderId":"9","clientOid":"c"}}"#,
    );
    assert!(
        drain(&mut h).is_empty(),
        "UTA's post-write order read is off"
    );
}

/// The writes a command sends, answering each with `body` until none is left.
fn writes(h: &mut Harness, command: Command, body: &str) -> Vec<(Method, String, Option<String>)> {
    h.requests.clear();
    h.command(command);
    let mut sent = Vec::new();
    while let Some(request) = h.requests.iter().position(|r| r.method == Method::Post) {
        let request = h.requests.remove(request);
        sent.push((request.method, request.url.clone(), request.body.clone()));
        h.reply(request.id, 200, body);
    }
    sent
}

#[test]
fn cancel_all_keeps_the_local_request_sequence() {
    let command = || Command::CancelSymbolOrders {
        command: 9,
        symbol: "BTCUSDT".into(),
    };
    let ok = r#"{"code":"00000","data":{"successList":[{"orderId":"1"}],"failureList":[]}}"#;
    let mut local = Harness::classic();
    let local_writes = writes(&mut local, command(), ok);
    let mut h = fed();
    let page_writes = writes(&mut h, command(), ok);
    assert!(!local_writes.is_empty());
    assert_eq!(page_writes, local_writes);
    assert!(drain(&mut h).is_empty(), "and no read on the page engine");
}
