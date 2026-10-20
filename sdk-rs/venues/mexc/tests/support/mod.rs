#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_mexc::{MexcFutures, Options};

pub const MARKETS: &str = r#"{"success":true,"code":0,"data":[{"symbol":"BTC_USDT","baseCoin":"BTC","quoteCoin":"USDT","settleCoin":"USDT","state":0,"apiAllowed":true,"contractSize":0.0001,"volUnit":1,"priceUnit":0.1,"minVol":1,"maxVol":400000,"minLeverage":1,"maxLeverage":500,"makerFeeRate":0,"takerFeeRate":0.0002}]}"#;
pub const TICKERS: &str = r#"{"success":true,"code":0,"data":[{"symbol":"BTC_USDT","lastPrice":60000,"fairPrice":60001,"indexPrice":59999,"bid1":59999.9,"ask1":60000,"riseFallRate":0.01,"holdVol":10000,"fundingRate":0.0001,"volume24":50000,"amount24":300000}]}"#;
pub type TestEngine = Engine<MexcFutures>;
pub fn response(e: &mut TestEngine, r: &HttpRequest, body: &str) {
    e.handle(
        100.0,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
pub fn requests(e: &mut TestEngine) -> Vec<HttpRequest> {
    e.drain_actions()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect()
}
pub fn started() -> (TestEngine, SocketId) {
    let mut e = Engine::new(MexcFutures::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, MARKETS);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, TICKERS);
    let socket = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(100.0, Event::WsOpened { socket });
    (e, socket)
}
pub fn ws(e: &mut TestEngine, socket: SocketId, text: &str) {
    e.handle(200.0, Event::WsMessage { socket, text });
}

pub fn private_started() -> (
    TestEngine,
    SocketId,
    Vec<HttpRequest>,
    safe_cex_engine::TimerId,
) {
    private_started_with_markets(MARKETS)
}
pub fn private_started_with_markets(
    markets: &str,
) -> (
    TestEngine,
    SocketId,
    Vec<HttpRequest>,
    safe_cex_engine::TimerId,
) {
    let mut e = Engine::new(MexcFutures::new(Options {
        public_data_only: false,
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, markets);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, TICKERS);
    let actions: Vec<_> = e.drain_actions().collect();
    let sockets: Vec<_> = actions
        .iter()
        .filter_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(*socket)
            } else {
                None
            }
        })
        .collect();
    let mode = actions
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    response(&mut e, mode, r#"{"success":true,"code":0,"data":2}"#);
    let actions: Vec<_> = e.drain_actions().collect();
    let timer = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                if *at_ms == 2100.0 { Some(*timer) } else { None }
            } else {
                None
            }
        })
        .unwrap();
    let reads = actions
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    e.handle(100.0, Event::WsOpened { socket: sockets[1] });
    (e, sockets[1], reads, timer)
}
