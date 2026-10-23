//! Polymarket CLOB V2 outcome tokens on the sans-I/O engine.
mod account;
mod history;
mod public_read;
pub mod sign;
mod trading;
mod wire;

use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Candle, Interval, Limits, Market, MinMax, Precision, PredictionMarket, Ticker,
};
use safe_cex_engine::socket::{Config, Supervisor, Tick as SocketTick};
pub(crate) use safe_cex_engine::url::encode;
use safe_cex_engine::{
    Command, CommandId, Cx, Method, PublicFeedPolicy, Response, SocketChannel, SocketId, TimerId,
    Venue, VenueError,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use wire::*;

pub(crate) const REST: &str = "https://clob.polymarket.com";
const GAMMA_PAGE: usize = 100;
const GAMMA: &str = "https://gamma-api.polymarket.com";
pub const DATA: &str = "https://data-api.polymarket.com";
const SOCKETS: [&str; 2] = [
    "wss://ws-subscriptions-clob.polymarket.com/ws/market",
    "wss://ws-subscriptions-clob.polymarket.com/ws/user",
];

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolymarketOptions {
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub api_passphrase: Option<String>,
    #[serde(default)]
    pub signer_type: u8,
    pub funder_address: Option<String>,
    pub market_slugs: Option<Vec<String>>,
}
safe_cex_crypto::redacted_debug!(PolymarketOptions);
impl safe_cex_crypto::secrets::EraseSecrets for PolymarketOptions {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.api_key);
        safe_cex_crypto::secrets::erase(&mut self.api_secret);
        safe_cex_crypto::secrets::erase(&mut self.api_passphrase);
    }
}
#[derive(Clone, Default)]
pub struct Options {
    pub public_data_only: bool,
    pub key: Option<String>,
    pub polymarket: Option<PolymarketOptions>,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        if let Some(options) = self.polymarket.as_mut() {
            safe_cex_crypto::secrets::EraseSecrets::erase_secrets(options);
        }
    }
}

