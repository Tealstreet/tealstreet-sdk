//! WOO perpetuals on the active legacy-public/V1/V3 protocol.
mod account;
mod history;
mod identity;
mod seed_read;
mod sign;
mod socket_codec;
mod trading;
mod wire;

use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Candle, Interval, Market};
use safe_cex_engine::{
    Command, CommandId, Cx, Method, PublicFeedPolicy, Response, SocketChannel, SocketId, TimerId,
    Venue, num,
};
use std::collections::HashMap;
use wire::*;

#[derive(Clone, Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub application_id: Option<String>,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
        if let Some(read) = self.read_credentials.as_mut() {
            read.erase_secrets();
        }
    }
}

pub enum Pending {
    Identity(CommandId),
    Trade(trading::Request),
    History(history::Request),
    Account(account::Request),
    Seed(Box<seed_read::Read>),
    #[cfg(test)]
    SigningProbe,
    Depth {
        symbol: String,
        generation: u64,
    },
    SocketBookSeeds(Vec<(String, u64)>),
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        bad: bool,
    },
}
#[derive(Default)]
struct Connection {
    feed: Option<safe_cex_engine::socket_lifecycle::FeedId>,
    socket: Option<SocketId>,
    open: bool,
    ping_at: f64,
}
struct Buffered {
    time: f64,
    delta: Delta,
}
enum Sync {
    Buffer(Vec<Buffered>),
    Live(f64),
    Retry(TimerId),
}
struct Depth {
    count: u32,
    generation: u64,
    seed_due: bool,
    book: Book,
    sync: Sync,
}
#[derive(Default)]
struct CandleState {
    count: u32,
    last: Option<Candle>,
}
pub struct Woo {
    trading: trading::Trading,
    history: history::History,
    account: account::Account,
    options: safe_cex_crypto::secrets::Sensitive<Options>,
    rest: &'static str,
    public: Connection,
    depth: HashMap<String, Depth>,
    candles: HashMap<(String, Interval), CandleState>,
    generation: u64,
    funding: Option<TimerId>,
    seed_backoff: safe_cex_engine::startup::Backoff<bool>,
    skipped_markets: std::collections::HashSet<String>,
    /// Set by the first committed catalogue; later reads only refresh markets.
    catalogue_committed: bool,
    malformed_rows: usize,
    reread_stalls: u8,
    price_ranges: HashMap<String, f64>,
    ticker_indexes: HashMap<String, u32>,
}

