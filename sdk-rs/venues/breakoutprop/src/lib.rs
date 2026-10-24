//! Legacy Breakout DXtrade: authenticated REST polling, with no exchange socket.
mod account;
mod history;
mod session;
mod trading;
pub(crate) mod wire;

use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Command, Cx, ErrorKind, Method, Outcome, Response, SocketId, TimerId, Venue, VenueError,
};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};

pub use session::timeout_ms as parse_session_timeout_ms;

const BASE_URL: &str = "https://breakout.prosp.devexperts.com/dxsca-web";

#[derive(Clone)]
pub struct Options {
    pub key: String,
    pub secret: String,
    pub subaccount: Option<String>,
    pub public_data_only: bool,
    pub tick_interval: f64,
    pub timeout_ms: u32,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            key: String::new(),
            secret: String::new(),
            subaccount: None,
            public_data_only: false,
            tick_interval: 2_000.0,
            timeout_ms: 20_000,
        }
    }
}

pub struct BreakoutProp {
    pub(crate) options: safe_cex_crypto::secrets::Sensitive<Options>,
    pub(crate) session: session::Session,
    /// Engine session owning password logins. Not repeat-safe: a login that
    /// does not succeed retires it, so a reissue is a new generation and the
    /// engine drops late answers to the abandoned one.
    login_session: Option<safe_cex_engine::request_context::Session>,
    /// A 2xx login awaiting body acceptance; only an accepted token proves it.
    answered_login: Option<safe_cex_engine::request_context::Context>,
    auth_failed: bool,
    auth_error: String,
    auth_draining: bool,
    queue: VecDeque<Pending>,
    dispatch_timer: Option<(TimerId, f64)>,
    poll_timer: Option<TimerId>,
    request_times: VecDeque<f64>,
    instruments: Option<Vec<wire::Instrument<'static>>>,
    details: Option<Vec<wire::Details<'static>>>,
    instruments_pending: bool,
    details_pending: bool,
    quotes_pending: bool,
    portfolio_pending: bool,
    portfolio_started: bool,
    last_portfolio_version: Option<u64>,
    order_revision: u64,
    trading: trading::TradingState,
    terminal_orders: HashMap<String, f64>,
    books: HashMap<String, (usize, Book)>,
    candles: HashMap<(String, Interval), usize>,
}

pub struct Pending {
    pub(crate) operation: Operation,
    generation: u64,
    auth_retried: bool,
    network_attempt: u8,
    ready_at: f64,
    /// Engine authority of a dispatched session login; None for legacy work.
    context: Option<safe_cex_engine::request_context::Context>,
}

pub(crate) enum Operation {
    Login,
    Discover,
    Trade(trading::Request),
    Instruments {
        rows: Vec<wire::Instrument<'static>>,
        invalid: bool,
    },
    Details {
        rows: Vec<wire::Details<'static>>,
        invalid: bool,
    },
    Quotes {
        rows: Vec<Ticker>,
        invalid: bool,
        validated_time: Option<String>,
    },
    Portfolio {
        revision: u64,
        initial: bool,
        snapshot: Option<account::Snapshot>,
        invalid: bool,
    },
    Candles {
        command: u32,
        symbol: String,
        interval: Interval,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
        rows: Vec<Candle>,
        invalid: bool,
    },
}

impl Operation {
    fn stream(&self) -> Option<ArrayAt> {
        match self {
            Self::Portfolio { .. } => Some(ArrayAt::Field("portfolios")),
            Self::Instruments { .. } => Some(ArrayAt::Field("instruments")),
            Self::Details { .. } => Some(ArrayAt::Field("instrumentDetails")),
            Self::Quotes { .. } | Self::Candles { .. } => Some(ArrayAt::Field("events")),
            _ => None,
        }
    }

    fn reset(&mut self) {
        match self {
            Self::Portfolio {
                snapshot, invalid, ..
            } => {
                *snapshot = None;
                *invalid = false;
            }
            Self::Instruments { rows, invalid } => {
                rows.clear();
                *invalid = false;
            }
            Self::Details { rows, invalid } => {
                rows.clear();
                *invalid = false;
            }
            Self::Quotes {
                rows,
                invalid,
                validated_time,
            } => {
                rows.clear();
                *invalid = false;
                *validated_time = None;
            }
            Self::Candles { rows, invalid, .. } => {
                rows.clear();
                *invalid = false;
            }
            _ => {}
        }
    }
}

#[derive(Serialize)]
struct LoginBody<'a> {
    domain: &'static str,
    username: &'a str,
    password: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketData<'a> {
    event_types: Vec<EventType<'a>>,
    symbols: Vec<&'a str>,
    account: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EventType<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    candle_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to_time: Option<String>,
}