pub enum Pending {
    Account(account::Request),
    Trade(trading::Request),
    Book {
        read: public_read::Read,
        token: String,
        limit: Option<u32>,
    },
    Markets {
        count: usize,
        rows: Vec<Market>,
        metadata: Vec<PredictionMarket>,
        skipped: Vec<String>,
    },
    Midpoints {
        offset: usize,
    },
    Candles {
        read: public_read::Read,
        symbol: String,
        interval: Interval,
        limit: usize,
        rows: Vec<Candle>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
        invalid: bool,
    },
}
pub(crate) struct Connection {
    socket: Option<SocketId>,
    open: bool,
    supervisor: Supervisor,
    awaiting_pong: bool,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            socket: None,
            open: false,
            awaiting_pong: false,
            supervisor: Supervisor::new(Config {
                ping_interval_ms: Some(10_000.),
                ..Config::default()
            }),
        }
    }
}
#[derive(Default)]
struct BookState {
    book: Book,
    subscribers: usize,
    synced: bool,
    timestamp: f64,
}
struct CandleSubscription {
    users: usize,
    last: f64,
}
pub struct Polymarket {
    pub(crate) options: safe_cex_crypto::secrets::Sensitive<Options>,
    pub(crate) account: account::Account,
    pub(crate) trading: trading::Trading,
    pub(crate) credentials: Option<sign::Credentials>,
    pub(crate) address: Option<String>,
    pub(crate) connections: [Connection; 2],
    books: HashMap<String, BookState>,
    market_ids: HashSet<String>,
    market_cursors: HashSet<String>,
    market_symbols: HashSet<String>,
    walk_skipped_slugs: HashSet<String>,
    skipped_slugs: HashSet<String>,
    candles: BTreeMap<(String, Interval), CandleSubscription>,
    candle_timer: Option<TimerId>,
    seed_tickers: Vec<Ticker>,
    seeded_tokens: HashSet<String>,
    public_retry: safe_cex_engine::startup::Backoff<Option<usize>>,
    live_tickers: safe_cex_engine::startup::LiveTickers,
    midpoint_next: Option<usize>,
    midpoint_inflight: HashSet<usize>,
    midpoint_failed: HashSet<usize>,
    quote_times: HashMap<String, f64>,
    trade_times: HashMap<String, f64>,
    tick_times: HashMap<String, f64>,
}
pub(crate) fn success(response: Response<'_>) -> Result<(), String> {
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "Polymarket HTTP {}: {}",
            response.status, response.body
        ));
    }
    if let Ok(e) = serde_json::from_str::<ApiError>(response.body)
        && let Some(error) = e.error
    {
        return Err(format!("Polymarket: {}", e.message.unwrap_or(error)));
    }
    Ok(())
}
fn tick_supported(tick: f64) -> bool {
    [0.1, 0.01, 0.005, 0.0025, 0.001, 0.0001].contains(&tick)
}
impl Polymarket {
    pub fn new(options: Options) -> Self {
        let options = safe_cex_crypto::secrets::Sensitive::new(options);
        let address = options
            .key
            .as_deref()
            .and_then(|k| safe_cex_crypto::ethereum::private_key_address(k).ok());
        Self {
            options,
            address,
            credentials: None,
            account: Default::default(),
            trading: Default::default(),
            market_ids: Default::default(),
            market_cursors: Default::default(),
            market_symbols: Default::default(),
            walk_skipped_slugs: Default::default(),
            skipped_slugs: Default::default(),
            connections: Default::default(),
            books: Default::default(),
            candles: Default::default(),
            candle_timer: None,
            seed_tickers: vec![],
            seeded_tokens: Default::default(),
            public_retry: Default::default(),
            live_tickers: Default::default(),
            midpoint_next: None,
            midpoint_inflight: Default::default(),
            midpoint_failed: Default::default(),
            quote_times: Default::default(),
            trade_times: Default::default(),
            tick_times: Default::default(),
        }
    }
    fn markets(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        cursor: Option<String>,
        rows: Vec<Market>,
        metadata: Vec<PredictionMarket>,
    ) {
        let mut url = format!("{GAMMA}/markets/keyset?active=true&closed=false&limit={GAMMA_PAGE}");
        if let Some(slugs) = self
            .options
            .polymarket
            .as_ref()
            .and_then(|p| p.market_slugs.as_ref())
        {
            if slugs.is_empty()
                || slugs.len() > 100
                || slugs.iter().any(|s| s.trim().is_empty())
                || slugs.iter().collect::<HashSet<_>>().len() != slugs.len()
            {
                cx.notice(
                    Level::Error,
                    code::CONFIG_INVALID,
                    "Polymarket marketSlugs must contain1..100 unique nonempty slugs",
                );
                return;
            }
            for slug in slugs {
                url.push_str(&format!("&slug={}", encode(slug)));
            }
        }
        if let Some(cursor) = &cursor {
            url.push_str(&format!("&after_cursor={}", encode(cursor)));
        }
        cx.request_streamed(
            Method::Get,
            url,
            vec![],
            ArrayAt::Field("markets"),
            Pending::Markets {
                count: 0,
                rows,
                metadata,
                skipped: vec![],
            },
        );
    }
    fn markets_failed(&mut self, cx: &mut Cx<'_, Pending>, error: &str) {
        cx.notice(Level::Warn, code::REQUEST_FAILED, error);
        self.public_retry.failed(cx, None);
    }
    /// Whether an unlisted owned token could belong to a skipped catalogue row.
    pub(crate) fn catalogue_may_hold(&self, slug: &str) -> bool {
        self.skipped_slugs.contains(slug)
            || self.skipped_slugs.contains("unknown")
            || (slug.is_empty() && !self.skipped_slugs.is_empty())
    }
    /// Every catalogue and midpoint seed read has succeeded and none awaits a retry.
    pub(crate) fn discovery_settled(&self) -> bool {
        self.public_retry.idle()
            && self.midpoint_next.is_none()
            && self.midpoint_inflight.is_empty()
            && self.midpoint_failed.is_empty()
    }
    fn flush_midpoints(&mut self, cx: &mut Cx<'_, Pending>) {
        let accepted: Vec<_> = std::mem::take(&mut self.seed_tickers)
            .into_iter()
            .filter(|t| self.seeded_tokens.contains(&t.id))
            .collect();
        self.seeded_tokens.clear();
        let complete = self.midpoint_next.is_none()
            && self.midpoint_inflight.is_empty()
            && self.midpoint_failed.is_empty();
        // A trailing batch of book-less tokens still has to finish startup.
        if accepted.is_empty() && !complete {
            return;
        }
        let mut rows = cx.store.tickers().to_vec();
        for ticker in accepted {
            if let Some(old) = rows.iter_mut().find(|old| old.id == ticker.id) {
                *old = ticker;
            } else {
                rows.push(ticker);
            }
        }
        if complete {
            self.live_tickers.finish(cx.store, &mut rows);
        } else {
            self.live_tickers.overlay(cx.store, &mut rows);
        }
        cx.store.set_tickers(rows);
    }
    fn advance_midpoints(&mut self, cx: &mut Cx<'_, Pending>, offset: usize) {
        if self.midpoint_next == Some(offset) {
            let next = offset + 100;
            self.midpoint_next = (next < cx.store.markets().len()).then_some(next);
            if let Some(next) = self.midpoint_next {
                self.midpoints(cx, next);
            }
        }
    }
    fn midpoints(&mut self, cx: &mut Cx<'_, Pending>, offset: usize) {
        let rows = cx.store.markets();
        if offset >= rows.len() || !self.midpoint_inflight.insert(offset) {
            return;
        }
        self.live_tickers.begin();
        #[derive(Serialize)]
        struct Token<'a> {
            token_id: &'a str,
        }
        let body = serde_json::to_string(
            &rows[offset..rows.len().min(offset + 100)]
                .iter()
                .map(|m| Token { token_id: &m.id })
                .collect::<Vec<_>>(),
        )
        .expect("token list");
        cx.request(
            Method::Post,
            format!("{REST}/midpoints"),
            vec![("Content-Type", "application/json".into())],
            Some(body),
            Pending::Midpoints { offset },
        );
    }
    fn public_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.books.is_empty() {
            self.connect(cx, 0);
        }
        let private_intent = self.options.key.as_ref().is_some_and(|k| !k.is_empty())
            || self.options.polymarket.as_ref().is_some_and(|p| {
                p.api_key.is_some()
                    || p.api_secret.is_some()
                    || p.api_passphrase.is_some()
                    || p.signer_type != 0
                    || p.funder_address.is_some()
            });
        if self.options.public_data_only || !private_intent {
            cx.store.set_balance(Default::default());
            cx.store.set_positions(vec![]);
            cx.store.set_orders(vec![]);
        } else {
            self.start_private(cx);
        }
    }
    pub(crate) fn connect(&mut self, cx: &mut Cx<'_, Pending>, index: usize) {
        let c = &mut self.connections[index];
        if c.socket.is_some() || c.supervisor.is_waiting() {
            return;
        }
        // Suppressed public feeds never open; supervising them would only churn retries.
        if index != 0 || cx.public_feed_policy() == PublicFeedPolicy::Direct {
            c.supervisor.connecting(cx);
        }
        c.socket = Some(
            cx.connect(
                safe_cex_engine::SocketSpec::new(SOCKETS[index])
                    .channel(if index == 0 {
                        SocketChannel::Public
                    } else {
                        SocketChannel::Private
                    })
                    .feed(if index == 0 { "market" } else { "user" }, true),
            ),
        );
    }
    fn subscription(&self, cx: &mut Cx<'_, Pending>, ids: Vec<String>, operation: Option<&str>) {
        if ids.is_empty() {
            return;
        }
        if let Some(socket) = self.connections[0]
            .socket
            .filter(|_| self.connections[0].open)
        {
            #[derive(Serialize)]
            struct Subscription<'a> {
                assets_ids: Vec<String>,
                #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
                kind: Option<&'a str>,
                #[serde(skip_serializing_if = "Option::is_none")]
                operation: Option<&'a str>,
                custom_feature_enabled: bool,
            }
            cx.send(
                socket,
                serde_json::to_string(&Subscription {
                    assets_ids: ids,
                    kind: operation.is_none().then_some("market"),
                    operation,
                    custom_feature_enabled: true,
                })
                .expect("subscription"),
            );
        }
    }
    fn subscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        let state = self.books.entry(symbol.clone()).or_default();
        state.subscribers += 1;
        if state.subscribers > 1 {
            return;
        }
        if let Some(market) = cx.store.market(&symbol) {
            let id = market.id.clone();
            if self.connections[0].socket.is_none() {
                self.connect(cx, 0);
            } else {
                self.subscription(cx, vec![id], Some("subscribe"));
            }
        } else if cx.store.loaded().markets {
            self.books.remove(&symbol);
            cx.notice(
                Level::Error,
                code::COMMAND_UNSUPPORTED,
                format!("Unknown Polymarket symbol: {symbol}"),
            );
        }
    }
    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        if text.trim_start().starts_with('[') {
            if let Ok(events) = serde_json::from_str::<Vec<PublicEvent>>(text) {
                for event in events {
                    self.public_event(cx, event);
                }
            }
        } else if let Ok(event) = serde_json::from_str::<PublicEvent>(text) {
            self.public_event(cx, event);
        }
    }
    fn public_event(&mut self, cx: &mut Cx<'_, Pending>, event: PublicEvent<'_>) {
        match event.event_type.as_ref() {
            "book" => {
                let Some(symbol) = cx
                    .store
                    .market_by_id(&event.asset_id)
                    .map(|m| m.symbol.clone())
                else {
                    return;
                };
                let Some(state) = self.books.get_mut(&symbol) else {
                    return;
                };
                let timestamp = event.timestamp.map_or(0.0, |n| n.0);
                if timestamp < state.timestamp {
                    return;
                }
                let (Some(bids), Some(asks)) = (event.bids, event.asks) else {
                    return;
                };
                if bids
                    .iter()
                    .chain(&asks)
                    .any(|l| !valid_level(l.price.0, l.size.0))
                {
                    return;
                }
                state.book.clear();
                for (side, levels) in [(BookSide::Bids, bids), (BookSide::Asks, asks)] {
                    for level in levels {
                        state.book.apply(side, level.price.0, level.size.0);
                    }
                }
                self.quote_times
                    .entry(symbol.clone())
                    .and_modify(|t| *t = t.max(timestamp))
                    .or_insert(timestamp);
                state.timestamp = timestamp;
                state.synced = true;
                cx.book_changed(&symbol);
            }
            "price_change" => {
                for change in event.price_changes {
                    if !valid_level(change.price.0, change.size.0)
                        || !["BUY", "SELL"].contains(&change.side.as_ref())
                    {
                        continue;
                    }
                    let Some(symbol) = cx
                        .store
                        .market_by_id(&change.asset_id)
                        .map(|m| m.symbol.clone())
                    else {
                        continue;
                    };
                    let timestamp = event.timestamp.map_or(cx.now, |n| n.0);
                    let previous = self.quote_times.entry(symbol.clone()).or_insert(0.0);
                    if timestamp < *previous {
                        continue;
                    }
                    *previous = timestamp;
                    if let Some(state) = self.books.get_mut(&symbol).filter(|s| s.synced) {
                        let timestamp = event.timestamp.map_or(state.timestamp, |n| n.0);
                        let side = match change.side.as_ref() {
                            "BUY" => BookSide::Bids,
                            "SELL" => BookSide::Asks,
                            _ => continue,
                        };
                        if timestamp >= state.timestamp
                            && valid_level(change.price.0, change.size.0)
                        {
                            state.book.apply(side, change.price.0, change.size.0);
                            state.timestamp = timestamp;
                            cx.book_changed(&symbol);
                        }
                    }
                    let bid = change
                        .best_bid
                        .map(|n| n.0)
                        .filter(|v| (0.0..=1.0).contains(v));
                    let ask = change
                        .best_ask
                        .map(|n| n.0)
                        .filter(|v| (0.0..=1.0).contains(v));
                    let supplied = u16::from(bid.is_some())
                        | (u16::from(ask.is_some()) << 1)
                        | (u16::from(bid.is_some() && ask.is_some()) << 3)
                        | (u16::from(bid.is_some() && ask.is_some()) << 4);
                    cx.store.update_ticker(&symbol, |t| {
                        if let Some(bid) = bid {
                            t.bid = bid;
                        }
                        if let Some(ask) = ask {
                            t.ask = ask;
                        }
                        if let (Some(bid), Some(ask)) = (bid, ask) {
                            t.mark = (bid + ask) / 2.0;
                            t.index = t.mark;
                        }
                    });
                    self.live_tickers.record(cx.store, &symbol, supplied);
                }
            }
            "last_trade_price" => {
                if let (Some(m), Some(p)) = (cx.store.market_by_id(&event.asset_id), event.price) {
                    if !(0.0..=1.0).contains(&p.0) {
                        return;
                    }
                    let symbol = m.symbol.clone();
                    let timestamp = event.timestamp.map_or(cx.now, |n| n.0);
                    let previous = self.trade_times.entry(symbol.clone()).or_insert(0.0);
                    if timestamp < *previous {
                        return;
                    }
                    *previous = timestamp;
                    if (0.0..=1.0).contains(&p.0) {
                        cx.store.update_ticker(&symbol, |t| t.last = p.0);
                        self.live_tickers.record(cx.store, &symbol, 1 << 2);
                    }
                }
            }
            "tick_size_change" => {
                if let (Some(market), Some(tick)) = (
                    cx.store.market_index_by_id(&event.asset_id),
                    event.new_tick_size,
                ) {
                    if !tick_supported(tick.0) {
                        return;
                    }
                    let timestamp = event.timestamp.map_or(cx.now, |n| n.0);
                    let previous = self
                        .tick_times
                        .entry(event.asset_id.to_string())
                        .or_insert(0.0);
                    if timestamp < *previous {
                        return;
                    }
                    *previous = timestamp;
                    let mut rows = cx.store.prediction_markets().unwrap_or_default().to_vec();
                    if let Some(row) = rows.iter_mut().find(|m| m.market == market) {
                        row.tick_size = tick.0;
                    }
                    cx.store.set_prediction_markets(rows);
                    cx.store.update_markets(|m| {
                        if m.id == event.asset_id {
                            m.precision.price = f64::from(safe_cex_engine::num::decimals(tick.0));
                        }
                    });
                }
            }
            "market_resolved" => {
                cx.store.update_markets(|m| {
                    if event.assets_ids.iter().any(|id| *id == m.id) {
                        m.active = false;
                    }
                });
            }
            _ => {}
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn fetch_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Option<CommandId>,
        symbol: String,
        interval: Interval,
        limit: Option<u32>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
    ) {
        let result = (|| {
            let market = cx
                .store
                .market(&symbol)
                .ok_or("Unknown Polymarket symbol")?;
            let fidelity = fidelity(interval).ok_or_else(|| interval.unsupported("Polymarket"))?;
            if limit == Some(0)
                || from_ms
                    .into_iter()
                    .chain(to_ms)
                    .any(|v| !v.is_finite() || v < 0.0)
                || matches!((from_ms,to_ms),(Some(a),Some(b)) if a>b)
            {
                return Err("Invalid Polymarket candle bounds".to_owned());
            }
            if from_ms.is_none() && to_ms.is_some() {
                return Err("Polymarket upper-only candle bounds require from".to_owned());
            }
            if from_ms.is_some_and(|from| from < 1000.0) {
                return Err("Polymarket candle from must be at least 1000 milliseconds".to_owned());
            }
            let mut path = format!("{REST}/prices-history?market={}", encode(&market.id));
            if let Some(from) = from_ms {
                path.push_str(&format!("&startTs={}", (from / 1000.0).floor()));
            }
            if let Some(to) = to_ms {
                path.push_str(&format!("&endTs={}", (to / 1000.0).floor()));
            }
            // The API requires a time selector even when callers omit bounds.
            if from_ms.is_none() && to_ms.is_none() {
                path.push_str("&interval=max");
            }
            path.push_str(&format!("&fidelity={fidelity}"));
            Ok(path)
        })();
        match result {
            Ok(path) => {
                let Some(read) = public_read::Read::new(cx, command.is_some()) else {
                    return;
                };
                match cx.request_managed(
                    read.context.clone(),
                    Method::Get,
                    path,
                    None,
                    Pending::Candles {
                        read,
                        symbol,
                        interval,
                        limit: limit.unwrap_or(u32::MAX) as usize,
                        rows: vec![],
                        from_ms,
                        to_ms,
                        invalid: false,
                    },
                ) {
                    Ok(request) => cx.stream_response(request, ArrayAt::Field("history")),
                    Err((Pending::Candles { read, .. }, reason)) => {
                        read.failed(cx, reason.venue_error())
                    }
                    Err(_) => unreachable!("price history continuation"),
                }
            }
            Err(e) => {
                if command.is_some() {
                    public_read::local(
                        cx,
                        safe_cex_engine::request_context::Operation::Read,
                        Err(e),
                    );
                } else {
                    cx.notice(Level::Error, code::COMMAND_UNSUPPORTED, e);
                }
            }
        }
    }
}
fn valid_level(price: f64, size: f64) -> bool {
    (0.0..=1.0).contains(&price) && size >= 0.0
}
impl Venue for Polymarket {
    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            cancel_all: safe_cex_engine::CancelAll::Native,
            leverage_query: safe_cex_engine::LeverageQuery::OnRead,
            max_leverage: safe_cex_engine::MaxLeverage::Rails,
            order_book_query: true,
            fill_history: false,
            order_history: false,
            income_history: false,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.markets(cx, None, vec![], vec![]);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let Some(i) = self
            .connections
            .iter()
            .position(|c| c.socket == Some(socket))
        else {
            return;
        };
        self.connections[i].open = true;
        self.connections[i].supervisor.opened(cx);
        if i == 0 {
            let ids = self
                .books
                .keys()
                .filter_map(|s| cx.store.market(s).map(|m| m.id.clone()))
                .collect();
            self.subscription(cx, ids, None);
        } else {
            if let Some(subscription) = self.private_subscription() {
                cx.send(socket, subscription);
            }
            self.account_reconcile(cx);
        }
        cx.send(socket, "PING");
        self.connections[i].awaiting_pong = true;
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(i) = self
            .connections
            .iter()
            .position(|c| c.socket == Some(socket) && c.open)
        else {
            return;
        };
        if text == "PONG" {
            self.connections[i].awaiting_pong = false;
            self.connections[i].supervisor.received(cx.now);
            return;
        }
        if i == 0 {
            self.public_message(cx, text);
        } else {
            self.private_message(cx, text);
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        let Some(i) = self
            .connections
            .iter()
            .position(|c| c.socket == Some(socket))
        else {
            return;
        };
        let c = &mut self.connections[i];
        c.socket = None;
        c.open = false;
        c.awaiting_pong = false;
        c.supervisor.retry(cx);
        if i == 0 {
            for state in self.books.values_mut() {
                state.synced = false;
                state.timestamp = 0.0;
                state.book.clear();
            }
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Account(request) = pending {
            self.account_response(cx, request, response);
            return;
        }
        if let Pending::Trade(request) = pending {
            self.trading_response(cx, request, response);
            return;
        }
        if let Pending::Book { read, token, limit } = pending {
            self.book_response(cx, read, &token, limit, response);
            return;
        }
        if let Err(e) = success(response) {
            self.on_request_failed(cx, pending, &e);
            return;
        }
        match pending {
            Pending::Account(_) | Pending::Trade(_) | Pending::Book { .. } => unreachable!(),
            Pending::Markets {
                count,
                rows,
                metadata,
                skipped,
            } => {
                if !skipped.is_empty() {
                    cx.notice(
                        Level::Warn,
                        code::ROW_SKIPPED,
                        format!(
                            "Skipped {} malformed Polymarket market rows: {}",
                            skipped.len(),
                            skipped.join(", ")
                        ),
                    );
                    self.walk_skipped_slugs.extend(skipped);
                }
                if count > GAMMA_PAGE {
                    self.markets_failed(cx, "Oversized Polymarket Gamma page");
                    return;
                }
                #[derive(Deserialize)]
                struct Page<'a> {
                    markets: Vec<serde::de::IgnoredAny>,
                    #[serde(borrow)]
                    next_cursor: Option<std::borrow::Cow<'a, str>>,
                }
                let Ok(page) = serde_json::from_str::<Page>(response.body) else {
                    self.markets_failed(cx, "Invalid Polymarket keyset envelope");
                    return;
                };
                if !page.markets.is_empty() {
                    self.markets_failed(cx, "Unstreamed Polymarket market page");
                    return;
                }
                if let Some(cursor) = page.next_cursor {
                    if cursor.is_empty()
                        || self.market_cursors.len() >= 10000
                        || !self.market_cursors.insert(cursor.to_string())
                    {
                        self.markets_failed(cx, "Polymarket market cursor did not advance");
                        return;
                    }
                    self.markets(cx, Some(cursor.into_owned()), rows, metadata);
                    return;
                }
                if count == GAMMA_PAGE {
                    self.markets_failed(cx, "Polymarket market page is missing its next cursor");
                    return;
                }
                if rows.is_empty() {
                    cx.notice(
                        Level::Warn,
                        code::REQUEST_FAILED,
                        "No supported Polymarket market rows",
                    );
                    self.public_retry.failed(cx, None);
                    return;
                }
                self.public_retry.succeeded(cx, &None);
                self.skipped_slugs = std::mem::take(&mut self.walk_skipped_slugs);
                cx.store.set_markets(rows);
                cx.store.set_prediction_markets(metadata);
                let tickers = cx
                    .store
                    .markets()
                    .iter()
                    .map(|m| Ticker {
                        id: m.id.clone(),
                        symbol: m.symbol.clone(),
                        ..Ticker::default()
                    })
                    .collect();
                self.seed_tickers = tickers;
                self.seeded_tokens.clear();
                self.midpoint_inflight.clear();
                self.midpoint_failed.clear();
                self.midpoint_next = Some(0);
                self.public_ready(cx);
                self.midpoints(cx, 0);
            }
            Pending::Midpoints { offset } => {
                self.midpoint_inflight.remove(&offset);
                let requested: HashSet<String> = cx
                    .store
                    .markets()
                    .iter()
                    .skip(offset)
                    .take(100)
                    .map(|market| market.id.clone())
                    .collect();
                let mut rejected = false;
                match serde_json::from_str::<HashMap<String, &serde_json::value::RawValue>>(
                    response.body,
                ) {
                    Ok(prices) => {
                        for (id, raw) in prices {
                            // A price from another batch cannot satisfy this request's
                            // coverage or overwrite that batch's live/seeded quote.
                            if !requested.contains(&id) {
                                continue;
                            }
                            let Ok(p) = serde_json::from_str::<wire::PublicNumber>(raw.get())
                            else {
                                rejected = true;
                                safe_cex_engine::startup::skipped_row(cx);
                                continue;
                            };
                            if !(0.0..=1.0).contains(&p.0) {
                                rejected = true;
                                safe_cex_engine::startup::skipped_row(cx);
                                continue;
                            }
                            let Some(market) = cx.store.market_by_id(&id) else {
                                continue;
                            };
                            let symbol = market.symbol.clone();
                            let ticker = if let Some(index) =
                                self.seed_tickers.iter().position(|t| t.id == id)
                            {
                                &mut self.seed_tickers[index]
                            } else {
                                self.seed_tickers.push(
                                    cx.store.ticker(&symbol).cloned().unwrap_or_else(|| Ticker {
                                        id: id.clone(),
                                        symbol,
                                        ..Ticker::default()
                                    }),
                                );
                                self.seed_tickers.last_mut().unwrap()
                            };
                            ticker.bid = p.0;
                            ticker.ask = p.0;
                            ticker.last = p.0;
                            ticker.mark = p.0;
                            ticker.index = p.0;
                            self.seeded_tokens.insert(id);
                        }
                    }
                    Err(_) => {
                        self.on_request_failed(
                            cx,
                            Pending::Midpoints { offset },
                            "Invalid Polymarket midpoints",
                        );
                        return;
                    }
                }
                // CLOB omits tokens without a book: an omitted token is unseeded, not a
                // failed batch, even when every token is omitted. Only rejected rows retry.
                if rejected {
                    cx.notice(
                        Level::Warn,
                        code::SNAPSHOT_DISCARDED,
                        "Incomplete Polymarket midpoint rows; preserving prior quotes",
                    );
                    self.midpoint_failed.insert(offset);
                    self.public_retry.failed(cx, Some(offset));
                } else {
                    self.midpoint_failed.remove(&offset);
                    self.public_retry.succeeded(cx, &Some(offset));
                }
                self.advance_midpoints(cx, offset);
                self.flush_midpoints(cx);
            }
            Pending::Candles {
                read,
                symbol,
                interval,
                limit,
                mut rows,
                from_ms,
                to_ms,
                invalid,
            } => {
                if !read.validate(cx) {
                    return;
                }
                if invalid {
                    read.finish(cx, Err("Invalid Polymarket price history".into()));
                    return;
                }
                rows.retain(|c| {
                    from_ms.is_none_or(|from| c.timestamp >= from)
                        && to_ms.is_none_or(|to| c.timestamp <= to)
                });
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                rows.dedup_by(|later, earlier| {
                    if later.timestamp == earlier.timestamp {
                        *earlier = *later;
                        true
                    } else {
                        false
                    }
                });
                if rows.len() > limit {
                    rows.drain(..rows.len() - limit);
                }
                if read.command() {
                    let result = serde_json::to_string(&rows).map_err(|e| e.to_string());
                    read.finish(cx, result);
                } else {
                    if let Some(sub) = self.candles.get_mut(&(symbol.clone(), interval)) {
                        for row in rows {
                            if row.timestamp > sub.last {
                                cx.candle(&symbol, interval, row);
                                sub.last = row.timestamp;
                            }
                        }
                    }
                    read.finish(cx, Ok(String::new()));
                }
            }
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Account(request) => self.account_item(cx, request, item),
            Pending::Markets {
                count,
                rows,
                metadata,
                skipped,
            } => {
                *count += 1;
                let gm = match serde_json::from_str::<Gamma>(item) {
                    Ok(gm) => gm,
                    Err(_) => {
                        #[derive(Deserialize)]
                        struct Slug<'a> {
                            #[serde(borrow)]
                            slug: std::borrow::Cow<'a, str>,
                        }
                        let slug = serde_json::from_str::<Slug>(item).ok();
                        skipped.push(slug.map_or_else(|| "unknown".into(), |s| s.slug.into()));
                        safe_cex_engine::startup::skipped_row(cx);
                        return;
                    }
                };
                let Some((ids, outcomes)) =
                    gm.clob_token_ids.as_deref().zip(gm.outcomes.as_deref())
                else {
                    return;
                };
                let (Ok(ids), Ok(outcomes)) = (
                    serde_json::from_str::<Vec<String>>(ids),
                    serde_json::from_str::<Vec<String>>(outcomes),
                ) else {
                    skipped.push(gm.slug.to_string());
                    safe_cex_engine::startup::skipped_row(cx);
                    return;
                };
                if ids.len() != outcomes.len() {
                    skipped.push(gm.slug.to_string());
                    safe_cex_engine::startup::skipped_row(cx);
                    return;
                }
                let tick = gm.order_price_min_tick_size.map_or(0.01, |n| n.0);
                // Precision holds the tick's decimal count, so the tick itself is checked.
                if !tick.is_finite() || tick <= 0.0 {
                    skipped.push(gm.slug.to_string());
                    safe_cex_engine::startup::skipped_row(cx);
                    return;
                }
                for (id, outcome) in ids.into_iter().zip(outcomes) {
                    if id.is_empty()
                        || !id.bytes().all(|b| b.is_ascii_digit())
                        || outcome.is_empty()
                    {
                        continue;
                    }
                    let symbol = format!("{}-{outcome}", gm.slug);
                    if !self.market_ids.insert(id.clone())
                        || !self.market_symbols.insert(symbol.clone())
                    {
                        continue;
                    }
                    let market = rows.len() as u32;
                    metadata.push(PredictionMarket {
                        market,
                        condition_id: gm.condition_id.to_string(),
                        outcome: outcome.clone(),
                        question: gm.question.as_deref().unwrap_or_default().into(),
                        description: gm.description.as_deref().unwrap_or_default().into(),
                        end_date: gm.end_date.as_deref().unwrap_or_default().into(),
                        slug: gm.slug.to_string(),
                        tick_size: tick,
                        neg_risk: gm.neg_risk.unwrap_or(false),
                    });
                    rows.push(Market {
                        id,
                        symbol,
                        pretty_symbol: None,
                        base: gm.slug.to_string(),
                        quote: "pUSD".into(),
                        active: gm.active == Some(true)
                            && gm.closed != Some(true)
                            && gm.enable_order_book != Some(false)
                            && gm.accepting_orders != Some(false),
                        precision: Precision {
                            amount: 2.0,
                            price: f64::from(safe_cex_engine::num::decimals(tick)),
                        },
                        limits: Limits {
                            amount: MinMax {
                                min: gm.order_min_size.map_or(0.0, |n| n.0),
                                max: 1_000_000.0,
                            },
                            leverage: MinMax { min: 1.0, max: 1.0 },
                            price: None,
                        },
                        contract_size: Some(1.0),
                        is_expiring: None,
                        is_inverse: None,
                        is_spot: None,
                        is_perpetual: None,
                        is_read_only: None,
                        taker_fee: None,
                        maker_fee: None,
                    });
                }
            }
            Pending::Candles {
                read,
                rows,
                invalid,
                ..
            } => {
                if !read.validate(cx) {
                    return;
                }
                match serde_json::from_str::<HistoryPoint>(item) {
                    Ok(point) if point.t.0 >= 0.0 && (0.0..=1.0).contains(&point.p.0) => {
                        rows.push(Candle {
                            timestamp: point.t.0 * 1000.0,
                            open: point.p.0,
                            high: point.p.0,
                            low: point.p.0,
                            close: point.p.0,
                            volume: 0.0,
                        })
                    }
                    _ => *invalid = true,
                }
            }
            _ => {}
        }
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        _reason: safe_cex_engine::request_context::Error,
        error: VenueError,
    ) {
        self.on_http_failed(cx, pending, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending {
            Pending::Book { read, .. } | Pending::Candles { read, .. } => read.failed(cx, error),
            other => self.on_request_failed(cx, other, &error.message),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        let retry = match pending {
            Pending::Trade(request) => {
                self.trading_failed(cx, request, error);
                return;
            }
            Pending::Account(request) => {
                self.account_failed(cx, request);
                return;
            }
            Pending::Book { read, .. } | Pending::Candles { read, .. } => {
                read.failed(cx, VenueError::new(error));
                return;
            }
            Pending::Markets { .. } => Some(None),
            Pending::Midpoints { offset } => {
                self.midpoint_inflight.remove(&offset);
                self.midpoint_failed.insert(offset);
                cx.notice(
                    Level::Warn,
                    code::REQUEST_FAILED,
                    "Polymarket midpoint read failed",
                );
                self.public_retry.failed(cx, Some(offset));
                self.advance_midpoints(cx, offset);
                self.flush_midpoints(cx);
                return;
            }
        };
        cx.notice(
            if retry.is_some() {
                Level::Warn
            } else {
                Level::Error
            },
            code::REQUEST_FAILED,
            error,
        );
        if let Some(offset) = retry {
            self.public_retry.failed(cx, offset);
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account_timer(cx, timer) {
            return;
        }
        if let Some(offset) = self.public_retry.on_timer(timer) {
            if let Some(offset) = offset {
                self.midpoints(cx, offset);
            } else {
                self.market_ids.clear();
                self.market_symbols.clear();
                self.market_cursors.clear();
                self.walk_skipped_slugs.clear();
                self.markets(cx, None, vec![], vec![]);
            }
            return;
        }
        for i in 0..2 {
            if !self.connections[i].supervisor.owns_timer(timer) {
                continue;
            }
            match self.connections[i].supervisor.on_timer(cx, timer) {
                SocketTick::Reconnect => self.connect(cx, i),
                SocketTick::Ping if self.connections[i].awaiting_pong => {
                    if let Some(socket) = self.connections[i].socket {
                        cx.close(socket);
                        self.on_close(cx, socket, 1006);
                    }
                }
                SocketTick::TimedOut(_) => {
                    if let Some(socket) = self.connections[i].socket {
                        cx.close(socket);
                        self.on_close(cx, socket, 1006);
                    }
                }
                SocketTick::Ping => {
                    if let Some(socket) = self.connections[i].socket {
                        cx.send(socket, "PING");
                        self.connections[i].awaiting_pong = true;
                    }
                }
                _ => {}
            }
            return;
        }
        if self.candle_timer == Some(timer) {
            self.candle_timer = None;
            let subscriptions = self.candles.keys().cloned().collect::<Vec<_>>();
            for (symbol, interval) in subscriptions {
                self.fetch_candles(
                    cx,
                    None,
                    symbol,
                    interval,
                    None,
                    Some(cx.now - 60000.0),
                    Some(cx.now),
                );
            }
            if !self.candles.is_empty() {
                self.candle_timer = Some(cx.set_timer(5000.0));
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::FetchOptionChain { .. }
            | Command::FetchOptionExpiries { .. }
            | Command::FetchAccountGreeks { .. } => {
                public_read::local(
                    cx,
                    safe_cex_engine::request_context::Operation::Read,
                    Err("Options are not enabled on this account".into()),
                );
            }
            Command::SetTickerSymbols { .. } => {}
            Command::Venue { command, .. } => {
                safe_cex_engine::unsupported_venue_action(cx, command)
            }
            Command::SubscribeOrderBook { symbol } => self.subscribe_book(cx, symbol),
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers = state.subscribers.saturating_sub(1);
                    if state.subscribers == 0 {
                        self.books.remove(&symbol);
                        if let Some(m) = cx.store.market(&symbol) {
                            self.subscription(cx, vec![m.id.clone()], Some("unsubscribe"));
                        }
                    }
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => self.fetch_candles(cx, Some(command), symbol, interval, limit, from_ms, to_ms),
            Command::SubscribeCandles { symbol, interval } => {
                if fidelity(interval).is_none() {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        interval.unsupported("Polymarket"),
                    );
                    return;
                }
                let sub =
                    self.candles
                        .entry((symbol.clone(), interval))
                        .or_insert(CandleSubscription {
                            users: 0,
                            last: 0.0,
                        });
                sub.users += 1;
                if sub.users == 1 {
                    self.fetch_candles(
                        cx,
                        None,
                        symbol,
                        interval,
                        None,
                        Some(cx.now - 60000.0),
                        Some(cx.now),
                    );
                }
                if self.candle_timer.is_none() {
                    self.candle_timer = Some(cx.set_timer(5000.0));
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol, interval);
                if let Some(s) = self.candles.get_mut(&key) {
                    s.users = s.users.saturating_sub(1);
                    if s.users == 0 {
                        self.candles.remove(&key);
                    }
                }
                if self.candles.is_empty()
                    && let Some(t) = self.candle_timer.take()
                {
                    cx.clear_timer(t);
                }
            }
            Command::CancelOrdersWithSymbols { command, orders } => self.trading_command(
                cx,
                Command::CancelOrders {
                    command,
                    ids: orders.into_iter().map(|o| o.id).collect(),
                },
            ),
            Command::AmendOrderWithSymbol { .. } => {
                public_read::local(
                    cx,
                    safe_cex_engine::request_context::Operation::Mutation,
                    Err("Symbol-scoped amendment is unsupported".into()),
                );
            }
            command @ (Command::PlaceOrders { .. }
            | Command::CancelOrders { .. }
            | Command::AmendOrder { .. }
            | Command::CancelSymbolOrders { .. }) => self.trading_command(cx, command),
            command @ (Command::Query { .. }
            | Command::SetLeverage { .. }
            | Command::SetMarginType { .. }
            | Command::SetPositionMode { .. }
            | Command::FetchLeverage { .. }
            | Command::FetchMaxLeverage { .. }
            | Command::FetchAccount { .. }
            | Command::FetchOrderBook { .. }
            | Command::FetchStpMode { .. }
            | Command::SetStpMode { .. }) => self.read_command(cx, command),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).filter(|s| s.synced).map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}

/// Polymarket price-history fidelity in minutes.
fn fidelity(interval: Interval) -> Option<u32> {
    use Interval::*;
    Some(match interval {
        M1 => 1,
        M5 => 5,
        M15 => 15,
        H1 => 60,
        H4 => 240,
        D1 => 1440,
        _ => return None,
    })
}
