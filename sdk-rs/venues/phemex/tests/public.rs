//! Oracles: https://phemex-docs.github.io/ USDⓈ-M public response examples.
use safe_cex_engine::{Action, Command, Engine, Event, RequestId, Response, SocketId, Venue};
use safe_cex_venue_phemex::{Options, PhemexPerpetuals};
const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"500","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"error":null,"result":[{"symbol":"BTCUSDT","bidRp":"20690.1","askRp":"20690.2","lastRp":"20690.2","markRp":"20690.3","indexRp":"20690","openRp":"20000","openInterestRv":"250","fundingRateRr":"0.0001","volumeRq":"10"}]}"#;
fn respond(e: &mut Engine<PhemexPerpetuals>, id: RequestId, body: &str) {
    e.handle(
        0.,
        Event::HttpResponse {
            request: id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    )
}
fn request(e: &mut Engine<PhemexPerpetuals>, path: &str) -> RequestId {
    e.drain_actions()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.id),
            _ => None,
        })
        .unwrap()
}
fn started() -> (Engine<PhemexPerpetuals>, SocketId) {
    let mut e = Engine::new(PhemexPerpetuals::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(0.);
    let id = request(&mut e, "products");
    respond(&mut e, id, PRODUCTS);
    let id = request(&mut e, "24hr/all");
    respond(&mut e, id, DAILY);
    let socket = e
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(0., Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    (e, socket)
}
#[test]
fn real_value_products_and_packed_ticker() {
    let (mut e, socket) = started();
    assert_eq!(e.store().market("BTCUSDT").unwrap().precision.price, 0.1);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().bid, 20690.1);
    e.handle(1.,Event::WsMessage{socket,text:r#"{"method":"perp_market24h_pack_p.update","data":[["BTCUSDT","20000",0,0,"21000","12",0,"300","20999","21001","0.0002",0]]}"#});
    let t = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!(t.bid, 20690.1);
    assert_eq!(t.last, 21000.);
    assert_eq!(t.quote_volume, 252000.);
    assert_eq!(t.percentage, 5.);
}
#[test]
fn delayed_snapshot_applies_newer_diffs_and_discards_old() {
    let (mut e, socket) = started();
    e.handle(
        0.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    e.handle(1.,Event::WsMessage{socket,text:r#"{"symbol":"BTCUSDT","sequence":102,"type":"incremental","orderbook_p":{"bids":[["20690","2"]],"asks":[]}}"#});
    let id = request(&mut e, "fullbook");
    respond(
        &mut e,
        id,
        r#"{"result":{"sequence":101,"orderbook_p":{"bids":[["20690","1"],["20689","1"]],"asks":[["20691","1"]]}}}"#,
    );
    assert_eq!(
        e.venue().book("BTCUSDT").unwrap().bids(),
        &[(20690., 2.), (20689., 1.)]
    );
    e.handle(2.,Event::WsMessage{socket,text:r#"{"symbol":"BTCUSDT","sequence":100,"type":"incremental","orderbook_p":{"bids":[["20690","999"]]}}"#});
    assert_eq!(e.venue().book("BTCUSDT").unwrap().bids()[0].1, 2.);
    e.handle(3.,Event::WsMessage{socket,text:r#"{"symbol":"BTCUSDT","sequence":103,"type":"snapshot","orderbook_p":{"bids":[["20688","3"]],"asks":[["20692","4"]]}}"#});
    assert_eq!(e.venue().book("BTCUSDT").unwrap().bids(), &[(20688., 3.)]);
}
#[test]
fn unsubscribed_snapshot_is_fenced() {
    let (mut e, socket) = started();
    e.handle(
        0.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        1.,
        Event::WsMessage {
            socket,
            text: r#"{"symbol":"BTCUSDT","sequence":102,"orderbook_p":{"bids":[],"asks":[]}}"#,
        },
    );
    let id = request(&mut e, "fullbook");
    e.handle(
        2.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    respond(
        &mut e,
        id,
        r#"{"result":{"sequence":101,"orderbook_p":{"bids":[["1","2"]],"asks":[]}}}"#,
    );
    assert!(e.venue().book("BTCUSDT").is_none());
}
#[test]
fn silent_socket_closes_and_reconnects() {
    let (mut e, socket) = started();
    e.handle(0., Event::WsClosed { socket, code: 1006 });
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1000., Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    let timer = actions
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(22000., Event::TimerFired { timer });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );
}

#[test]
fn incomplete_book_snapshot_is_not_committed() {
    let (mut e, socket) = started();
    e.handle(
        0.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        1.,
        Event::WsMessage {
            socket,
            text: r#"{"symbol":"BTCUSDT","sequence":2,"orderbook_p":{"bids":[],"asks":[]}}"#,
        },
    );
    let id = request(&mut e, "fullbook");
    respond(
        &mut e,
        id,
        r#"{"result":{"sequence":1,"orderbook_p":{"bids":[["1","1"]]}}}"#,
    );
    assert!(e.venue().book("BTCUSDT").is_none());
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
}
#[test]
fn invalid_supported_product_rejects_the_entire_seed() {
    let mut e = Engine::new(PhemexPerpetuals::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(0.);
    let id = request(&mut e, "products");
    respond(
        &mut e,
        id,
        &PRODUCTS.replace("\"qtyStepSize\":\"0.001\"", "\"qtyStepSize\":\"0\""),
    );
    assert!(!e.store().loaded().markets);
    assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
}
