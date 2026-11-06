#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_coinbase::{Coinbase, Options};
pub type Driver = Engine<Coinbase>;
/// One keyless spot product: the public catalogue Coinbase still serves.
pub const PRODUCTS: &str = r#"{"products":[{"product_type":"SPOT","product_id":"BTC-USD","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"0.0001","price_increment":"0.1","base_min_size":"0.0001","base_max_size":"10000","price":"60000","volume_24h":"20","price_percentage_change_24h":"1"}],"pagination":{"has_next":false}}"#;
pub fn requests(e: &mut Driver) -> Vec<HttpRequest> {
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
