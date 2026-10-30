//! Frame-fed Binance spot and portfolio margin engines: public instruments only,
//! and fed a LOCAL engine's snapshot, each write sends exactly LOCAL's requests.
//! Cancel routing by client id needs the PM owner and the client-to-native maps
//! that VENUE_STATE v2 carries.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Command, Event, HttpRequest, Method};
use safe_cex_venue_binance::Variant;
use support::{Harness, Options};

const PM_OPEN: &str = r#"[{"avgPrice":"0.00000","clientOrderId":"x-s3vFFRQpresting","cumQuote":"0","executedQty":"0","orderId":1917641,"origQty":"0.40","origType":"LIMIT","price":"90000","reduceOnly":false,"side":"BUY","positionSide":"BOTH","status":"NEW","symbol":"BTCUSDT","time":1579276756075,"timeInForce":"GTC","type":"LIMIT","updateTime":1579276756075,"selfTradePreventionMode":"NONE","goodTillDate":0}]"#;
const PM_CONDITIONAL: &str = r#"[{"newClientStrategyId":"abc","strategyId":123445,"strategyStatus":"NEW","strategyType":"TAKE_PROFIT_MARKET","origQty":"0.010","price":"0","reduceOnly":true,"side":"SELL","positionSide":"BOTH","stopPrice":"99000","symbol":"BTCUSDT","bookTime":1566818724710,"updateTime":1566818724722,"timeInForce":"GTC","workingType":"MARK_PRICE","priceProtect":false}]"#;
const PM_POSITION: &str = r#"[{"entryPrice":"95000.0","leverage":"20","markPrice":"96000.0","maxNotionalValue":"6000000.0","positionAmt":"0.010","notional":"960.0","symbol":"BTCUSDT","unRealizedProfit":"10.0","liquidationPrice":"80000.5","positionSide":"BOTH","updateTime":1720736417660}]"#;
const PM_BALANCE: &str = r#"[{"asset":"USDT","totalWalletBalance":"1000.0","crossMarginFree":"900.0","umWalletBalance":"1000.0","umUnrealizedPNL":"10.0","cmWalletBalance":"0","cmUnrealizedPNL":"0","updateTime":0,"negativeBalance":"0"}]"#;
const SPOT_INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"}]}]}"#;
const SPOT_OPEN: &str = r#"[{"symbol":"BTCUSDT","orderId":12,"clientOrderId":"maker","price":"9000","origQty":"7","executedQty":"0","type":"LIMIT","side":"BUY","timeInForce":"GTC"}]"#;

fn pm_options() -> Options {
    Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        portfolio_margin: true,
        ..Options::default()
    }
}

fn spot_options() -> Options {
    Options {
        variant: Variant::Spot,
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    }
}

