//! A frame-fed Binance USD-M engine (the browser's main-thread engine for one
//! account): public instrument metadata only, no sockets, no account reads or
//! polls. Fed a LOCAL engine's snapshot, each write sends exactly the requests
//! LOCAL sends, cancel-then-replace amends included, and never a read.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Command, Event, HttpRequest, Method};
use support::{Harness, Options};

const OPEN_ORDERS: &str = r#"[{"symbol":"BTCUSDT","orderId":12,"clientOrderId":"maker","price":"100","origQty":"7","executedQty":"2","type":"LIMIT","side":"BUY","status":"PARTIALLY_FILLED","timeInForce":"GTC"}]"#;
const STOP_LIMIT: &str = r#"[{"algoId":43,"clientAlgoId":"stoplimit","algoType":"CONDITIONAL","orderType":"STOP","symbol":"BTCUSDT","side":"SELL","quantity":"7","price":"89","algoStatus":"NEW","triggerPrice":"90","workingType":"MARK_PRICE"}]"#;
const POSITION: &str = r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"7","entryPrice":"100","markPrice":"100","unRealizedProfit":"0","leverage":"20","marginType":"cross","notional":"700","liquidationPrice":"50"}]"#;
const BRACKETS: &str = r#"[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":50,"notionalCap":5000,"notionalFloor":0,"maintMarginRatio":0.01,"cum":0}]}]"#;

fn keyed() -> Options {
    Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    }
}

/// The data source: a LOCAL USD-M account with one open limit order, one
/// stop-limit and one position.
fn local() -> Harness {
    let mut h = Harness::started(keyed());
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond("/fapi/v1/leverageBracket", 200, BRACKETS);
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond("/fapi/v1/listenKey", 200, r#"{"listenKey":"private-key"}"#);
    h.respond(
        "/fapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/fapi/v3/positionRisk", 200, POSITION);
    h.respond("/fapi/v1/symbolConfig", 200, "[]");
    h.respond("/fapi/v3/account", 200, r#"{"assets":[]}"#);
    h.respond("/fapi/v1/openOrders", 200, OPEN_ORDERS);
    h.respond("/fapi/v1/openAlgoOrders", 200, STOP_LIMIT);
    let private = h.socket("/private/ws").unwrap();
    h.event(Event::WsOpened { socket: private });
    h.requests.clear();
    h
}

/// A frame-fed engine: one public instrument read, no socket, nothing after.
fn frame_fed() -> Harness {
    let mut engine = support::engine(keyed());
    engine.set_frame_fed().unwrap();
    let mut h = Harness::from_engine(engine);
    assert_eq!(h.paths().len(), 1, "{:?}", h.paths());
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    assert!(
        h.requests.is_empty(),
        "nothing after metadata: {:?}",
        h.paths()
    );
    h.advance(120_000.0);
    assert!(h.requests.is_empty(), "no polls: {:?}", h.paths());
    assert!(h.sockets.is_empty(), "no socket: {:?}", h.sockets);
    assert!(h.engine.store().loaded().markets);
    h
}

/// `fed` after ingesting `source`'s snapshot frame.
fn fed_from(source: &mut Harness) -> Harness {
    let mut fed = frame_fed();
    fed.now = source.now;
    let frame = source.engine.snapshot_frame(source.now).to_vec();
    fed.engine.apply_frame(fed.now, &frame, true).unwrap();
    fed.collect();
    assert!(
        fed.requests.is_empty(),
        "ingest reads nothing: {:?}",
        fed.paths()
    );
    fed
}

/// Method, path and query without the per-request signature, clock and
/// generated client ids, then the body.
fn shape(request: &HttpRequest) -> (Method, String, Vec<String>, String) {
    let (path, query) = request.url.split_once('?').unwrap_or((&request.url, ""));
    let mut params: Vec<String> = query
        .split('&')
        .chain(request.body.as_deref().unwrap_or("").split('&'))
        .filter(|pair| {
            !pair.is_empty()
                && ![
                    "signature=",
                    "timestamp=",
                    "newClientOrderId=",
                    "clientAlgoId=",
                ]
                .iter()
                .any(|key| pair.starts_with(key))
        })
        .map(str::to_owned)
        .collect();
    params.sort();
    let path = path.rsplit_once(".com").map_or(path, |(_, path)| path);
    (request.method, path.to_owned(), params, String::new())
}

fn shapes(h: &Harness) -> Vec<(Method, String, Vec<String>, String)> {
    h.requests.iter().map(shape).collect()
}

/// Run `command` on both engines and answer each step identically; the
/// frame-fed engine must send what LOCAL sends, and read nothing.
fn same_writes(local: &mut Harness, fed: &mut Harness, command: Command, replies: &[&str]) {
    local.event(Event::Command(command.clone()));
    fed.event(Event::Command(command));
    for body in replies {
        assert_eq!(shapes(fed), shapes(local), "fed {:?}", fed.paths());
        assert_eq!(
            fed.requests.len(),
            1,
            "one request per step: {:?}",
            fed.paths()
        );
        assert_ne!(fed.requests[0].method, Method::Get, "never a read");
        let (l, f) = (local.requests.remove(0), fed.requests.remove(0));
        local.reply(l.id, 200, "", body);
        fed.reply(f.id, 200, "", body);
    }
    assert!(
        fed.requests.is_empty(),
        "no read after the reply: {:?}",
        fed.paths()
    );
}

fn limit() -> PlaceOrderOpts {
    serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01,"price":99}"#,
    )
    .unwrap()
}