impl BreakoutProp {
    pub fn new(options: Options) -> Self {
        let mut options = safe_cex_crypto::secrets::Sensitive::new(options);
        options.tick_interval =
            safe_cex_engine::engine::bounded_tick_interval(options.tick_interval);
        Self {
            options,
            session: session::Session::default(),
            login_session: None,
            answered_login: None,
            auth_failed: false,
            auth_error: String::new(),
            auth_draining: false,
            queue: VecDeque::new(),
            dispatch_timer: None,
            poll_timer: None,
            request_times: VecDeque::with_capacity(5),
            instruments: None,
            details: None,
            instruments_pending: false,
            details_pending: false,
            quotes_pending: false,
            portfolio_pending: false,
            portfolio_started: false,
            last_portfolio_version: None,
            order_revision: 0,
            trading: trading::TradingState::default(),
            terminal_orders: HashMap::new(),
            books: HashMap::new(),
            candles: HashMap::new(),
        }
    }

    pub(crate) fn account_code(&self) -> Option<&str> {
        self.session.account.as_deref()
    }

    pub fn public_only(&self) -> bool {
        self.options.public_data_only
    }

    pub(crate) fn schedule(&mut self, cx: &mut Cx<'_, Pending>, operation: Operation) {
        if self.auth_failed || self.auth_draining {
            self.fail_operation(cx, operation, not_sent(self.auth_error.clone()));
            return;
        }
        if self.queue.len() >= 10_000 {
            self.fail_operation(cx, operation, not_sent("DXtrade request queue is full"));
            return;
        }
        self.queue.push_back(Pending {
            operation,
            generation: self.session.generation,
            auth_retried: false,
            network_attempt: 0,
            ready_at: cx.now,
            context: None,
        });
        self.drive(cx);
    }

    fn prerequisite(&mut self, cx: &mut Cx<'_, Pending>, login: bool) {
        let requested = if login {
            &mut self.session.login_pending
        } else {
            &mut self.session.discovery_pending
        };
        if !*requested {
            *requested = true;
            self.queue.push_front(Pending {
                operation: if login {
                    Operation::Login
                } else {
                    Operation::Discover
                },
                generation: self.session.generation,
                auth_retried: false,
                network_attempt: 0,
                ready_at: cx.now,
                context: None,
            });
        }
    }

    fn drive(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.auth_failed || self.auth_draining || self.queue.is_empty() {
            return;
        }
        if !self.session.valid(cx.now) {
            self.prerequisite(cx, true);
        } else if self.session.account.is_none() {
            self.prerequisite(cx, false);
        }
        let token_ready = self.session.valid(cx.now);
        let eligible = |pending: &Pending| match pending.operation {
            Operation::Login => true,
            Operation::Discover => token_ready,
            _ => token_ready && self.session.account.is_some(),
        };
        let Some(next_ready) = self
            .queue
            .iter()
            .filter(|pending| eligible(pending))
            .map(|pending| pending.ready_at)
            .min_by(f64::total_cmp)
        else {
            return;
        };
        while self
            .request_times
            .front()
            .is_some_and(|time| *time + 1_000.0 <= cx.now)
        {
            self.request_times.pop_front();
        }
        let capacity_at = if self.request_times.len() < 5 {
            cx.now
        } else {
            self.request_times[0] + 1_000.0
        };
        let dispatch_at = capacity_at.max(next_ready);
        if dispatch_at > cx.now {
            if self.dispatch_timer.is_none_or(|(_, at)| dispatch_at < at) {
                if let Some((timer, _)) = self.dispatch_timer.take() {
                    cx.clear_timer(timer);
                }
                self.dispatch_timer = Some((cx.set_timer(dispatch_at - cx.now), dispatch_at));
            }
            return;
        }
        let Some(index) = self
            .queue
            .iter()
            .position(|pending| eligible(pending) && pending.ready_at <= cx.now)
        else {
            return;
        };
        let mut pending = self.queue.remove(index).expect("queued request exists");
        pending.generation = self.session.generation;
        if let Operation::Portfolio { revision, .. } = &mut pending.operation {
            *revision = self.order_revision;
        }
        let operation = &pending.operation;
        let spec = self.request_spec(cx, operation);
        let (method, path, body) = match spec {
            Ok(spec) => spec,
            Err(error) => {
                self.fail_operation(cx, pending.operation, not_sent(error));
                self.drive(cx);
                return;
            }
        };
        let mut headers = vec![("Content-Type", "application/json".into())];
        if !matches!(operation, Operation::Login) {
            let token = self.session.token.as_deref().unwrap_or_default();
            let mut authorization = String::with_capacity("DXAPI ".len() + token.len());
            authorization.push_str("DXAPI ");
            authorization.push_str(token);
            headers.push(("Authorization", authorization));
        }
        if method == Method::Delete {
            headers.push(("If-Match", "*".into()));
        }
        let stream = operation.stream();
        let path = path.trim_start_matches('/');
        let mut url = String::with_capacity(BASE_URL.len() + 1 + path.len());
        url.push_str(BASE_URL);
        url.push('/');
        url.push_str(path);
        if let Operation::Trade(request) = &pending.operation {
            request.report_progress(cx);
        }
        let id = if matches!(pending.operation, Operation::Login) {
            match self.send_login(cx, url, body, pending) {
                Some(id) => id,
                None => {
                    self.drive(cx);
                    return;
                }
            }
        } else {
            let id = cx.request(method, url, headers, body, pending);
            cx.request_timeout(id, self.options.timeout_ms);
            id
        };
        if let Some(stream) = stream {
            cx.stream_response(id, stream);
        }
        self.request_times.push_back(cx.now);
        if let Some((timer, _)) = self.dispatch_timer.take() {
            cx.clear_timer(timer);
        }
        self.drive(cx);
    }

