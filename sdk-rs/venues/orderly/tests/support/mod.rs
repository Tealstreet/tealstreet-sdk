#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId, TimerId};
use safe_cex_venue_orderly::{Options, Orderly};
pub type Driver = Engine<Orderly>;
pub const MARKETS: &str = r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","base_tick":0.001,"quote_tick":0.1,"base_min":0.001,"base_max":20,"base_imr":0.01},{"symbol":"PERP_ETH_USDC","base_tick":0.001,"quote_tick":0.01,"base_min":0.001,"base_max":100,"base_imr":0.02}]}}"#;
pub const TICKERS: &str = r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","24h_open":50000,"24h_close":51000,"index_price":50999,"mark_price":51000,"24h_volume":10,"24h_amount":500000},{"symbol":"PERP_ETH_USDC","24h_open":2000,"24h_close":2100,"mark_price":2100}]}}"#;
pub const ORDER: &str = r#"{"order_id":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"NEW","price":50000,"quantity":1,"total_executed_quantity":0,"executed":0,"visible_quantity":1,"reduce_only":false,"margin_mode":"CROSS","client_order_id":"caller-42"}"#;
pub const POSITIONS: &str = r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","position_qty":1,"average_open_price":50000,"mark_price":51000,"margin_mode":"CROSS","leverage":10},{"symbol":"PERP_BTC_USDC","position_qty":-0.5,"average_open_price":52000,"mark_price":51000,"margin_mode":"ISOLATED","leverage":5,"margin":6000}],"total_collateral_value":10000,"free_collateral":7000}}"#;
pub fn actions(e: &mut Driver) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, 1000.0)
}
/// Tracks armed timers so tests can advance time instead of firing timer IDs.
pub struct Clock {
    pub now: f64,
    timers: Vec<(TimerId, f64)>,
}
impl Clock {
    pub fn new(now: f64) -> Self {
        Self {
            now,
            timers: vec![],
        }
    }
    pub fn track(&mut self, out: &[Action]) {
        for action in out {
            match action {
                Action::SetTimer { timer, at_ms } => self.timers.push((*timer, *at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| t != timer),
                _ => {}
            }
        }
    }
    pub fn drain(&mut self, e: &mut Driver) -> Vec<Action> {
        let out = safe_cex_engine::test_support::prepared_actions(e, self.now);
        self.track(&out);
        out
    }
    pub fn handle(&mut self, e: &mut Driver, event: Event<'_>) -> Vec<Action> {
        e.handle(self.now, event);
        self.drain(e)
    }
    pub fn advance(&mut self, e: &mut Driver, until: f64) -> Vec<(f64, Action)> {
        let mut fired = vec![];
        while self.timers.iter().any(|(_, at)| *at <= until) {
            fired.extend(self.next(e));
        }
        self.now = until;
        fired
    }
    /// Fires only the earliest armed timer.
    pub fn next(&mut self, e: &mut Driver) -> Vec<(f64, Action)> {
        let Some((timer, at)) = self
            .timers
            .iter()
            .copied()
            .min_by(|a, b| a.1.total_cmp(&b.1))
        else {
            return vec![];
        };
        self.timers.retain(|(t, _)| *t != timer);
        self.now = self.now.max(at);
        let out = self.handle(e, Event::TimerFired { timer });
        out.into_iter().map(|a| (self.now, a)).collect()
    }
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
pub fn ws(e: &mut Driver, socket: SocketId, text: &str) {
    e.handle(2000.0, Event::WsMessage { socket, text });
}
pub fn options() -> Options {
    Options {
        key: Some("ed25519:11111111111111111111111111111111".into()),
        secret: Some(bs58::encode([7; 32]).into_string()),
        application_id: Some("test-account".into()),
        ..Default::default()
    }
}
pub fn ready() -> (Driver, SocketId) {
    ready_with(ORDER, "[]", POSITIONS)
}
pub fn ready_with(order: &str, algos: &str, positions: &str) -> (Driver, SocketId) {
    let (e, socket, _, _) = ready_full(order, algos, positions);
    (e, socket)
}
/// Bootstraps like `ready_with` and returns the messages logged on the way.
pub fn ready_with_logs(
    order: &str,
    algos: &str,
    positions: &str,
) -> (Driver, SocketId, Vec<String>) {
    let (e, socket, _, logs) = ready_full(order, algos, positions);
    (e, socket, logs)
}
/// Like `ready_with`, also returning the account poll tick timer.
pub fn ready_ticking(order: &str, algos: &str, positions: &str) -> (Driver, SocketId, TimerId) {
    let (e, socket, tick, _) = ready_full(order, algos, positions);
    (e, socket, tick.unwrap())
}
fn ready_full(
    order: &str,
    algos: &str,
    positions: &str,
) -> (Driver, SocketId, Option<TimerId>, Vec<String>) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Orderly::new(options()));
    e.start(1000.0);
    let mut private = None;
    let mut logs = vec![];
    let mut tick = None;
    for _ in 0..10 {
        let out = actions(&mut e);
        if out.is_empty() {
            break;
        }
        for a in out {
            match a {
                Action::Http(r) => {
                    let body = account_body(&r.url, order, algos, positions);
                    reply(&mut e, &r, 200, &body);
                }
                Action::WsConnect { socket, url, .. } if url.contains("private") => {
                    private = Some(socket)
                }
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    message,
                    ..
                })) => logs.push(message),
                Action::SetTimer {
                    timer,
                    at_ms: 3000.0,
                } => tick = Some(timer),
                _ => {}
            }
        }
    }
    let socket = private.unwrap();
    e.handle(1000.0, Event::WsOpened { socket });
    actions(&mut e);
    ws(&mut e, socket, r#"{"event":"auth","success":true}"#);
    (e, socket, tick, logs)
}
pub fn account_body(url: &str, order: &str, algos: &str, positions: &str) -> String {
    if url.ends_with("/v1/client/info") {
        r#"{"success":true,"data":{"max_leverage":20}}"#.into()
    } else if url.ends_with("/v1/public/info") {
        MARKETS.into()
    } else if url.ends_with("/v1/public/futures") {
        TICKERS.into()
    } else if url.ends_with("/v1/client/margin_modes") {
        r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","default_margin_mode":"CROSS"},{"symbol":"PERP_ETH_USDC","default_margin_mode":"ISOLATED"}]}}"#.into()
    } else if url.ends_with("/v1/positions") {
        positions.into()
    } else if url.contains("/v1/algo/orders") {
        format!(
            r#"{{"success":true,"data":{{"rows":{algos},"meta":{{"total":{}}}}}}}"#,
            if algos == "[]" { 0 } else { 1 }
        )
    } else {
        format!(r#"{{"success":true,"data":{{"rows":[{order}],"meta":{{"total":1}}}}}}"#)
    }
}
pub fn fill_amounts(e: &mut Driver) -> Vec<f64> {
    let Some(bytes) = e.take_frame(2000.0) else {
        return vec![];
    };
    let mut at = 16;
    let count = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
    for _ in 0..count {
        let kind = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let start = at + 8;
        if kind == 11 {
            let n = u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap()) as usize;
            let data = (start + 4 + n * 12 + 7) & !7;
            return (0..n)
                .map(|i| {
                    f64::from_le_bytes(
                        bytes[data + i * 16 + 8..data + i * 16 + 16]
                            .try_into()
                            .unwrap(),
                    )
                })
                .collect();
        }
        at = start + len;
    }
    vec![]
}
