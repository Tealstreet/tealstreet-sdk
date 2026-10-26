#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_coinbase::{Coinbase, Options};
pub type Driver = Engine<Coinbase>;
// Coinbase INTX instrument schema, https://docs.cdp.coinbase.com/api-reference/international-exchange-api/rest-api/instruments/list-instruments
pub const INSTRUMENTS: &str = r#"[{"symbol":"BTC-PERP","type":"PERP","trading_state":"TRADING","base_asset_name":"BTC","quote_asset_name":"USDC","base_increment":"0.0001","quote_increment":"0.1","min_quantity":"0.0001","qty_24hr":"20","notional_24hr":"1200000","open_interest":"100","funding_interval":"3600000000","quote":{"best_bid_price":"59999","best_ask_price":"60001","trade_price":"60000","mark_price":"60002","index_price":"60003","predicted_funding":"0.0001"}}]"#;
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
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, INSTRUMENTS);
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
