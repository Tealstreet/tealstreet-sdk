#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, PublicFeedPolicy, Response, SocketId};
use safe_cex_venue_cryptocom::Cryptocom;
pub mod fixture;
use fixture::Options;
pub const INSTRUMENTS: &str = r#"{"code":0,"result":{"data":[{"symbol":"BTCUSD-PERP","inst_type":"PERPETUAL_SWAP","tradable":true,"base_ccy":"BTC","quote_ccy":"USD","price_tick_size":"0.1","qty_tick_size":"0.001","quote_decimals":1,"quantity_decimals":3,"max_leverage":"50","underlying_symbol":"BTCUSD-INDEX"}]}}"#;
pub const TICKERS: &str = r#"{"code":0,"result":{"data":[{"i":"BTCUSD-PERP","b":"99","k":"101","a":"100","c":"0.01","v":"1","vv":"100"}]}}"#;
pub const EMPTY: &str = r#"{"code":0,"result":{"data":[]}}"#;
pub const BALANCE: &str = r#"{"code":0,"result":{"data":[{"total_margin_balance":"1000","total_available_balance":"950","total_initial_margin":"50","total_session_unrealized_pnl":"10"}]}}"#;
pub const ORDER: &str = r#"{"order_id":"9007199254740993","client_oid":"caller","instrument_name":"BTCUSD-PERP","order_type":"LIMIT","side":"BUY","quantity":"1","limit_price":"90","cumulative_quantity":"0","status":"ACTIVE","time_in_force":"GOOD_TILL_CANCEL","update_time":1}"#;
pub struct Harness {
    pub engine: Engine<Cryptocom>,
    pub actions: Vec<Action>,
    pub failures: Vec<safe_cex_engine::VenueError>,
    pub now: f64,
}
impl Harness {
    pub fn new() -> Self {
        Self::with_instruments(INSTRUMENTS)
    }
    pub fn with_instruments(instruments: &str) -> Self {
        Self::start(Self::options(), PublicFeedPolicy::Direct, instruments)
    }
    pub fn with_policy(options: Options, policy: PublicFeedPolicy) -> Self {
        Self::start(options, policy, INSTRUMENTS)
    }
    fn options() -> Options {
        Options {
            key: Some("replay-key".into()),
            secret: Some("replay-secret".into()),
            ..Default::default()
        }
    }
    fn start(options: Options, policy: PublicFeedPolicy, instruments: &str) -> Self {
        let mut h = Self {
            engine: fixture::engine(options),
            actions: vec![],
            failures: vec![],
            now: 1_610_000_000_000.0,
        };
        h.engine.set_public_feed_policy(policy).unwrap();
        h.engine.start(h.now);
        h.collect();
        h.reply("public/get-instruments", instruments);
        h.reply("public/get-tickers", TICKERS);
        h
    }
    pub fn collect(&mut self) {
        for action in fixture::actions(&mut self.engine, self.now) {
            // A host removes cancelled deadlines from its active timer queue.
            if let Action::ClearTimer { timer } = &action {
                self.actions.retain(
                    |a| !matches!(a, Action::SetTimer { timer: pending, .. } if pending == timer),
                );
            }
            // Failures as the plain text the host renders.
            let action = match action {
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command, error }) => {
                    self.failures.push(error.clone());
                    Action::Notice(safe_cex_engine::Notice::CommandFailed { command, error })
                }
                action => action,
            };
            self.actions.push(action);
        }
    }
    pub fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    pub fn http(&mut self, path: &str) -> HttpRequest {
        let i = self
            .actions
            .iter()
            .position(|a| matches!(a, Action::Http(r) if r.url.contains(path)))
            .unwrap_or_else(|| panic!("missing {path}: {:?}", self.actions));
        match self.actions.remove(i) {
            Action::Http(r) => r,
            _ => unreachable!(),
        }
    }
    pub fn reply(&mut self, path: &str, body: &str) {
        let r = self.http(path);
        self.respond(&r, body);
    }
    pub fn respond(&mut self, r: &HttpRequest, body: &str) {
        self.event(Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        });
    }
    pub fn socket(&self, private: bool) -> SocketId {
        self.actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, url, .. }
                    if url.ends_with(if private { "/user" } else { "/market" }) =>
                {
                    Some(*socket)
                }
                _ => None,
            })
            .unwrap()
    }
    pub fn ws(&mut self, channel: &str, row: &str) {
        let socket = self.socket(true);
        let text =
            format!("{{\"code\":0,\"result\":{{\"channel\":\"{channel}\",\"data\":[{row}]}}}}");
        self.event(Event::WsMessage {
            socket,
            text: &text,
        });
    }
    pub fn load(&mut self) {
        self.reply("get-positions", EMPTY);
        self.reply("user-balance", BALANCE);
        self.reply("private/get-open-orders", EMPTY);
        self.reply("advanced/get-open-orders", EMPTY);
    }
}

/// A failed-read retry delay lies in the engine's jittered `[0.75, 1.0] * nominal`.
pub fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
