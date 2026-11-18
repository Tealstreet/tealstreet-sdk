//! BingX Spot. Native/fixture-only until transport and live-key smoke validation.
mod sockets;
mod trading;
mod wire;
use crate::{
    Options,
    sign::{Params, dispatch::Http},
};
use safe_cex_engine::{
    AccountReadScope, Command, CommandId, Cx, ErrorKind, Fill, FillFinancials, Level, Method,
    PublicFeedPolicy, Query, Response, SocketId, TimerId, Venue, VenueError,
    book::{Book, BookSide},
    credential_selection::{PrimaryReason, SlotPolicy, Slots},
    history::{IncompleteReason, ReadCompletion, ReadHistoryRows},
    model::*,
    request_context::{
        Context, Credential, Operation, Owner, Policy, RetryPolicy, Scope, Session, SessionPolicy,
    },
    selected_read::{LocalRefusal, ReadLegPolicy, ReadPolicy, ReadRequest, ReadRole, SelectedRead},
    signing::Identity,
    socket_lifecycle::FeedId,
};
use std::collections::{HashMap, HashSet, VecDeque};
use wire::*;
const MARKETS: &str = "/openApi/spot/v1/common/symbols";
const TICKERS: &str = "/openApi/spot/v2/quote/ticker";
const BALANCE: &str = "/openApi/spot/v1/account/balance";
const ORDERS: &str = "/openApi/spot/v1/trade/openOrders";
const FILLS: &str = "/openApi/spot/v1/trade/myTrades";
const CANDLES: &str = "/openApi/spot/v2/quote/klines";
const DEPTH: &str = "/openApi/spot/v1/market/depth";
const LISTEN: &str = "/openApi/user/auth/userDataStream";
const WS: &str = "wss://open-api-ws.bingx.com/market";
const HISTORY_LIMIT: usize = 500;

