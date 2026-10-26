//! Drives a Bybit engine through scripted responses and frames built from the
//! documented V5 payloads (bybit-exchange.github.io/docs/v5).
#![allow(dead_code)]
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, RequestId, Response, SocketChannel,
    SocketId, TimerId, VenueError,
};
use safe_cex_venue_bybit::BybitLinear;
mod config;
pub use config::{Options, engine};

/// "Get Instruments Info", the USDT perpetual example, plus ETHUSDT.
pub const INSTRUMENTS: &str = r#"{"retCode":0,"retMsg":"OK","result":{"category":"linear","list":[{"symbol":"BTCUSDT","symbolId":5,"contractType":"LinearPerpetual","status":"Trading","baseCoin":"BTC","quoteCoin":"USDT","launchTime":"1585526400000","deliveryTime":"0","deliveryFeeRate":"","priceScale":"2","leverageFilter":{"minLeverage":"1","maxLeverage":"100.00","leverageStep":"0.01"},"priceFilter":{"minPrice":"0.10","maxPrice":"1999999.80","tickSize":"0.10"},"lotSizeFilter":{"maxOrderQty":"1190.000","minOrderQty":"0.001","qtyStep":"0.001","postOnlyMaxOrderQty":"1190.000","maxMktOrderQty":"500.000","minNotionalValue":"5"},"unifiedMarginTrade":true,"fundingInterval":480,"settleCoin":"USDT","copyTrading":"both","upperFundingRate":"0.00375","lowerFundingRate":"-0.00375","isPreListing":false,"preListingInfo":null,"riskParameters":{"priceLimitRatioX":"0.01","priceLimitRatioY":"0.02"},"symbolType":"","tags":[]},{"symbol":"ETHUSDT","contractType":"LinearPerpetual","status":"Trading","baseCoin":"ETH","quoteCoin":"USDT","deliveryTime":"0","leverageFilter":{"minLeverage":"1","maxLeverage":"100.00","leverageStep":"0.01"},"priceFilter":{"minPrice":"0.01","maxPrice":"199999.98","tickSize":"0.01"},"lotSizeFilter":{"maxOrderQty":"7240.00","minOrderQty":"0.01","qtyStep":"0.01"},"settleCoin":"USDT"}],"nextPageCursor":""},"retExtInfo":{},"time":1735809771618}"#;
pub const PRELAUNCH: &str = r#"{"retCode":0,"retMsg":"OK","result":{"category":"linear","list":[],"nextPageCursor":""},"retExtInfo":{},"time":1735809771618}"#;
/// "Get Tickers", linear rows in the documented shape.
pub const TICKERS: &str = r#"{"retCode":0,"retMsg":"OK","result":{"category":"linear","list":[{"symbol":"BTCUSDT","lastPrice":"66666.60","indexPrice":"66660.19","markPrice":"66666.60","prevPrice24h":"79206.20","price24hPcnt":"-0.158315","highPrice24h":"79266.30","lowPrice24h":"65076.90","prevPrice1h":"66666.60","openInterest":"492373.72","openInterestValue":"32824881841.75","turnover24h":"4936790807.6521","volume24h":"73191.3870","fundingRate":"-0.005","nextFundingTime":"1760342400000","predictedDeliveryPrice":"","basisRate":"","deliveryFeeRate":"","deliveryTime":"0","ask1Size":"23775.469","bid1Price":"66666.60","ask1Price":"66666.70","bid1Size":"23789.165","basis":"","preOpenPrice":"","preQty":"","curPreListingPhase":"","fundingIntervalHour":"8","fundingCap":"0.005"},{"symbol":"ETHUSDT","lastPrice":"3000.00","indexPrice":"2999.50","markPrice":"3000.00","price24hPcnt":"0.01","openInterest":"1000","turnover24h":"3000000","volume24h":"1000","fundingRate":"0.0001","nextFundingTime":"1760342400000","bid1Price":"2999.99","ask1Price":"3000.00"}]},"retExtInfo":{},"time":1760352369814}"#;
pub const FEE_RATES: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTCUSDT","takerFeeRate":"0.00055","makerFeeRate":"0.0002"}]},"retExtInfo":{},"time":1676360412576}"#;
/// "Get Account Info".
pub const ACCOUNT_INFO: &str = r#"{"retCode":0,"retMsg":"OK","result":{"marginMode":"REGULAR_MARGIN","updatedTime":"1697078946000","unifiedMarginStatus":4,"dcpStatus":"OFF","timeWindow":10,"smpGroup":0,"isMasterTrader":false,"spotHedgingStatus":"OFF"}}"#;