    /// One session operation per physical login; reissues are new operations.
    /// The legacy password body rides the primary private session egress;
    /// provider send-time presentation remains a separate obligation.
    fn send_login(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        url: String,
        body: Option<String>,
        mut pending: Pending,
    ) -> Option<safe_cex_engine::RequestId> {
        use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
        use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy, SessionPolicy};
        const HEADERS: &[(&str, &str)] = &[("Content-Type", "application/json")];
        let session = match self.login_session.take() {
            Some(session) if cx.session_live(&session) => Ok(session),
            _ => {
                let slots = Slots::new(
                    (!self.options.key.is_empty()).then_some(()),
                    None,
                    SlotPolicy::default(),
                );
                let private_egress = slots
                    .select(CredentialRole::SessionAuth)
                    .ok()
                    .and_then(|(selection, _)| selection.session_egress());
                cx.open_session(SessionPolicy {
                    credential: None,
                    repeat_safe: false,
                    private_egress,
                })
            }
        };
        let context = session.and_then(|session| {
            let context = cx.session_context(
                &session,
                Policy {
                    operation: Operation::Mutation,
                    credential: None,
                    timeout_ms: self.options.timeout_ms,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            );
            self.login_session = Some(session);
            context
        });
        let refused = match context {
            Ok(context) => {
                pending.context = Some(context.clone());
                match cx.request_managed_with_headers(
                    context,
                    Method::Post,
                    url,
                    HEADERS,
                    body,
                    pending,
                ) {
                    Ok(id) => return Some(id),
                    Err((pending, _)) => pending,
                }
            }
            Err(_) => pending,
        };
        self.retire_login_session(cx);
        self.fail_operation(
            cx,
            refused.operation,
            not_sent("DXtrade session login unavailable"),
        );
        None
    }

    fn retire_login_session(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(session) = self.login_session.take() {
            let _ = cx.retire_session(&session);
        }
    }

