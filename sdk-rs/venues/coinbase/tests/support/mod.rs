#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_coinbase::{Coinbase, Options};
pub type Driver = Engine<Coinbase>;
// Legacy Advanced Trade spot coverage remains alongside the drb perpetual gate.
pub const PRODUCTS: &str = r#"{"products":[{"product_id":"BTC-USD","product_type":"SPOT","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"0.0001","price_increment":"0.1","base_min_size":"0.0001","base_max_size":"1000000000","price":"60000","volume_24h":"20","price_percentage_change_24h":"0"}]}"#;
pub fn actions(e: &mut Driver) -> Vec<Action> {
    let now = safe_cex_engine::test_support::dispatch_now(e);
    safe_cex_engine::test_support::prepared_actions(e, now)
}
pub fn requests(e: &mut Driver) -> Vec<HttpRequest> {
    actions(e)
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect()
}
pub fn reply(e: &mut Driver, r: &HttpRequest, status: u16, body: &str) {
    e.handle(
        1000.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
pub fn drb_response(method: &str, request: &HttpRequest) -> String {
    let record = include_str!("../fixtures/drb-public.jsonl")
        .lines()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap())
        .find(|r| r["method"] == method)
        .unwrap();
    let sent: serde_json::Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    let mut body: serde_json::Value =
        serde_json::from_str(record["body"].as_str().unwrap()).unwrap();
    body["id"] = sent["id"].clone();
    body.to_string()
}
pub fn ready() -> (Driver, SocketId) {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            spot_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, PRODUCTS);
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
    e.handle(1000., Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    (e, socket)
}
pub fn message(e: &mut Driver, socket: SocketId, text: &str) {
    e.handle(2000., Event::WsMessage { socket, text });
}

/// A failed-read retry delay lies in the engine's jittered `[0.75, 1.0] * nominal`.
pub fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