pub(crate) fn compact(native: &str) -> String {
    native
        .strip_prefix("PERP_")
        .unwrap_or(native)
        .replace('_', "")
}
pub(crate) fn native(m: &Market) -> String {
    format!("PERP_{}_{}", m.base, m.quote)
}
fn candle(raw: Kline) -> Option<Candle> {
    if raw.start_timestamp < 0.0
        || raw.low <= 0.0
        || raw.high < raw.low
        || raw.open <= 0.0
        || raw.close <= 0.0
        || raw.volume < 0.0
    {
        return None;
    }
    Some(Candle {
        timestamp: raw.start_timestamp / 1000.0,
        open: raw.open,
        high: raw.high,
        low: raw.low,
        close: raw.close,
        volume: raw.volume,
    })
}
impl Woo {
    fn signed(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        params: Vec<(String, String)>,
        body: Option<String>,
        pending: Pending,
    ) -> Result<safe_cex_engine::RequestId, String> {
        if method == Method::Get && path.contains("/public/") {
            let query = sign::query(&params, true);
            let path = if query.is_empty() {
                path.to_owned()
            } else {
                format!("{path}?{query}")
            };
            return Ok(cx.request(
                method,
                format!("{}{}", self.rest, path),
                vec![],
                None,
                pending,
            ));
        }
        let (Some(key), Some(secret)) = (&self.options.key, &self.options.secret) else {
            return Err("WOO private requests require credentials".into());
        };
        if self.options.public_data_only || key.is_empty() || secret.is_empty() {
            return Err("WOO private requests require credentials".into());
        }
        let read = self
            .options
            .read_credentials
            .as_ref()
            .filter(|_| method == Method::Get);
        let key = read.map_or(key.as_str(), |r| r.key.as_str());
        let secret = read.map_or(secret.as_str(), |r| r.secret.as_str());
        let request = sign::request(key, secret, cx.now, method, path, &params, body);
        let url = if request.path.starts_with("https://") {
            request.path
        } else {
            format!("{}{}", self.rest, request.path)
        };
        Ok(cx.request(method, url, request.headers, request.body, pending))
    }
    pub fn new(options: Options) -> Self {
        let options = safe_cex_crypto::secrets::Sensitive::new(options);
        Self {
            account: account::Account::new(),
            trading: trading::Trading::default(),
            history: history::History::default(),
            rest: if options.testnet {
                "https://api.staging.woox.io"
            } else {
                "https://api.woox.io"
            },
            options,
            public: Connection::default(),
            depth: HashMap::new(),
            candles: HashMap::new(),
            generation: 0,
            funding: None,
            seed_backoff: Default::default(),
            skipped_markets: Default::default(),
            catalogue_committed: false,
            malformed_rows: 0,
            reread_stalls: 0,
            price_ranges: HashMap::new(),
            ticker_indexes: HashMap::new(),
        }
    }
    pub fn price_range(&self, symbol: &str) -> Option<f64> {
        self.price_ranges.get(symbol).copied()
    }
    fn seed(&mut self, cx: &mut Cx<'_, Pending>, tickers: bool) {
        self.seed_read(
            cx,
            if tickers {
                seed_read::Kind::Tickers
            } else {
                seed_read::Kind::Markets
            },
        );
    }
    fn apply_ticks(&self, cx: &mut Cx<'_, Pending>, topic: &str, rows: Vec<Tick<'_>>) {
        #[derive(Clone, Copy)]
        enum Kind {
            Tickers,
            Bbos,
            Marks,
        }
        let kind = match topic {
            "tickers" => Kind::Tickers,
            "bbos" => Kind::Bbos,
            _ => Kind::Marks,
        };
        for row in rows {
            if !row.symbol.starts_with("PERP_") {
                continue;
            }
            // Like TS, live data only updates rows the REST ticker seed listed.
            let Some(&index) = self.ticker_indexes.get(row.symbol.as_ref()) else {
                continue;
            };
            let Some(t) = cx.store.tickers().get(index as usize) else {
                continue;
            };
            // Most WOO batches repeat existing values. Avoid the store's full-field
            // dirty-mask scan unless this topic changes a supplied field. Compare
            // bits to retain its signed-zero change semantics.
            let changed =
                |value: Option<f64>, old: f64| value.is_some_and(|v| v.to_bits() != old.to_bits());
            let dirty = match kind {
                Kind::Tickers => {
                    changed(row.close, t.last)
                        || changed(row.volume, t.volume)
                        || changed(row.amount, t.quote_volume)
                }
                Kind::Bbos => changed(row.bid, t.bid) || changed(row.ask, t.ask),
                Kind::Marks => changed(row.price, t.mark),
            };
            if !dirty {
                continue;
            }
            cx.store.update_ticker_at(index, |t| match kind {
                Kind::Tickers => {
                    if let Some(v) = row.close {
                        t.last = v;
                    }
                    if let Some(v) = row.volume {
                        t.volume = v;
                    }
                    if let Some(v) = row.amount {
                        t.quote_volume = v;
                    }
                }
                Kind::Bbos => {
                    if let Some(v) = row.bid {
                        t.bid = v;
                    }
                    if let Some(v) = row.ask {
                        t.ask = v;
                    }
                }
                Kind::Marks => {
                    if let Some(v) = row.price {
                        t.mark = v;
                    }
                }
            });
        }
    }
    fn funding_read(&mut self, cx: &mut Cx<'_, Pending>) {
        self.seed_read(cx, seed_read::Kind::Funding);
    }
    fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Err(error) = self.managed_connect(cx, false) {
            self.socket_error(cx, false, error);
        }
    }
    fn book_request(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(m) = cx.store.market(symbol) else {
            return;
        };
        let Some(state) = self.depth.get_mut(symbol) else {
            return;
        };
        self.generation += 1;
        state.generation = self.generation;
        if !matches!(state.sync, Sync::Buffer(_)) {
            state.sync = Sync::Buffer(Vec::new());
        }
        cx.request(
            Method::Get,
            format!("{}/v1/public/orderbook/{}", self.rest, native(m)),
            vec![],
            None,
            Pending::Depth {
                symbol: symbol.into(),
                generation: state.generation,
            },
        );
        state.seed_due = false;
    }
    fn book_retry(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(state) = self.depth.get_mut(symbol) else {
            return;
        };
        if let Sync::Retry(timer) = state.sync {
            cx.clear_timer(timer);
        }
        self.generation += 1;
        state.generation = self.generation;
        state.seed_due = false;
        state.book.clear();
        state.sync = Sync::Retry(cx.set_timer(1000.0));
        cx.book_changed(symbol);
    }
    fn apply(book: &mut Book, delta: Delta) {
        for (side, rows) in [(BookSide::Bids, delta.bids), (BookSide::Asks, delta.asks)] {
            for (p, q) in rows {
                book.apply(side, p.0, q.0);
            }
        }
    }
    fn depth_message(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, time: f64, text: &str) {
        let Some(state) = self.depth.get_mut(symbol) else {
            return;
        };
        let Ok(delta) = serde_json::from_str::<Delta>(text) else {
            self.book_retry(cx, symbol);
            return;
        };
        if !delta.valid() || !time.is_finite() || time <= 0.0 {
            self.book_retry(cx, symbol);
            return;
        }
        match &mut state.sync {
            Sync::Buffer(rows) if rows.len() < 2000 => rows.push(Buffered { time, delta }),
            Sync::Buffer(_) => self.book_retry(cx, symbol),
            Sync::Live(previous) => {
                if time <= *previous {
                    return;
                }
                if delta.previous.is_some_and(|p| p != *previous) {
                    self.book_retry(cx, symbol);
                    return;
                }
                *previous = time;
                Self::apply(&mut state.book, delta);
                cx.book_changed(symbol);
            }
            Sync::Retry(_) => (),
        }
    }
    fn subscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        if let Some(state) = self.depth.get_mut(&symbol) {
            state.count += 1;
            return;
        }
        self.generation += 1;
        self.depth.insert(
            symbol.clone(),
            Depth {
                count: 1,
                generation: self.generation,
                seed_due: true,
                book: Book::default(),
                sync: Sync::Buffer(Vec::new()),
            },
        );
        self.sync_public_interests(cx);
    }
    fn stream_candle(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        interval: Interval,
        raw: Kline,
    ) {
        let Some(mut next) = candle(raw) else {
            return;
        };
        let Some(state) = self.candles.get_mut(&(symbol.into(), interval)) else {
            return;
        };
        if let Some(old) = &state.last {
            if next.timestamp < old.timestamp {
                return;
            }
            if next.timestamp == old.timestamp {
                next.high = next.high.max(old.high);
                next.low = next.low.min(old.low);
                next.open = old.open;
            }
        }
        state.last = Some(next);
        cx.candle(symbol, interval, next);
    }
    fn trade_candles(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, time: f64, text: &str) {
        #[derive(serde::Deserialize)]
        struct Trade {
            #[serde(deserialize_with = "number")]
            p: f64,
            #[serde(deserialize_with = "number")]
            a: f64,
        }
        let Ok(rows) = serde_json::from_str::<Vec<Trade>>(text) else {
            return;
        };
        // The TS stream synthesizes from the first trade in each message.
        let Some(t) = rows.first().filter(|t| t.p > 0.0 && t.a >= 0.0) else {
            return;
        };
        for ((s, interval), state) in &mut self.candles {
            if s != symbol {
                continue;
            }
            let ms = interval.ms();
            let start = (time / ms).floor() * ms / 1000.0;
            if interval.as_str().ends_with('s')
                && state.last.as_ref().is_none_or(|c| c.timestamp != start)
            {
                if state.last.as_ref().is_some_and(|c| c.timestamp > start) {
                    continue;
                }
                state.last = Some(Candle {
                    timestamp: start,
                    open: state.last.as_ref().map_or(t.p, |c| c.close),
                    high: t.p,
                    low: t.p,
                    close: t.p,
                    volume: 0.0,
                });
            }
            let Some(c) = &mut state.last else {
                continue;
            };
            if start < c.timestamp {
                continue;
            }
            if start > c.timestamp {
                *c = Candle {
                    timestamp: start,
                    open: c.close,
                    high: c.close,
                    low: c.close,
                    close: c.close,
                    volume: 0.0,
                };
            }
            c.high = c.high.max(t.p);
            c.low = c.low.min(t.p);
            c.close = t.p;
            c.volume += t.a;
            cx.candle(s, *interval, *c);
        }
    }
    fn fetch_candles(&self, cx: &mut Cx<'_, Pending>, cmd: Command) {
        let Command::FetchCandles {
            command,
            symbol,
            interval,
            limit,
            from_ms: from,
            to_ms: to,
        } = cmd
        else {
            unreachable!()
        };
        let symbol = symbol.as_str();
        let Some(m) = cx.store.market(symbol) else {
            cx.fail(command, "Market not found");
            return;
        };
        if from.is_some_and(|v| !v.is_finite())
            || to.is_some_and(|v| !v.is_finite())
            || from.zip(to).is_some_and(|(a, b)| a > b)
        {
            cx.fail(command, "Invalid WOO candle range or interval");
            return;
        }
        let size = limit.unwrap_or(500).clamp(1, 1000);
        let ms = interval.ms();
        let recent = (((cx.now / ms).floor() * ms - to.unwrap_or(0.0)).abs() / ms) < 100.0;
        let (mut url, at) = if recent {
            (
                format!(
                    "{}/v1/kline?symbol={}&type={interval}&limit={size}",
                    self.rest,
                    native(m)
                ),
                ArrayAt::Field("rows"),
            )
        } else {
            (
                format!(
                    "https://api-pub.woox.io/v1/hist/kline?symbol={}&type={interval}&size={size}",
                    native(m)
                ),
                ArrayAt::Path(&["data", "rows"]),
            )
        };
        if !recent {
            if let Some(v) = from {
                url.push_str(&format!("&start_time={}", num::format(v)));
            }
            if let Some(v) = to {
                url.push_str(&format!("&end_time={}", num::format(v)));
            }
        }
        let id = cx.request_streamed(
            Method::Get,
            url,
            vec![],
            at,
            Pending::Candles {
                command,
                rows: Vec::new(),
                bad: false,
            },
        );
        if !recent {
            cx.route_request(id, safe_cex_engine::RequestRoute::Primary);
        }
    }
}

