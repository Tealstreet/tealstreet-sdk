#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketChannel, SocketId};
use safe_cex_venue_kraken::{KrakenSpot, Options};
pub const NOW: f64 = 1_700_000_000_000.;
pub const META: &str = r#"{"error":[],"result":{"XXBTZUSD":{"altname":"XBTUSD","wsname":"XBT/USD","pair_decimals":1,"lot_decimals":4,"ordermin":"0.0001","status":"online"},"XETHZUSD":{"altname":"ETHUSD","wsname":"ETH/USD","pair_decimals":2,"lot_decimals":3,"ordermin":"0.001","status":"online"},"XXBTZEUR":{"wsname":"XBT/EUR","status":"online"},"XXBTZUSD.d":{"wsname":"XBT/USD","status":"online"}}}"#;
pub const TICKERS: &str = r#"{"error":[],"result":{"XXBTZUSD":{"a":["101","1","1"],"b":["99","1","1"],"c":["100","1"],"v":["2","4"],"o":"80"},"XETHZUSD":{"a":["51","1","1"],"b":["49","1","1"],"c":["50","1"],"v":["2","4"],"o":"50"}}}"#;
pub const OPEN: &str = r#"{"error":[],"result":{"open":{"O-100":{"descr":{"pair":"XBTUSD","type":"buy","ordertype":"limit","price":"99"},"vol":"2","vol_exec":"0.5","opentm":1700000000,"oflags":"post"}}}}"#;
pub const BALANCE: &str =
    r#"{"error":[],"result":{"ZUSD":"1000","XXBT":"2","XETH":"3","USDT":"5","XXBT.B":"4"}}"#;
pub fn options() -> Options {
    Options {
        key: Some("replay-key".into()),
        secret: Some("cmVwbGF5LXNlY3JldA==".into()),
        ..Options::default()
    }
}
pub fn reply(e: &mut Engine<KrakenSpot>, r: &HttpRequest, body: &str) {
    reply_at(e, NOW, r, body)
}
pub fn reply_at(e: &mut Engine<KrakenSpot>, now: f64, r: &HttpRequest, body: &str) {
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
pub fn requests(e: &mut Engine<KrakenSpot>) -> Vec<HttpRequest> {
    safe_cex_engine::test_support::prepared_actions(e, NOW)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect()
}
pub fn message(e: &mut Engine<KrakenSpot>, s: SocketId, text: &str) {
    e.handle(NOW, Event::WsMessage { socket: s, text });
}
pub fn ready(public: bool) -> (Engine<KrakenSpot>, SocketId, Option<SocketId>) {
    ready_with_payloads(public, META, TICKERS, BALANCE)
}
pub fn ready_with_payloads(
    public: bool,
    meta: &str,
    tickers: &str,
    balance: &str,
) -> (Engine<KrakenSpot>, SocketId, Option<SocketId>) {
    let mut opts = options();
    opts.public_data_only = public;
    let mut e = safe_cex_engine::test_support::seeded_engine(KrakenSpot::new(opts));
    e.start(NOW);
    let mut pubsocket = None;
    let mut private = None;
    for _ in 0..12 {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        if actions.is_empty() {
            break;
        }
        for a in actions {
            match a {
                Action::Http(r) => {
                    let body = if r.url.ends_with("AssetPairs") {
                        meta
                    } else if r.url.ends_with("Ticker") {
                        tickers
                    } else if r.url.ends_with("Balance") {
                        balance
                    } else if r.url.ends_with("OpenOrders") {
                        OPEN
                    } else if r.url.ends_with("GetWebSocketsToken") {
                        r#"{"error":[],"result":{"token":"fresh-token","expires":900}}"#
                    } else {
                        panic!("unknown {}", r.url)
                    };
                    reply(&mut e, &r, body)
                }
                Action::WsConnect {
                    socket, channel, ..
                } => {
                    if channel == Some(SocketChannel::Private) {
                        private = Some(socket)
                    } else {
                        pubsocket = Some(socket)
                    }
                    e.handle(NOW, Event::WsOpened { socket });
                }
                _ => {}
            }
        }
    }
    (e, pubsocket.unwrap(), private)
}

// AssetPairs REST identities are legacy XDG; WS v2 uses DOGE in every quote currency.
pub const DOGE_META: &str = r#"{"error":[],"result":{"XXDGZUSD":{"altname":"XDGUSD","wsname":"XDG/USD","pair_decimals":5,"lot_decimals":8,"ordermin":"13","status":"online"},"XDGUSDT":{"altname":"XDGUSDT","wsname":"XDG/USDT","pair_decimals":5,"lot_decimals":8,"ordermin":"13","status":"online"},"XDGUSDC":{"altname":"XDGUSDC","wsname":"XDG/USDC","pair_decimals":5,"lot_decimals":8,"ordermin":"13","status":"online"}}}"#;
pub const DOGE_TICKERS: &str = r#"{"error":[],"result":{"XXDGZUSD":{"a":["0.151","100","100"],"b":["0.149","100","100"],"c":["0.15","100"],"v":["1000","2000"],"o":"0.14"},"XDGUSDT":{"a":["0.151","100","100"],"b":["0.149","100","100"],"c":["0.15","100"],"v":["1000","2000"],"o":"0.14"},"XDGUSDC":{"a":["0.151","100","100"],"b":["0.149","100","100"],"c":["0.15","100"],"v":["1000","2000"],"o":"0.14"}}}"#;
/// Drains actions with failures as the plain text the host renders.
pub trait Plain {
    fn drain_plain(&mut self) -> std::vec::IntoIter<Action>;
}
impl<V: safe_cex_engine::Venue> Plain for Engine<V> {
    fn drain_plain(&mut self) -> std::vec::IntoIter<Action> {
        use safe_cex_engine::Notice;
        self.drain_actions()
            .map(|a| match a {
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    Action::Notice(Notice::CommandFailed {
                        command,
                        error: error.clone(),
                    })
                }
                a => a,
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}
/// Drains the classified failures, dropping every other action.
pub fn failures<V: safe_cex_engine::Venue>(e: &mut Engine<V>) -> Vec<safe_cex_engine::VenueError> {
    use safe_cex_engine::Notice;
    e.drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect()
}

/// A failed-read retry delay lies in the engine's jittered `[0.75, 1.0] * nominal`.
pub fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