    fn request_spec(
        &self,
        cx: &Cx<'_, Pending>,
        operation: &Operation,
    ) -> Result<(Method, String, Option<String>), String> {
        let account = self.account_code().unwrap_or_default();
        let serialize = |body: &MarketData<'_>| {
            serde_json::to_string(body)
                .map(Some)
                .map_err(|_| "Invalid DXtrade market-data request".to_owned())
        };
        Ok(match operation {
            Operation::Trade(request) => {
                (request.method, request.path.clone(), request.body.clone())
            }
            Operation::Login => {
                if self.options.key.is_empty() || self.options.secret.is_empty() {
                    return Err("Missing DXtrade username or password".into());
                }
                (
                    Method::Post,
                    "login".into(),
                    Some(
                        serde_json::to_string(&LoginBody {
                            domain: "default",
                            username: &self.options.key,
                            password: &self.options.secret,
                        })
                        .map_err(|_| "Invalid DXtrade login request".to_owned())?,
                    ),
                )
            }
            Operation::Portfolio { .. } => (
                Method::Get,
                format!(
                    "accounts/{}/portfolio",
                    safe_cex_engine::url::encode(account)
                ),
                None,
            ),
            Operation::Discover => (
                Method::Get,
                format!("users/{}", safe_cex_engine::url::encode(&self.options.key)),
                None,
            ),
            Operation::Instruments { .. } => (
                Method::Get,
                format!(
                    "instruments/query?accountCode={}",
                    safe_cex_engine::url::encode(account)
                ),
                None,
            ),
            Operation::Details { .. } => (
                Method::Get,
                format!(
                    "accounts/{}/instruments/query",
                    safe_cex_engine::url::encode(account)
                ),
                None,
            ),
            Operation::Quotes { .. } => {
                let symbols: Vec<_> = cx
                    .store
                    .markets()
                    .iter()
                    .map(|market| market.symbol.as_str())
                    .collect();
                (
                    Method::Post,
                    "marketdata".into(),
                    serialize(&MarketData {
                        event_types: vec![EventType {
                            kind: "Quote",
                            candle_type: None,
                            from_time: None,
                            to_time: None,
                        }],
                        symbols,
                        account,
                    })?,
                )
            }
            Operation::Candles {
                symbol,
                interval,
                from_ms,
                to_ms,
                ..
            } => {
                let (kind, _) = wire::interval(*interval)
                    .ok_or_else(|| "Unsupported DXtrade candle interval".to_owned())?;
                (
                    Method::Post,
                    "marketdata".into(),
                    serialize(&MarketData {
                        event_types: vec![EventType {
                            kind: "Candle",
                            candle_type: Some(kind),
                            from_time: from_ms.map(wire::iso_time).transpose()?,
                            to_time: to_ms.map(wire::iso_time).transpose()?,
                        }],
                        symbols: vec![symbol],
                        account,
                    })?,
                )
            }
        })
    }

    pub(crate) fn arm_poll(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.auth_failed && !self.auth_draining && self.poll_timer.is_none() {
            self.poll_timer = Some(cx.set_poll_timer(self.options.tick_interval));
        }
    }

    pub(crate) fn poll(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.auth_failed || self.auth_draining {
            return;
        }
        if !cx.store.loaded().markets {
            self.fetch_markets(cx);
        } else if !cx.store.loaded().tickers || self.public_only() {
            self.fetch_quotes(cx);
        } else {
            self.fetch_portfolio(cx, false);
        }
    }

    fn fetch_quotes(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.quotes_pending {
            self.quotes_pending = true;
            self.schedule(
                cx,
                Operation::Quotes {
                    rows: Vec::new(),
                    invalid: false,
                    validated_time: None,
                },
            );
        }
    }

    fn fetch_portfolio(&mut self, cx: &mut Cx<'_, Pending>, initial: bool) {
        if !self.portfolio_pending {
            self.portfolio_pending = true;
            self.schedule(
                cx,
                Operation::Portfolio {
                    revision: self.order_revision,
                    initial,
                    snapshot: None,
                    invalid: false,
                },
            );
        }
    }

    fn fetch_markets(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.instruments_pending && self.instruments.is_none() {
            self.instruments_pending = true;
            self.schedule(
                cx,
                Operation::Instruments {
                    rows: Vec::new(),
                    invalid: false,
                },
            );
        }
        if !self.details_pending && self.details.is_none() {
            self.details_pending = true;
            self.schedule(
                cx,
                Operation::Details {
                    rows: Vec::new(),
                    invalid: false,
                },
            );
        }
    }

    fn join_markets(&mut self, cx: &mut Cx<'_, Pending>) {
        let (Some(instruments), Some(details)) = (&self.instruments, &self.details) else {
            return;
        };
        let by_symbol: HashMap<_, _> = instruments
            .iter()
            .map(|instrument| (instrument.symbol.as_ref(), instrument))
            .collect();
        let mut markets = Vec::with_capacity(details.len());
        for detail in details {
            let Some(instrument) = by_symbol.get(detail.symbol.as_ref()) else {
                continue;
            };
            let rate = detail
                .margin_rate
                .tiers
                .first()
                .map(|tier| tier.value)
                .unwrap_or(0.0);
            let leverage = 1.0 / rate;
            let values = [
                instrument.price_increment,
                instrument.lot_size,
                detail.min_order_size,
                detail.min_order_size_increment,
                detail.max_order_size.value,
                rate,
            ];
            if values
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
                || detail.max_order_size.value < detail.min_order_size
                || !leverage.is_finite()
                || leverage < 1.0
                || detail.account.as_ref() != self.account_code().unwrap_or_default()
            {
                self.instruments = None;
                self.details = None;
                cx.notice(
                    Level::Error,
                    code::REQUEST_FAILED,
                    "Invalid DXtrade instrument details",
                );
                self.arm_poll(cx);
                return;
            }
            let symbol = detail.symbol.to_string();
            markets.push(Market {
                id: symbol.clone(),
                base: symbol.replacen("USD", "", 1),
                symbol,
                quote: "USD".into(),
                pretty_symbol: None,
                active: detail.trading_status == "FULL",
                precision: Precision {
                    amount: detail.min_order_size_increment,
                    price: instrument.price_increment,
                },
                limits: Limits {
                    amount: MinMax {
                        min: detail.min_order_size,
                        max: detail.max_order_size.value,
                    },
                    leverage: MinMax {
                        min: 1.0,
                        max: leverage,
                    },
                    price: None,
                },
                contract_size: Some(instrument.lot_size),
                is_expiring: Some(false),
                is_inverse: Some(false),
                is_spot: Some(false),
                is_perpetual: None,
                is_read_only: Some(false),
                taker_fee: Some(0.0),
                maker_fee: Some(0.0),
            });
        }
        if markets.is_empty() {
            self.instruments = None;
            self.details = None;
            cx.notice(
                Level::Error,
                code::REQUEST_FAILED,
                "DXtrade returned no matching instruments",
            );
            self.arm_poll(cx);
            return;
        }
        cx.store.set_markets(markets);
        self.poll(cx);
    }

    fn publish_quotes(&mut self, cx: &mut Cx<'_, Pending>, rows: Vec<Ticker>) {
        for ticker in &rows {
            if let Some((_, book)) = self.books.get_mut(&ticker.symbol) {
                book.load_sorted(BookSide::Bids, [(ticker.bid, 1.0)]);
                book.load_sorted(BookSide::Asks, [(ticker.ask, 1.0)]);
                cx.book_changed(&ticker.symbol);
            }
            for ((symbol, interval), _) in self
                .candles
                .iter()
                .filter(|((symbol, _), _)| symbol == &ticker.symbol)
            {
                let seconds =
                    wire::synthetic_interval_seconds(*interval).expect("validated candle interval");
                cx.candle(
                    symbol,
                    *interval,
                    Candle {
                        timestamp: (cx.now / 1_000.0 / seconds).floor() * seconds,
                        open: ticker.last,
                        high: ticker.last,
                        low: ticker.last,
                        close: ticker.last,
                        volume: 0.0,
                    },
                );
            }
        }
        cx.store.set_tickers(rows);
        if !self.public_only() && !self.portfolio_started {
            self.portfolio_started = true;
            self.fetch_portfolio(cx, true);
        } else {
            self.arm_poll(cx);
        }
    }

    fn record_canceled(&mut self, cx: &Cx<'_, Pending>, id: &str, code: &str) {
        self.terminal_orders.retain(|_, at| cx.now - *at < 60_000.0);
        self.terminal_orders.insert(id.to_owned(), cx.now);
        self.terminal_orders.insert(code.to_owned(), cx.now);
        for order in cx.store.orders().iter().filter(|order| {
            order.id == id || order.id == code || order.client_order_id.as_deref() == Some(code)
        }) {
            self.terminal_orders.insert(order.id.clone(), cx.now);
            if let Some(client) = order.client_order_id.as_ref().filter(|id| !id.is_empty()) {
                self.terminal_orders.insert(client.clone(), cx.now);
            }
        }
    }

    fn fail_operation(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        operation: Operation,
        error: VenueError,
    ) {
        let operation = match operation {
            Operation::Trade(request) => return self.on_trade_failed(cx, request, error),
            operation => operation,
        };
        let session_code = match error.kind {
            Some(ErrorKind::AuthInvalid | ErrorKind::AuthPermission) => code::AUTH_REJECTED,
            _ => code::REQUEST_FAILED,
        };
        let failure = error;
        let error = failure.message.as_str();
        match operation {
            Operation::Trade(_) => unreachable!("trade failures return above"),
            Operation::Portfolio { .. } => {
                self.portfolio_pending = false;
                cx.notice(Level::Error, code::REQUEST_FAILED, error);
                if !self.auth_failed && !self.auth_draining {
                    self.fetch_quotes(cx);
                }
            }
            Operation::Login | Operation::Discover => {
                self.auth_error = error.into();
                if !cx.store.loaded().markets {
                    self.auth_failed = true;
                    let message = format!("DXtrade startup failed: {error}");
                    cx.notice(Level::Error, session_code, &message);
                    cx.startup_failed(message);
                }
                if matches!(operation, Operation::Login) {
                    self.session.login_pending = false;
                    self.session.token = None;
                } else {
                    self.session.discovery_pending = false;
                }
                let was_draining = std::mem::replace(&mut self.auth_draining, true);
                let queued = std::mem::take(&mut self.queue);
                for pending in queued {
                    let mut unsent = not_sent(error);
                    // Only a credential refusal types the work it never sent.
                    unsent.kind = failure.kind.filter(|kind| {
                        matches!(kind, ErrorKind::AuthInvalid | ErrorKind::AuthPermission)
                    });
                    self.fail_operation(cx, pending.operation, unsent);
                }
                self.auth_draining = was_draining;
                cx.notice(Level::Error, session_code, error);
                if self.auth_failed {
                    self.retire_login_session(cx);
                    self.session.token = None;
                    self.session.expires_at = 0.0;
                    self.session.login_pending = false;
                    self.session.discovery_pending = false;
                    if let Some((timer, _)) = self.dispatch_timer.take() {
                        cx.clear_timer(timer);
                    }
                    if let Some(timer) = self.poll_timer.take() {
                        cx.clear_timer(timer);
                    }
                } else {
                    self.arm_poll(cx);
                }
            }
            Operation::Instruments { .. } => {
                self.instruments_pending = false;
                cx.notice(Level::Error, code::REQUEST_FAILED, error);
                self.arm_poll(cx);
            }
            Operation::Details { .. } => {
                self.details_pending = false;
                cx.notice(Level::Error, code::REQUEST_FAILED, error);
                self.arm_poll(cx);
            }
            Operation::Quotes { .. } => {
                self.quotes_pending = false;
                cx.notice(Level::Error, code::REQUEST_FAILED, error);
                self.arm_poll(cx);
            }
            Operation::Candles { command, .. } => {
                // A session refusal keeps its type; other failures stay text.
                let error = if matches!(
                    failure.kind,
                    Some(ErrorKind::AuthInvalid | ErrorKind::AuthPermission)
                ) {
                    failure
                } else {
                    VenueError::new(failure.message)
                };
                cx.fail(command, error)
            }
        }
    }
}

