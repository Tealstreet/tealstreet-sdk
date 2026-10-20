#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_apexomni::{ApexOmni, Options};
pub type TestEngine = Engine<ApexOmni>;
pub const MARKETS: &str = r#"{"code":0,"data":{"contractConfig":{"perpetualContract":[{"symbol":"BTC-USDT","crossSymbolName":"BTCUSDT","baseTokenId":"BTC","settleAssetId":"USDT","enableTrade":true,"stepSize":"0.001","tickSize":"0.1","minOrderSize":"0.001","maxOrderSize":"1000","displayMinLeverage":"1","displayMaxLeverage":"100","l2PairId":"50000","initialMarginRate":"0.01","defaultInitialMarginRate":"0.05","maxMarketPriceRange":"0.045"}]}}}"#;
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
pub fn response(e: &mut TestEngine, r: &HttpRequest, now: f64, body: &str) {
    e.handle(
        now,
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
pub fn started() -> (TestEngine, SocketId) {
    let mut e = Engine::new(ApexOmni::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
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
    e.handle(20.0, Event::WsOpened { socket });
    (e, socket)
}
pub fn ws(e: &mut TestEngine, socket: SocketId, time: f64, text: &str) {
    e.handle(time, Event::WsMessage { socket, text });
}
