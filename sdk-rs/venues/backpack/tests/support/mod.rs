#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_backpack::{Backpack, Options};
pub type Driver = Engine<Backpack>;
pub mod managed;
/// Retry delays are jittered into `[0.75, 1.0] * nominal`.
pub fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
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
    let (e, socket, _, _) =
        ready_snapshot(MARKETS, position_status, position_body, auth_window, "[]");
    (e, socket)
}
pub fn ready_orders(orders: &str) -> (Driver, SocketId) {
    let (e, socket, _, _) = ready_snapshot(MARKETS, 200, POSITIONS, None, orders);
    (e, socket)
}
/// Startup against a custom market catalogue; also returns the startup logs and timers.
pub fn ready_catalogue(
    markets: &str,
    positions: &str,
    orders: &str,
) -> (
    Driver,
    SocketId,
    Vec<String>,
    Vec<(safe_cex_engine::TimerId, f64)>,
) {
    ready_snapshot(markets, 200, positions, None, orders)
}
pub const MARKETS: &str = r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"},{"symbol":"BTC_USDC_PERP","marketType":"PERP","quantityIncrement":"0.001","priceIncrement":"0.1"}]"#;
fn ready_snapshot(
    markets: &str,
    position_status: u16,
    position_body: &str,
    auth_window: Option<(f64, u32)>,
    orders: &str,
) -> (
    Driver,
    SocketId,
    Vec<String>,
    Vec<(safe_cex_engine::TimerId, f64)>,
) {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    // Public deterministic test seed; never a real exchange account.
    let secret = STANDARD.encode([7u8; 32]);
    let (provider, identities) =
        safe_cex_crypto::signer::LocalSigner::from_setup(safe_cex_crypto::signer::LocalSetup {
            primary: Some(safe_cex_crypto::signer::LocalIdentity {
                key: safe_cex_crypto::signer::Secret::new("test-public-key".into()),
                secret: safe_cex_crypto::signer::Secret::new(secret.clone()),
                secret_encoding: safe_cex_engine::signing::KeyEncoding::Base64,
                ..Default::default()
            }),
            secondary: None,
        })
        .unwrap();
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        identity: identities.primary.clone(),
        read_credentials: None,
        public_data_only: false,
        key: Some("test-public-key".into()),
        secret: Some(secret),
        recv_window: auth_window.map(|w| w.0),
    }));
    e.set_signer(Some(std::sync::Arc::new(provider))).unwrap();
    e.set_identities(identities).unwrap();
    e.start(1000.0);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, markets);
    let mut logs = vec![];
    let mut timers = vec![];
    let mut private = None;
    for _ in 0..6 {
        let actions: Vec<_> = e.drain_actions().collect();
        for a in actions {
            match a {
                Action::Http(r) => {
                    if r.url.contains("/tickers") || r.url.contains("/markPrices") {
                        reply(
                            &mut e,
                            &r,
                            200,
                            if r.url.contains("/tickers") {
                                r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.01","volume":"10","quoteVolume":"1410"}]"#
                            } else {
                                "[]"
                            },
                        );
                        continue;
                    }
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
                Action::WsConnect {
                    socket,
                    channel: Some(safe_cex_engine::SocketChannel::Private),
                    ..
                } => private = Some(socket),
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    message,
                    ..
                })) => logs.push(message),
                Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)),
                _ => {}
            }
        }
    }
    (e, private.expect("private socket"), logs, timers)
}