/// LOCAL portfolio margin with a resting regular order and a strategy order.
fn pm_local() -> Harness {
    let mut h = Harness::started(pm_options());
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond(
        "/papi/v1/um/leverageBracket",
        200,
        r#"[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":125}]}]"#,
    );
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond("/papi/v1/listenKey", 200, r#"{"listenKey":"pmkey"}"#);
    h.respond(
        "/papi/v1/um/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/papi/v1/um/positionRisk", 200, PM_POSITION);
    h.respond("/papi/v1/balance", 200, PM_BALANCE);
    h.respond("/papi/v1/um/openOrders", 200, PM_OPEN);
    h.respond("/papi/v1/um/conditional/openOrders", 200, PM_CONDITIONAL);
    for path in ["/public/ws", "/market/ws", "/pm/ws"] {
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
    }
    h.requests.clear();
    h
}

/// LOCAL spot with one resting order.
fn spot_local() -> Harness {
    let mut h = Harness::started(spot_options());
    h.respond("/api/v3/exchangeInfo", 200, SPOT_INFO);
    h.respond("/api/v3/ticker/24hr", 200, r#"[{"symbol":"BTCUSDT","lastPrice":"10000","bidPrice":"9999","askPrice":"10001","priceChangePercent":"0","volume":"1","quoteVolume":"10000"}]"#);
    let socket = h.socket("ws-api/v3").unwrap();
    h.event(Event::WsOpened { socket });
    h.respond("/api/v3/account", 200, r#"{"balances":[]}"#);
    h.respond("/api/v3/openOrders", 200, SPOT_OPEN);
    h.requests.clear();
    h
}

/// A frame-fed engine after its one public instrument read, fed `source`'s snapshot.
fn fed_from(source: &mut Harness, options: Options, instruments: (&str, &str)) -> Harness {
    let mut engine = support::engine(options);
    engine.set_frame_fed().unwrap();
    let mut fed = Harness::from_engine(engine);
    assert_eq!(fed.paths().len(), 1, "{:?}", fed.paths());
    fed.respond(instruments.0, 200, instruments.1);
    fed.advance(120_000.0);
    assert!(
        fed.requests.is_empty(),
        "no reads or polls: {:?}",
        fed.paths()
    );
    assert!(fed.sockets.is_empty(), "no socket: {:?}", fed.sockets);
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

/// Method, path and parameters without signature, clock and generated ids.
fn shape(request: &HttpRequest) -> (Method, String, Vec<String>) {
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
                    "newClientStrategyId=",
                ]
                .iter()
                .any(|key| pair.starts_with(key))
        })
        .map(str::to_owned)
        .collect();
    params.sort();
    let path = path.rsplit_once(".com").map_or(path, |(_, path)| path);
    (request.method, path.to_owned(), params)
}

/// Run `command` on both engines and answer each step alike: the frame-fed
/// engine sends what LOCAL sends, one write per step, and never a read.
fn same_writes(local: &mut Harness, fed: &mut Harness, command: Command, replies: &[&str]) {
    local.event(Event::Command(command.clone()));
    fed.event(Event::Command(command));
    for body in replies {
        let shapes = |h: &Harness| h.requests.iter().map(shape).collect::<Vec<_>>();
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

#[test]
fn a_frame_fed_pm_engine_cancels_regular_and_strategy_orders_as_local() {
    let mut local = pm_local();
    let mut fed = fed_from(
        &mut local,
        pm_options(),
        ("/fapi/v1/exchangeInfo", support::EXCHANGE_INFO),
    );
    same_writes(
        &mut local,
        &mut fed,
        Command::CancelOrders {
            command: 3,
            ids: vec!["x-s3vFFRQpresting".into(), "abc".into()],
        },
        &[
            r#"{"symbol":"BTCUSDT","orderId":1917641,"clientOrderId":"x-s3vFFRQpresting","origQty":"0.40","executedQty":"0","status":"CANCELED"}"#,
            r#"{"symbol":"BTCUSDT","strategyId":123445,"newClientStrategyId":"abc","strategyStatus":"CANCELED"}"#,
        ],
    );
    assert!(matches!(fed.result(3), Some(Ok(_))), "{:?}", fed.done);
}

#[test]
fn a_frame_fed_pm_placement_is_the_local_request_and_nothing_follows() {
    let mut local = pm_local();
    let mut fed = fed_from(
        &mut local,
        pm_options(),
        ("/fapi/v1/exchangeInfo", support::EXCHANGE_INFO),
    );
    let order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01,"price":90000}"#,
    )
    .unwrap();
    same_writes(
        &mut local,
        &mut fed,
        Command::PlaceOrders {
            command: 4,
            orders: vec![order],
        },
        &[
            r#"{"symbol":"BTCUSDT","orderId":77,"clientOrderId":"x","origQty":"0.01","executedQty":"0","price":"90000","type":"LIMIT","side":"BUY","status":"NEW"}"#,
        ],
    );
}

#[test]
fn a_frame_fed_spot_engine_cancels_and_places_as_local() {
    let mut local = spot_local();
    let mut fed = fed_from(
        &mut local,
        spot_options(),
        ("/api/v3/exchangeInfo", SPOT_INFO),
    );
    same_writes(
        &mut local,
        &mut fed,
        Command::CancelOrders {
            command: 5,
            ids: vec!["maker".into()],
        },
        &[
            r#"{"symbol":"BTCUSDT","orderId":12,"clientOrderId":"cancel-request","origClientOrderId":"maker","origQty":"7","executedQty":"0","status":"CANCELED"}"#,
        ],
    );
    let order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":9000}"#,
    )
    .unwrap();
    same_writes(
        &mut local,
        &mut fed,
        Command::PlaceOrders {
            command: 6,
            orders: vec![order],
        },
        &[
            r#"{"symbol":"BTCUSDT","orderId":13,"clientOrderId":"x","origQty":"1","executedQty":"0","price":"9000","type":"LIMIT","side":"BUY","status":"NEW"}"#,
        ],
    );
}
