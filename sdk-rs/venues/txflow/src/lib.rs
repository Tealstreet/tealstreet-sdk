//! TxFlow testnet perpetuals, using separate node and API-service feeds.
mod account;
mod history;
mod local_context;
pub mod sign;
mod socket_codec;
mod trading;
mod wire;

use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::{self, ArrayAt};
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{FeedId, SocketError};
use safe_cex_engine::{
    Command, CommandId, Cx, Method, PublicFeedPolicy, Response, SocketChannel, SocketId, TimerId,
    Venue,
};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use wire::*;

pub const REST_URL: &str = "https://rpc.txflow-testnet.xyz";
pub const API_VERSION: &str = "2026-08-16";
#[derive(Clone, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub application_id: Option<String>,
    pub vault: Option<String>,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
    }
}

impl Options {
    pub fn validate(&self) -> Result<(), String> {
        if !self.testnet {
            return Err("TxFlow mainnet is offline; testnet is required".into());
        }
        if let Some(vault) = self.vault.as_deref() {
            safe_cex_crypto::ethereum::get_address(vault)?;
        }
        if !self.public_data_only {
            safe_cex_crypto::ethereum::get_address(self.application_id.as_deref().unwrap_or(""))?;
            safe_cex_crypto::ethereum::private_key_address(self.secret.as_deref().unwrap_or(""))?;
        }
        Ok(())
    }
}
pub enum Pending {
    Meta {
        rows: Vec<Instrument>,
        valid: bool,
        /// A malformed row was skipped, so an unknown asset may be listed.
        skipped: bool,
    },
    Ticker {
        asset: u32,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        valid: bool,
    },
    Account(account::Request),
    Trade(trading::Request),
    History(Box<history::Request>),
    SocketRefresh,
}
#[derive(Default)]
struct Connection {
    feed: Option<FeedId>,
    open: bool,
    refresh_due: bool,
}
#[derive(Default)]
struct Ladder {
    subscribers: usize,
    book: Book,
    time: Option<f64>,
    /// A snapshot was accepted; resync keeps the ladder listed so snapshots clear it.
    published: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sub<'a> {
    r#type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    coin: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interval: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instrument_id: Option<u32>,
}
pub struct TxFlow {
    pub(crate) options: safe_cex_crypto::secrets::Sensitive<Options>,
    pub(crate) account: account::AccountState,
    pub(crate) trading: trading::Trading,
    pub(crate) assets: HashMap<String, u32>,
    pub(crate) coins: HashMap<String, String>,
    pub(crate) instruments: HashMap<u32, Instrument>,
    historical_assets: HashMap<String, Option<u32>>,
    historical_coins: HashMap<String, String>,
    node: Connection,
    api: Connection,
    books: HashMap<String, Ladder>,
    candles: HashMap<(String, Interval), usize>,
    ticker_queue: VecDeque<u32>,
    seed_tickers: Vec<Ticker>,
    ticker_seeded: std::collections::HashSet<u32>,
    ticker_timer: Option<TimerId>,
    discovery: safe_cex_engine::startup::Backoff<Option<u32>>,
    live_tickers: safe_cex_engine::startup::LiveTickers,
    catalogue_partial: bool,
}
impl TxFlow {
    pub fn new(options: Options) -> Self {
        let options = safe_cex_crypto::secrets::Sensitive::new(options);
        Self {
            options,
            account: Default::default(),
            trading: Default::default(),
            assets: Default::default(),
            coins: Default::default(),
            instruments: Default::default(),
            historical_assets: Default::default(),
            historical_coins: Default::default(),
            node: Default::default(),
            api: Default::default(),
            books: Default::default(),
            candles: Default::default(),
            ticker_queue: Default::default(),
            seed_tickers: Vec::new(),
            ticker_seeded: Default::default(),
            ticker_timer: None,
            discovery: Default::default(),
            live_tickers: Default::default(),
            catalogue_partial: false,
        }
    }
    pub(crate) fn rest(&self) -> &'static str {
        REST_URL
    }
    pub(crate) fn asset(&self, _cx: &Cx<'_, Pending>, symbol: &str) -> Option<u32> {
        self.assets.get(symbol).copied()
    }
    /// The catalogue is complete and its read awaits no retry; ticker seeds do not list assets.
    pub(crate) fn discovery_settled(&self) -> bool {
        !self.catalogue_partial && !self.discovery.waiting(&None)
    }
    pub(crate) fn symbol(&self, asset: u32) -> Option<&str> {
        self.instruments.get(&asset).map(|row| row.name.as_str())
    }
    pub(crate) fn coin_symbol(&self, coin: &str) -> Option<&str> {
        self.coins
            .get(coin)
            .or_else(|| self.coins.get(&coin.to_ascii_lowercase()))
            .map(String::as_str)
    }
    pub(crate) fn info(
        &self,
        cx: &mut Cx<'_, Pending>,
        body: String,
        at: ArrayAt,
        pending: Pending,
    ) {
        #[derive(serde::Deserialize)]
        struct Kind<'a> {
            r#type: &'a str,
        }
        let public = serde_json::from_str::<Kind>(&body).is_ok_and(|k| {
            matches!(
                k.r#type,
                "perpMeta"
                    | "activeAssetCtx"
                    | "candleSnapshot"
                    | "l2Book"
                    | "recentTrades"
                    | "trades"
            )
        });
        let id = cx.request_streamed_body(
            Method::Post,
            format!("{}/info", self.rest()),
            vec![
                ("Content-Type", "application/json".into()),
                ("x-api-version", API_VERSION.into()),
            ],
            Some(body),
            at,
            pending,
        );
        if public {
            cx.route_request(id, safe_cex_engine::RequestRoute::PublicCors);
        }
    }
    fn metadata(&self, cx: &mut Cx<'_, Pending>) {
        self.info(
            cx,
            "{\"type\":\"perpMeta\",\"includeDelisted\":true}".into(),
            ArrayAt::Path(&["data", "universe"]),
            Pending::Meta {
                rows: Vec::new(),
                valid: true,
                skipped: false,
            },
        );
    }
    fn ticker_request(&self, cx: &mut Cx<'_, Pending>, asset: u32) {
        let id = cx.request(
            Method::Post,
            format!("{}/info", self.rest()),
            vec![
                ("Content-Type", "application/json".into()),
                ("x-api-version", API_VERSION.into()),
            ],
            Some(format!(
                "{{\"type\":\"activeAssetCtx\",\"coin\":\"{asset}\"}}"
            )),
            Pending::Ticker { asset },
        );
        cx.route_request(id, safe_cex_engine::RequestRoute::PublicCors);
    }
    fn next_ticker(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(asset) = self.ticker_queue.pop_front() {
            self.ticker_request(cx, asset);
            return;
        }
        if !self.seed_tickers.is_empty() && self.ticker_seeded.len() == self.seed_tickers.len() {
            self.ticker_seeded.clear();
            let mut rows = std::mem::take(&mut self.seed_tickers);
            self.live_tickers.finish(cx.store, &mut rows);
            cx.store.set_tickers(rows);
            self.index_tickers(cx.store);
            self.open_sockets(cx);
        }
        // TS records the account execution window after the paced seed; once every
        // read has been attempted, a market still retrying must not block private data.
        if !self.assets.is_empty() {
            self.account_start(cx);
        }
    }
    fn open_sockets(&mut self, cx: &mut Cx<'_, Pending>) {
        for api in [false, true] {
            if (if api { &self.api } else { &self.node }).feed.is_none()
                && let Err(error) = self.managed_connect(cx, api)
            {
                Self::socket_refused(cx, error);
            }
        }
    }
    fn context(cx: &mut Cx<'_, Pending>, symbol: &str, c: Context) {
        cx.store
            .update_ticker(symbol, |t| Self::context_update(t, c));
    }
    fn context_mask(c: &Context) -> u16 {
        (u16::from(c.last_price.is_some_and(|n| n > 0.0)) << 2)
            | (u16::from(c.mark_px.is_some()) << 3)
            | (u16::from(c.oracle_px.is_some()) << 4)
            | (u16::from(c.prev_day_px.is_some_and(|n| n > 0.0)) << 5)
            | (u16::from(c.open_interest.is_some()) << 6)
            | (u16::from(c.funding.is_some()) << 7)
            | (u16::from(c.next_funding_time.is_some()) << 8)
            | (u16::from(c.day_base_vlm.is_some()) << 9)
            | (u16::from(c.day_ntl_vlm.is_some()) << 10)
            | if c.mid_px.is_some_and(|n| n > 0.0) {
                safe_cex_engine::startup::LiveTickers::MID_PRICE
            } else {
                0
            }
    }
    fn context_update(t: &mut Ticker, c: Context) {
        if let Some(mark) = c.mark_px {
            t.mark = mark;
        }
        if let Some(index) = c.oracle_px {
            t.index = index;
        }
        if let Some(open_interest) = c.open_interest {
            t.open_interest = open_interest * t.mark;
        }
        if let Some(funding) = c.funding {
            t.funding_rate = funding / 100.0;
        }
        if let Some(next) = c.next_funding_time {
            t.funding_time = next;
        }
        if let Some(mid) = c.mid_px.filter(|n| *n > 0.0) {
            t.mid_price = Some(mid);
        }
        if let Some(last) = c.last_price.filter(|n| *n > 0.0) {
            t.last = last;
        }
        if let Some(previous) = c.prev_day_px.filter(|n| *n > 0.0) {
            t.percentage = (t.last - previous) / previous * 100.0;
        }
        if let Some(volume) = c.day_base_vlm {
            t.volume = volume;
        }
        if let Some(volume) = c.day_ntl_vlm {
            t.quote_volume = volume;
        }
    }
    fn ticker_mask(row: &TickerRow, seed: bool) -> u16 {
        let mut mask = (u16::from(row.last_price.is_some()) << 2)
            | (u16::from(row.prev_day_px.is_some()) << 5)
            | (u16::from(row.coin_vlm.is_some()) << 9)
            | (u16::from(row.day_ntl_vlm.is_some()) << 10);
        if seed {
            mask |= (u16::from(row.mark_px.is_some()) << 3)
                | (u16::from(row.oracle_px.is_some()) << 4)
                | (u16::from(row.open_interest.is_some()) << 6);
        }
        mask | row.node_ctx.as_ref().map_or(0, Self::context_mask)
    }
    fn ticker_update(t: &mut Ticker, row: TickerRow, seed: bool) {
        if let Some(last) = row.last_price {
            t.last = last;
        }
        if let Some(volume) = row.coin_vlm {
            t.volume = volume;
        }
        if let Some(volume) = row.day_ntl_vlm {
            t.quote_volume = volume;
        }
        if let Some(previous) = row.prev_day_px {
            t.percentage = if previous > 0.0 {
                (t.last - previous) / previous * 100.0
            } else {
                0.0
            };
        }
        if seed {
            if let Some(mark) = row.mark_px {
                t.mark = mark;
            }
            if let Some(index) = row.oracle_px {
                t.index = index;
            }
            if let Some(open_interest) = row.open_interest {
                t.open_interest = open_interest * t.mark;
            }
        }
        if let Some(ctx) = row.node_ctx {
            Self::context_update(t, ctx);
        }
    }
    fn apply_ticker(cx: &mut Cx<'_, Pending>, symbol: &str, row: TickerRow, seed: bool) {
        cx.store
            .update_ticker(symbol, |t| Self::ticker_update(t, row, seed));
    }
    fn index_tickers(&mut self, store: &safe_cex_engine::Store) {
        for instrument in self.instruments.values_mut() {
            instrument.ticker_index = None;
        }
        for (index, ticker) in store.tickers().iter().enumerate() {
            if let Some(instrument) = self
                .assets
                .get(&ticker.symbol)
                .and_then(|asset| self.instruments.get_mut(asset))
            {
                instrument.ticker_index = Some(index as u32);
            }
        }
    }
    fn public_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) -> bool {
        let Ok(frame) = PublicFrame::decode(text) else {
            return false;
        };
        // Errors are transport controls even when this mixed socket owns no public feed.
        if cx.public_feed_policy() != PublicFeedPolicy::Direct
            && !matches!(
                &frame,
                PublicFrame::Error(_) | PublicFrame::Acknowledgement(_) | PublicFrame::Other
            )
        {
            return false;
        }
        match frame {
            PublicFrame::Contexts(rows) => {
                for row in rows.ctxs {
                    if let Some(symbol) = self.coin_symbol(&row.coin).map(str::to_owned) {
                        let mask = Self::context_mask(&row.ctx);
                        Self::context(cx, &symbol, row.ctx);
                        self.live_tickers.record(cx.store, &symbol, mask);
                    }
                }
            }
            PublicFrame::Context(row) => {
                if let Some(symbol) = self.coin_symbol(&row.coin).map(str::to_owned) {
                    let mask = Self::context_mask(&row.ctx);
                    Self::context(cx, &symbol, row.ctx);
                    self.live_tickers.record(cx.store, &symbol, mask);
                }
            }
            PublicFrame::Ticker(row) => {
                if let Some(instrument) = self.instruments.get(&row.instrument_id) {
                    let symbol = instrument.name.as_str();
                    let mask = Self::ticker_mask(&row, false);
                    if let Some(index) = instrument.ticker_index.filter(|index| {
                        cx.store
                            .tickers()
                            .get(*index as usize)
                            .is_some_and(|ticker| ticker.symbol == symbol)
                    }) {
                        cx.store.update_ticker_at(index, |ticker| {
                            Self::ticker_update(ticker, row, false)
                        });
                    } else {
                        Self::apply_ticker(cx, symbol, row, false);
                    }
                    self.live_tickers.record(cx.store, symbol, mask);
                }
            }
            PublicFrame::Depth(row) => {
                if let Some(symbol) = self.coin_symbol(&row.coin).map(str::to_owned)
                    && let Some(ladder) = self.books.get_mut(&symbol)
                    && ladder.time.is_none_or(|t| row.time >= t)
                {
                    if row
                        .levels
                        .iter()
                        .flatten()
                        .any(|l| l.px <= 0.0 || l.sz < 0.0)
                    {
                        return false;
                    }
                    for (index, side) in [BookSide::Bids, BookSide::Asks].into_iter().enumerate() {
                        ladder.book.load_sorted(
                            side,
                            row.levels[index]
                                .iter()
                                .filter(|l| l.sz > 0.0)
                                .map(|l| (l.px, l.sz)),
                        );
                    }
                    ladder.time = Some(row.time);
                    ladder.published = true;
                    cx.book_changed(&symbol);
                }
            }
            PublicFrame::Candle(data) => {
                let mut apply = |row: CandleRow<'_>| {
                    if let Some(symbol) = self.coin_symbol(&row.s)
                        && let Some(interval) = Interval::parse(&row.i)
                        && self.candles.contains_key(&(symbol.to_owned(), interval))
                    {
                        cx.candle(symbol, interval, row.candle());
                    }
                };
                let raw = data.get();
                if raw.starts_with('[') {
                    let _ = json::each(raw, &mut apply);
                } else if let Ok(row) = serde_json::from_str(raw) {
                    apply(row);
                }
            }
            PublicFrame::Error(error) => {
                return error.code.as_deref() == Some("RISK_RATE_LIMITED")
                    || serde_json::from_str::<ErrorCode<'_>>(text)
                        .is_ok_and(|error| error.code.as_deref() == Some("RISK_RATE_LIMITED"));
            }
            PublicFrame::Acknowledgement(_) | PublicFrame::Other => {}
        }
        false
    }
    fn book_sub(&mut self, cx: &mut Cx<'_, Pending>, symbol: String, add: bool) {
        if cx.store.loaded().markets && !self.assets.contains_key(&symbol) {
            return;
        }
        let ladder = self.books.entry(symbol.clone()).or_default();
        if add {
            ladder.subscribers += 1;
            if ladder.subscribers > 1 {
                return;
            }
        } else {
            ladder.subscribers = ladder.subscribers.saturating_sub(1);
            if ladder.subscribers > 0 {
                return;
            }
            self.books.remove(&symbol);
        }
        self.propose_socket_interests(cx, false);
    }
    fn candle_sub(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        interval: Interval,
        add: bool,
    ) {
        if !supported(interval) {
            if add {
                cx.notice(
                    Level::Error,
                    code::COMMAND_UNSUPPORTED,
                    interval.unsupported("TxFlow"),
                );
            }
            return;
        }
        if cx.store.loaded().markets && !self.assets.contains_key(&symbol) {
            return;
        }
        let count = self.candles.entry((symbol.clone(), interval)).or_default();
        if add {
            *count += 1;
            if *count > 1 {
                return;
            }
        } else {
            *count = count.saturating_sub(1);
            if *count > 0 {
                return;
            }
            self.candles.remove(&(symbol.clone(), interval));
        }
        self.propose_socket_interests(cx, true);
    }
    fn fetch_candles(&self, cx: &mut Cx<'_, Pending>, request: Command) {
        let Command::FetchCandles {
            command,
            symbol,
            interval,
            limit,
            from_ms,
            to_ms,
        } = request
        else {
            return;
        };
        let Some(asset) = self.asset(cx, &symbol) else {
            local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Read,
                Err("Market not found".into()),
            );
            return;
        };
        if !supported(interval) {
            local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Read,
                Err(interval.unsupported("TxFlow").into()),
            );
            return;
        }
        let end = to_ms.unwrap_or(cx.now);
        let start = from_ms
            .unwrap_or(end - interval.ms() * f64::from(limit.unwrap_or(500).min(1500)))
            .max(0.0);
        if !start.is_finite()
            || !end.is_finite()
            || end < 0.0
            || start > end
            || end > 9_007_199_254_740_991.0
            || from_ms.is_some_and(|t| t < 0.0)
        {
            local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Read,
                Err("Invalid candle time range".into()),
            );
            return;
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Req<'a> {
            coin: String,
            interval: &'a str,
            start_time: u64,
            end_time: u64,
        }
        #[derive(Serialize)]
        struct Read<'a> {
            r#type: &'a str,
            req: Req<'a>,
        }
        let body = serde_json::to_string(&Read {
            r#type: "candleSnapshot",
            req: Req {
                coin: asset.to_string(),
                interval: interval.as_str(),
                start_time: start.floor() as u64,
                end_time: end.floor() as u64,
            },
        })
        .unwrap();
        self.info(
            cx,
            body,
            ArrayAt::Field("data"),
            Pending::Candles {
                command,
                rows: Vec::new(),
                valid: true,
            },
        );
    }
}
impl Venue for TxFlow {
    fn cancel_needs_stored_order(&self) -> bool {
        true
    }

    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            reject_order_history_filters: true,
            leverage_query: safe_cex_engine::LeverageQuery::OnRead,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Err(error) = self.options.validate() {
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            return;
        }
        self.metadata(cx);
    }
    fn on_open(&mut self, _cx: &mut Cx<'_, Pending>, _socket: SocketId) {}
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), SocketError> {
        self.managed_socket_event(cx, event)
    }
    fn on_socket_interests_released(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        epoch: safe_cex_engine::socket_lifecycle::Epoch,
        pending: Pending,
    ) -> Result<(), SocketError> {
        if self.node.feed != Some(epoch.feed()) || cx.socket_epoch(epoch.feed()) != Some(epoch) {
            return Err(SocketError::Stale);
        }
        if matches!(pending, Pending::SocketRefresh) && self.node.refresh_due {
            self.account_refresh(cx);
        }
        Ok(())
    }
    fn encode_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        feed: FeedId,
        subscribe: bool,
        interests: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        self.managed_socket_encode(store, feed, subscribe, interests, emit)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(feed) = [self.node.feed, self.api.feed]
            .into_iter()
            .flatten()
            .find(|feed| {
                cx.socket_epoch(*feed)
                    .is_some_and(|epoch| epoch.socket() == socket)
            })
        else {
            return;
        };
        let node = self.node.feed == Some(feed);
        if self.public_frame(cx, text) {
            cx.notice(
                Level::Warn,
                code::WS_DISCONNECTED,
                "TxFlow socket rate limited; reconnecting subscriptions",
            );
            cx.retire_socket(
                feed,
                safe_cex_engine::socket_lifecycle::RetireReason::Closed,
            );
            return;
        }
        if node {
            self.account_node_frame(cx, text);
        } else {
            self.account_api_frame(cx, text);
        }
    }
    fn on_close(&mut self, _cx: &mut Cx<'_, Pending>, _socket: SocketId, _code: u16) {}
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if !(200..300).contains(&response.status) {
            self.on_request_failed(cx, pending, &format!("TxFlow HTTP {}", response.status));
            return;
        }
        match pending {
            Pending::Meta {
                rows,
                valid,
                skipped,
            } => {
                if !valid || !Header::valid(response.body) {
                    self.on_request_failed(
                        cx,
                        Pending::Meta {
                            rows,
                            valid: false,
                            skipped,
                        },
                        "Invalid TxFlow metadata",
                    );
                    return;
                }
                let mut historical_assets = HashMap::new();
                let mut historical_coins = HashMap::new();
                let mut identities = std::collections::HashSet::new();
                let mut valid = true;
                for row in rows.iter().filter(|r| r.currency_type == "usdc") {
                    if row.name.trim().is_empty()
                        || row.base_currency.trim().is_empty()
                        || !identities.insert(row.index)
                    {
                        valid = false;
                    }
                    historical_assets
                        .entry(row.name.clone())
                        .and_modify(|id| {
                            if *id != Some(row.index) {
                                *id = None;
                            }
                        })
                        .or_insert(Some(row.index));
                    for alias in [
                        row.index.to_string(),
                        row.name.to_ascii_lowercase(),
                        row.base_currency.to_ascii_lowercase(),
                    ] {
                        if historical_coins
                            .insert(alias, row.name.clone())
                            .is_some_and(|prior| prior != row.name)
                        {
                            valid = false;
                        }
                    }
                }
                if !valid {
                    self.on_request_failed(
                        cx,
                        Pending::Meta {
                            rows: Vec::new(),
                            valid: false,
                            skipped: false,
                        },
                        "Duplicate or ambiguous TxFlow historical identities",
                    );
                    return;
                }
                let mut markets = Vec::new();
                let mut assets = HashMap::new();
                let mut coins = HashMap::new();
                let mut instruments = HashMap::new();
                let mut ticker_queue = VecDeque::new();
                for row in rows {
                    if let Some(market) = row.market() {
                        assets.insert(market.symbol.clone(), row.index);
                        for key in [
                            row.index.to_string(),
                            row.name.to_ascii_lowercase(),
                            row.base_currency.to_ascii_lowercase(),
                        ] {
                            coins.insert(key, market.symbol.clone());
                        }
                        ticker_queue.push_back(row.index);
                        instruments.insert(row.index, row);
                        markets.push(market);
                    }
                }
                if markets.is_empty() {
                    self.on_request_failed(
                        cx,
                        Pending::Meta {
                            rows: Vec::new(),
                            valid: false,
                            skipped: false,
                        },
                        "No supported TxFlow markets",
                    );
                    return;
                }
                let mut active_names = std::collections::HashSet::new();
                if markets.iter().any(|m| !active_names.insert(&m.symbol)) {
                    self.on_request_failed(
                        cx,
                        Pending::Meta {
                            rows: Vec::new(),
                            valid: false,
                            skipped: false,
                        },
                        "Duplicate TxFlow tradable market identities",
                    );
                    return;
                }
                self.assets = assets;
                self.coins = coins;
                self.instruments = instruments;
                self.catalogue_partial = skipped;
                self.ticker_queue = ticker_queue;
                self.ticker_seeded.clear();
                self.historical_assets = historical_assets;
                self.historical_coins = historical_coins;
                let tickers = markets
                    .iter()
                    .map(|m| Ticker {
                        id: m.id.clone(),
                        symbol: m.symbol.clone(),
                        ..Default::default()
                    })
                    .collect();
                self.books
                    .retain(|symbol, _| self.assets.contains_key(symbol));
                self.candles
                    .retain(|(symbol, _), _| self.assets.contains_key(symbol));
                self.discovery.succeeded(cx, &None);
                cx.store.set_markets(markets);
                self.open_sockets(cx);
                self.seed_tickers = tickers;
                self.live_tickers.begin();
                self.next_ticker(cx);
            }
            Pending::Ticker { asset } => {
                match serde_json::from_str::<Reply<TickerRow>>(response.body) {
                    Ok(reply)
                        if reply.success
                            && reply.code == 0
                            && reply.data.instrument_id == asset =>
                    {
                        if let Some(ticker) = self
                            .seed_tickers
                            .iter_mut()
                            .find(|t| t.id == asset.to_string())
                        {
                            Self::ticker_update(ticker, reply.data, true);
                        } else if let Some(symbol) = cx
                            .store
                            .markets()
                            .iter()
                            .find(|m| m.id == asset.to_string())
                            .map(|m| m.symbol.clone())
                        {
                            cx.store.update_ticker(&symbol, |ticker| {
                                Self::ticker_update(ticker, reply.data, true)
                            });
                        }
                        self.ticker_seeded.insert(asset);
                        self.discovery.succeeded(cx, &Some(asset));
                    }
                    _ => {
                        self.on_request_failed(
                            cx,
                            Pending::Ticker { asset },
                            "Invalid TxFlow ticker seed identity or envelope",
                        );
                        return;
                    }
                };
                self.ticker_timer = Some(cx.set_timer(25.0));
            }
            Pending::Candles {
                command,
                rows,
                valid,
            } => {
                let result: Result<String, String> = if valid && Header::valid(response.body) {
                    Ok(serde_json::to_string(&rows).unwrap())
                } else {
                    Err("Invalid TxFlow candle response".into())
                };
                match result {
                    Ok(data) => cx.done(command, data),
                    Err(error) => cx.fail(command, error),
                }
            }
            Pending::Account(request) => self.account_response(cx, request, response),
            Pending::Trade(request) => self.trade_response(cx, request, Ok(response)),
            Pending::History(request) => self.history_response(cx, *request, response),
            Pending::SocketRefresh => {}
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Meta { rows, skipped, .. } => match serde_json::from_str(item) {
                Ok(row) => rows.push(row),
                Err(_) => {
                    *skipped = true;
                    safe_cex_engine::startup::skipped_row(cx);
                }
            },
            Pending::Candles { rows, valid, .. } => match serde_json::from_str::<CandleRow>(item) {
                Ok(row) => rows.push(row.candle()),
                Err(_) => *valid = false,
            },
            Pending::Account(request) => self.account_item(cx, request, item),
            Pending::History(request) => self.history_item(cx, request, item),
            _ => {}
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Meta { .. } => {
                cx.notice(Level::Warn, code::REQUEST_FAILED, error);
                self.discovery.failed(cx, None);
            }
            Pending::Ticker { asset } => {
                cx.notice(Level::Warn, code::REQUEST_FAILED, error);
                self.discovery.failed(cx, Some(asset));
                self.ticker_timer = Some(cx.set_timer(25.0));
            }
            Pending::Candles { command, .. } => cx.fail(command, error),
            Pending::Account(request) => self.account_failed(cx, request, error),
            Pending::Trade(request) => self.trade_response(cx, request, Err(error)),
            Pending::History(request) => self.history_failed(cx, *request, error),
            Pending::SocketRefresh => {}
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.account_timer(cx, timer);
        self.trade_timer(cx, timer);
        if let Some(asset) = self.discovery.on_timer(timer) {
            if let Some(asset) = asset {
                self.ticker_request(cx, asset);
            } else {
                self.metadata(cx);
            }
        }
        if self.ticker_timer == Some(timer) {
            self.ticker_timer = None;
            self.next_ticker(cx);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => self.book_sub(cx, symbol, true),
            Command::UnsubscribeOrderBook { symbol } => self.book_sub(cx, symbol, false),
            Command::SubscribeCandles { symbol, interval } => {
                self.candle_sub(cx, symbol, interval, true)
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                self.candle_sub(cx, symbol, interval, false)
            }
            request @ Command::FetchCandles { .. } => self.fetch_candles(cx, request),
            Command::Query { command, query } => self.query(cx, command, query),
            other => self.trade_command(cx, other),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|l| l.time.is_some())
            .map(|l| &l.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, l)| l.published)
                .map(|(symbol, ladder)| (symbol.as_str(), &ladder.book)),
        )
    }
}
// TxFlow's candle codes are the TS spellings.
fn supported(interval: Interval) -> bool {
    use Interval::*;
    matches!(
        interval,
        M1 | M3 | M5 | M15 | M30 | H1 | H2 | H4 | H8 | H12 | D1
    )
}
