//! Drives a Binance engine through scripted responses and frames.
#![allow(dead_code)]
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, PublicFeedPolicy, RequestId, Response,
    SocketId, TimerId,
};
use safe_cex_venue_binance::BinanceFutures;
mod config;
#[allow(unused_imports)]
pub use config::keyed;
pub use config::{Options, engine};
use safe_cex_engine::signing::{
    Algorithm, Capability, KeyRef, Provider, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The fixture's local signer, made to defer while `hold` is set: each signature
/// is then a parked `Action::Sign` that the test delivers in the order it chooses.
struct Gated {
    inner: Provider,
    hold: Arc<AtomicBool>,
}
impl Signer for Gated {
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.inner.sign(request)
    }
    fn immediate(&self) -> bool {
        !self.hold.load(Ordering::SeqCst)
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if self.hold.load(Ordering::SeqCst) {
            None
        } else {
            self.inner.sign_ready(request)
        }
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.inner.can_sign(key, algorithm)
    }
}

pub struct Deferred {
    inner: Provider,
    hold: Arc<AtomicBool>,
    /// Parked signatures in arrival order: (attempt, work).
    pub parked: Vec<(RequestId, SignRequest)>,
}

pub const EXCHANGE_INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"1000"},{"filterType":"MARKET_LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"120"}]}]}"#;
pub const DAILY: &str = r#"[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"1","volume":"2","quoteVolume":"3"}]"#;
pub const BOOK_TICKERS: &str = r#"[{"symbol":"BTCUSDT","bidPrice":"99.9","askPrice":"100.1"}]"#;
pub const PREMIUM: &str = r#"[{"symbol":"BTCUSDT","markPrice":"100","indexPrice":"100","lastFundingRate":"0.0001","nextFundingTime":1700000000000}]"#;

pub struct Harness {
    pub engine: Engine<BinanceFutures>,
    pub now: f64,
    pub requests: Vec<HttpRequest>,
    pub sockets: Vec<(SocketId, String)>,
    pub sent: Vec<(SocketId, String)>,
    pub closed: Vec<SocketId>,
    pub timers: Vec<(TimerId, f64)>,
    pub done: Vec<(u32, Result<String, String>)>,
    /// Structured detail of failed commands.
    pub failures: Vec<(u32, safe_cex_engine::VenueError)>,
    pub logs: Vec<String>,
    /// Logs at `Level::Error`, also present in `logs`.
    pub errors: Vec<String>,
    pub command_errors: Vec<(u32, String)>,
    pub events: Vec<safe_cex_engine::log::LogEvent>,
    pub command_notices: Vec<(u32, &'static str)>,
    pub progress: Vec<(u32, Vec<safe_cex_engine::OrderIdentityReceipt>)>,
    pub progress_at_request: Vec<(RequestId, usize)>,
    pub deferred: Option<Deferred>,
}

impl Harness {
    /// A public-data engine with markets and tickers loaded and sockets open.
    pub fn new() -> Self {
        let mut harness = Self::started(Options {
            public_data_only: true,
            ..Options::default()
        });
        harness.load_public();
        harness.event(Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }));
        harness
    }

    pub fn started(options: Options) -> Self {
        Self::started_with_policy(options, PublicFeedPolicy::Direct)
    }

    pub fn started_with_policy(options: Options, policy: PublicFeedPolicy) -> Self {
        let mut engine = engine(options);
        engine.set_public_feed_policy(policy).unwrap();
        Self::from_engine(engine)
    }

    /// `started`, with signing that defers on `hold_signatures(true)`.
    pub fn started_deferred(options: Options) -> Self {
        let (venue, inner) = options.build();
        let hold = Arc::new(AtomicBool::new(false));
        let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
        engine
            .set_signer(Some(Arc::new(Gated {
                inner: inner.clone(),
                hold: hold.clone(),
            })))
            .unwrap();
        engine
            .set_public_feed_policy(PublicFeedPolicy::Direct)
            .unwrap();
        Self::build(
            engine,
            Some(Deferred {
                inner,
                hold,
                parked: Vec::new(),
            }),
        )
    }

    pub fn from_engine(engine: Engine<BinanceFutures>) -> Self {
        Self::build(engine, None)
    }

    fn build(engine: Engine<BinanceFutures>, deferred: Option<Deferred>) -> Self {
        let mut harness = Self {
            engine,
            now: 1_000.0,
            requests: Vec::new(),
            sockets: Vec::new(),
            sent: Vec::new(),
            closed: Vec::new(),
            timers: Vec::new(),
            done: Vec::new(),
            failures: Vec::new(),
            logs: Vec::new(),
            errors: Vec::new(),
            command_errors: Vec::new(),
            command_notices: Vec::new(),
            events: Vec::new(),
            progress: Vec::new(),
            progress_at_request: Vec::new(),
            deferred,
        };
        harness.engine.start(harness.now);
        harness.collect();
        harness
    }

    pub fn load_public(&mut self) {
        self.respond("/fapi/v1/exchangeInfo", 200, EXCHANGE_INFO);
        self.respond("/fapi/v1/ticker/24hr", 200, DAILY);
        self.respond("/fapi/v1/ticker/bookTicker", 200, BOOK_TICKERS);
        self.respond("/fapi/v1/premiumIndex", 200, PREMIUM);
        for path in ["/public/ws", "/market/ws"] {
            let socket = self.socket(path).expect("socket connects after tickers");
            self.event(Event::WsOpened { socket });
        }
    }

    pub fn hold_signatures(&mut self, hold: bool) {
        let deferred = self.deferred.as_ref().expect("a deferred-signing harness");
        deferred.hold.store(hold, Ordering::SeqCst);
    }

    /// Delivers the parked signature of the request whose path contains `path`.
    pub fn sign(&mut self, path: &str) {
        let deferred = self.deferred.as_mut().expect("a deferred-signing harness");
        let index = deferred
            .parked
            .iter()
            .position(|(_, work)| work.context.path.contains(path))
            .unwrap_or_else(|| panic!("no parked signature for {path}"));
        let (attempt, work) = deferred.parked.remove(index);
        let result = deferred.inner.sign_ready(&work).expect("local signer");
        self.event(Event::Signed { attempt, result });
    }

    /// Prepares every managed route at once and parks each `Action::Sign`.
    fn deferred_actions(&mut self) -> Vec<Action> {
        use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
        let now = self.now;
        self.engine.prepare_dispatch(now);
        let mut queue: std::collections::VecDeque<_> = self.engine.drain_actions().collect();
        let mut actions = Vec::new();
        while let Some(action) = queue.pop_front() {
            match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        if self.engine.http_preparation_pending(preparation) {
                            let id = request.id.0;
                            let prepared = Prepared {
                                id,
                                expires_at: None,
                            };
                            self.engine.http_prepared(now, preparation, prepared);
                        }
                    }
                    HttpAction::Bind {
                        permit, request, ..
                    } => match self.engine.claim_http(now, permit) {
                        Ok(Claim::Released(_)) => actions.push(Action::Http(request)),
                        Ok(Claim::WaitUntil(_)) => panic!("delayed fixture pacing is unsupported"),
                        Err(_) => {}
                    },
                    HttpAction::BindGroup { .. } => {
                        panic!("grouped binds are unsupported by the deferred fixture")
                    }
                    HttpAction::Cancel { .. } => {}
                },
                Action::Sign { attempt, work, .. } => {
                    let deferred = self.deferred.as_mut().expect("checked by caller");
                    deferred.parked.push((attempt, work));
                }
                action => actions.push(action),
            }
            for action in self
                .engine
                .drain_actions()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                queue.push_front(action);
            }
        }
        actions
    }

    pub fn collect(&mut self) {
        let actions = if self.deferred.is_some() {
            self.deferred_actions()
        } else {
            safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now)
        };
        for action in actions {
            match action {
                Action::RetireAddressSetup { .. } => {
                    panic!("address setup is unsupported in the binance test harness")
                }
                Action::ManagedHttp(_) => {
                    panic!("managed HTTP preparation is not configured in this harness yet")
                }
                Action::SignPayload { .. }
                | Action::Sign { .. }
                | Action::SignedOutbound { .. } => {
                    panic!("delegated signing is unsupported in the binance test harness")
                }
                Action::Http(request) => {
                    self.progress_at_request
                        .push((request.id, self.progress.len()));
                    self.requests.push(request);
                }
                Action::WsConnect { headers, .. } if !headers.is_empty() => {
                    panic!("Binance authenticates sockets with listen keys")
                }
                Action::WsConnect { socket, url, .. } => self.sockets.push((socket, url)),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::Notice(Notice::AccountAuth { .. }) => {}
                Action::Notice(Notice::CommandProgress { command, receipts }) => {
                    self.progress.push((command, receipts))
                }
                Action::Notice(Notice::CommandError { command, message }) => {
                    self.logs.push(message.clone());
                    self.command_errors.push((command, message));
                    self.command_notices.push((command, "error"));
                }
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.command_notices.push((command, "done"));
                    self.done.push((command, Ok(result)))
                }
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.command_notices.push((command, "failed"));
                    self.done.push((command, Err(error.legacy_text())));
                    self.failures.push((command, error));
                }
                Action::Notice(Notice::StartupFailed { error }) => {
                    panic!("unexpected Binance startup failure: {error}")
                }
                Action::Notice(Notice::Event(event)) => {
                    if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE {
                        for (key, value) in &event.fields {
                            if *key == "command"
                                && let safe_cex_engine::Value::Num(command) = value
                            {
                                self.command_notices.push((*command as u32, "incomplete"));
                            }
                        }
                    }
                    self.events.push(event.clone());
                    if event.level == safe_cex_engine::Level::Error {
                        self.errors.push(event.message.clone());
                    }
                    self.logs.push(event.message);
                }
                Action::Notice(Notice::Deactivate) => {
                    panic!("unexpected Binance account deactivation")
                }
            }
        }
    }

    pub fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }

    /// The most recent socket whose URL contains `path`.
    pub fn socket(&self, path: &str) -> Option<SocketId> {
        self.sockets
            .iter()
            .rev()
            .find(|(_, url)| url.contains(path))
            .map(|(socket, _)| *socket)
    }

    pub fn url_of(&self, socket: SocketId) -> &str {
        &self.sockets.iter().find(|(s, _)| *s == socket).unwrap().1
    }

    pub fn has_request(&self, path: &str) -> bool {
        self.requests.iter().any(|r| r.url.contains(path))
    }

    /// Nothing is replayed: an unproven write only starts the fence's open-orders reconcile.
    pub fn only_reconcile_reads(&self) -> bool {
        self.requests.iter().all(|r| {
            r.method == safe_cex_engine::Method::Get
                && (r.url.contains("openOrders") || r.url.contains("openAlgoOrders"))
        })
    }
    pub fn take(&mut self, path: &str) -> HttpRequest {
        let index = self
            .requests
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("no request for {path}; have {:?}", self.paths()));
        self.requests.remove(index)
    }

    pub fn paths(&self) -> Vec<String> {
        self.requests
            .iter()
            .map(|r| format!("{} {}", r.method.as_str(), r.url))
            .collect()
    }

    pub fn reply(&mut self, request: RequestId, status: u16, headers: &str, body: &str) {
        self.event(Event::HttpResponse {
            request,
            response: Response {
                status,
                headers,
                body,
            },
        });
    }

    pub fn respond(&mut self, path: &str, status: u16, body: &str) {
        let request = self.take(path).id;
        self.reply(request, status, "", body);
    }

    pub fn message(&mut self, path: &str, text: &str) {
        let socket = self.socket(path).unwrap();
        self.event(Event::WsMessage { socket, text });
    }

    pub fn diff(&mut self, first: u64, last: u64, previous: u64, bid: &str) {
        let text = format!(
            r#"{{"e":"depthUpdate","E":1,"T":1,"s":"BTCUSDT","U":{first},"u":{last},"pu":{previous},"b":[["{bid}","1"]],"a":[]}}"#
        );
        self.message("/public/ws", &text);
    }

    pub fn snapshot_requests(&self) -> usize {
        self.requests
            .iter()
            .filter(|r| r.url.contains("/fapi/v1/depth"))
            .count()
    }

    pub fn best_bid(&self) -> Option<f64> {
        self.engine.book("BTCUSDT").and_then(|book| book.best_bid())
    }

    /// Move the clock forward, firing every timer that comes due.
    pub fn advance(&mut self, ms: f64) {
        let until = self.now + ms;
        loop {
            self.timers.sort_by(|a, b| a.1.total_cmp(&b.1));
            let Some(&(timer, at)) = self.timers.first() else {
                break;
            };
            if at > until {
                break;
            }
            self.timers.remove(0);
            self.now = self.now.max(at);
            self.event(Event::TimerFired { timer });
        }
        self.now = until;
    }

    pub fn result(&self, command: u32) -> Option<&Result<String, String>> {
        self.done
            .iter()
            .find(|(c, _)| *c == command)
            .map(|(_, r)| r)
    }
}

/// A failed-read retry delay lies in the engine's jittered `[0.75, 1.0] * nominal`.
pub fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
