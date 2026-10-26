#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_woo::{Options, Woo};
pub type TestEngine = Engine<Woo>;
pub const MARKETS: &str = r#"{"success":true,"rows":[{"symbol":"SPOT_BTC_USDT"},{"symbol":"PERP_BTC_USDT","base_tick":0.001,"quote_tick":0.1,"base_min":0.001,"base_max":100,"base_imr":0.02,"price_range":0.01}]}"#;
pub const TICKERS: &str = r#"{"success":true,"rows":[{"symbol":"PERP_UNKNOWN_USDT"},{"symbol":"PERP_BTC_USDT","24h_open":50000,"24h_close":60000,"24h_volume":4,"24h_amount":240000,"index_price":59999,"mark_price":60001,"last_funding_rate":0.0001,"next_funding_time":1700028800000,"open_interest":10000}]}"#;
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
pub fn released_actions(e: &mut TestEngine, now: f64) -> Vec<Action> {
    let mut actions: Vec<_> = e.drain_actions().collect();
    let wakes: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == now => Some(*timer),
            _ => None,
        })
        .collect();
    for timer in wakes {
        e.handle(now, Event::TimerFired { timer });
        actions.extend(e.drain_actions());
    }
    actions
}
pub fn released_requests(e: &mut TestEngine, now: f64) -> Vec<HttpRequest> {
    released_actions(e, now)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
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
    let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 20.0, TICKERS);
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
    e.handle(30.0, Event::WsOpened { socket });
    (e, socket)
}
pub fn ws(e: &mut TestEngine, socket: SocketId, time: f64, text: &str) {
    e.handle(time, Event::WsMessage { socket, text });
}
pub fn snapshot(e: &mut TestEngine, r: &HttpRequest, now: f64, time: u64) {
    response(
        e,
        r,
        now,
        &format!(
            r#"{{"success":true,"timestamp":{time},"bids":[{{"price":59000,"quantity":2}}],"asks":[{{"price":61000,"quantity":3}}]}}"#
        ),
    );
}

/// A failed-read retry delay lies in the engine's jittered `[0.75, 1.0] * nominal`.
pub fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