pub enum Pending {
    Read {
        context: Context,
        kind: ReadKind,
    },
    Listen {
        context: Context,
        generation: u64,
        create: bool,
    },
    History {
        context: Context,
        rows: Box<ReadHistoryRows<Execution>>,
        limit: usize,
        window: bool,
        symbol: Option<String>,
        order_id: Option<String>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
    },
    Write(Box<trading::Request>),
}
pub enum ReadKind {
    Markets,
    Tickers,
    Balance { revision: u64 },
    Orders { epoch: u64 },
    Candles,
    Depth,
    Lookup { symbol: String, client_id: String },
}
#[derive(Default)]
struct Feed {
    feed: Option<FeedId>,
    socket: Option<SocketId>,
}
#[derive(Default)]
struct SpotBook {
    book: Book,
    subscribers: u32,
}
pub struct BingxSpot {
    options: Options,
    slots: Slots<Identity>,
    rules: HashMap<String, SpotMarket>,
    public: Feed,
    private: Feed,
    listen_key: Option<String>,
    session: Option<Session>,
    listen_generation: u64,
    keepalive: Option<TimerId>,
    poll: Option<TimerId>,
    retry: Option<TimerId>,
    books: HashMap<String, SpotBook>,
    candles: HashMap<(String, Interval), u32>,
    ticker_symbols: Option<Vec<String>>,
    live_tickers: HashSet<String>,
    stream_symbol: String,
    sequence: std::cell::Cell<u64>,
    balance_revision: u64,
    terminal: HashSet<String>,
    terminal_fifo: VecDeque<String>,
    seen_fills: HashSet<String>,
    fill_fifo: VecDeque<String>,
    jobs: HashMap<CommandId, trading::Run>,
}
impl BingxSpot {
    pub fn new(options: Options) -> Self {
        let slots = Slots::new(
            if options.public_data_only {
                None
            } else {
                options.credentials.clone()
            },
            if options.public_data_only {
                None
            } else {
                options.read_credentials.clone()
            },
            SlotPolicy {
                secondary_requires_primary: true,
            },
        );
        Self {
            options,
            slots,
            rules: HashMap::new(),
            public: Feed::default(),
            private: Feed::default(),
            listen_key: None,
            session: None,
            listen_generation: 0,
            keepalive: None,
            poll: None,
            retry: None,
            books: HashMap::new(),
            candles: HashMap::new(),
            ticker_symbols: None,
            live_tickers: HashSet::new(),
            stream_symbol: String::new(),
            sequence: Default::default(),
            balance_revision: 0,
            terminal: HashSet::new(),
            terminal_fifo: VecDeque::new(),
            seen_fills: HashSet::new(),
            fill_fifo: VecDeque::new(),
            jobs: HashMap::new(),
        }
    }
    fn remember_terminal(&mut self, id: String) {
        if self.terminal.insert(id.clone()) {
            self.terminal_fifo.push_back(id);
            while self.terminal_fifo.len() > 20_000 {
                if let Some(id) = self.terminal_fifo.pop_front() {
                    self.terminal.remove(&id);
                }
            }
        }
    }
    fn credentials(&self) -> Option<&Identity> {
        self.options
            .credentials
            .as_ref()
            .filter(|_| !self.options.public_data_only)
    }
    fn policy(credential: Option<Credential>) -> Policy {
        Policy {
            operation: Operation::Read,
            credential,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }
    }
    fn fail_read(cx: &mut Cx<'_, Pending>, context: &Context, error: VenueError) {
        if matches!(context.owner(), Owner::Command { .. }) {
            let _ = cx.fail_context(context, error);
        } else {
            cx.notice(Level::Warn, "spot.read.failed", error.message);
            let _ = cx.finish_http_context(context);
        }
    }
    fn public_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &'static str,
        params: Params,
        kind: ReadKind,
        scope: Scope,
    ) {
        let issue = if matches!(
            kind,
            ReadKind::Tickers | ReadKind::Candles | ReadKind::Depth
        ) {
            Cx::http_live_context
        } else {
            Cx::http_context
        };
        let Ok(context) = issue(cx, scope, Self::policy(None)) else {
            return;
        };
        let (url, _, body) = crate::sign::dispatch::unsigned(Method::Get, path, &params);
        if let Err((Pending::Read { context, .. }, error)) = cx.request_managed_with_headers(
            context.clone(),
            None,
            Method::Get,
            url,
            crate::sign::unsigned_headers(Method::Get),
            body,
            Pending::Read { context, kind },
        ) {
            Self::fail_read(cx, &context, error.venue_error());
        }
    }
    fn private_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &'static str,
        params: Params,
        kind: ReadKind,
        scope: Scope,
        primary: bool,
    ) {
        let account_scope = Some(match kind {
            ReadKind::Orders { .. } | ReadKind::Lookup { .. } => AccountReadScope::Orders,
            _ => AccountReadScope::Balance,
        });
        let role = if primary {
            ReadRole::Primary {
                reason: if matches!(kind, ReadKind::Lookup { .. }) {
                    PrimaryReason::ClientOrderLookup
                } else {
                    PrimaryReason::PostUnknownOrderRead
                },
                scope: account_scope,
            }
        } else {
            ReadRole::Read {
                scope: account_scope,
            }
        };
        let selected = SelectedRead::select(&self.slots, role);
        let policy = ReadPolicy {
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        };
        let read = if matches!(scope, Scope::Command(_)) {
            SelectedRead::command(
                cx,
                selected,
                policy,
                LocalRefusal {
                    diagnostic: None,
                    error: invalid("BingX Spot credentials required")
                        .kind(ErrorKind::AuthInvalid)
                        .outcome(safe_cex_engine::Outcome::NotSent),
                },
            )
            .ok()
        } else {
            SelectedRead::scoped(cx, selected, scope, policy).ok()
        };
        let Some(read) = read else {
            return;
        };
        let (url, _, body) = crate::sign::dispatch::unsigned(Method::Get, path, &params);
        if let Err((Pending::Read { context, .. }, error)) = read.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url,
                body,
                stream: None,
            },
            |identity| Box::new(Http::spot(identity.clone(), Method::Get, path, params)),
            |read| Pending::Read {
                context: read.context().clone(),
                kind,
            },
        ) {
            Self::fail_read(cx, &context, error.venue_error());
        }
    }
    fn orders(&mut self, cx: &mut Cx<'_, Pending>, primary: bool) {
        if self.credentials().is_none() || cx.frame_fed() {
            return;
        }
        self.private_read(
            cx,
            ORDERS,
            Params::new(),
            ReadKind::Orders {
                epoch: cx.order_read_epoch(),
            },
            Scope::Background,
            primary,
        );
    }
    fn balances(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.credentials().is_none() || cx.frame_fed() {
            return;
        }
        self.balance_revision = self.balance_revision.saturating_add(1);
        self.private_read(
            cx,
            BALANCE,
            Params::new(),
            ReadKind::Balance {
                revision: self.balance_revision,
            },
            Scope::Background,
            false,
        );
    }
    fn listen(&mut self, cx: &mut Cx<'_, Pending>, create: bool) {
        let Some(identity) = self.credentials().cloned() else {
            return;
        };
        let credential = Credential {
            slot: safe_cex_engine::CredentialSlot::Primary,
            reference: identity.credential.clone(),
        };
        if create {
            if let Some(s) = self.session.take() {
                let _ = cx.retire_session(&s);
            }
            self.listen_generation = self.listen_generation.saturating_add(1);
            self.listen_key = None;
            match cx.open_session(SessionPolicy {
                credential: Some(credential.clone()),
                repeat_safe: true,
                private_egress: None,
            }) {
                Ok(s) => self.session = Some(s),
                Err(_) => {
                    self.schedule_retry(cx);
                    return;
                }
            }
        }
        let Some(session) = &self.session else {
            return;
        };
        let method = if create { Method::Post } else { Method::Put };
        let params = if create {
            Params::new()
        } else {
            Params::new().str("listenKey", self.listen_key.as_deref().unwrap_or_default())
        };
        let Ok(context) = cx.session_context(
            session,
            Policy {
                operation: Operation::Mutation,
                credential: Some(credential),
                timeout_ms: 10_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) else {
            self.schedule_retry(cx);
            return;
        };
        let recipe = Http::spot(identity, method, LISTEN, params);
        let (url, _, body) = recipe.unsigned();
        if let Err((pending, error)) = cx.request_signed_managed(
            context.clone(),
            None,
            method,
            url,
            body,
            Box::new(recipe),
            Pending::Listen {
                context,
                generation: self.listen_generation,
                create,
            },
        ) {
            self.on_sign_error(cx, pending, error.venue_error());
        }
    }
    fn schedule_retry(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.retry.is_none() {
            self.retry = Some(cx.set_timer(30_000.));
        }
    }
    fn valuation(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        currencies: Vec<CurrencyBalance>,
    ) -> Result<(), VenueError> {
        let mut sum = Balance::default();
        let mut coins = vec![];
        let mut unpriced = false;
        for r in &currencies {
            let price = if matches!(r.currency.as_str(), "USD" | "USDT" | "USDC") {
                Some(1.)
            } else {
                ["USDT", "USDC", "USD"]
                    .iter()
                    .filter_map(|q| cx.store.ticker(&format!("{}{q}", r.currency)))
                    .map(|t| t.last)
                    .find(|p| p.is_finite() && *p > 0.)
            };
            if r.balance.total > 0. && price.is_none() {
                unpriced = true;
            }
            let p = price.unwrap_or(0.);
            sum.free += r.balance.free * p;
            sum.used += r.balance.used * p;
            sum.total += r.balance.total * p;
            coins.push(CoinBalance {
                base: r.currency.clone(),
                amount: r.balance.total,
                notional: r.balance.total * p,
            });
        }
        cx.store.set_currency_balances(currencies);
        if unpriced {
            return Err(invalid(
                "BingX Spot wallet valuation requires a USD quote; currency balances retained",
            ));
        }
        if !sum.total.is_finite() {
            return Err(invalid("BingX Spot wallet valuation overflow"));
        }
        cx.store.set_coin_balances(coins);
        cx.store.set_balance(sum);
        Ok(())
    }
    fn read_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: &Context,
        kind: ReadKind,
        response: Response<'_>,
    ) -> Result<Option<String>, VenueError> {
        match kind {
            ReadKind::Markets => {
                let source = data::<Symbols>(response)?.symbols;
                let rows = source
                    .iter()
                    .map(SpotMarket::map)
                    .collect::<Result<Vec<_>, _>>()?;
                if rows.is_empty() {
                    return Err(invalid("Empty BingX Spot market catalog"));
                }
                self.rules = source
                    .into_iter()
                    .map(|r| (wire::symbol(&r.symbol), r))
                    .collect();
                cx.store.set_markets(rows);
                cx.store.set_positions(vec![]);
                cx.store.set_hedged(false);
                if !cx.frame_fed() {
                    self.public_read(
                        cx,
                        TICKERS,
                        Params::new(),
                        ReadKind::Tickers,
                        Scope::Background,
                    );
                    if cx.public_feed_policy() == PublicFeedPolicy::Direct {
                        self.connect_public(cx)
                            .map_err(|_| invalid("Spot socket unavailable"))?;
                    }
                    self.balances(cx);
                    self.orders(cx, false);
                    if self.credentials().is_some() {
                        self.listen(cx, true);
                    }
                    if self.poll.is_none() {
                        self.poll = Some(cx.set_timer(30_000.));
                    }
                }
            }
            ReadKind::Tickers => {
                let rows = data::<Vec<Daily>>(response)?
                    .into_iter()
                    .map(Daily::map)
                    .collect::<Result<Vec<_>, _>>()?;
                if rows.is_empty() {
                    return Err(invalid("Empty BingX Spot ticker snapshot"));
                }
                let mut current = cx.store.tickers().to_vec();
                for r in rows {
                    if cx.store.market(&r.symbol).is_some() {
                        if let Some(t) = current.iter_mut().find(|t| t.symbol == r.symbol) {
                            if !self.live_tickers.contains(&r.symbol) {
                                *t = r;
                            }
                        } else {
                            current.push(r);
                        }
                    }
                }
                cx.store.set_tickers(current);
                if let Some(currencies) = cx.store.currency_balances().map(<[_]>::to_vec)
                    && let Err(e) = self.valuation(cx, currencies)
                {
                    cx.notice(Level::Warn, "valuation.unavailable", e.message);
                }
            }
            ReadKind::Balance { revision } => {
                let rows = data::<Balances>(response)?
                    .balances
                    .into_iter()
                    .map(Wallet::map)
                    .collect::<Result<Vec<_>, _>>()?;
                if revision == self.balance_revision {
                    self.valuation(cx, rows)?;
                }
            }
            ReadKind::Orders { epoch } => {
                let source = data::<Orders>(response)?.orders;
                if source.len() >= 2000 {
                    return Err(invalid(
                        "BingX Spot open-order limit cannot prove account coverage",
                    ));
                }
                let rows = source
                    .into_iter()
                    .map(SpotOrder::map)
                    .collect::<Result<Vec<_>, _>>()?;
                if rows.iter().any(|r| cx.store.market(&r.symbol).is_none()) {
                    return Err(invalid("Spot orders refer to an unknown market"));
                }
                let scope = safe_cex_engine::order_read_cover::OrderReadScope::Account {
                    key: "spot.orders",
                    account: &["spot.orders"],
                };
                if let Some(refused) = cx.commit_orders_read(scope, |_| true, rows, &[]) {
                    cx.orders_reconciled_except(&refused, epoch);
                }
            }
            ReadKind::Candles => {
                let mut rows = data::<Vec<Vec<safe_cex_engine::de::Number>>>(response)?
                    .into_iter()
                    .map(candle)
                    .collect::<Result<Vec<_>, _>>()?;
                safe_cex_engine::model::sort_candles_by_time(&mut rows);
                return Ok(Some(
                    serde_json::to_string(&rows)
                        .map_err(|_| invalid("Spot candle encoding failed"))?,
                ));
            }
            ReadKind::Depth => {
                let d = data::<Depth>(response)?;
                if !d.valid() {
                    return Err(invalid("Invalid Spot depth"));
                }
                let mut book = Book::default();
                for (p, q) in d.bids {
                    book.apply(BookSide::Bids, p.0, q.0);
                }
                for (p, q) in d.asks {
                    book.apply(BookSide::Asks, p.0, q.0);
                }
                return Ok(Some(
                    serde_json::json!({"bids":book.bids(),"asks":book.asks()}).to_string(),
                ));
            }
            ReadKind::Lookup { symbol, client_id } => {
                if (200..300).contains(&response.status)
                    && serde_json::from_str::<Envelope<serde::de::IgnoredAny>>(response.body)
                        .is_ok_and(|e| e.code == 100404)
                {
                    return Ok(Some(r#"{"found":false,"definitive":false}"#.into()));
                }
                let row = data::<SpotOrder>(response)?.map()?;
                if row.symbol != symbol
                    || row
                        .client_order_id
                        .as_ref()
                        .is_some_and(|id| id != &client_id)
                {
                    return Err(invalid("Contradictory Spot client-ID lookup"));
                }
                return Ok(Some(
                    serde_json::json!({"found":true,"order":row}).to_string(),
                ));
            }
        }
        let _ = context;
        Ok(None)
    }
    fn fills(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, query: Query) {
        let Query::Fills {
            symbol,
            order_id,
            from_ms,
            to_ms,
            limit,
        } = query
        else {
            self.local_failure(
                cx,
                "BingX Spot history axis is unsupported",
                Operation::Read,
                ErrorKind::Unsupported,
            );
            return;
        };
        let selected = SelectedRead::select(
            &self.slots,
            ReadRole::Read {
                scope: Some(AccountReadScope::Account),
            },
        );
        let Ok(root) = SelectedRead::command_root(
            cx,
            selected,
            LocalRefusal {
                diagnostic: None,
                error: invalid("BingX Spot credentials required").kind(ErrorKind::AuthInvalid),
            },
        ) else {
            return;
        };
        let Ok(mut completion) = ReadCompletion::from_context(cx, command, root.context().clone())
        else {
            return;
        };
        if from_ms.is_some_and(|t| t < cx.protocol_now() - 7. * 24. * 60. * 60. * 1000.) {
            completion.incomplete(
                IncompleteReason::RetentionLimit,
                "BingX Spot trade history is limited to the past seven days",
            );
        }
        let rows = ReadHistoryRows::new(completion, |rows| {
            serde_json::to_string(rows).map_err(|_| invalid("Spot fill encoding failed"))
        });
        if from_ms
            .into_iter()
            .chain(to_ms)
            .any(|t| !t.is_finite() || t < 0.)
            || matches!((from_ms,to_ms),(Some(a),Some(b))if a>b)
            || limit == Some(0)
        {
            let _ = rows.finish(cx, Err(invalid("Invalid Spot history range")));
            return;
        }
        let native_symbol = match &symbol {
            Some(s) => match cx.store.market(s) {
                Some(m) => Some(m.id.clone()),
                None => {
                    let _ = rows.finish(cx, Err(invalid("Unknown Spot history symbol")));
                    return;
                }
            },
            None => None,
        };
        let cap = (limit.unwrap_or(HISTORY_LIMIT as u32) as usize).min(HISTORY_LIMIT);
        let params = Params::new()
            .opt_str("symbol", native_symbol.as_deref())
            .opt_str("orderId", order_id.as_deref())
            .opt_num("startTime", from_ms)
            .opt_num("endTime", to_ms)
            .num("limit", cap as f64);
        let Ok(leg) = root.leg(
            cx,
            ReadLegPolicy {
                timeout_ms: 20_000,
                pacing: None,
            },
        ) else {
            let _ = rows.finish(cx, Err(invalid("Spot history read unavailable")));
            return;
        };
        let (url, _, body) = crate::sign::dispatch::unsigned(Method::Get, FILLS, &params);
        if let Err((Pending::History { rows, .. }, error)) = leg.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url,
                body,
                stream: None,
            },
            |id| Box::new(Http::spot(id.clone(), Method::Get, FILLS, params)),
            |leg| Pending::History {
                context: leg.context().clone(),
                rows: Box::new(rows),
                limit: cap,
                window: from_ms.is_some()
                    || limit.is_none()
                    || limit.is_some_and(|v| v as usize > HISTORY_LIMIT),
                symbol,
                order_id,
                from_ms,
                to_ms,
            },
        ) {
            let _ = rows.finish(cx, Err(error.venue_error()));
        }
    }
    fn local_failure(
        &self,
        cx: &mut Cx<'_, Pending>,
        message: &str,
        operation: Operation,
        kind: ErrorKind,
    ) {
        let mut policy = Self::policy(None);
        policy.operation = operation;
        if let Ok(context) = cx.admit_http_command(policy) {
            let _ = cx.fail_context(
                &context,
                invalid(message)
                    .kind(kind)
                    .outcome(safe_cex_engine::Outcome::NotSent),
            );
        }
    }
}
impl Venue for BingxSpot {
    type Pending = Pending;
    fn unknown_outcome_telemetry() -> Option<safe_cex_engine::UnknownOutcomeTelemetry> {
        Some(|facts| {
            let mut fields = vec![
                ("command", facts.command.into()),
                ("outcome", "unknown".into()),
                ("request", facts.request.0.to_string().into()),
            ];
            if let Some(status) = facts.status {
                fields.push(("status", u32::from(status).into()));
            }
            safe_cex_engine::LogEvent {
                level: Level::Error,
                code: "order.outcome_unknown",
                message: safe_cex_engine::UNKNOWN_OUTCOME_TEXT.into(),
                fields,
            }
        })
    }
    fn managed_sockets() -> bool {
        true
    }
    fn account_auth_outcome(
        &self,
        pending: &Pending,
        response: &Response<'_>,
        _: f64,
        _: &safe_cex_engine::Store,
    ) -> Option<safe_cex_engine::AuthOutcome> {
        let Pending::Read { kind, .. } = pending else {
            return None;
        };
        if !matches!(
            kind,
            ReadKind::Balance { .. } | ReadKind::Orders { .. } | ReadKind::Lookup { .. }
        ) || matches!(kind, ReadKind::Balance { revision } if *revision != self.balance_revision)
            || (!(200..300).contains(&response.status) && response.status != 400)
        {
            return None;
        }
        let envelope =
            serde_json::from_str::<Envelope<serde::de::IgnoredAny>>(response.body).ok()?;
        if crate::trading::KEY_REFUSALS.contains(&envelope.code) {
            return Some(safe_cex_engine::AuthOutcome::Rejected);
        }
        let accepted = match kind {
            ReadKind::Balance { .. } => data::<Balances>(*response).is_ok(),
            ReadKind::Orders { .. } => data::<Orders>(*response).is_ok(),
            ReadKind::Lookup { .. } => data::<SpotOrder>(*response).is_ok(),
            _ => false,
        };
        accepted.then_some(safe_cex_engine::AuthOutcome::Accepted)
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Unsupported,
            client_order_lookup: true,
            fills_by_order_id: true,
            finite_command_numbers: true,
            finite_history_bounds: true,
            reject_client_slippage: true,
            order_history: false,
            income_history: false,
            cancel_all: safe_cex_engine::CancelAll::EachPlacedSymbol,
            order_book_query: true,
            cancel_with_symbols: true,
            order_read_covers: true,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn fence_uncertain_placements(&self) -> bool {
        true
    }
    fn guard_order_mutations(&self) -> bool {
        true
    }
    fn refreshes_orders(&self) -> bool {
        true
    }
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, _: Option<&[String]>) {
        self.orders(cx, true);
    }
    fn startup_reads(p: &Pending) -> safe_cex_engine::startup::Sections {
        use safe_cex_engine::startup::Sections as S;
        match p {
            Pending::Read {
                kind: ReadKind::Markets,
                ..
            } => S::MARKETS,
            Pending::Read {
                kind: ReadKind::Tickers,
                ..
            } => S::TICKERS,
            Pending::Read {
                kind: ReadKind::Balance { .. },
                ..
            } => S::BALANCE,
            Pending::Read {
                kind: ReadKind::Orders { .. },
                ..
            } => S::ORDERS,
            _ => S::NONE,
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self
            .options
            .read_credentials
            .as_ref()
            .is_some_and(|id| id.validate().is_err())
            || self.credentials().is_some_and(|id| id.validate().is_err())
            || (!self.options.public_data_only && self.credentials().is_none())
        {
            cx.notice(
                Level::Error,
                "config.invalid",
                "BingX Spot requires valid signing identities",
            );
            return;
        }
        if self.register_feeds(cx).is_err() {
            cx.notice(
                Level::Error,
                "stream.unavailable",
                "BingX Spot socket registration failed",
            );
            return;
        }
        self.public_read(
            cx,
            MARKETS,
            Params::new(),
            ReadKind::Markets,
            Scope::Background,
        );
    }
    fn socket_startup_ready(&self, store: &safe_cex_engine::Store) -> bool {
        store.loaded().markets
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.lifecycle(cx, event)
    }
    fn encode_socket_interests(
        &self,
        _: &safe_cex_engine::Store,
        feed: FeedId,
        subscribe: bool,
        items: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.encode(feed, subscribe, items, emit)
    }
    fn subscription_interest(
        &self,
        store: &safe_cex_engine::Store,
        topic: &safe_cex_engine::SubscriptionTopic,
    ) -> Option<(FeedId, String)> {
        self.interest(store, topic)
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.message(cx, socket, text);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        match pending {
            Pending::Write(r) => self.write_response(cx, *r, response),
            Pending::Read { context, kind } => {
                if cx.validate_http_context(&context).is_err() {
                    return;
                }
                let discovery = matches!(kind, ReadKind::Markets | ReadKind::Tickers);
                match self.read_response(cx, &context, kind, response) {
                    Ok(Some(value)) => {
                        let _ = cx.done_context(&context, value);
                    }
                    Ok(None) => {
                        let _ = cx.finish_http_context(&context);
                    }
                    Err(error) => {
                        Self::fail_read(cx, &context, error);
                        if discovery {
                            self.schedule_retry(cx);
                        }
                    }
                }
            }
            Pending::Listen {
                context,
                generation,
                create,
            } => {
                if generation != self.listen_generation
                    || cx.validate_http_context(&context).is_err()
                {
                    return;
                }
                #[derive(serde::Deserialize)]
                struct Key {
                    #[serde(rename = "listenKey")]
                    key: String,
                }
                let ok = if create {
                    serde_json::from_str::<Key>(response.body)
                        .ok()
                        .filter(|_| (200..300).contains(&response.status))
                        .filter(|r| !r.key.is_empty())
                        .map(|r| {
                            self.listen_key = Some(r.key);
                            self.connect_private(cx).is_ok()
                        })
                        .unwrap_or(false)
                } else {
                    (200..300).contains(&response.status)
                        && serde_json::from_str::<Envelope<serde::de::IgnoredAny>>(response.body)
                            .is_ok_and(|r| r.code == 0)
                };
                let _ = cx.finish_http_context(&context);
                if ok {
                    if let Some(t) = self.keepalive.take() {
                        cx.clear_timer(t);
                    }
                    self.keepalive = Some(cx.set_timer(30. * 60. * 1000.));
                } else {
                    self.schedule_retry(cx);
                }
            }
            Pending::History {
                context,
                mut rows,
                limit,
                window,
                symbol,
                order_id,
                from_ms,
                to_ms,
            } => {
                if rows.validate_delivery(cx, &context).is_err() {
                    return;
                }
                let result = data::<Fills>(response).and_then(|f| {
                    let count = f.fills.len();
                    let outside = f.fills.iter().any(|r| {
                        from_ms.is_some_and(|t| r.time < t) || to_ms.is_some_and(|t| r.time > t)
                    });
                    let mut ids = HashSet::new();
                    let mapped = f
                        .fills
                        .into_iter()
                        .filter(|r| {
                            from_ms.is_none_or(|t| r.time >= t) && to_ms.is_none_or(|t| r.time <= t)
                        })
                        .map(Trade::map)
                        .collect::<Result<Vec<_>, _>>()?;
                    if mapped.iter().any(|r| {
                        symbol.as_ref().is_some_and(|s| s != &r.symbol)
                            || order_id.as_ref().is_some_and(|id| id != &r.order_id)
                            || !ids.insert((r.symbol.clone(), r.id.clone()))
                    }) {
                        return Err(invalid("Inconsistent Spot fill page"));
                    }
                    {
                        let mut page = rows.admit(cx, &context).map_err(|e| e.venue_error())?;
                        for r in mapped {
                            page.stage(r);
                        }
                        let gap = if outside {
                            Some("BingX Spot returned fills outside the requested window".into())
                        } else {
                            (count >= limit && window).then(|| {
                                "BingX Spot capped fill page cannot prove full history coverage"
                                    .into()
                            })
                        };
                        page.accept_page_with_coverage(gap);
                    }
                    Ok(())
                });
                let _ = rows.finish(cx, result);
            }
        }
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: VenueError) {
        match p {
            Pending::Write(r) => self.write_failed(cx, *r, error, true),
            Pending::Read { context, .. } => Self::fail_read(cx, &context, error),
            Pending::History { rows, .. } => {
                let _ = rows.finish(cx, Err(error));
            }
            Pending::Listen {
                context,
                generation,
                ..
            } => {
                let _ = cx.finish_http_context(&context);
                if generation == self.listen_generation {
                    self.schedule_retry(cx);
                }
            }
        }
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: VenueError) {
        match p {
            // Managed HTTP failures report once through the engine's telemetry hook.
            Pending::Write(r) => self.write_failed(cx, *r, error, false),
            pending => self.on_sign_error(cx, pending, error),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: &str) {
        self.on_http_failed(cx, p, invalid(error).kind(ErrorKind::Network));
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.poll == Some(timer) {
            self.poll = None;
            self.balances(cx);
            self.orders(cx, false);
            self.poll = Some(cx.set_timer(30_000.));
        }
        if self.keepalive == Some(timer) {
            self.keepalive = None;
            self.listen(cx, false);
        }
        if self.retry == Some(timer) {
            self.retry = None;
            if !cx.store.loaded().markets {
                self.public_read(
                    cx,
                    MARKETS,
                    Params::new(),
                    ReadKind::Markets,
                    Scope::Background,
                );
            } else {
                if !cx.store.loaded().tickers {
                    self.public_read(
                        cx,
                        TICKERS,
                        Params::new(),
                        ReadKind::Tickers,
                        Scope::Background,
                    );
                }
                if self.credentials().is_some() {
                    self.listen(
                        cx,
                        self.private.socket.is_none() || self.listen_key.is_none(),
                    );
                }
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => {
                self.books.entry(symbol).or_default().subscribers += 1;
                self.sync_public(cx);
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(b) = self.books.get_mut(&symbol) {
                    b.subscribers = b.subscribers.saturating_sub(1);
                    if b.subscribers == 0 {
                        self.books.remove(&symbol);
                    }
                }
                self.sync_public(cx);
            }
            Command::SubscribeCandles { symbol, interval } => {
                if wire::interval(interval, true).is_some() {
                    *self.candles.entry((symbol, interval)).or_default() += 1;
                    self.sync_public(cx);
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                if let Some(n) = self.candles.get_mut(&(symbol.clone(), interval)) {
                    *n = n.saturating_sub(1);
                    if *n == 0 {
                        self.candles.remove(&(symbol, interval));
                    }
                }
                self.sync_public(cx);
            }
            Command::SetTickerSymbols { symbols } => {
                self.ticker_symbols = Some(symbols);
                self.sync_public(cx);
            }
            Command::FetchCandles {
                command,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => {
                let Some(i) = wire::interval(interval, false) else {
                    self.local_failure(
                        cx,
                        "Unsupported BingX Spot interval",
                        Operation::Read,
                        ErrorKind::Unsupported,
                    );
                    return;
                };
                let Some(m) = cx.store.market(&symbol) else {
                    self.local_failure(
                        cx,
                        "Unknown BingX Spot market",
                        Operation::Read,
                        ErrorKind::InvalidInput,
                    );
                    return;
                };
                let p = Params::new()
                    .str("symbol", &m.id)
                    .str("interval", i)
                    .num("timeZone", 0.)
                    .num("limit", limit.unwrap_or(500).clamp(1, 1440) as f64)
                    .opt_num("startTime", from_ms)
                    .opt_num("endTime", to_ms);
                self.public_read(cx, CANDLES, p, ReadKind::Candles, Scope::Command(command));
            }
            Command::FetchOrderBook {
                command,
                symbol,
                limit,
            } => {
                let Some(m) = cx.store.market(&symbol) else {
                    self.local_failure(
                        cx,
                        "Unknown BingX Spot market",
                        Operation::Read,
                        ErrorKind::InvalidInput,
                    );
                    return;
                };
                let p = Params::new()
                    .str("symbol", &m.id)
                    .num("limit", limit.unwrap_or(50).clamp(1, 1000) as f64);
                self.public_read(cx, DEPTH, p, ReadKind::Depth, Scope::Command(command));
            }
            Command::Query {
                command,
                query:
                    Query::OrderByClientId {
                        symbol,
                        client_order_id,
                    },
            } => {
                if !(1..=40).contains(&client_order_id.len())
                    || !client_order_id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    self.local_failure(
                        cx,
                        "Invalid BingX Spot clientOrderId",
                        Operation::Read,
                        ErrorKind::InvalidInput,
                    );
                    return;
                }
                let Some(m) = cx.store.market(&symbol) else {
                    self.local_failure(
                        cx,
                        "Unknown BingX Spot market",
                        Operation::Read,
                        ErrorKind::InvalidInput,
                    );
                    return;
                };
                let params = Params::new()
                    .str("symbol", &m.id)
                    .str("clientOrderID", &client_order_id);
                self.private_read(
                    cx,
                    "/openApi/spot/v1/trade/query",
                    params,
                    ReadKind::Lookup {
                        symbol,
                        client_id: client_order_id,
                    },
                    Scope::Command(command),
                    true,
                );
            }
            Command::Query { command, query } => self.fills(cx, command, query),
            Command::FetchAccount { .. } => self.local_failure(
                cx,
                "BingX Spot has no account identity endpoint",
                Operation::Read,
                ErrorKind::Unsupported,
            ),
            c => self.write_command(cx, c),
        }
    }
    fn capture_http_reconciliation(
        &self,
        p: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        match p {
            Pending::Write(r) => {
                safe_cex_engine::request_context::ReconciliationData::new(r.command.to_string())
            }
            _ => Err(safe_cex_engine::request_context::Error::Invalid),
        }
    }
    fn on_http_owner_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) {
        self.jobs.retain(|_, r| !owner.matches_plan(&r.plan));
    }
    fn on_http_plan_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        plan: &safe_cex_engine::request_context::Plan,
    ) {
        if let Some(command) = self
            .jobs
            .iter()
            .find_map(|(c, r)| (r.plan.owner() == plan.owner()).then_some(*c))
        {
            self.advance(cx, command);
        }
    }
    fn book(&self, s: &str) -> Option<&Book> {
        self.books.get(s).map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}