const PLACED: &str = r#"{"symbol":"BTCUSDT","orderId":77,"clientOrderId":"x","origQty":"0.01","executedQty":"0","price":"99","type":"LIMIT","side":"BUY","status":"NEW"}"#;

fn canceled(filled: u32) -> String {
    format!(
        r#"{{"symbol":"BTCUSDT","orderId":12,"clientOrderId":"maker","origQty":"7","executedQty":"{filled}","status":"CANCELED"}}"#
    )
}

#[test]
fn a_frame_fed_engine_loads_public_metadata_once_and_opens_no_socket() {
    frame_fed();
}

#[test]
fn ingested_source_state_reaches_the_store_with_its_leverage_brackets() {
    let mut source = local();
    let fed = fed_from(&mut source);
    let store = fed.engine.store();
    assert_eq!(store.orders().len(), source.engine.store().orders().len());
    assert_eq!(store.positions().len(), 1);
    assert!(store.loaded().orders && store.loaded().positions);
    assert_eq!(store.market("BTCUSDT").unwrap().limits.leverage.max, 50.);
}

#[test]
fn a_placement_is_the_local_request_and_nothing_follows() {
    let mut source = local();
    let mut fed = fed_from(&mut source);
    same_writes(
        &mut source,
        &mut fed,
        Command::PlaceOrders {
            command: 5,
            orders: vec![limit()],
        },
        &[PLACED],
    );
    assert!(matches!(fed.result(5), Some(Ok(_))), "{:?}", fed.done);
}

#[test]
fn a_cancel_is_the_local_request_and_nothing_follows() {
    let mut source = local();
    let mut fed = fed_from(&mut source);
    same_writes(
        &mut source,
        &mut fed,
        Command::CancelOrders {
            command: 6,
            ids: vec!["12".into()],
        },
        &[&canceled(2)],
    );
    assert!(matches!(fed.result(6), Some(Ok(_))), "{:?}", fed.done);
}

#[test]
fn an_amend_cancels_then_replaces_exactly_as_local() {
    let mut source = local();
    let mut fed = fed_from(&mut source);
    same_writes(
        &mut source,
        &mut fed,
        Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(101.0),
            amount: None,
        },
        &[&canceled(2), PLACED],
    );
    assert!(matches!(fed.result(7), Some(Ok(_))), "{:?}", fed.done);
}

#[test]
fn a_leverage_set_clamps_to_the_ingested_bracket_as_local() {
    let mut source = local();
    let mut fed = fed_from(&mut source);
    same_writes(
        &mut source,
        &mut fed,
        Command::SetLeverage {
            command: 8,
            symbol: "BTCUSDT".into(),
            leverage: 75.0,
            side: None,
        },
        &[r#"{"symbol":"BTCUSDT","leverage":50,"maxNotionalValue":"5000"}"#],
    );
    assert!(matches!(fed.result(8), Some(Ok(_))), "{:?}", fed.done);
}

#[test]
fn a_stop_limit_amend_is_refused_from_the_ingested_venue_state() {
    let mut source = local();
    let stop = source
        .engine
        .store()
        .orders()
        .iter()
        .find(|order| order.trigger_type.is_some() || order.price == 89.)
        .map(|order| order.id.clone())
        .expect("the stop-limit is stored");
    let mut fed = fed_from(&mut source);
    for h in [&mut source, &mut fed] {
        h.event(Event::Command(Command::AmendOrder {
            command: 9,
            id: stop.clone(),
            price: Some(88.0),
            amount: None,
        }));
        assert!(h.requests.is_empty(), "{:?}", h.paths());
        let failed = h.result(9).cloned();
        assert!(
            matches!(&failed, Some(Err(e)) if e.contains("stop-limit")),
            "{failed:?}"
        );
    }
}

#[test]
fn an_unknown_placement_outcome_is_never_re_sent_or_followed_by_a_read() {
    let mut source = local();
    let mut fed = fed_from(&mut source);
    fed.event(Event::Command(Command::PlaceOrders {
        command: 10,
        orders: vec![limit()],
    }));
    let placed = fed.take("/fapi/v1/order");
    assert_eq!(placed.method, Method::Post);
    fed.reply(placed.id, 502, "", "<html>bad gateway</html>");
    fed.advance(120_000.0);
    assert!(
        fed.requests.is_empty(),
        "no re-send, no read: {:?}",
        fed.paths()
    );
}