pub struct Harness {
    pub engine: Engine<BybitLinear>,
    pub now: f64,
    pub requests: Vec<HttpRequest>,
    pub sockets: Vec<(SocketId, String)>,
    pub socket_channels: Vec<(SocketId, Option<SocketChannel>)>,
    pub sent: Vec<(SocketId, String)>,
    pub closed: Vec<SocketId>,
    pub timers: Vec<(TimerId, f64)>,
    pub done: Vec<(u32, Result<String, String>)>,
    /// Commands completed with `outcome: Unknown`.
    pub unknown: Vec<u32>,
    pub outcomes: Vec<(u32, Option<safe_cex_engine::Outcome>)>,
    pub logs: Vec<String>,
    pub notices: Vec<Notice>,
    pub command_errors: Vec<(u32, String)>,
    pub command_notices: Vec<(u32, &'static str)>,
    pub progress: Vec<(u32, Vec<safe_cex_engine::OrderIdentityReceipt>)>,
    pub progress_at_request: Vec<(RequestId, usize)>,
    pub startup_failures: Vec<VenueError>,
    pub deactivated: bool,
}

impl Harness {
    /// A public-data engine with markets and tickers loaded and the socket open.
    pub fn new() -> Self {
        let mut harness = Self::started(Options {
            public_data_only: true,
            ..Options::default()
        });
        harness.load_public();
        harness
    }

    pub fn started(options: Options) -> Self {
        Self::started_engine(engine(options))
    }

    pub fn started_engine(engine: Engine<BybitLinear>) -> Self {
        let mut harness = Self {
            engine,
            now: 1_000.0,
            requests: Vec::new(),
            sockets: Vec::new(),
            socket_channels: Vec::new(),
            sent: Vec::new(),
            closed: Vec::new(),
            timers: Vec::new(),
            done: Vec::new(),
            unknown: Vec::new(),
            outcomes: Vec::new(),
            logs: Vec::new(),
            notices: Vec::new(),
            command_errors: Vec::new(),
            command_notices: Vec::new(),
            progress: Vec::new(),
            progress_at_request: Vec::new(),
            startup_failures: Vec::new(),
            deactivated: false,
        };
        harness.engine.start(harness.now);
        harness.collect();
        harness
    }

    pub fn load_public(&mut self) {
        self.respond("status=PreLaunch", 200, PRELAUNCH);
        self.respond("/v5/market/instruments-info", 200, INSTRUMENTS);
        self.respond("/v5/market/tickers", 200, TICKERS);
        let socket = self.socket("/v5/public/linear").expect("public socket");
        self.event(Event::WsOpened { socket });
        self.flush_subscriptions();
    }

    /// Fire the pacing timers until every queued subscribe message is sent.
    pub fn flush_subscriptions(&mut self) {
        self.advance(100.0);
    }