impl Venue for BreakoutProp {
    fn guard_pending_placements(&self) -> bool {
        true
    }
    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            public_catalogue_validation: true,
            cancel_all: safe_cex_engine::CancelAll::Native,
            max_leverage: safe_cex_engine::MaxLeverage::Unsupported,
            ..safe_cex_engine::Capabilities::NO_HISTORY
        }
    }

    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.fetch_markets(cx);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}

    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Pending,
        response: Response<'_>,
    ) {
        if let Some(context) = pending.context.take() {
            if (200..300).contains(&response.status) {
                self.answered_login = Some(context);
            } else {
                self.retire_login_session(cx);
            }
        }
        if self.auth_failed {
            self.answered_login = None;
            self.retire_login_session(cx);
            let error = unanswered(&pending.operation, self.auth_error.clone());
            self.fail_operation(cx, pending.operation, error);
            return;
        }
        if matches!(pending.operation, Operation::Login) && matches!(response.status, 401 | 403) {
            self.auth_failed = true;
            self.retire_login_session(cx);
            // Typed (AuthInvalid/AuthPermission); the body is never echoed.
            let error = http_failure(response.status);
            self.auth_error = error.message.clone();
            self.fail_operation(cx, pending.operation, error);
            cx.deactivate();
            return;
        }
        if response.status == 401
            && !matches!(pending.operation, Operation::Login)
            && !pending.auth_retried
        {
            if pending.generation == self.session.generation {
                self.session.token = None;
                self.session.expires_at = 0.0;
            }
            pending.auth_retried = true;
            pending.operation.reset();
            pending.ready_at = cx.now;
            self.queue.push_front(pending);
            self.drive(cx);
            return;
        }
        if !(200..300).contains(&response.status) {
            let mut error = http_failure(response.status);
            // The amend's history read precedes the replacement, which was never sent.
            if matches!(&pending.operation, Operation::Trade(request) if request.method == Method::Get)
            {
                error.outcome = Some(Outcome::NotSent);
            }
            self.fail_operation(cx, pending.operation, error);
            self.drive(cx);
            return;
        }
        match pending.operation {
            Operation::Trade(request) => self.on_trade_response(cx, request, response.body),
            Operation::Portfolio {
                revision,
                initial,
                mut snapshot,
                invalid,
            } => {
                self.portfolio_pending = false;
                if invalid || snapshot.is_none() {
                    cx.notice(
                        Level::Error,
                        code::SNAPSHOT_DISCARDED,
                        "Invalid or missing selected DXtrade portfolio",
                    );
                } else if revision == self.order_revision {
                    let mut snapshot = snapshot.take().expect("validated portfolio");
                    if self
                        .last_portfolio_version
                        .is_none_or(|last| snapshot.version >= last)
                    {
                        self.terminal_orders.retain(|_, at| cx.now - *at < 60_000.0);
                        snapshot.orders.retain(|order| {
                            !self.terminal_orders.contains_key(&order.id)
                                && !order
                                    .client_order_id
                                    .as_ref()
                                    .is_some_and(|id| self.terminal_orders.contains_key(id))
                        });
                        self.last_portfolio_version = Some(snapshot.version);
                        snapshot.commit(cx);
                        self.trading.reconcile(cx.store, cx.now);
                    }
                }
                if initial && cx.store.loaded().balance {
                    self.poll(cx);
                } else {
                    self.fetch_quotes(cx);
                }
            }
            Operation::Login => {
                let context = self.answered_login.take();
                match self.session.accept(response.body, cx.now) {
                    Ok(()) => {
                        if let Some(context) = context {
                            let _ = cx.http_write_fact(
                                &context,
                                safe_cex_engine::classify::WriteFact::Accepted {
                                    order_ids: vec![],
                                },
                                None,
                            );
                            let _ = cx.finish_http_context(&context);
                        }
                    }
                    Err(error) => {
                        // Unproven: the next login must be a new generation.
                        self.retire_login_session(cx);
                        self.fail_operation(cx, Operation::Login, error.into());
                    }
                }
            }
            Operation::Discover => {
                if let Err(error) = self.session.select_account(
                    response.body,
                    &self.options.key,
                    self.options.subaccount.as_deref(),
                ) {
                    self.fail_operation(cx, Operation::Discover, error.into());
                }
            }
            Operation::Instruments { rows, invalid } => {
                self.instruments_pending = false;
                if invalid {
                    self.fail_operation(
                        cx,
                        Operation::Instruments { rows, invalid },
                        "Invalid DXtrade instruments response".into(),
                    );
                } else {
                    self.instruments = Some(rows);
                    self.join_markets(cx);
                }
            }
            Operation::Details { rows, invalid } => {
                self.details_pending = false;
                if invalid {
                    self.fail_operation(
                        cx,
                        Operation::Details { rows, invalid },
                        "Invalid DXtrade instrument details response".into(),
                    );
                } else {
                    self.details = Some(rows);
                    self.join_markets(cx);
                }
            }
            Operation::Quotes {
                rows,
                invalid,
                validated_time,
            } => {
                self.quotes_pending = false;
                if invalid {
                    self.fail_operation(
                        cx,
                        Operation::Quotes {
                            rows,
                            invalid,
                            validated_time,
                        },
                        "Invalid DXtrade quotes response".into(),
                    );
                } else {
                    self.publish_quotes(cx, rows);
                }
            }
            Operation::Candles {
                command,
                rows,
                invalid,
                ..
            } => {
                if invalid {
                    cx.fail(command, "Invalid DXtrade candles response");
                } else {
                    {
                        let result: Result<String, String> = serde_json::to_string(&rows)
                            .map_err(|_| "Invalid DXtrade candles".into());
                        match result {
                            Ok(data) => cx.done(command, data),
                            Err(error) => cx.fail(command, error),
                        }
                    };
                }
            }
        }
        self.drive(cx);
    }

    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        if self.auth_failed {
            return;
        }
        match &mut pending.operation {
            Operation::Portfolio {
                snapshot, invalid, ..
            } => {
                match account::parse_portfolio(
                    item,
                    cx.store,
                    self.account_code().unwrap_or_default(),
                ) {
                    Ok(Some(row)) if snapshot.is_none() => *snapshot = Some(row),
                    Ok(None) => {}
                    _ => *invalid = true,
                }
            }
            Operation::Instruments { rows, invalid } => {
                match serde_json::from_str::<wire::Instrument<'_>>(item) {
                    Ok(row) if row.symbol == "XBTUSD" => {}
                    Ok(row)
                        if !row.symbol.is_empty()
                            && !rows.iter().any(|old| old.symbol == row.symbol) =>
                    {
                        rows.push(row.owned())
                    }
                    _ => *invalid = true,
                }
            }
            Operation::Details { rows, invalid } => {
                match serde_json::from_str::<wire::Details<'_>>(item) {
                    Ok(row) if row.symbol == "XBTUSD" => {}
                    Ok(row)
                        if !row.symbol.is_empty()
                            && !rows.iter().any(|old| old.symbol == row.symbol) =>
                    {
                        rows.push(row.owned())
                    }
                    _ => *invalid = true,
                }
            }
            Operation::Quotes {
                rows,
                invalid,
                validated_time,
            } => {
                match serde_json::from_str::<wire::Quote<'_>>(item) {
                    Ok(row)
                        if row.kind == "Quote"
                            && !row.symbol.is_empty()
                            && row.bid.is_finite()
                            && row.bid > 0.0
                            && row.ask.is_finite()
                            && row.ask > 0.0
                            && (validated_time.as_deref() == Some(row.time.as_ref())
                                || wire::timestamp(&row.time).is_ok())
                            && !rows.iter().any(|old| old.symbol == row.symbol) =>
                    {
                        // DXtrade quotes share a snapshot time; validate identical
                        // bytes once within this response, never across retries.
                        if validated_time.as_deref() != Some(row.time.as_ref()) {
                            *validated_time = Some(row.time.into_owned());
                        }
                        rows.push(Ticker {
                            id: row.symbol.to_string(),
                            symbol: row.symbol.into_owned(),
                            bid: row.bid,
                            ask: row.ask,
                            last: row.bid,
                            mark: row.bid,
                            index: row.bid,
                            ..Ticker::default()
                        });
                    }
                    _ => *invalid = true,
                }
            }
            Operation::Candles {
                rows,
                invalid,
                symbol,
                interval,
                ..
            } => match serde_json::from_str::<wire::Bar<'_>>(item) {
                Ok(row)
                    if row.kind == "Candle"
                        && row.symbol == symbol.as_str()
                        && wire::interval(*interval)
                            .is_some_and(|(kind, _)| row.candle_type == kind)
                        && [row.open, row.high, row.low, row.close]
                            .iter()
                            .all(|price| price.is_finite() && *price > 0.0)
                        && row.volume.is_finite()
                        && row.volume >= 0.0 =>
                {
                    if let Ok(time) = wire::timestamp(&row.time) {
                        rows.push(Candle {
                            timestamp: time / 1_000.0,
                            open: row.open,
                            high: row.high,
                            low: row.low,
                            close: row.close,
                            volume: row.volume,
                        });
                    } else {
                        *invalid = true;
                    }
                }
                _ => *invalid = true,
            },
            _ => {}
        }
    }

    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            self.on_response(cx, pending, response);
        } else {
            // A 2xx answer that does not prove what happened.
            self.fail_operation(
                cx,
                pending.operation,
                VenueError::new("DXtrade response missing its expected array")
                    .outcome(Outcome::Unknown),
            );
            self.drive(cx);
        }
    }

    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending, _error: &str) {
        if pending.context.take().is_some() {
            self.retire_login_session(cx);
        }
        if self.auth_failed {
            let error = unanswered(&pending.operation, self.auth_error.clone());
            self.fail_operation(cx, pending.operation, error);
            return;
        }
        if !matches!(&pending.operation, Operation::Trade(request) if request.method != Method::Get)
            && pending.network_attempt < 3
        {
            pending.network_attempt += 1;
            pending.ready_at = cx.now + 1_000.0 * f64::from(pending.network_attempt);
            pending.operation.reset();
            self.queue.push_back(pending);
        } else {
            let error =
                unanswered(&pending.operation, "DXtrade transport failed").kind(ErrorKind::Network);
            self.fail_operation(cx, pending.operation, error);
        }
        self.drive(cx);
    }

    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.dispatch_timer.is_some_and(|(id, _)| id == timer) {
            self.dispatch_timer = None;
            self.drive(cx);
        } else if self.poll_timer == Some(timer) {
            self.poll_timer = None;
            self.poll(cx);
        }
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::FetchOptionChain { command, .. }
            | Command::FetchOptionExpiries { command, .. }
            | Command::FetchAccountGreeks { command } => {
                cx.fail(command, "Options are not enabled on this account");
            }
            Command::SetTickerSymbols { .. } => {}
            Command::Venue { command, .. } => {
                safe_cex_engine::unsupported_venue_action(cx, command)
            }
            Command::SubscribeOrderBook { symbol } => {
                let entry = self.books.entry(symbol).or_default();
                entry.0 += 1;
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some((count, _)) = self.books.get_mut(&symbol) {
                    *count -= 1;
                    if *count == 0 {
                        self.books.remove(&symbol);
                    }
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                if wire::synthetic_interval_seconds(interval).is_some() {
                    *self.candles.entry((symbol, interval)).or_default() += 1;
                } else {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        interval.unsupported("DXtrade"),
                    );
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol, interval);
                if let Some(count) = self.candles.get_mut(&key) {
                    *count -= 1;
                    if *count == 0 {
                        self.candles.remove(&key);
                    }
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval,
                from_ms,
                to_ms,
                ..
            } => {
                if wire::interval(interval).is_none() {
                    cx.fail(command, interval.unsupported("DXtrade"));
                } else if from_ms.is_some_and(|value| wire::iso_time(value).is_err())
                    || to_ms.is_some_and(|value| wire::iso_time(value).is_err())
                {
                    cx.fail(command, "Invalid DXtrade candle request");
                } else {
                    self.schedule(
                        cx,
                        Operation::Candles {
                            command,
                            symbol,
                            interval,
                            from_ms,
                            to_ms,
                            rows: Vec::new(),
                            invalid: false,
                        },
                    );
                }
            }
            command @ (Command::PlaceOrders { .. }
            | Command::CancelOrders { .. }
            | Command::CancelOrdersWithSymbols { .. }
            | Command::AmendOrder { .. }
            | Command::CancelSymbolOrders { .. }
            | Command::SetLeverage { .. }
            | Command::SetMarginType { .. }
            | Command::SetPositionMode { .. }) => {
                self.on_trade_command(cx, command);
            }
            Command::Query { command, query } => history::query(cx, command, query),
            Command::FetchLeverage { command, .. } => cx.done(command, "null".into()),
            Command::FetchMaxLeverage { command, .. } => cx.fail(
                command,
                "Maximum leverage queries are unavailable for DXtrade",
            ),
            Command::AmendOrderWithSymbol { command, .. }
            | Command::FetchStpMode { command }
            | Command::SetStpMode { command, .. }
            | Command::FetchAccount { command }
            | Command::FetchOrderBook { command, .. } => {
                cx.fail(command, "Unsupported BreakoutProp command");
            }
        }
    }

    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).map(|(_, book)| book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .map(|(symbol, (_, book))| (symbol.as_str(), book)),
        )
    }
}

