#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_mexc::MexcFutures;
mod credentials;
pub use credentials::{Options, configured};

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
pub fn prepared(e: &mut TestEngine, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
pub fn public_requests(e: &mut TestEngine, now: f64) -> Vec<HttpRequest> {
    prepared(e, now)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect()
}
pub fn started() -> (TestEngine, SocketId) {
    let mut e = configured(Options {
        public_data_only: true,
        ..Default::default()
    });
    e.start(0.0);
    let r = public_requests(&mut e, 0.).remove(0);
    response(&mut e, &r, MARKETS);
    let r = public_requests(&mut e, 100.).remove(0);
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
    let (e, socket, reads, timer, _) = private_started_with_retry(markets);
    (e, socket, reads, timer)
}
/// Also returns the catalogue retry a partial first read schedules.
pub fn private_started_with_retry(
    markets: &str,
) -> (
    TestEngine,
    SocketId,
    Vec<HttpRequest>,
    safe_cex_engine::TimerId,
    Option<safe_cex_engine::TimerId>,
) {
    let mut e = configured(Options {
        recv_window: None,
        read_credentials: None,
        public_data_only: false,
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
    });
    e.start(0.0);
    let r = public_requests(&mut e, 0.).remove(0);
    response(&mut e, &r, markets);
    let mut actions = prepared(&mut e, 100.);
    let retry = actions.iter().find_map(|a| match a {
        Action::SetTimer { timer, at_ms } if jittered(*at_ms - 100.0, 1000.0) => Some(*timer),
        _ => None,
    });
    let ticker = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("contract/ticker") => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    response(&mut e, &ticker, TICKERS);
    actions.extend(prepared(&mut e, 100.));
    let private_socket = actions
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
    let mode = actions
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a
                && r.url.contains("position_mode")
            {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    response(&mut e, mode, r#"{"success":true,"code":0,"data":2}"#);
    let actions = prepared(&mut e, 100.);
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
    e.handle(
        100.0,
        Event::WsOpened {
            socket: private_socket,
        },
    );
    e.prepare_dispatch(100.);
    (e, private_socket, reads, timer, retry)
}

/// A failed-read retry delay lies in the engine's jittered `[0.75, 1.0] * nominal`.
pub fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
