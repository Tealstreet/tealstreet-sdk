//! KuCoin Spot HF: base-unit quantities and native order alteration.
mod trading;
mod wire;
use crate::wire::{Bullet, Number};
use crate::{Options, encoded_path, interval, sign, success};
use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::coverage::SnapshotBuilder;
use safe_cex_engine::execution::{Arithmetic, CumulativePrice, Cursor, Rejection, Report};
use safe_cex_engine::fence::StaleGuard;
use safe_cex_engine::history::{Completion, HistoryRows};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{
    Command, CommandId, Cx, Fill, Method, PublicFeedPolicy, Query, Response, SocketChannel,
    SocketId, TimerId, Venue, VenueError,
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::{BTreeMap, HashSet, VecDeque};
use wire::*;
const REST: &str = "https://api.kucoin.com";
const STALE_SNAPSHOT_LIMIT: u8 = 2;
const SKIP_LOG_LIMIT: usize = 4_096;
const EXECUTION_PROGRESS_LIMIT: usize = 20_000;

pub enum Pending {
    Markets {
        context: Context,
        rows: Vec<Market>,
        invalid: bool,
    },
    Tickers {
        context: Context,
        rows: Vec<Ticker>,
        invalid: bool,
    },
    Bullet {
        channel: usize,
        generation: u64,
    },
    Balance(u64),
    Symbols {
        revision: u64,
    },
    Orders(ReadOrders),
    Write(trading::Job),
    Candles {
        context: Context,
        rows: Vec<Candle>,
        invalid: bool,
    },
    Fills(Box<HistoryRows<String>>),
    Account(CommandId),
}
impl Pending {
    fn discovery_context(&self) -> Option<&Context> {
        match self {
            Self::Markets { context, .. } | Self::Tickers { context, .. } => Some(context),
            _ => None,
        }
    }
}
pub struct ReadOrders {
    revision: u64,
    symbols: VecDeque<String>,
    symbol: String,
    page: u32,
    rows: SnapshotBuilder<String, Order>,
    /// Raw validated REST prefixes, never cached display replacements.
    seeds: Vec<(String, Cursor)>,
    lookup: Option<(CommandId, String)>,
}
struct Socket {
    socket: Option<SocketId>,
    open: bool,
    ready: bool,
    generation: u64,
    token_pending: bool,
    supervisor: Supervisor,
    ping_interval: f64,
    ping_timeout: f64,
    pong: Option<TimerId>,
}
impl Default for Socket {
    fn default() -> Self {
        Self {
            socket: None,
            open: false,
            ready: false,
            generation: 0,
            token_pending: false,
            supervisor: Supervisor::new(Config {
                auth_timeout_ms: Some(10_000.0),
                idle_timeout_ms: 90_000.0,
                ..Config::default()
            }),
            ping_interval: 18_000.0,
            ping_timeout: 10_000.0,
            pong: None,
        }
    }
}
#[derive(Default)]
struct SpotBook {
    book: Book,
    subscribers: u32,
    last: f64,
}
pub struct KucoinSpot {
    options: safe_cex_crypto::secrets::Sensitive<Options>,
    discovery: safe_cex_engine::startup::Backoff<bool>,
    credentials: Option<sign::Credentials>,
    sockets: [Socket; 2],
    books: BTreeMap<String, SpotBook>,
    candles: BTreeMap<(String, Interval), u32>,
    revision: u64,
    balance_revision: u64,
    orders_loading: bool,
    balance_loading: bool,
    balance_guard: StaleGuard,
    wallet_valuation_pending: bool,
    tickers_applied: bool,
    wallet_unquoted_warned: bool,
    /// `coin:`/`order:` + identity (empty when unreadable) of skips already logged.
    skip_logged: HashSet<String>,
    poll: Option<TimerId>,
    heal_orders: Option<TimerId>,
    heal_balance: Option<TimerId>,
    next_id: u64,
    terminal: HashSet<String>,
    terminal_order: VecDeque<String>,
    fills: HashSet<String>,
    fill_order: VecDeque<String>,
    execution_progress: BTreeMap<String, Cursor>,
    execution_order: VecDeque<String>,
}
impl KucoinSpot {
    pub fn new(options: Options) -> Self {
        let options = safe_cex_crypto::secrets::Sensitive::new(options);
        let credentials = (!options.public_data_only)
            .then(|| {
                options
                    .key
                    .as_ref()
                    .filter(|v| !v.is_empty())
                    .zip(options.secret.as_ref().filter(|v| !v.is_empty()))
                    .zip(options.passphrase.as_ref().filter(|v| !v.is_empty()))
                    .map(|((key, secret), passphrase)| sign::Credentials {
                        key: key.clone(),
                        secret: secret.clone(),
                        passphrase: passphrase.clone(),
                        broker: options.broker.clone(),
                    })
            })
            .flatten();
        Self {
            options,
            discovery: Default::default(),
            credentials,
            sockets: Default::default(),
            books: Default::default(),
            candles: Default::default(),
            revision: 0,
            balance_revision: 0,
            orders_loading: false,
            balance_loading: false,
            balance_guard: StaleGuard::default(),
            wallet_valuation_pending: false,
            tickers_applied: false,
            wallet_unquoted_warned: false,
            skip_logged: HashSet::new(),
            poll: None,
            heal_orders: None,
            heal_balance: None,
            next_id: 0,
            terminal: Default::default(),
            terminal_order: Default::default(),
            fills: Default::default(),
            fill_order: Default::default(),
            execution_progress: Default::default(),
            execution_order: Default::default(),
        }
    }
    fn discovery_read(&mut self, cx: &mut Cx<'_, Pending>, tickers: bool) {
        let context = match cx.http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) {
            Ok(context) => context,
            Err(error) => {
                self.discovery.failed(cx, tickers);
                cx.notice(
                    Level::Warn,
                    code::REQUEST_FAILED,
                    error.venue_error().message,
                );
                return;
            }
        };
        let (path, at, pending) = if tickers {
            (
                "/api/v1/market/allTickers",
                ArrayAt::Path(&["data", "ticker"]),
                Pending::Tickers {
                    context: context.clone(),
                    rows: vec![],
                    invalid: false,
                },
            )
        } else {
            (
                "/api/v2/symbols",
                ArrayAt::EnvelopeField("data"),
                Pending::Markets {
                    context: context.clone(),
                    rows: vec![],
                    invalid: false,
                },
            )
        };
        match cx.request_managed_with_headers(
            context,
            Method::Get,
            format!("{REST}{path}"),
            sign::PUBLIC_HEADERS,
            None,
            pending,
        ) {
            Ok(id) => cx.stream_response(id, at),
            Err((pending, error)) => {
                self.on_request_failed(cx, pending, &error.venue_error().message)
            }
        }
    }
    fn request(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        body: Option<String>,
        private: bool,
        pending: Pending,
    ) {
        let headers = if private {
            let read_credentials = self
                .options
                .read_credentials
                .as_ref()
                .filter(|_| method == Method::Get && self.credentials.is_some())
                .map(|read| sign::Credentials {
                    key: read.key.clone(),
                    secret: read.secret.clone(),
                    passphrase: read
                        .passphrase
                        .clone()
                        .or_else(|| self.options.passphrase.clone())
                        .unwrap_or_default(),
                    broker: self.options.broker.clone(),
                });
            read_credentials
                .as_ref()
                .or(self.credentials.as_ref())
                .map_or_else(sign::public_headers, |c| {
                    sign::headers(c, method, path, body.as_deref().unwrap_or(""), cx.now)
                })
        } else {
            sign::public_headers()
        };
        cx.request(
            method,
            format!("{REST}{}", encoded_path(path)),
            headers,
            body,
            pending,
        );
    }
    fn bullet(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        let c = &mut self.sockets[channel];
        if c.token_pending || c.socket.is_some() || c.supervisor.is_waiting() {
            return;
        }
        // Suppressed public feeds never open; bullets and supervision would only churn retries.
        if channel == 0 && cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return;
        }
        c.token_pending = true;
        c.generation = c.generation.wrapping_add(1);
        c.supervisor.set_ping_interval(cx, None);
        c.supervisor.connecting(cx);
        let generation = c.generation;
        self.request(
            cx,
            Method::Post,
            if channel == 1 {
                "/api/v1/bullet-private"
            } else {
                "/api/v1/bullet-public"
            },
            None,
            channel == 1,
            Pending::Bullet {
                channel,
                generation,
            },
        );
    }
    fn topic(&mut self, cx: &mut Cx<'_, Pending>, channel: usize, topic: &str, subscribe: bool) {
        if !self.sockets[channel].ready {
            return;
        }
        if let Some(socket) = self.sockets[channel].socket {
            self.next_id += 1;
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Subscription<'a> {
                id: u64,
                #[serde(rename = "type")]
                kind: &'a str,
                topic: &'a str,
                private_channel: bool,
                response: bool,
            }
            cx.send(
                socket,
                serde_json::to_string(&Subscription {
                    id: self.next_id,
                    kind: if subscribe {
                        "subscribe"
                    } else {
                        "unsubscribe"
                    },
                    topic,
                    private_channel: channel == 1,
                    response: true,
                })
                .unwrap(),
            );
        }
    }
    fn subscribe(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        if channel == 1 {
            for t in ["/spotMarket/tradeOrdersV2", "/account/balance"] {
                self.topic(cx, channel, t, true);
            }
        } else {
            self.topic(cx, 0, "/market/ticker:all", true);
            let topics = self
                .books
                .iter()
                .filter(|(_, b)| b.subscribers > 0)
                .map(|(s, _)| format!("/spotMarket/level2Depth50:{s}"))
                .chain(
                    self.candles
                        .keys()
                        .map(|(s, i)| format!("/market/candles:{s}_{}", interval(*i).unwrap().1)),
                )
                .collect::<Vec<_>>();
            for t in topics {
                self.topic(cx, 0, &t, true);
            }
        }
    }
    fn ping(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        let c = &mut self.sockets[channel];
        if !c.ready || c.pong.is_some() {
            return;
        }
        if let Some(socket) = c.socket {
            cx.send(
                socket,
                format!(r#"{{"id":{},"type":"ping"}}"#, cx.now as u64),
            );
            c.pong = Some(cx.set_timer(c.ping_timeout.max(1000.0)));
        }
    }
    fn reconnect(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        if channel == 1 {
            self.revision += 1;
            self.balance_revision += 1;
        }
        let c = &mut self.sockets[channel];
        c.ready = false;
        c.open = false;
        c.token_pending = false;
        c.generation = c.generation.wrapping_add(1);
        if let Some(timer) = c.pong.take() {
            cx.clear_timer(timer);
        }
        if let Some(socket) = c.socket.take() {
            cx.close(socket);
        }
        c.supervisor.retry(cx);
    }

    fn fixed_settings(&self, cx: &mut Cx<'_, Pending>) {
        let settings = cx
            .store
            .markets()
            .iter()
            .enumerate()
            .map(|(i, _)| {
                (
                    i as u32,
                    MarketSetting {
                        leverage: 1.0,
                        margin_type: Some(MarginType::Isolated),
                        buy_leverage: Some(1.0),
                        sell_leverage: Some(1.0),
                    },
                )
            })
            .collect::<Vec<_>>();
        cx.store.set_settings(settings);
        cx.store.set_hedged(false);
        cx.store.set_positions(vec![]);
    }
    fn balance(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.balance_loading || self.credentials.is_none() {
            return;
        }
        self.balance_loading = true;
        self.request(
            cx,
            Method::Get,
            "/api/v1/accounts?type=trade",
            None,
            true,
            Pending::Balance(self.balance_revision),
        );
    }
    fn orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.orders_loading || self.credentials.is_none() {
            return;
        }
        self.orders_loading = true;
        self.request(
            cx,
            Method::Get,
            "/api/v1/hf/orders/active/symbols",
            None,
            true,
            Pending::Symbols {
                revision: self.revision,
            },
        );
    }
    fn page(&self, cx: &mut Cx<'_, Pending>, r: ReadOrders) {
        self.request(
            cx,
            Method::Get,
            &format!(
                "/api/v1/hf/orders/active/page?symbol={}&pageNum={}&pageSize=50",
                r.symbol, r.page
            ),
            None,
            true,
            Pending::Orders(r),
        );
    }
    fn heal(&mut self, cx: &mut Cx<'_, Pending>, balance: bool) {
        let timer = if balance {
            &mut self.heal_balance
        } else {
            &mut self.heal_orders
        };
        if timer.is_none() {
            *timer = Some(cx.set_timer(250.0));
        }
    }
    fn account_balance(&mut self, cx: &mut Cx<'_, Pending>, data: &RawValue) -> Result<(), String> {
        let rows: Vec<&RawValue> = serde_json::from_str(data.get()).map_err(|e| e.to_string())?;
        let mut builder = SnapshotBuilder::default();
        for raw in rows {
            let row = serde_json::from_str::<Account>(raw.get()).ok().filter(|r| {
                !r.currency.is_empty()
                    && r.available.0 >= 0.0
                    && r.holds.0 >= 0.0
                    && r.balance.0 >= 0.0
            });
            let Some(r) = row else {
                // An unreadable currency row keeps its stored balance.
                let currency = serde_json::from_str::<RowIdentity>(raw.get())
                    .ok()
                    .map(|row| row.currency.to_owned())
                    .filter(|currency| !currency.is_empty());
                let message = match &currency {
                    Some(c) => format!("KuCoin Spot skipped malformed {c} balance row"),
                    None => "KuCoin Spot skipped balance row without a currency".into(),
                };
                self.log_skip(
                    cx,
                    format!("coin:{}", currency.as_deref().unwrap_or_default()),
                    message,
                );
                builder.reject(currency);
                continue;
            };
            builder.push(CurrencyBalance {
                currency: r.currency.into(),
                balance: Balance {
                    free: r.available.0,
                    used: r.holds.0,
                    total: r.balance.0,
                    upnl: 0.0,
                },
            });
        }
        let mapped: Vec<String> = builder
            .rows()
            .iter()
            .filter(|c| builder.allows_absence(&c.currency))
            .map(|c| c.currency.clone())
            .collect();
        self.rearm_skips("coin:", builder.rejected() == 0, mapped);
        // A retained stale row cannot be summed with fresh ones (a conversion would count
        // twice), so the USD wallet keeps its previous total until every row reads.
        let partial = builder.rejected() > 0;
        let stored = cx.store.currency_balances().unwrap_or_default().to_vec();
        let currencies = builder.commit(stored, |c| c.currency.clone());
        let mut sum = Balance::default();
        let mut coins = vec![];
        let mut unpriced = false;
        let mut unquoted = false;
        for r in &currencies {
            let currency = r.currency.as_str();
            let p = if matches!(currency, "USD" | "USDT" | "USDC") {
                1.0
            } else {
                let symbols = ["USDT", "USDC", "USD"].map(|q| format!("{currency}-{q}"));
                let price = symbols
                    .iter()
                    .filter_map(|symbol| cx.store.ticker(symbol))
                    .map(|t| t.last)
                    .find(|last| last.is_finite() && *last > 0.0);
                // Before the first ticker snapshot, a tradable USD route without a
                // quote is not evidence of zero value; afterwards it is valued at 0.
                if price.is_none()
                    && r.balance.total > 0.0
                    && symbols
                        .iter()
                        .any(|symbol| cx.store.market(symbol).is_some_and(|m| m.active))
                {
                    if self.tickers_applied {
                        unquoted = true;
                    } else {
                        unpriced = true;
                    }
                }
                price.unwrap_or(0.0)
            };
            coins.push(CoinBalance {
                base: currency.into(),
                amount: r.balance.total,
                notional: r.balance.total * p,
            });
            sum.free += r.balance.free * p;
            sum.used += r.balance.used * p;
            sum.total += r.balance.total * p;
        }
        cx.store.set_currency_balances(currencies);
        let newly_unpriced = unpriced && !self.wallet_valuation_pending;
        self.wallet_valuation_pending = unpriced;
        if unpriced {
            if newly_unpriced {
                cx.notice(Level::Warn, code::VALUATION_UNAVAILABLE,
                    "KuCoin Spot USD wallet valuation awaits a valid quote; retaining prior USD balance",
                );
            }
            return Ok(());
        }
        if unquoted && !self.wallet_unquoted_warned {
            cx.notice(
                Level::Warn,
                code::VALUATION_UNAVAILABLE,
                "KuCoin Spot wallet currency has no USD quote; valued at 0",
            );
        }
        self.wallet_unquoted_warned = unquoted;
        cx.store.set_coin_balances(coins);
        if !partial {
            cx.store.set_balance(sum);
        }
        Ok(())
    }
    /// Logs a skipped row once per `key` until it maps again; polls would otherwise repeat it.
    fn log_skip(&mut self, cx: &mut Cx<'_, Pending>, key: String, message: String) {
        if self.skip_logged.len() >= SKIP_LOG_LIMIT {
            self.skip_logged.clear();
        }
        if self.skip_logged.insert(key) {
            cx.notice(Level::Warn, code::ROW_SKIPPED, message);
        }
    }
    /// Re-arms the skip logs of `prefix`: all after a clean read, else those `ids` mapped.
    fn rearm_skips(&mut self, prefix: &str, clean: bool, ids: Vec<String>) {
        if clean {
            self.skip_logged.retain(|key| !key.starts_with(prefix));
        } else {
            for id in ids {
                self.skip_logged.remove(&format!("{prefix}{id}"));
            }
        }
    }
    fn reject_order(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        rows: &mut SnapshotBuilder<String, Order>,
        id: &str,
    ) {
        if id.is_empty() {
            let message = "KuCoin Spot skipped active-order row without an id".to_owned();
            self.log_skip(cx, "order:".into(), message);
            rows.reject(None);
        } else {
            let message = format!("KuCoin Spot skipped unmappable active order {id}");
            self.log_skip(cx, format!("order:{id}"), message);
            rows.reject(Some(id.to_owned()));
        }
    }
    /// Native retention only; validation and monotonic progress come from the engine.
    fn remember_execution(&mut self, id: String, cursor: Cursor) {
        if !self.execution_progress.contains_key(&id) {
            self.execution_order.push_back(id.clone());
            if self.execution_order.len() > EXECUTION_PROGRESS_LIMIT
                && let Some(old) = self.execution_order.pop_front()
            {
                self.execution_progress.remove(&old);
            }
        }
        self.execution_progress.insert(id, cursor);
    }
    fn private_message(&mut self, cx: &mut Cx<'_, Pending>, m: Message<'_>) {
        if m.topic.starts_with("/account/balance") {
            self.balance_revision += 1;
            self.heal(cx, true);
            return;
        }
        if !m.topic.starts_with("/spotMarket/tradeOrdersV2") {
            return;
        }
        self.revision += 1;
        self.heal(cx, false);
        let Some(row) = m
            .data
            .and_then(|d| serde_json::from_str::<OrderRow>(d.get()).ok())
        else {
            return;
        };
        if row.id().is_empty() {
            return;
        }
        let previous = cx.store.order(row.id());
        if !matches!(row.side, "" | "buy" | "sell")
            || previous.is_some_and(|o| !row.symbol.is_empty() && row.symbol != o.symbol)
            || (row.event == "match" && row.side.is_empty() && previous.is_none())
        {
            return;
        }
        let Some(mut order) = row.order(cx.now, previous, true) else {
            return;
        };
        if !valid_native_quantities(&row) || order.price < 0.0 {
            return;
        }
        if order.status == OrderStatus::Open
            && !self.terminal.contains(row.id())
            && previous.is_none_or(|old| order.last_update >= old.last_update)
            && (!valid_order(&order) || previous.is_some_and(|old| old.filled > order.amount))
        {
            return;
        }
        if row.event == "match" {
            let amount = row.match_size.or(row.filled_size).map_or(0.0, |n| n.0);
            let price = row.match_price.or(row.price).map_or(0.0, |n| n.0);
            if amount <= 0.0
                || price <= 0.0
                || row.total().is_some_and(|total| exceeds(amount, total))
                || row
                    .cumulative()
                    .is_some_and(|filled| exceeds(amount, filled))
            {
                return;
            }
            let key = if let Some(id) = row.trade_id {
                format!("{}:{id}", row.id())
            } else {
                format!(
                    "{}:{}:{amount}:{price}",
                    row.id(),
                    row.ts.map_or(order.last_update, |n| n.0)
                )
            };
            let cursor = self
                .execution_progress
                .get(row.id())
                .copied()
                .unwrap_or_default();
            let (cursor, report) = if row.match_size.is_some() {
                // No native endpoint: validate the authoritative last independently
                // of retained progress, and never install its synthetic next value.
                let cursor = if row.cumulative().is_some() {
                    cursor
                } else {
                    Cursor::default()
                };
                (
                    cursor,
                    Report::Last {
                        amount,
                        cumulative: row.cumulative(),
                        price,
                    },
                )
            } else {
                (
                    cursor,
                    Report::Cumulative {
                        quantity: amount,
                        price: CumulativePrice::Direct(price),
                    },
                )
            };
            let delivery = match cursor.prepare(report, Arithmetic::Binary) {
                Ok(delivery) => delivery,
                Err(Rejection::QuantityRegressed) => None,
                Err(_) => return,
            };
            if let Some(delivery) = delivery
                && self.fills.insert(key.clone())
            {
                self.fill_order.push_back(key);
                if self.fill_order.len() > 20_000
                    && let Some(k) = self.fill_order.pop_front()
                {
                    self.fills.remove(&k);
                }
                cx.fill(Fill {
                    id: None,
                    order_id: order.id.clone(),
                    symbol: order.symbol.clone(),
                    side: order.side,
                    price: delivery.price(),
                    amount: delivery.amount(),
                });
                if row.match_size.is_none() || row.cumulative().is_some() {
                    self.remember_execution(row.id().to_owned(), delivery.next());
                }
            }
        }
        let old = cx.store.order(row.id());
        if old.is_some_and(|o| order.last_update < o.last_update) {
            return;
        }
        if self.terminal.contains(row.id()) && order.status == OrderStatus::Open {
            return;
        }
        if order.status == OrderStatus::Open && !valid_order(&order) {
            return;
        }
        if let Some(old) = old {
            order.filled = order.filled.max(old.filled);
            order.remaining = order.remaining.min((order.amount - order.filled).max(0.0));
        }
        if order.status == OrderStatus::Open && !valid_order(&order) {
            return;
        }
        if matches!(order.status, OrderStatus::Canceled | OrderStatus::Closed) {
            // Terminal identities fence snapshots started before a fill/cancel.
            if self.terminal.insert(order.id.clone()) {
                self.terminal_order.push_back(order.id.clone());
                if self.terminal_order.len() > 20_000
                    && let Some(id) = self.terminal_order.pop_front()
                {
                    self.terminal.remove(&id);
                }
            }
            cx.store.remove_orders(&[&order.id]);
        } else {
            cx.store.upsert_order(order);
        }
    }
    fn price(cx: &mut Cx<'_, Pending>, topic: &str, subject: &str, p: Price<'_>) {
        let symbol = if !p.symbol.is_empty() {
            p.symbol
        } else if !subject.is_empty() {
            subject
        } else {
            topic.split_once(':').map_or("", |(_, s)| s)
        };
        cx.store.update_ticker(symbol, |t| p.apply(t));
    }
    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, m: Message<'_>) {
        let Some(data) = m.data else {
            return;
        };
        if m.topic.starts_with("/market/ticker") {
            if let Ok(p) = serde_json::from_str::<Price>(data.get()) {
                Self::price(cx, m.topic, m.subject, p);
            }
        } else if let Some(symbol) = m.topic.strip_prefix("/spotMarket/level2Depth50:") {
            #[derive(Deserialize)]
            struct Depth {
                bids: Vec<(Number, Number)>,
                asks: Vec<(Number, Number)>,
                #[serde(default)]
                timestamp: Option<Number>,
            }
            if let Some(b) = self.books.get_mut(symbol).filter(|b| b.subscribers > 0)
                && let Ok(d) = serde_json::from_str::<Depth>(data.get())
            {
                let time = d.timestamp.map_or(cx.now, |n| n.0);
                if time < b.last {
                    return;
                }
                b.last = time;
                b.book.clear();
                for (side, rows) in [(BookSide::Bids, d.bids), (BookSide::Asks, d.asks)] {
                    for (p, a) in rows {
                        b.book.apply(side, p.0, a.0);
                    }
                }
                cx.book_changed(symbol);
            }
        } else if let Some(topic) = m.topic.strip_prefix("/market/candles:")
            && let Some((symbol, native)) = topic.rsplit_once('_')
        {
            let Some(i) = self
                .candles
                .keys()
                .find(|(s, i)| s == symbol && interval(*i).is_ok_and(|(_, v)| v == native))
                .map(|(_, i)| *i)
            else {
                return;
            };
            #[derive(Deserialize)]
            struct Update<'a> {
                #[serde(borrow)]
                candles: &'a RawValue,
            }
            if let Ok(u) = serde_json::from_str::<Update>(data.get())
                && let Ok(c) = candle(u.candles.get())
            {
                cx.candle(symbol, i, c);
            }
        }
    }
}
fn exceeds(value: f64, bound: f64) -> bool {
    value > bound + (bound.abs() * 1e-12).max(1e-12)
}
fn valid_native_quantities(row: &OrderRow<'_>) -> bool {
    let (total, filled, remaining) = (row.total(), row.cumulative(), row.remain_size.map(|n| n.0));
    if [total, filled, remaining]
        .into_iter()
        .flatten()
        .any(|n| n < 0.0)
    {
        return false;
    }
    match total {
        Some(total) => {
            !filled.is_some_and(|filled| exceeds(filled, total))
                && !remaining.is_some_and(|remaining| exceeds(remaining, total))
                && !filled
                    .zip(remaining)
                    .is_some_and(|(filled, remaining)| exceeds(filled + remaining, total))
        }
        None => true,
    }
}
fn valid_order(o: &Order) -> bool {
    o.amount >= 0.0
        && o.filled >= 0.0
        && o.filled <= o.amount
        && o.remaining >= 0.0
        && o.remaining <= o.amount
        && o.price >= 0.0
        && o.filled + o.remaining <= o.amount + (o.amount * 1e-12).max(1e-12)
}
fn lookup_output(symbol: &str, id: &str, rows: &[Order]) -> String {
    #[derive(Serialize)]
    struct Lookup<'a> {
        found: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        order: Option<&'a Order>,
        #[serde(skip_serializing_if = "Option::is_none")]
        definitive: Option<bool>,
    }
    let order = rows.iter().find(|o| {
        o.symbol == symbol
            && !matches!(id, "" | "0" | "undefined")
            && (o.id == id || o.client_order_id.as_deref() == Some(id))
    });
    serde_json::to_string(&Lookup {
        found: order.is_some(),
        order,
        definitive: order.is_none().then_some(false),
    })
    .unwrap()
}
fn candle(text: &str) -> Result<Candle, String> {
    let r: Vec<Number> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if r.len() < 6 {
        return Err("invalid KuCoin Spot candle".into());
    }
    Ok(Candle {
        timestamp: if r[0].0 > 1e10 {
            (r[0].0 / 1000.0).floor()
        } else {
            r[0].0
        },
        open: r[1].0,
        close: r[2].0,
        high: r[3].0,
        low: r[4].0,
        volume: r[5].0,
    })
}