/// Refused before any request was sent.
pub(crate) fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

/// A sent request whose answer never arrived or was discarded. Only a write
/// can have taken effect.
fn unanswered(operation: &Operation, message: impl Into<String>) -> VenueError {
    let write = matches!(operation, Operation::Trade(request) if request.method != Method::Get);
    VenueError::new(message).outcome(if write {
        Outcome::Unknown
    } else {
        Outcome::NotSent
    })
}

/// A non-2xx answer: 408 and 5xx do not prove the venue refused it.
fn http_failure(status: u16) -> VenueError {
    let error = VenueError::new(format!("DXtrade HTTP {status}"));
    let rejected = |error: VenueError, kind| error.kind(kind).outcome(Outcome::Rejected);
    match status {
        401 => rejected(error, ErrorKind::AuthInvalid),
        403 => rejected(error, ErrorKind::AuthPermission),
        429 => rejected(error, ErrorKind::RateLimited),
        408 => error.outcome(Outcome::Unknown),
        400..=499 => rejected(error, ErrorKind::Rejected),
        _ => error.outcome(Outcome::Unknown),
    }
}

#[cfg(test)]
mod security_poll_interval {
    use super::*;
    #[test]
    fn native_constructor_clamps_polling_before_any_timer_is_scheduled() {
        for (value, expected) in [
            (0.001, 1000.0),
            (1e20, 120000.0),
            (f64::INFINITY, f64::INFINITY),
        ] {
            let venue = BreakoutProp::new(Options {
                tick_interval: value,
                ..Options::default()
            });
            assert_eq!(venue.options.tick_interval, expected);
        }
    }
}
