#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_backpack::{Backpack, Options};
pub type Driver = Engine<Backpack>;
pub const SYMBOL: &str = "SOL_USDC_PERP";
pub const POSITIONS: &str = r#"[{"symbol":"SOL_USDC_PERP","netQuantity":"5","entryPrice":"140","netExposureNotional":"705","pnlUnrealized":"5","estLiquidationPrice":"100"}]"#;
pub const BALANCE: &str = r#"{"netEquity":"1005","netEquityAvailable":"905","netEquityLocked":"100","pnlUnrealized":"5"}"#;
pub const ORDER: &str = r#"{"id":"123","clientId":42,"symbol":"SOL_USDC_PERP","side":"Bid","orderType":"Limit","price":"140","quantity":"5","executedQuantity":"0","timeInForce":"GTC","status":"New","createdAt":1000}"#;
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
        1000.0,
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
pub fn ws(e: &mut Driver, socket: SocketId, stream: &str, data: &str) {
    let text = format!(r#"{{"stream":"{stream}","data":{data}}}"#);
    e.handle(
        2000.0,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
}
pub fn ready() -> (Driver, SocketId) {
    ready_position(200, POSITIONS)
}
pub fn ready_position(position_status: u16, position_body: &str) -> (Driver, SocketId) {
    ready_window(position_status, position_body, None)
}
pub fn ready_window(
    position_status: u16,
    position_body: &str,
    auth_window: Option<(f64, u32)>,
) -> (Driver, SocketId) {
    ready_snapshot(position_status, position_body, auth_window, "[]")
}
pub fn ready_orders(orders: &str) -> (Driver, SocketId) {
    ready_snapshot(200, POSITIONS, None, orders)
}
fn ready_snapshot(
    position_status: u16,
    position_body: &str,
    auth_window: Option<(f64, u32)>,
    orders: &str,
) -> (Driver, SocketId) {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    // Public deterministic test seed; never a real exchange account.
    let secret = STANDARD.encode([7u8; 32]);
    let mut e = Engine::new(Backpack::new(Options {
        public_data_only: false,
        key: Some("test-public-key".into()),
        secret: Some(secret),
        recv_window: auth_window.map(|w| w.0),
    }));
    e.start(1000.0);
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        200,
        r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"},{"symbol":"BTC_USDC_PERP","marketType":"PERP","quantityIncrement":"0.001","priceIncrement":"0.1"}]"#,
    );
    for r in requests(&mut e) {
        reply(
            &mut e,
            &r,
            200,
            if r.url.contains("tickers") {
                r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.01","volume":"10","quoteVolume":"1410"}]"#
            } else {
                "[]"
            },
        );
    }
    let mut private = None;
    for _ in 0..6 {
        let actions: Vec<_> = e.drain_actions().collect();
        for a in actions {
            match a {
                Action::Http(r) => {
                    let body = if r.url.ends_with("/account") {
                        r#"{"leverageLimit":"10"}"#
                    } else if r.url.ends_with("/position") {
                        position_body
                    } else if r.url.ends_with("/collateral") {
                        BALANCE
                    } else {
                        orders
                    };
                    assert!(r.headers.iter().any(|(n, _)| *n == "X-Signature"));
                    let expected = auth_window.map(|w| w.1).unwrap_or(60000);
                    assert!(
                        r.headers
                            .iter()
                            .any(|(n, v)| *n == "X-Window" && v == &expected.to_string())
                    );
                    reply(
                        &mut e,
                        &r,
                        if r.url.ends_with("/position") {
                            position_status
                        } else {
                            200
                        },
                        body,
                    );
                }
                Action::WsConnect { socket, .. } => private = Some(socket),
                _ => {}
            }
        }
    }
    (e, private.expect("private socket"))
}