impl Venue for KucoinSpot {
    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            client_order_lookup: true,
            order_history: false,
            income_history: false,
            cancel_all: safe_cex_engine::CancelAll::Native,
            account_margin_type: true,
            cancel_with_symbols: true,
            amend_with_symbol: true,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read
                .validate()
                .and_then(|_| read.validate_passphrase(self.options.passphrase.as_deref()))
        {
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            return;
        }
        if self.options.testnet || (!self.options.public_data_only && self.credentials.is_none()) {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "KuCoin Spot requires production key, secret and passphrase",
            );
            return;
        }
        self.discovery_read(cx, false);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if let Some(channel) = self.sockets.iter().position(|c| c.socket == Some(socket)) {
            self.sockets[channel].open = true;
            self.sockets[channel].supervisor.opened(cx);
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(channel) = self
            .sockets
            .iter()
            .position(|c| c.socket == Some(socket) && c.open)
        else {
            return;
        };
        self.sockets[channel].supervisor.received(cx.now);
        // Decode high-volume ticker envelopes and prices together, without a raw-data rescan.
        if channel == 0
            && self.sockets[channel].ready
            && let Ok(m) = serde_json::from_str::<PriceMessage>(text)
            && m.kind == "message"
            && m.topic.starts_with("/market/ticker")
        {
            Self::price(cx, m.topic, m.subject, m.data);
            return;
        }
        let Ok(m) = serde_json::from_str::<Message>(text) else {
            return;
        };
        match m.kind {
            "welcome" => {
                if self.sockets[channel].ready {
                    return;
                }
                let c = &mut self.sockets[channel];
                c.ready = true;
                c.supervisor.authenticated(cx);
                c.supervisor.set_ping_interval(cx, Some(c.ping_interval));
                self.subscribe(cx, channel);
                self.ping(cx, channel);
                if channel == 1 {
                    self.balance(cx);
                    self.orders(cx);
                }
            }
            "pong" => {
                let c = &mut self.sockets[channel];
                // A pong acknowledges a sent ping, never the welcome watchdog.
                if !c.ready || c.pong.is_none() {
                    return;
                }
                if let Some(t) = c.pong.take() {
                    cx.clear_timer(t);
                }
                c.supervisor.set_ping_interval(cx, Some(c.ping_interval));
            }
            "message" if self.sockets[channel].ready => {
                if channel == 1 {
                    self.private_message(cx, m);
                } else {
                    self.public_message(cx, m);
                }
            }
            _ => {}
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if let Some(c) = self.sockets.iter().position(|c| c.socket == Some(socket)) {
            self.sockets[c].socket = None;
            self.reconnect(cx, c);
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        if let Pending::Candles { context, .. } = pending
            && cx.validate_read_http_delivery(context, context).is_err()
        {
            return;
        }
        if pending
            .discovery_context()
            .is_some_and(|context| cx.validate_http_context(context).is_err())
        {
            return;
        }
        match pending {
            Pending::Markets { rows, .. } => {
                match serde_json::from_str::<Instrument>(item).map(Instrument::market) {
                    Ok(market) if market.precision.is_tradable() => rows.push(market),
                    _ => safe_cex_engine::startup::skipped_row(cx),
                }
            }
            Pending::Tickers { rows, .. } => match serde_json::from_str::<Price>(item) {
                Ok(r) => rows.push(r.ticker()),
                Err(_) => safe_cex_engine::startup::skipped_row(cx),
            },
            Pending::Candles { rows, invalid, .. } => match candle(item) {
                Ok(c) => rows.push(c),
                Err(_) => *invalid = true,
            },
            _ => {}
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Candles { context, .. } = &pending
            && cx.validate_read_http_delivery(context, context).is_err()
        {
            return;
        }
        if let Some(context) = pending.discovery_context() {
            if cx.validate_http_context(context).is_err() {
                let _ = cx.finish_http_context(context);
                return;
            }
            if cx.finish_http_context(context).is_err() {
                return;
            }
        }
        if let Pending::Bullet {
            channel,
            generation,
        } = &pending
            && (!self.sockets[*channel].token_pending
                || self.sockets[*channel].generation != *generation)
        {
            return;
        }
        let result = success(response.status, response.body)
            .and_then(|e| e.data.ok_or_else(|| "KuCoin Spot missing data".into()));
        let data = match result {
            Ok(d) => d,
            Err(e) => {
                if let Pending::Write(job) = pending {
                    cx.command_error(job.command, &e);
                    let error = crate::wire::write_failure(response.status, response.body, e);
                    self.write_failed(cx, job, error);
                } else {
                    self.on_request_failed(cx, pending, &e);
                }
                return;
            }
        };
        match pending {
            Pending::Markets { rows, invalid, .. } => {
                if invalid || rows.is_empty() {
                    self.discovery.failed(cx, false);
                    return;
                }
                self.discovery.succeeded(cx, &false);
                cx.store.set_markets(rows);
                self.fixed_settings(cx);
                self.discovery_read(cx, true);
                if self.credentials.is_some() {
                    self.balance(cx);
                    self.orders(cx);
                    self.poll = Some(cx.set_poll_timer(5000.0));
                } else {
                    cx.store.set_orders(vec![]);
                    cx.store.set_balance(Balance::default());
                }
            }
            Pending::Tickers {
                mut rows, invalid, ..
            } => {
                rows.retain(|r| cx.store.market(&r.symbol).is_some());
                if invalid || rows.is_empty() {
                    self.discovery.failed(cx, true);
                    return;
                }
                self.discovery.succeeded(cx, &true);
                cx.store.set_tickers(rows);
                self.tickers_applied = true;
                if self.wallet_valuation_pending {
                    self.balance(cx);
                }
                self.bullet(cx, 0);
                if self.credentials.is_some() {
                    self.bullet(cx, 1);
                }
            }
            Pending::Bullet {
                channel,
                generation,
            } => {
                self.sockets[channel].token_pending = false;
                match serde_json::from_str::<Bullet>(data.get()) {
                    Ok(t)
                        if !t.token.is_empty()
                            && t.instance_servers.first().is_some_and(|s| {
                                s.endpoint.starts_with("wss://")
                                    && s.ping_interval.is_finite()
                                    && s.ping_interval >= 1000.0
                                    && s.ping_timeout.is_finite()
                                    && s.ping_timeout >= 1000.0
                            }) =>
                    {
                        let server = &t.instance_servers[0];
                        let c = &mut self.sockets[channel];
                        c.ping_interval = server.ping_interval;
                        c.ping_timeout = server.ping_timeout;
                        c.supervisor.set_auth_timeout(cx, Some(server.ping_timeout));
                        c.supervisor.connecting(cx);
                        let url = format!(
                            "{}{}token={}&connectId={}-{}-{generation}",
                            server.endpoint,
                            if server.endpoint.contains('?') {
                                "&"
                            } else {
                                "?"
                            },
                            crate::encode(t.token),
                            cx.now as u64,
                            channel,
                        );
                        c.socket = Some(cx.connect(safe_cex_engine::SocketSpec::new(url).channel(
                            if channel == 1 {
                                SocketChannel::Private
                            } else {
                                SocketChannel::Public
                            },
                        )));
                    }
                    _ => {
                        cx.notice(
                            Level::Error,
                            code::STREAM_UNAVAILABLE,
                            "invalid KuCoin Spot bullet server",
                        );
                        self.reconnect(cx, channel);
                    }
                }
            }
            Pending::Balance(revision) => {
                self.balance_loading = false;
                let current = revision == self.balance_revision;
                if self.balance_guard.admit(current, STALE_SNAPSHOT_LIMIT)
                    && let Err(e) = self.account_balance(cx, data)
                {
                    cx.retried_failure("kucoin.spot.mod:1001", e);
                }
                if !current {
                    self.heal(cx, true);
                }
            }
            Pending::Symbols { revision } => {
                let symbols = match serde_json::from_str::<Symbols>(data.get()) {
                    Ok(s) => s
                        .symbols
                        .into_iter()
                        .map(str::to_owned)
                        .collect::<VecDeque<_>>(),
                    Err(e) => {
                        self.orders_loading = false;
                        cx.retried_failure("kucoin.spot.mod:1016", e.to_string());
                        return;
                    }
                };
                let mut r = ReadOrders {
                    revision,
                    symbols,
                    symbol: String::new(),
                    page: 1,
                    rows: SnapshotBuilder::default(),
                    seeds: vec![],
                    lookup: None,
                };
                if let Some(symbol) = r.symbols.pop_front() {
                    r.symbol = symbol;
                    self.page(cx, r);
                } else {
                    self.orders_loading = false;
                    if revision == self.revision {
                        cx.store.set_orders(vec![]);
                    } else {
                        self.heal(cx, false);
                    }
                }
            }
            Pending::Orders(mut r) => {
                let p = match serde_json::from_str::<Page>(data.get()) {
                    Ok(p) => p,
                    Err(e) => {
                        self.on_request_failed(cx, Pending::Orders(r), &e.to_string());
                        return;
                    }
                };
                let count = p.items.len();
                for raw in p.items {
                    let Ok(row) = serde_json::from_str::<OrderRow>(raw.get()) else {
                        let id = serde_json::from_str::<RowIdentity>(raw.get()).map_or("", |row| {
                            if row.id.is_empty() {
                                row.order_id
                            } else {
                                row.id
                            }
                        });
                        self.reject_order(cx, &mut r.rows, id);
                        continue;
                    };
                    if row.id().is_empty()
                        || row.symbol.is_empty()
                        || !matches!(row.side, "buy" | "sell")
                        || !matches!(row.event, "limit" | "market")
                        || row.size.or(row.origin_size).is_none()
                    {
                        self.reject_order(cx, &mut r.rows, row.id());
                        continue;
                    }
                    let Some(mut o) = row.order(cx.now, None, false) else {
                        self.reject_order(cx, &mut r.rows, row.id());
                        continue;
                    };
                    if o.id.is_empty()
                        || !valid_order(&o)
                        || (o.kind == OrderType::Limit && row.price.is_none())
                    {
                        self.reject_order(cx, &mut r.rows, row.id());
                        continue;
                    }
                    if o.symbol != r.symbol
                        || r.rows.rows().len() >= 20_000
                        || r.rows.rows().iter().any(|row| row.id == o.id)
                    {
                        self.on_request_failed(
                            cx,
                            Pending::Orders(r),
                            "invalid or oversized KuCoin Spot active-order snapshot",
                        );
                        return;
                    }
                    let seed = Cursor::seed(o.filled, None).expect("validated REST quantity");
                    if let Some(current) = cx.store.order(&o.id) {
                        if o.last_update < current.last_update || o.amount < current.filled {
                            o = current.clone();
                        } else {
                            o.filled = o.filled.max(current.filled);
                            o.remaining = o.remaining.min((o.amount - o.filled).max(0.0));
                        }
                    }
                    if !self.terminal.contains(&o.id) && o.status == OrderStatus::Open {
                        r.seeds.push((o.id.clone(), seed));
                        r.rows.push(o);
                    }
                }
                let more = p.total_page.map_or(count >= 50, |n| r.page < n);
                if more {
                    if r.page >= 100 {
                        self.on_request_failed(
                            cx,
                            Pending::Orders(r),
                            "KuCoin Spot orders exceeded 100 pages",
                        );
                        return;
                    }
                    r.page += 1;
                    self.page(cx, r);
                } else if let Some(symbol) = r.symbols.pop_front() {
                    r.symbol = symbol;
                    r.page = 1;
                    self.page(cx, r);
                } else {
                    let mapped: Vec<String> = r
                        .rows
                        .rows()
                        .iter()
                        .filter(|o| r.rows.allows_absence(&o.id))
                        .map(|o| o.id.clone())
                        .collect();
                    let clean = r.rows.rejected() == 0;
                    // Rejected rows keep their stored copies.
                    let rows = r
                        .rows
                        .commit(cx.store.orders().iter().cloned(), |o| o.id.clone());
                    if let Some((command, id)) = r.lookup {
                        cx.done(command, lookup_output(&r.symbol, &id, &rows));
                    } else {
                        self.orders_loading = false;
                        if r.revision == self.revision {
                            for (id, seed) in r.seeds {
                                if !self.execution_progress.contains_key(&id) {
                                    self.remember_execution(id, seed);
                                }
                            }
                            self.rearm_skips("order:", clean, mapped);
                            cx.store.set_orders(rows);
                        } else {
                            self.heal(cx, false);
                        }
                    }
                }
            }
            Pending::Write(job) => self.write_response(cx, job, data),
            Pending::Candles {
                context,
                mut rows,
                invalid,
            } => {
                if invalid {
                    let _ =
                        cx.fail_context(&context, VenueError::new("invalid KuCoin Spot candles"));
                } else {
                    rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                    let _ = cx.done_context(&context, serde_json::to_string(&rows).unwrap());
                }
            }
            Pending::Account(command) => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Summary<'a> {
                    #[serde(borrow)]
                    user_id: Option<&'a RawValue>,
                    #[serde(borrow)]
                    uid: Option<&'a RawValue>,
                    #[serde(borrow)]
                    sub_user_id: Option<&'a RawValue>,
                    #[serde(borrow)]
                    referrer_user_id: Option<&'a RawValue>,
                }
                let result = serde_json::from_str::<Summary>(data.get())
                    .map(|s| {
                        fn present(v: Option<&RawValue>) -> Option<&str> {
                            v.map(RawValue::get)
                                .filter(|s| !matches!(*s, "null" | "0" | "false" | "\"\""))
                        }
                        format!(
                            r#"{{"userId":{},"subId":{},"affiliateId":{}}}"#,
                            present(s.user_id)
                                .or_else(|| present(s.uid))
                                .unwrap_or("\"\""),
                            present(s.sub_user_id).unwrap_or("null"),
                            present(s.referrer_user_id).unwrap_or("null")
                        )
                    })
                    .map_err(|e| e.to_string());
                match result {
                    Ok(data) => cx.done(command, data),
                    Err(error) => cx.fail(command, error),
                };
            }
            Pending::Fills(mut owner) => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Execution<'a> {
                    timestamp: f64,
                    symbol: &'a str,
                    order_id: &'a str,
                    price: f64,
                    qty: f64,
                    side: &'a str,
                    commission: f64,
                    commission_asset: &'a str,
                }
                let out = (|| {
                    let p: Page = serde_json::from_str(data.get()).map_err(|e| e.to_string())?;
                    let mut rows = vec![];
                    for raw in p.items {
                        let r: FillRow =
                            serde_json::from_str(raw.get()).map_err(|e| e.to_string())?;
                        rows.push(Execution {
                            timestamp: r.created_at.0,
                            symbol: r.symbol,
                            order_id: r.order_id,
                            price: r.price.0,
                            qty: r.size.0,
                            side: if r.side == "sell" { "sell" } else { "buy" },
                            commission: r.fee.0,
                            commission_asset: r.fee_currency,
                        });
                    }
                    rows.iter()
                        .map(|row| serde_json::to_string(row).map_err(|e| e.to_string()))
                        .collect::<Result<Vec<_>, _>>()
                })();
                match out {
                    Ok(rows) => {
                        for row in rows {
                            owner.stage(row);
                        }
                        owner.accept_page();
                        owner.finish(cx, Ok(()));
                    }
                    Err(error) => owner.finish(cx, Err(VenueError::new(error))),
                }
            }
        }
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.on_http_failed(cx, pending, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending {
            Pending::Candles { context, .. } => {
                if cx.command_error_context(&context, &error.message).is_ok() {
                    let _ = cx.fail_context(&context, error);
                }
            }
            other => self.on_request_failed(cx, other, &error.message),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Some(context) = pending.discovery_context() {
            // Engine-admitted failures may retire after the original deadline.
            let _ = cx.finish_http_context(context);
        }
        let discovery = matches!(&pending, Pending::Markets { .. } | Pending::Tickers { .. });
        match pending {
            Pending::Bullet {
                channel,
                generation,
            } => {
                if !self.sockets[channel].token_pending
                    || self.sockets[channel].generation != generation
                {
                    return;
                }
                self.reconnect(cx, channel);
            }
            Pending::Balance(_) => self.balance_loading = false,
            Pending::Symbols { .. } => self.orders_loading = false,
            Pending::Orders(r) => {
                if let Some((command, id)) = r.lookup {
                    cx.command_error(command, error);
                    let out = lookup_output(&r.symbol, &id, cx.store.orders());
                    cx.done(command, out);
                    return;
                } else {
                    self.orders_loading = false;
                }
            }
            Pending::Write(j) => {
                cx.command_error(j.command, error);
                self.write_failed(cx, j, crate::wire::write_failure(0, "", error.into()));
                return;
            }
            Pending::Fills(owner) => {
                cx.command_error(owner.command(), error);
                owner.finish(cx, Err(VenueError::new(error)));
                return;
            }
            pending @ Pending::Candles { .. } => {
                self.on_http_failed(cx, pending, VenueError::new(error));
                return;
            }
            Pending::Account(command) => {
                cx.command_error(command, error);
                cx.fail(command, error);
                return;
            }
            Pending::Markets { .. } => self.discovery.failed(cx, false),
            Pending::Tickers { .. } => self.discovery.failed(cx, true),
        }
        let level = if discovery { Level::Warn } else { Level::Error };
        cx.notice(level, code::REQUEST_FAILED, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(tickers) = self.discovery.on_timer(timer) {
            self.discovery_read(cx, tickers);
            return;
        }
        for channel in 0..2 {
            match self.sockets[channel].supervisor.on_timer(cx, timer) {
                Tick::Reconnect => {
                    self.bullet(cx, channel);
                    return;
                }
                Tick::TimedOut(_) => {
                    self.reconnect(cx, channel);
                    return;
                }
                Tick::Ping => {
                    self.ping(cx, channel);
                    return;
                }
                Tick::Handled => return,
                Tick::Unhandled => (),
            }
            if self.sockets[channel].pong == Some(timer) {
                self.sockets[channel].pong = None;
                self.reconnect(cx, channel);
                return;
            }
        }
        if self.heal_orders == Some(timer) {
            self.heal_orders = None;
            self.orders(cx);
        } else if self.heal_balance == Some(timer) {
            self.heal_balance = None;
            self.balance(cx);
        } else if self.poll == Some(timer) {
            self.orders(cx);
            self.balance(cx);
            self.poll = Some(cx.set_poll_timer(5000.0));
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => {
                let b = self.books.entry(symbol.clone()).or_default();
                b.subscribers += 1;
                if b.subscribers == 1 {
                    self.topic(cx, 0, &format!("/spotMarket/level2Depth50:{symbol}"), true);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(b) = self.books.get_mut(&symbol) {
                    b.subscribers = b.subscribers.saturating_sub(1);
                    if b.subscribers == 0 {
                        self.topic(cx, 0, &format!("/spotMarket/level2Depth50:{symbol}"), false);
                        self.books.remove(&symbol);
                    }
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: i,
            } => {
                if interval(i).is_err() {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        i.unsupported("KuCoin Spot"),
                    );
                    return;
                }
                let n = self.candles.entry((symbol.clone(), i)).or_default();
                *n += 1;
                if *n == 1 {
                    self.topic(
                        cx,
                        0,
                        &format!("/market/candles:{symbol}_{}", interval(i).unwrap().1),
                        true,
                    );
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: i,
            } => {
                if let Some(n) = self.candles.get_mut(&(symbol.clone(), i)) {
                    *n = n.saturating_sub(1);
                    if *n == 0 {
                        self.candles.remove(&(symbol.clone(), i));
                        self.topic(
                            cx,
                            0,
                            &format!("/market/candles:{symbol}_{}", interval(i).unwrap().1),
                            false,
                        );
                    }
                }
            }
            Command::SetTickerSymbols { .. } => {}
            Command::FetchCandles {
                command: _,
                symbol,
                interval: i,
                limit,
                from_ms,
                to_ms,
            } => {
                let context = match cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) {
                    Ok(context) => context,
                    Err(_) => return,
                };
                let Ok((minutes, native)) = interval(i) else {
                    let _ = cx.fail_context(
                        &context,
                        VenueError::new(i.unsupported("KuCoin Spot"))
                            .kind(safe_cex_engine::ErrorKind::Unsupported)
                            .outcome(safe_cex_engine::Outcome::NotSent),
                    );
                    return;
                };
                let end = to_ms.unwrap_or(cx.now);
                let start = match (limit, from_ms) {
                    (None, Some(from)) => from,
                    _ => {
                        end - f64::from(minutes)
                            * 60_000.0
                            * f64::from(limit.filter(|n| *n > 0).unwrap_or(500).min(1500))
                    }
                };
                let path = format!(
                    "/api/v1/market/candles?symbol={symbol}&type={native}&startAt={}&endAt={}",
                    (start / 1000.0).floor() as u64,
                    (end / 1000.0).floor() as u64
                );
                match cx.request_managed_with_headers(
                    context.clone(),
                    Method::Get,
                    format!("{REST}{path}"),
                    sign::PUBLIC_HEADERS,
                    None,
                    Pending::Candles {
                        context,
                        rows: vec![],
                        invalid: false,
                    },
                ) {
                    Ok(id) => cx.stream_response(id, ArrayAt::EnvelopeField("data")),
                    Err((pending, error)) => self.on_sign_error(cx, pending, error.venue_error()),
                }
            }
            Command::Query { command, query } => self.query(cx, command, query),
            other => self.trade(cx, other),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|b| b.subscribers > 0)
            .map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, b)| b.subscribers > 0)
                .map(|(s, b)| (s.as_str(), &b.book)),
        )
    }
}
impl KucoinSpot {
    fn query(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, query: Query) {
        if self.credentials.is_none() {
            let Ok(context) = cx.admit_http_command(crate::local_read_command_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                crate::wire::not_sent("KuCoin Spot requires credentials")
                    .kind(safe_cex_engine::ErrorKind::InvalidInput),
            );
            return;
        }
        match query {
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => {
                let Some(symbol) = symbol else {
                    let Ok(context) = cx.admit_http_command(crate::local_read_command_policy())
                    else {
                        return;
                    };
                    let _ = cx.done_context(&context, "[]".into());
                    return;
                };
                let mut path = format!("/api/v1/hf/fills?symbol={symbol}");
                if let Some(o) = order_id {
                    path.push_str(&format!("&orderId={o}"));
                }
                path.push_str(&format!(
                    "&limit={}",
                    limit.filter(|n| *n > 0).unwrap_or(100).min(100)
                ));
                if let Some(n) = from_ms {
                    path.push_str(&format!("&startAt={}", n as u64));
                }
                if let Some(n) = to_ms {
                    path.push_str(&format!("&endAt={}", n as u64));
                }
                self.request(
                    cx,
                    Method::Get,
                    &path,
                    None,
                    true,
                    Pending::Fills(Box::new(HistoryRows::new(
                        Completion::new(command),
                        |rows| Ok(format!("[{}]", rows.join(","))),
                    ))),
                );
            }
            Query::OrderByClientId {
                symbol,
                client_order_id,
            } => {
                if let Some(o) = cx.store.orders().iter().find(|o| {
                    o.symbol == symbol
                        && !matches!(client_order_id.as_str(), "" | "0" | "undefined")
                        && (o.client_order_id.as_deref() == Some(&client_order_id)
                            || o.id == client_order_id)
                }) {
                    #[derive(Serialize)]
                    struct Found<'a> {
                        found: bool,
                        order: &'a Order,
                    }
                    let result = serde_json::to_string(&Found {
                        found: true,
                        order: o,
                    })
                    .unwrap();
                    let Ok(context) = cx.admit_http_command(crate::local_read_command_policy())
                    else {
                        return;
                    };
                    if cx.validate_read_root_context(&context).is_err() {
                        return;
                    }
                    let _ = cx.done_context(&context, result);
                } else {
                    self.page(
                        cx,
                        ReadOrders {
                            revision: self.revision,
                            symbols: VecDeque::new(),
                            symbol,
                            page: 1,
                            rows: SnapshotBuilder::default(),
                            seeds: vec![],
                            lookup: Some((command, client_order_id)),
                        },
                    );
                }
            }
            Query::PublicTrades { .. } => {
                let Ok(context) = cx.admit_http_command(crate::local_read_command_policy()) else {
                    return;
                };
                let _ = cx.fail_context(
                    &context,
                    crate::wire::not_sent("KuCoin Spot public trades are not implemented in TS")
                        .kind(safe_cex_engine::ErrorKind::Unsupported),
                );
            }
            Query::OrderHistory { .. } | Query::Income { .. } => {
                let Ok(context) = cx.admit_http_command(crate::local_read_command_policy()) else {
                    return;
                };
                let _ = cx.done_context(&context, "[]".into());
            }
        }
    }
}