    pub fn collect(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            if let Action::Notice(notice) = &action {
                self.notices.push(notice.clone());
            }
            match action {
                Action::Http(request) => {
                    self.progress_at_request
                        .push((request.id, self.progress.len()));
                    self.requests.push(request);
                }
                Action::ManagedHttp(_) => {
                    panic!("managed HTTP requires the prepared test-driver handshake")
                }
                Action::RetireAddressSetup { .. } => {
                    panic!("unexpected address setup retirement")
                }
                Action::Sign { .. }
                | Action::SignedOutbound { .. }
                | Action::SignPayload { .. } => {
                    panic!("immediate fixture provider must materialize signing")
                }
                Action::WsConnect { headers, .. } if !headers.is_empty() => {
                    panic!("Bybit authenticates sockets with an auth message")
                }
                Action::WsConnect {
                    socket,
                    url,
                    channel,
                    ..
                } => {
                    self.sockets.push((socket, url));
                    self.socket_channels.push((socket, channel));
                }
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
                    if error.outcome == Some(safe_cex_engine::Outcome::Unknown) {
                        self.unknown.push(command);
                    }
                    self.command_notices.push((command, "done"));
                    self.outcomes.push((command, error.outcome));
                    self.done.push((command, Err(error.legacy_text())))
                }
                Action::Notice(Notice::Event(event)) => self.logs.push(event.message),
                Action::Notice(Notice::StartupFailed { error }) => {
                    self.startup_failures.push(error)
                }
                Action::Notice(Notice::Deactivate) => {
                    assert!(
                        !self.startup_failures.is_empty(),
                        "unexpected Bybit account deactivation"
                    );
                    self.deactivated = true;
                }
            }
        }
    }

    pub fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }

    /// Dispatch with an explicit caller wait.
    pub fn command_waiting(&mut self, command: Command, wait_ms: u32) {
        self.engine
            .handle_command(self.now, command, Some(wait_ms))
            .expect("valid caller receipt");
        self.collect();
    }

    /// Dispatch with the caller's default wait, as the host does.
    pub fn command(&mut self, command: Command) {
        self.engine
            .handle_command(self.now, command, None)
            .expect("valid caller receipt");
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

    pub fn has_request(&self, path: &str) -> bool {
        self.requests.iter().any(|r| r.url.contains(path))
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

    pub fn reply(&mut self, request: RequestId, status: u16, body: &str) {
        self.event(Event::HttpResponse {
            request,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }

    pub fn respond(&mut self, path: &str, status: u16, body: &str) {
        let request = self.take(path).id;
        self.reply(request, status, body);
    }

    pub fn message(&mut self, path: &str, text: &str) {
        let socket = self.socket(path).unwrap();
        self.event(Event::WsMessage { socket, text });
    }

    /// Messages sent on the socket whose URL contains `path`.
    pub fn sent_on(&self, path: &str) -> Vec<String> {
        let Some(socket) = self.socket(path) else {
            return Vec::new();
        };
        self.sent
            .iter()
            .filter(|(s, _)| *s == socket)
            .map(|(_, text)| text.clone())
            .collect()
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

    /// An Unknown write seal reconciles with exactly one fresh order read (one
    /// page per settle coin) and nothing else: no write, resend or refresh.
    pub fn reconcile_once(&mut self, empty: &str) {
        assert!(!self.requests.is_empty(), "one reconcile order read");
        assert!(
            self.requests
                .iter()
                .all(|r| r.method == safe_cex_engine::Method::Get
                    && r.url.contains("/v5/order/realtime")),
            "only the reconcile order read: {:?}",
            self.paths()
        );
        let mut urls: Vec<&str> = self.requests.iter().map(|r| r.url.as_str()).collect();
        let issued = urls.len();
        urls.sort_unstable();
        urls.dedup();
        assert_eq!(urls.len(), issued, "the reconcile read is issued once");
        let reads: Vec<RequestId> = self.requests.drain(..).map(|r| r.id).collect();
        for read in reads {
            self.reply(read, 200, empty);
        }
        assert!(
            self.requests.is_empty(),
            "nothing follows: {:?}",
            self.paths()
        );
    }

    pub fn result(&self, command: u32) -> Option<&Result<String, String>> {
        self.done
            .iter()
            .find(|(c, _)| *c == command)
            .map(|(_, r)| r)
    }

    pub fn header(request: &HttpRequest, name: &str) -> Option<String> {
        request
            .headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.clone())
    }
}

pub fn order() -> safe_cex_engine::model::PlaceOrderOpts {
    safe_cex_engine::model::PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: safe_cex_engine::model::OrderType::Limit,
        side: safe_cex_engine::model::OrderSide::Buy,
        amount: 0.01,
        price: Some(100.0),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: None,
        close: None,
        stp_mode: None,
        implied_volatility: None,
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
    }
}

/// Park the engine's command owners with ordinary placements whose mode
/// reads stay unanswered; returns the parked mode reads.
pub fn park_owners(h: &mut Harness) -> Vec<HttpRequest> {
    let mut reads = Vec::new();
    for command in 1_000..1_256 {
        h.command(Command::PlaceOrders {
            command,
            orders: vec![order()],
        });
        if h.has_request("/v5/position/list") {
            reads.push(h.take("/v5/position/list"));
        }
    }
    reads
}