impl Venue for Woo {
    fn managed_sockets() -> bool {
        true
    }
    fn cancel_needs_stored_order(&self) -> bool {
        true
    }

    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            position_history: safe_cex_engine::PositionHistory::WooOrders,
            fill_history_time: safe_cex_engine::FillHistoryTime::NonnegativeSecondsOrMilliseconds,
            cancel_all: safe_cex_engine::CancelAll::Native,
            leverage_query: safe_cex_engine::LeverageQuery::OnMetadata,
            account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            return;
        }
        self.seed(cx, false);
    }
    fn socket_control(
        &self,
        _: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        _: bool,
        text: &str,
    ) -> safe_cex_engine::socket_lifecycle::Control {
        self.managed_control(feed, text)
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.managed_event(cx, event)
    }
    fn on_socket_interests_released(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        epoch: safe_cex_engine::socket_lifecycle::Epoch,
        pending: Pending,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        if self.public.feed != Some(epoch.feed())
            || !self.public.open
            || cx.socket_epoch(epoch.feed()) != Some(epoch)
        {
            return Ok(());
        }
        if let Pending::SocketBookSeeds(seeds) = pending {
            for (symbol, generation) in seeds {
                if self
                    .depth
                    .get(&symbol)
                    .is_some_and(|state| state.seed_due && state.generation == generation)
                {
                    self.book_request(cx, &symbol);
                }
            }
        }
        Ok(())
    }
    fn encode_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        subscribe: bool,
        items: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.managed_encode(store, feed, subscribe, items, emit)
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account_message(cx, socket, text) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        if let Ok(msg) = serde_json::from_str::<TickerMessage<'_>>(text)
            && msg.event.is_empty()
            && matches!(msg.topic.as_ref(), "tickers" | "bbos" | "markprices")
        {
            self.apply_ticks(cx, msg.topic.as_ref(), msg.data);
            return;
        }
        let Ok(msg) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        if !msg.event.is_empty() {
            if msg.success == Some(false) {
                cx.notice(
                    Level::Error,
                    code::SUBSCRIPTION_REJECTED,
                    "WOO public subscription rejected",
                );
            }
            match msg.event.as_ref() {
                "ping" => cx.send(socket, r#"{"event":"pong"}"#),
                "pong" => cx
                    .store
                    .set_latency((cx.now - self.public.ping_at).max(0.0)),
                _ => (),
            }
            return;
        }
        let Some(data) = msg.data else {
            return;
        };
        match msg.topic.as_ref() {
            "tickers" | "bbos" | "markprices" => (),
            topic => {
                let Some((symbol, kind)) = topic.split_once('@') else {
                    return;
                };
                let symbol = compact(symbol);
                if kind == "orderbookupdate" {
                    self.depth_message(cx, &symbol, msg.ts, data.get());
                } else if let Some(interval) = kind.strip_prefix("kline_").and_then(Interval::parse)
                {
                    if let Ok(raw) = serde_json::from_str(data.get()) {
                        self.stream_candle(cx, &symbol, interval, raw);
                    }
                } else if kind == "trades" {
                    self.trade_candles(cx, &symbol, msg.ts, data.get());
                }
            }
        }
    }
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Identity(_) => {}
            Pending::History(request) => self.history_item(cx, request, item),
            Pending::Trade(_) => (),
            Pending::Account(request) => self.account_item(cx, request, item),
            Pending::Seed(read) => self.seed_item(cx, read, item),
            #[cfg(test)]
            Pending::SigningProbe => (),
            Pending::Candles { rows, bad, .. } => {
                if let Some(c) = serde_json::from_str(item).ok().and_then(candle) {
                    rows.push(c);
                } else {
                    *bad = true;
                }
            }
            Pending::Depth { .. } | Pending::SocketBookSeeds(_) => (),
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Identity(command) = pending {
            self.identity_response(cx, command, response);
            return;
        }
        let pending = match pending {
            Pending::Trade(request) => {
                self.trade_response(cx, request, response);
                return;
            }
            Pending::History(request) => {
                self.history_response(cx, request, response);
                return;
            }
            other => other,
        };
        if let Pending::Account(request) = pending {
            self.account_response(cx, request, response);
            return;
        }
        if let Pending::Seed(read) = pending {
            self.seed_response(cx, *read, response);
            return;
        }
        if !(200..300).contains(&response.status) || Envelope::checked(response.body).is_err() {
            self.on_request_failed(cx, pending, "WOO read rejected");
            return;
        }
        match pending {
            Pending::SocketBookSeeds(_) => (),
            Pending::Identity(_) | Pending::History(_) | Pending::Trade(_) => unreachable!(),
            Pending::Account(_) => unreachable!(),
            Pending::Seed(_) => unreachable!(),
            #[cfg(test)]
            Pending::SigningProbe => (),
            Pending::Candles {
                command,
                mut rows,
                bad,
            } => {
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                {
                    let result: Result<String, String> = if bad {
                        Err("Invalid WOO candle page".into())
                    } else {
                        Ok(serde_json::to_string(&rows).unwrap())
                    };
                    match result {
                        Ok(data) => cx.done(command, data),
                        Err(error) => cx.fail(command, error),
                    }
                };
            }
            Pending::Depth { symbol, generation } => {
                let Some(state) = self
                    .depth
                    .get_mut(&symbol)
                    .filter(|s| s.generation == generation)
                else {
                    return;
                };
                let Ok(raw) = serde_json::from_str::<Snapshot>(response.body) else {
                    self.book_retry(cx, &symbol);
                    return;
                };
                if !raw.success
                    || raw.timestamp <= 0.0
                    || !raw
                        .bids
                        .iter()
                        .chain(&raw.asks)
                        .all(|l| l.price > 0.0 && l.quantity >= 0.0)
                {
                    self.book_retry(cx, &symbol);
                    return;
                }
                if !matches!(state.sync, Sync::Buffer(_)) {
                    return;
                }
                let Sync::Buffer(buffer) =
                    std::mem::replace(&mut state.sync, Sync::Live(raw.timestamp))
                else {
                    unreachable!()
                };
                state.book.clear();
                for (side, levels) in [(BookSide::Bids, raw.bids), (BookSide::Asks, raw.asks)] {
                    for l in levels {
                        state.book.apply(side, l.price, l.quantity);
                    }
                }
                let mut last = raw.timestamp;
                for b in buffer {
                    if b.time > last {
                        // The first delta may overlap a REST snapshot between WS ticks.
                        if b.delta.previous.is_some_and(|previous| {
                            previous > last || (last > raw.timestamp && previous != last)
                        }) {
                            self.book_retry(cx, &symbol);
                            return;
                        }
                        last = b.time;
                        Self::apply(&mut state.book, b.delta);
                    }
                }
                state.sync = Sync::Live(last);
                cx.book_changed(&symbol);
            }
        }
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: safe_cex_engine::request_context::Error,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Seed(read) => self.seed_precommit_failed(cx, *read, reason, error),
            other => self.on_sign_error(cx, other, error),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::SocketBookSeeds(_) => (),
            Pending::Identity(command) => self.identity_failed(cx, command, error),
            Pending::History(request) => self.history_failed(cx, request, error),
            Pending::Trade(request) => self.trade_failed(cx, request, error),
            Pending::Account(request) => self.account_failed(cx, request, error),
            Pending::Seed(read) => self.seed_failed(cx, *read, error, true),
            #[cfg(test)]
            Pending::SigningProbe => (),
            Pending::Depth { symbol, generation } => {
                if self
                    .depth
                    .get(&symbol)
                    .is_some_and(|s| s.generation == generation)
                {
                    self.book_retry(cx, &symbol);
                }
            }
            Pending::Candles { command, .. } => cx.fail(command, error),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.trade_timer(cx, timer) || self.history_timer(cx, timer) {
            return;
        }
        if self.account_timer(cx, timer) {
            return;
        }
        if !cx.now.is_finite() {
            return;
        }
        if let Some(tickers) = self.seed_backoff.on_timer(timer) {
            self.seed(cx, tickers);
            return;
        }
        if self.funding == Some(timer) {
            self.funding_read(cx);
            self.funding = Some(cx.set_timer(60_000.0));
            return;
        }
        let retry = self.depth.iter().find_map(|(s, d)| {
            if matches!(d.sync,Sync::Retry(t) if t==timer) {
                Some(s.clone())
            } else {
                None
            }
        });
        if let Some(symbol) = retry {
            if let Some(state) = self.depth.get_mut(&symbol) {
                state.seed_due = true;
                state.sync = Sync::Buffer(Vec::new());
            }
            self.sync_public_interests(cx);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::FetchAccount { command } = command {
            self.fetch_identity(cx, command);
            return;
        }
        if self.account_command(cx, &command) {
            return;
        }
        match command {
            Command::SetTickerSymbols { .. } => (),
            Command::SubscribeOrderBook { symbol } => self.subscribe_book(cx, symbol),
            Command::UnsubscribeOrderBook { symbol } => {
                let Some(state) = self.depth.get_mut(&symbol) else {
                    return;
                };
                state.count = state.count.saturating_sub(1);
                if state.count > 0 {
                    return;
                }
                if let Some(d) = self.depth.remove(&symbol)
                    && let Sync::Retry(t) = d.sync
                {
                    cx.clear_timer(t);
                }
                self.sync_public_interests(cx);
            }
            Command::SubscribeCandles { symbol, interval } => {
                if cx.store.market(&symbol).is_none() {
                    return;
                }
                let state = self.candles.entry((symbol.clone(), interval)).or_default();
                state.count += 1;
                if state.count > 1 {
                    return;
                }
                self.sync_public_interests(cx);
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval);
                let Some(s) = self.candles.get_mut(&key) else {
                    return;
                };
                s.count = s.count.saturating_sub(1);
                if s.count > 0 {
                    return;
                }
                self.candles.remove(&key);
                self.sync_public_interests(cx);
            }
            cmd @ Command::FetchCandles { .. } => self.fetch_candles(cx, cmd),
            Command::Query { command, query } => self.history_command(cx, command, query),
            command => self.trade_command(cx, command),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.depth
            .get(symbol)
            .filter(|s| matches!(s.sync, Sync::Live(_)))
            .map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.depth.iter().map(|(s, d)| (s.as_str(), &d.book)))
    }
}

