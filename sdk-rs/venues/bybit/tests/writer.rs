//! A writesOnly Bybit engine: public instrument metadata only, no sockets, no
//! account reads, one venue request per write, and an unknown outcome is never re-sent.
mod support;
use safe_cex_engine::writer::{MARKETS_NOT_LOADED, WRITES_ONLY, WriterContext};
use safe_cex_engine::{Command, Event, Notice};
use support::{Harness, Options};

fn writer() -> Harness {
    let mut engine = support::engine(Options {
        key: Some("fixture-public".into()),
        secret: Some("fixture-provider".into()),
        ..Default::default()
    });
    engine.set_writes_only().unwrap();
    Harness::started_engine(engine)
}

fn load(h: &mut Harness) {
    let paths = h.paths();
    assert!(
        paths
            .iter()
            .all(|path| path.contains("/v5/market/instruments-info")),
        "only public instrument metadata: {paths:?}"
    );
    h.respond("status=PreLaunch", 200, support::PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    assert!(
        h.requests.is_empty(),
        "nothing after metadata: {:?}",
        h.paths()
    );
}

fn context(json: &str) -> Option<WriterContext> {
    Some(serde_json::from_str(json).unwrap())
}

fn write(h: &mut Harness, command: Command, context: Option<WriterContext>) {
    h.engine
        .handle_writer_command(h.now, command, None, context)
        .unwrap();
    h.collect();
}

fn hedged_btc() -> Option<WriterContext> {
    context(r#"{"settings":{"BTCUSDT":{"hedged":true}}}"#)
}

fn ready(h: &Harness) -> bool {
    h.notices
        .iter()
        .any(|notice| matches!(notice, Notice::Event(event) if event.code == "writer.ready"))
}

#[test]
fn a_writer_loads_public_metadata_once_and_opens_no_socket() {
    let mut h = writer();
    assert!(!ready(&h));
    load(&mut h);
    // Hosts take a frame after every event; a writer's carries nothing.
    assert!(h.engine.take_frame(h.now).is_none());
    h.collect();
    assert!(ready(&h), "writer.ready once markets load");
    assert!(h.engine.store().loaded().markets);
    h.advance(120_000.0);
    assert!(h.requests.is_empty(), "no polls: {:?}", h.paths());
    assert!(h.sockets.is_empty(), "no sockets: {:?}", h.sockets);
}

#[test]
fn a_place_with_context_is_exactly_one_venue_request() {
    let mut h = writer();
    load(&mut h);
    let mut order = support::order();
    order.client_order_id = Some("cid-1".into());
    write(
        &mut h,
        Command::PlaceOrders {
            command: 1,
            orders: vec![order],
        },
        hedged_btc(),
    );
    assert_eq!(h.paths().len(), 1, "{:?}", h.paths());
    let request = h.take("/v5/order/create-batch");
    let body = request.body.clone().unwrap();
    assert!(
        body.contains(r#""positionIdx":1"#),
        "hedged buy opens long: {body}"
    );
    h.reply(
        request.id,
        200,
        r#"{"retCode":0,"result":{"list":[{"orderId":"o-1","orderLinkId":"cid-1"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
    );
    assert!(matches!(h.result(1), Some(Ok(_))), "{:?}", h.result(1));
    h.advance(120_000.0);
    assert!(h.requests.is_empty(), "no follow-up read: {:?}", h.paths());
    assert!(h.sockets.is_empty());
}

#[test]
fn an_unknown_placement_is_reported_and_never_re_sent() {
    let mut h = writer();
    load(&mut h);
    write(
        &mut h,
        Command::PlaceOrders {
            command: 1,
            orders: vec![support::order()],
        },
        hedged_btc(),
    );
    let request = h.take("/v5/order/create-batch");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "connection reset",
    });
    assert_eq!(h.unknown, vec![1], "the outcome is unknown");
    h.advance(300_000.0);
    assert!(
        h.requests.is_empty(),
        "no reconcile read and no re-send: {:?}",
        h.paths()
    );
    // The symbol is not fenced: the next write goes straight out.
    write(
        &mut h,
        Command::CancelOrders {
            command: 2,
            ids: vec!["o-9".into()],
        },
        context(
            r#"{"orders":[{"id":"o-9","symbol":"BTCUSDT","side":"buy","type":"limit","price":100,"amount":0.01,"remaining":0.01,"reduceOnly":false}],"settings":{"BTCUSDT":{"hedged":true}}}"#,
        ),
    );
    let paths = h.paths();
    assert!(
        paths.len() == 1 && paths[0].contains("/v5/order/cancel"),
        "{paths:?}"
    );
}

#[test]
fn a_cancel_of_an_order_the_writer_never_placed_uses_its_context() {
    let mut h = writer();
    load(&mut h);
    write(
        &mut h,
        Command::CancelOrders {
            command: 1,
            ids: vec!["server-order".into()],
        },
        context(
            r#"{"orders":[{"id":"server-order","symbol":"BTCUSDT","side":"sell","type":"limit","price":120,"amount":0.5,"remaining":0.5,"reduceOnly":false}]}"#,
        ),
    );
    assert_eq!(h.requests.len(), 1, "{:?}", h.paths());
    let request = h.take("/v5/order/cancel");
    let body = request.body.clone().unwrap();
    assert!(
        body.contains("server-order") && body.contains("BTCUSDT"),
        "{body}"
    );
    assert!(h.requests.is_empty());
}

#[test]
fn writes_before_markets_and_non_writes_are_refused_without_a_request() {
    let mut h = writer();
    write(
        &mut h,
        Command::PlaceOrders {
            command: 1,
            orders: vec![support::order()],
        },
        hedged_btc(),
    );
    assert!(
        matches!(h.result(1), Some(Err(message)) if message.contains(MARKETS_NOT_LOADED)),
        "{:?}",
        h.result(1)
    );
    load(&mut h);
    h.command(Command::FetchAccount { command: 2 });
    assert!(
        matches!(h.result(2), Some(Err(message)) if message.contains(WRITES_ONLY)),
        "{:?}",
        h.result(2)
    );
    assert!(h.requests.is_empty());
}

#[test]
fn a_writer_rejects_context_on_a_normal_engine() {
    let mut h = Harness::started(Options {
        key: Some("fixture-public".into()),
        secret: Some("fixture-provider".into()),
        ..Default::default()
    });
    assert!(
        h.engine
            .handle_writer_command(
                h.now,
                Command::CancelSymbolOrders {
                    command: 1,
                    symbol: "BTCUSDT".into(),
                },
                None,
                hedged_btc(),
            )
            .is_err()
    );
    h.collect();
}