#[cfg(test)]
mod security_credentials {
    use super::*;
    use safe_cex_crypto::secrets::EraseSecrets;
    #[test]
    fn spot_owns_both_credential_copies_in_scrubbing_carriers() {
        let mut venue = KucoinSpot::new(Options {
            key: Some("private-key".into()),
            secret: Some("private-secret".into()),
            passphrase: Some("private-passphrase".into()),
            broker: Some(sign::BrokerOptions {
                partner: "partner".into(),
                broker_key: "private-broker-key".into(),
                broker_name: "broker".into(),
                verify: None,
            }),
            ..Options::default()
        });
        let _: &safe_cex_crypto::secrets::Sensitive<Options> = &venue.options;
        assert!(venue.credentials.is_some());
        venue.options.erase_secrets();
        venue.credentials.as_mut().unwrap().erase_secrets();
        assert!(venue.options.secret.is_none());
        assert!(venue.options.broker.as_ref().unwrap().broker_key.is_empty());
        let credentials = venue.credentials.as_ref().unwrap();
        assert!(credentials.secret.is_empty() && credentials.passphrase.is_empty());
        assert!(credentials.broker.as_ref().unwrap().broker_key.is_empty());
    }
    #[test]
    fn incomplete_and_public_only_credentials_keep_a_scrubbing_owner() {
        for (key, public_data_only) in [
            (None, false),
            (Some(String::new()), false),
            (Some("key".into()), true),
        ] {
            let venue = KucoinSpot::new(Options {
                key,
                secret: Some("private-secret".into()),
                passphrase: Some("private-passphrase".into()),
                public_data_only,
                ..Options::default()
            });
            let _: &safe_cex_crypto::secrets::Sensitive<Options> = &venue.options;
            assert!(venue.credentials.is_none());
        }
    }
}

#[cfg(test)]
mod execution_retention {
    use super::*;

    #[test]
    fn retained_execution_prefix_has_an_independent_bounded_fifo() {
        let mut venue = KucoinSpot::new(Options::default());
        let cursor = Cursor::seed(0.5, None).unwrap();
        for index in 0..EXECUTION_PROGRESS_LIMIT {
            venue.remember_execution(index.to_string(), cursor);
        }
        // Updating an admitted endpoint does not allocate/refresh a FIFO slot.
        venue.remember_execution("0".into(), Cursor::seed(0.7, None).unwrap());
        assert_eq!(venue.execution_order.len(), EXECUTION_PROGRESS_LIMIT);
        venue.remember_execution("new".into(), cursor);
        assert!(!venue.execution_progress.contains_key("0"));
        assert!(venue.execution_progress.contains_key("1"));
        assert_eq!(venue.execution_progress.len(), EXECUTION_PROGRESS_LIMIT);
        // Retention and dedupe remain independent native history domains.
        assert!(venue.fills.is_empty() && venue.terminal.is_empty());
    }
}
