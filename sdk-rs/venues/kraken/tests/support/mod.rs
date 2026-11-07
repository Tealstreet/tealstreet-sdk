#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketChannel, SocketId};
use safe_cex_venue_kraken::{Kraken, Options};
pub const NOW: f64 = 1_700_000_000_000.0;
pub const META: &str = r#"{"result":"success","instruments":[{"symbol":"PF_XBTUSD","base":"BTC","quote":"USD","tradeable":true,"tickSize":0.5,"contractSize":1,"contractValueTradePrecision":3,"marginLevels":[{"initialMargin":0.02}]},{"symbol":"PF_ETHUSD","base":"ETH","quote":"USD","tradeable":true,"tickSize":0.1,"contractSize":1,"contractValueTradePrecision":2,"marginLevels":[{"initialMargin":0.05}]}]}"#;
pub const TICKERS: &str = r#"{"result":"success","tickers":[{"symbol":"PF_XBTUSD","last":100,"markPrice":101,"indexPrice":102,"bid":99,"ask":101,"vol24h":2,"volumeQuote":200,"openInterest":4,"fundingRate":0.01},{"symbol":"PF_ETHUSD","last":50,"markPrice":50,"indexPrice":50,"bid":49,"ask":51}]}"#;
pub const ACCOUNT: &str = r#"{"result":"success","accounts":{"flex":{"type":"multiCollateralMarginAccount","portfolioValue":1000,"availableMargin":900,"balanceValue":1000,"pnl":0,"initialMargin":100,"maintenanceMargin":0}}}"#;
pub const OPEN: &str = r#"{"result":"success","openOrders":[]}"#;
pub fn options() -> Options {
    Options {
        key: Some("replay-key".into()),
        secret: Some("cmVwbGF5LXNlY3JldA==".into()),
        ..Options::default()
    }
}
pub fn reply(e: &mut Engine<Kraken>, r: &HttpRequest, status: u16, body: &str) {
    reply_at(e, NOW, r, status, body)
}
pub fn reply_at(e: &mut Engine<Kraken>, now: f64, r: &HttpRequest, status: u16, body: &str) {
    e.handle(
        now,
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
pub fn message(e: &mut Engine<Kraken>, socket: SocketId, now: f64, text: &str) {
    e.handle(now, Event::WsMessage { socket, text });
}
pub fn requests(e: &mut Engine<Kraken>) -> Vec<HttpRequest> {
    safe_cex_engine::test_support::prepared_actions(e, NOW)
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
pub fn ready() -> (Engine<Kraken>, SocketId) {
    ready_with(options())
}
pub fn ready_with(options: Options) -> (Engine<Kraken>, SocketId) {
    let (e, socket, _) = ready_with_catalogue(options, META);
    (e, socket)
}
/// Bootstraps against `meta` and returns the timers scheduled on the way.
pub fn ready_with_catalogue(
    options: Options,
    meta: &str,
) -> (
    Engine<Kraken>,
    SocketId,
    Vec<(safe_cex_engine::TimerId, f64)>,
) {
    let public_only = options.public_data_only;
    let mut e = safe_cex_engine::test_support::seeded_engine(Kraken::new(options));
    e.start(NOW);
    let mut private = None;
    let mut public = None;
    let mut timers = vec![];
    for _ in 0..30 {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        if actions.is_empty() {
            break;
        }
        for a in actions {
            match a {
                Action::Http(r) => {
                    let body = if r.url.contains("/instruments") {
                        meta
                    } else if r.url.contains("/tickers") {
                        TICKERS
                    } else if r.url.contains("/openorders") {
                        OPEN
                    } else if r.url.contains("/openpositions") {
                        r#"{"result":"success","openPositions":[]}"#
                    } else if r.url.contains("/accounts") {
                        ACCOUNT
                    } else if r.url.contains("/leveragepreferences") {
                        r#"{"result":"success","leveragePreferences":[]}"#
                    } else {
                        panic!("unhandled bootstrap {}", r.url)
                    };
                    reply(&mut e, &r, 200, body);
                }
                Action::WsConnect {
                    socket, channel, ..
                } => {
                    if channel == Some(SocketChannel::Private) {
                        private = Some(socket)
                    } else {
                        public = Some(socket)
                    }
                    e.handle(NOW, Event::WsOpened { socket });
                }
                Action::WsSend { socket, text } => {
                    if text.contains("\"challenge\"") {
                        message(
                            &mut e,
                            socket,
                            NOW,
                            r#"{"event":"challenge","message":"c100b894-1729-464d-ace1-52dbce11db42"}"#,
                        );
                    } else if text.contains("\"subscribe\"") {
                        let feed = if text.contains("open_orders") {
                            "open_orders"
                        } else if text.contains("open_positions") {
                            "open_positions"
                        } else if text.contains("fills") {
                            "fills"
                        } else {
                            continue;
                        };
                        message(
                            &mut e,
                            socket,
                            NOW,
                            &format!("{{\"event\":\"subscribed\",\"feed\":\"{feed}\"}}"),
                        );
                    }
                }
                Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)),
                _ => {}
            }
        }
    }
    (
        e,
        if public_only {
            public.expect("public socket")
        } else {
            private.expect("private socket")
        },
        timers,
    )
}
pub fn fill_count(frame: &[u8]) -> u32 {
    let mut at = 16;
    while at + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let size = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            return u32::from_le_bytes(frame[at + 8..at + 12].try_into().unwrap());
        }
        at += 8 + size;
    }
    0
}
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