/// A refresh keeps every listed market the new catalogue omits or filters out, unchanged.
fn keep_listed(
    listed: &[safe_cex_engine::model::Market],
    rows: Vec<safe_cex_engine::model::Market>,
) -> Vec<safe_cex_engine::model::Market> {
    let mut fresh: Vec<_> = rows.into_iter().map(Some).collect();
    let mut index = std::collections::HashMap::new();
    for (i, market) in fresh.iter().enumerate() {
        if let Some(market) = market {
            index.entry(market.symbol.clone()).or_insert(i);
        }
    }
    let mut merged: Vec<_> = listed
        .iter()
        .map(|old| {
            index
                .get(&old.symbol)
                .and_then(|&i| fresh[i].take())
                .unwrap_or_else(|| old.clone())
        })
        .collect();
    merged.extend(fresh.into_iter().flatten());
    merged
}

/// Whether a partial catalogue re-reads; five refreshes in a row that leave its
/// malformed set no smaller end the retry.
fn reread_partial<P>(
    cx: &mut safe_cex_engine::Cx<'_, P>,
    stalls: &mut u8,
    refresh: bool,
    before: usize,
    after: usize,
) -> bool {
    if after == 0 {
        *stalls = 0;
        return false;
    }
    *stalls = if refresh && after >= before {
        stalls.saturating_add(1)
    } else {
        0
    };
    if *stalls == 5 {
        cx.notice(
            Level::Warn,
            code::REQUEST_FAILED,
            format!("WOO stops re-reading a catalogue with {after} malformed markets"),
        );
    }
    *stalls < 5
}

#[cfg(test)]
mod secondary_tests;
