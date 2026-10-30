//! Phemex stablecoin perpetuals, as a typed sans-I/O state machine.
mod account;
mod account_wire;
mod history;
mod identity;
mod income;
mod public_candles;
mod public_reads;
mod public_trades;
mod read_owners;
mod sign;
mod signed_candles;
pub use sign::dispatch as dispatch_signing;
mod socket_codec;
mod trading;
mod venue_state;
mod wire;
use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Candle, Interval, Market, Ticker};
use safe_cex_engine::request_context::Context;
use safe_cex_engine::socket_lifecycle::{
    Control, EncodedBatch, FeedId, Interest, LifecycleEvent, SocketError,
};
use safe_cex_engine::{Command, Cx, PublicFeedPolicy, Response, SocketId, TimerId, Venue};
use std::collections::HashMap;
use wire::*;
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub credentials: Option<safe_cex_engine::signing::Identity>,
    pub read_credentials: Option<safe_cex_engine::signing::Identity>,
}

pub enum Pending {
    Identity(Box<identity::Read>),
    Markets(Vec<Market>, bool, public_reads::PublicRead),
    Tickers(Vec<Ticker>, bool, public_reads::PublicRead),
    Depth {
        symbol: String,
        generation: u64,
        read: public_reads::PublicRead,
    },
    Candles {
        root: safe_cex_engine::selected_read::ReadRoot,
        leg: safe_cex_engine::selected_read::ReadLeg,
        rows: Vec<Candle>,
        invalid: bool,
    },
    PublicCandles {
        context: Context,
        rows: Vec<Candle>,
        invalid: bool,
    },
    Account(read_owners::Background<account::Request>),
    Orders(read_owners::Background<account::OrdersRead>),
    Repair(trading::RepairRequest),
    PlannedTrade(trading::PlannedRequest),
    History(history::Request),
    Income(income::Request),
    PublicTrades(Box<public_trades::Read>),
}
#[derive(Default)]
struct Connection {
    feed: Option<FeedId>,
    opening_ping: Option<u64>,
    rpc_id: std::cell::Cell<u32>,
}
struct BookState {
    users: u32,
    book: Book,
    last: Option<u64>,
    generation: u64,
    fetching: bool,
    buffer: Vec<(u64, bool, Depth)>,
    retry: Option<TimerId>,
    deadline: Option<TimerId>,
}
impl BookState {
    fn new(generation: u64) -> Self {
        Self {
            users: 1,
            book: Book::default(),
            last: None,
            generation,
            fetching: false,
            buffer: vec![],
            retry: None,
            deadline: None,
        }
    }
}
pub struct PhemexPerpetuals {
    options: Options,
    credentials: Option<safe_cex_engine::signing::Identity>,
    read_credentials: Option<safe_cex_engine::signing::Identity>,
    read_slots: safe_cex_engine::credential_selection::Slots<safe_cex_engine::signing::Identity>,
    public_read_slots:
        safe_cex_engine::credential_selection::Slots<safe_cex_engine::signing::Identity>,
    public: Connection,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, Interval), u32>,
    generation: u64,
    native_symbols: HashMap<String, String>,
    startup_retry: Option<TimerId>,
    discovery: safe_cex_engine::startup::Backoff<bool>,
    /// The last product read skipped a malformed row, so an unknown symbol may be listed.
    catalogue_partial: bool,
    paused_until: f64,
    account: account::State,
    trading: trading::State,
    history: history::State,
    income: income::State,
}
impl PhemexPerpetuals {
    pub fn new(options: Options) -> Self {
        let credentials = options
            .credentials
            .as_ref()
            .filter(|identity| identity.validate().is_ok())
            .cloned();
        let (read_credentials, read_slots, public_read_slots) =
            read_owners::read_slots(credentials.clone(), options.read_credentials.clone());
        Self {
            options,
            credentials,
            read_credentials,
            read_slots,
            public_read_slots,
            public: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            generation: 0,
            native_symbols: HashMap::new(),
            startup_retry: None,
            discovery: Default::default(),
            catalogue_partial: false,
            paused_until: 0.,
            account: account::State::new(),
            trading: trading::State::default(),
            history: history::State::default(),
            income: income::State::default(),
        }
    }
    fn discovery_tickers(&mut self, cx: &mut Cx<'_, Pending>) {
        let read = match self.public_read(cx) {
            Ok(read) => read,
            Err(_) => {
                self.discovery.failed(cx, true);
                return;
            }
        };
        if let Err((pending, error)) = read.queue(
            cx,
            self.options.testnet,
            self.rest(),
            "/md/v3/ticker/24hr/all",
            String::new(),
            Some(ArrayAt::Field("result")),
            |read| Pending::Tickers(vec![], false, read),
        ) {
            self.on_sign_error(cx, pending, error.read_venue_error());
        }
    }
    /// The catalogue is complete and no discovery read awaits a retry.
    pub(crate) fn discovery_settled(&self) -> bool {
        !self.catalogue_partial && self.discovery.idle()
    }
    fn command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::FetchAccount { command } = command {
            self.fetch_identity(cx, command);
            return;
        }
        let Some(command) = self.trade_command(cx, command) else {
            return;
        };
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::Query { command, query } => self.history_query(cx, command, query),
            Command::SubscribeOrderBook { symbol } => {
                if let Some(b) = self.books.get_mut(&symbol) {
                    b.users += 1;
                    return;
                }
                if self.books.len() + self.candles.len() >= 19 {
                    cx.notice(
                        Level::Warn,
                        code::SUBSCRIPTION_REJECTED,
                        "Phemex subscription capacity reached",
                    );
                    return;
                }
                self.generation += 1;
                self.books
                    .insert(symbol.clone(), BookState::new(self.generation));
                self.sync_public_interests(cx);
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(b) = self.books.get_mut(&symbol) {
                    b.users -= 1;
                    if b.users > 0 {
                        return;
                    }
                }
                if let Some(b) = self.books.remove(&symbol) {
                    if let Some(t) = b.retry {
                        cx.clear_timer(t);
                    }
                    if let Some(t) = b.deadline {
                        cx.clear_timer(t);
                    }
                }
                self.sync_public_interests(cx);
            }
            Command::SubscribeCandles { symbol, interval } => {
                let Some(_) = resolution(interval) else {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        interval.unsupported("Phemex"),
                    );
                    return;
                };
                let key = (symbol.clone(), interval);
                if let Some(users) = self.candles.get_mut(&key) {
                    *users += 1;
                    return;
                }
                if self.books.len() + self.candles.len() >= 19 {
                    return;
                }
                self.candles.insert(key, 1);
                self.sync_public_interests(cx);
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval);
                if let Some(n) = self.candles.get_mut(&key) {
                    *n -= 1;
                    if *n > 0 {
                        return;
                    }
                }
                self.candles.remove(&key);
                if resolution(interval).is_some() {
                    self.sync_public_interests(cx);
                }
            }
            Command::FetchCandles {
                command: _,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => {
                if self.credentials.is_none() && self.read_credentials.is_none() {
                    self.fetch_public_candles(cx, &symbol, interval, limit, from_ms, to_ms);
                    return;
                }
                self.fetch_signed_candles(cx, &symbol, interval, limit, from_ms, to_ms);
            }
            _ => {}
        }
    }
    fn native_symbol<'a>(&'a self, symbol: &'a str) -> &'a str {
        self.native_symbols
            .get(symbol)
            .map(String::as_str)
            .unwrap_or(symbol)
    }
    fn rest(&self) -> &'static str {
        if self.options.testnet {
            "https://testnet-api.phemex.com"
        } else {
            "https://api.phemex.com"
        }
    }
    fn ws(&self) -> &'static str {
        if self.options.testnet {
            "wss://testnet-api.phemex.com/ws"
        } else {
            "wss://ws.phemex.com"
        }
    }
    fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Err(error) = self.connect_managed_public(cx) {
            self.public_socket_error(cx, error);
        }
    }
    fn snapshot(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(b) = self.books.get_mut(symbol) else {
            return;
        };
        if b.fetching {
            return;
        }
        if cx.now < self.paused_until {
            b.retry = Some(cx.set_timer(self.paused_until - cx.now));
            return;
        }
        b.fetching = true;
        b.deadline = Some(cx.set_timer(20_000.));
        let generation = b.generation;
        let read = match self.public_read(cx) {
            Ok(read) => read,
            Err(_) => {
                self.book_failed(cx, symbol);
                return;
            }
        };
        let query = format!(
            "symbol={}&limit=1000",
            safe_cex_engine::url::encode(self.native_symbol(symbol))
        );
        if let Err((pending, error)) = read.queue(
            cx,
            self.options.testnet,
            self.rest(),
            "/md/v2/fullbook",
            query,
            None,
            |read| Pending::Depth {
                symbol: symbol.into(),
                generation,
                read,
            },
        ) {
            self.on_sign_error(cx, pending, error.read_venue_error());
        }
    }
    fn book_failed(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if let Some(b) = self.books.get_mut(symbol) {
            self.generation += 1;
            b.generation = self.generation;
            b.fetching = false;
            if let Some(t) = b.deadline.take() {
                cx.clear_timer(t)
            }
            b.last = None;
            b.buffer.clear();
            if b.retry.is_none() {
                b.retry = Some(cx.set_timer(1_000.));
            }
        }
    }
    fn depth(&mut self, cx: &mut Cx<'_, Pending>, r: DepthEvent<'_>) {
        let normalized = r.symbol.to_ascii_uppercase();
        let symbol = normalized.as_str();
        let Some(b) = self.books.get_mut(symbol) else {
            return;
        };
        if b.last.is_none() {
            if b.buffer.len() >= 2_000 {
                self.book_failed(cx, symbol);
                return;
            }
            b.buffer
                .push((r.sequence, r.kind == "snapshot", r.orderbook_p));
            self.snapshot(cx, symbol);
            return;
        }
        // Phemex coalesces updates every 20ms; sequence numbers need not be adjacent.
        if r.sequence <= b.last.unwrap() {
            return;
        }
        if r.kind == "snapshot" {
            b.book.clear();
        }
        apply(&mut b.book, r.orderbook_p);
        b.last = Some(r.sequence);
        cx.book_changed(symbol);
    }
    fn fail_pending(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Some(context) = pending.public_read_context() {
            let _ = cx.finish_http_context(context);
        }
        match pending {
            Pending::Identity(command) => self.identity_failed(cx, command, error),
            Pending::Account(r) => self.account_read_failed(cx, r, error),
            Pending::Orders(r) => self.orders_background_failed(cx, r, error),
            Pending::Repair(r) => {
                self.repair_failed(cx, r, safe_cex_engine::VenueError::new(error))
            }
            Pending::PlannedTrade(read) => {
                self.planned_trade_failed(cx, read, safe_cex_engine::VenueError::new(error))
            }
            Pending::History(r) => self.history_failed(cx, r, error),
            Pending::PublicTrades(read) => {
                self.public_trades_failed(cx, *read, safe_cex_engine::VenueError::new(error));
            }
            Pending::Income(r) => self.income_failed(cx, r, error),
            Pending::Depth {
                symbol, generation, ..
            } => {
                if self
                    .books
                    .get(&symbol)
                    .is_some_and(|b| b.generation == generation)
                {
                    self.book_failed(cx, &symbol)
                }
            }
            Pending::Candles { root, .. } => {
                let _ = cx.fail_context(root.context(), error);
            }
            Pending::PublicCandles { context, .. } => {
                let _ = cx.fail_context(&context, error);
            }
            Pending::Markets(..) => self.discovery.failed(cx, false),
            Pending::Tickers(..) => self.discovery.failed(cx, true),
        }
    }
}
fn apply(book: &mut Book, d: Depth) {
    for (side, rows) in [(BookSide::Bids, d.bids), (BookSide::Asks, d.asks)] {
        for Level(N(p), N(a)) in rows {
            if p > 0. && a >= 0. {
                book.apply(side, p, a);
            }
        }
    }
}
impl Venue for PhemexPerpetuals {
    fn managed_sockets() -> bool {
        true
    }

    fn fence_uncertain_placements(&self) -> bool {
        true
    }

    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, _: Option<&[String]>) {
        self.reconcile_uncertain_orders(cx);
    }

    fn guard_pending_placements(&self) -> bool {
        true
    }

    fn guard_order_mutations(&self) -> bool {
        true
    }

    // A panic close must always go out; late landings are canceled after the read.
    fn fence_uncertain_cancel_all(&self) -> bool {
        false
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            cancel_all: safe_cex_engine::CancelAll::EachPlacedSymbol,
            client_order_lookup: true,
            account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.settle_retired_trading(cx);
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            return;
        }
        if cx.now < self.paused_until {
            self.startup_retry = Some(cx.set_timer(self.paused_until - cx.now));
            return;
        }
        self.catalogue_partial = false;
        let read = match self.public_read(cx) {
            Ok(read) => read,
            Err(_) => {
                self.discovery.failed(cx, false);
                return;
            }
        };
        if let Err((pending, error)) = read.queue(
            cx,
            self.options.testnet,
            self.rest(),
            "/public/products",
            String::new(),
            Some(ArrayAt::PathOmitLists(&["data", "perpProductsV2"])),
            |read| Pending::Markets(vec![], false, read),
        ) {
            self.on_sign_error(cx, pending, error.read_venue_error());
        }
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        self.settle_retired_trading(cx);
        let feed = match &event {
            LifecycleEvent::Opened(epoch)
            | LifecycleEvent::Ready { epoch, .. }
            | LifecycleEvent::Authenticated { epoch, .. }
            | LifecycleEvent::AuthFailure { epoch, .. }
            | LifecycleEvent::Retired { epoch, .. }
            | LifecycleEvent::Ping(epoch)
            | LifecycleEvent::RetryLogin(epoch) => epoch.feed(),
            LifecycleEvent::Reconnect(feed)
            | LifecycleEvent::InterestsCommitted { feed, .. }
            | LifecycleEvent::InterestsRefused { feed, .. } => *feed,
        };
        if self.is_account_feed(feed) {
            self.account_socket_event(cx, event)
        } else {
            self.public_socket_event(cx, event)
        }
    }
    fn encode_socket_interests(
        &self,
        _store: &safe_cex_engine::Store,
        feed: FeedId,
        subscribe: bool,
        interests: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        if self.is_account_feed(feed) {
            self.encode_account_interests(subscribe, interests, emit)
        } else {
            self.encode_public_interests(feed, subscribe, interests, emit)
        }
    }
    fn socket_control(
        &self,
        _: &safe_cex_engine::Store,
        feed: FeedId,
        _: bool,
        text: &str,
    ) -> Control {
        if self.is_account_feed(feed) {
            self.account_socket_control(text)
        } else {
            Control::Other
        }
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, _: SocketId) {
        self.settle_retired_trading(cx);
    }
    fn export_venue_state(&self) -> Option<Vec<u8>> {
        Some(self.account.export_venue_state().encode())
    }
    fn apply_venue_state(&mut self, cx: &mut Cx<'_, Pending>, bytes: &[u8]) {
        if let Some(state) = venue_state::VenueState::decode(bytes)
            && self.account.apply_venue_state(cx, state)
        {
            // The source's STATUS hedge flag predates the held mode.
            cx.store.set_hedged(self.account.any_hedged());
        }
    }
    /// The account cycle re-reads every symbol's mode, margin and leverage.
    fn refresh_settings(&mut self, cx: &mut Cx<'_, Pending>, _symbol: Option<&str>) {
        self.account_refresh(cx);
    }
    fn on_frame_ingested(&mut self, cx: &mut Cx<'_, Pending>) {
        self.cancel_landed_from_frames(cx);
        let client_ids = cx
            .store
            .orders()
            .iter()
            .filter_map(|order| order.client_order_id.clone())
            .collect();
        self.account.frame_ingested(
            cx.source_orders_cover().is_some(),
            cx.store.loaded().orders,
            &client_ids,
        );
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.settle_retired_trading(cx);
        if self.account_message(cx, socket, text) {
            return;
        }
        if self
            .public
            .feed
            .and_then(|feed| cx.socket_epoch(feed))
            .map(|epoch| epoch.socket())
            != Some(socket)
        {
            return;
        }
        let Ok(e) = serde_json::from_str::<Envelope<'_>>(text) else {
            return;
        };
        if e.method == "perp_market24h_pack_p.update" {
            #[derive(serde::Deserialize)]
            struct Pack<'a> {
                #[serde(borrow)]
                data: Vec<Packed<'a>>,
            }
            if let Ok(p) = serde_json::from_str::<Pack<'_>>(text) {
                let mut added = Vec::new();
                for r in p.data {
                    let symbol = r.0.to_ascii_uppercase();
                    if cx.store.ticker(&symbol).is_none()
                        && !added.iter().any(|t: &Ticker| t.symbol == symbol)
                        && let Some(m) = cx.store.market(&symbol)
                    {
                        added.push(Ticker {
                            id: m.id.clone(),
                            symbol: symbol.clone(),
                            last: r.4.0,
                            mark: r.9.0,
                            index: r.8.0,
                            bid: r.12.as_ref().map_or(r.4.0, |n| n.0),
                            ask: r.13.as_ref().map_or(r.4.0, |n| n.0),
                            volume: r.5.0,
                            quote_volume: r.5.0 * r.4.0,
                            percentage: percentage(r.4.0, r.1.0),
                            open_interest: r.7.0,
                            funding_rate: r.10.0,
                            ..Ticker::default()
                        });
                    }
                    cx.store.update_ticker(&r.0.to_ascii_uppercase(), |t| {
                        t.last = r.4.0;
                        t.volume = r.5.0;
                        t.quote_volume = r.5.0 * r.4.0;
                        t.percentage = percentage(r.4.0, r.1.0);
                        t.open_interest = r.7.0;
                        t.index = r.8.0;
                        t.mark = r.9.0;
                        t.funding_rate = r.10.0;
                        if let Some(b) = r.12 {
                            t.bid = b.0
                        }
                        if let Some(a) = r.13 {
                            t.ask = a.0
                        }
                    });
                }
                if !added.is_empty() {
                    let mut rows = cx.store.tickers().to_vec();
                    rows.extend(added);
                    cx.store.set_tickers(rows);
                }
            }
        } else if e.orderbook_p.is_some() {
            if let Ok(r) = serde_json::from_str::<DepthEvent<'_>>(text) {
                if r.kind == "snapshot"
                    && !e.orderbook_p.is_some_and(|raw| snapshot_sides(raw.get()))
                {
                    self.book_failed(cx, &e.symbol.to_ascii_uppercase());
                    return;
                }
                self.depth(cx, r)
            } else {
                self.book_failed(cx, &e.symbol.to_ascii_uppercase());
            }
        } else if e.kline_p.is_some() {
            #[derive(serde::Deserialize)]
            struct K {
                kline_p: Vec<CandleRow>,
            }
            if let Ok(k) = serde_json::from_str::<K>(text) {
                for r in k.kline_p.into_iter().rev() {
                    for ((s, i), _) in self.candles.iter().filter(|((s, i), _)| {
                        s == &e.symbol.to_ascii_uppercase()
                            && resolution(*i).is_some_and(|n| n as f64 == r.1.0)
                    }) {
                        cx.candle(s, *i, r.candle());
                    }
                }
            }
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, _: SocketId, _: u16) {
        self.settle_retired_trading(cx);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, item: &str) {
        self.settle_retired_trading(cx);
        if let Some(context) = p.public_read_context()
            && cx.validate_background_read_http_delivery(context).is_err()
        {
            return;
        }
        if let Pending::Candles { root, leg, .. } = p
            && root.validate_delivery(cx, leg).is_err()
        {
            return;
        }
        match p {
            Pending::Markets(rows, _, _) => {
                if let Ok(r) = serde_json::from_str::<Product<'_>>(item) {
                    if r.supported() && !r.valid() {
                        self.catalogue_partial = true;
                        safe_cex_engine::startup::skipped_row(cx);
                        return;
                    }
                    let native = r.symbol.to_string();
                    if let Some(m) = r.market() {
                        self.native_symbols.insert(m.symbol.clone(), native);
                        rows.push(m)
                    }
                } else {
                    self.catalogue_partial = true;
                    safe_cex_engine::startup::skipped_row(cx);
                }
            }
            Pending::Tickers(rows, _, _) => {
                if let Ok(r) = serde_json::from_str::<Daily<'_>>(item) {
                    if let Some(m) = cx.store.market(&r.symbol.to_ascii_uppercase()) {
                        rows.push(r.ticker(m.id.clone()))
                    }
                } else {
                    safe_cex_engine::startup::skipped_row(cx);
                }
            }
            Pending::Candles { rows, invalid, .. }
            | Pending::PublicCandles { rows, invalid, .. } => {
                if let Ok(r) = serde_json::from_str::<CandleRow>(item) {
                    rows.push(r.candle())
                } else {
                    *invalid = true;
                }
            }
            Pending::Account(r) => {
                if self.currency_read_delivery(cx, r).is_ok() {
                    self.account_item(cx, &mut r.native, item);
                }
            }
            Pending::History(r) => self.history_item(cx, r, item),
            Pending::PublicTrades(read) => read.item(cx, item),
            Pending::Income(r) => self.income_item(cx, r, item),
            _ => {}
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, r: Response<'_>) {
        self.settle_retired_trading(cx);
        if let Pending::PublicTrades(read) = &p
            && !read.valid_delivery(cx)
        {
            return;
        }
        let delivery = match &p {
            Pending::Identity(read) => read.validate_delivery(cx),
            Pending::Account(read) => self.currency_read_delivery(cx, read),
            Pending::Orders(read) => self.orders_read_delivery(cx, read),
            _ => Ok(()),
        };
        if let Err(error) = delivery {
            self.on_http_failed(cx, p, error.read_venue_error());
            return;
        }
        if r.status == 429 {
            self.paused_until = self.paused_until.max(
                cx.now
                    + r.header("x-ratelimit-retry-after-contract")
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(60.)
                        .max(1.)
                        * 1000.,
            );
        }
        if let Some(context) = p.public_read_context()
            && cx.validate_background_read_http_delivery(context).is_err()
        {
            return;
        }
        if let Pending::Candles { root, leg, .. } = &p
            && root.validate_delivery(cx, leg).is_err()
        {
            return;
        }
        if let Pending::History(read) = &p
            && read.validate_delivery(cx).is_err()
        {
            return;
        }
        if let Pending::Income(read) = &p
            && read.validate_delivery(cx).is_err()
        {
            return;
        }
        if let Pending::Repair(request) = p {
            self.repair_response(cx, request, r);
            return;
        }
        if let Pending::PlannedTrade(read) = p {
            self.planned_trade_response(cx, read, r);
            return;
        }
        if r.status >= 400 {
            self.fail_pending(cx, p, &format!("HTTP {}: {}", r.status, r.body));
            return;
        }
        if let Ok(e) = serde_json::from_str::<Envelope<'_>>(r.body)
            && (e.error.is_some() || e.code.is_some_and(|n| n != 0))
        {
            match p {
                Pending::Repair(_) => unreachable!("handled before native envelope"),
                Pending::Orders(read) => self.orders_background_response(cx, read, r.body),
                p => {
                    self.fail_pending(cx, p, &format!("Phemex {} {}", e.code.unwrap_or(-1), e.msg))
                }
            }
            return;
        }
        match p {
            Pending::Identity(command) => self.identity_response(cx, command, r),
            bad @ (Pending::Markets(_, true, _)
            | Pending::Tickers(_, true, _)
            | Pending::Candles { invalid: true, .. }
            | Pending::PublicCandles { invalid: true, .. }) => {
                self.fail_pending(cx, bad, "malformed public response row")
            }
            Pending::Markets(rows, false, read) => {
                if cx.finish_http_context(read.context()).is_err() {
                    return;
                }
                if rows.is_empty() {
                    self.discovery.failed(cx, false);
                    return;
                }
                cx.store.set_markets(rows);
                self.discovery.succeeded(cx, &false);
                // Frame-fed: public instrument metadata only; the source sends the rest.
                if cx.frame_fed() {
                    return;
                }
                self.start_account(cx);
                self.discovery_tickers(cx);
            }
            Pending::Tickers(rows, false, read) => {
                if cx.finish_http_context(read.context()).is_err() {
                    return;
                }
                if rows.is_empty() {
                    self.discovery.failed(cx, true);
                    return;
                }
                self.discovery.succeeded(cx, &true);
                cx.store.set_tickers(rows);
                self.connect_public(cx);
            }
            Pending::Depth {
                symbol,
                generation,
                read,
            } => {
                if cx.finish_http_context(read.context()).is_err() {
                    return;
                }
                let Some(b) = self.books.get_mut(&symbol) else {
                    return;
                };
                if b.generation != generation {
                    return;
                }
                let Ok(row) = serde_json::from_str::<Snapshot>(r.body) else {
                    self.book_failed(cx, &symbol);
                    return;
                };
                b.book.clear();
                apply(&mut b.book, row.result.orderbook_p.into());
                let mut last = row.result.sequence;
                for (seq, snapshot, d) in std::mem::take(&mut b.buffer) {
                    if seq > last {
                        if snapshot {
                            b.book.clear();
                        }
                        apply(&mut b.book, d);
                        last = seq
                    }
                }
                b.last = Some(last);
                b.fetching = false;
                if let Some(t) = b.deadline.take() {
                    cx.clear_timer(t)
                }
                cx.book_changed(&symbol);
            }
            Pending::Candles { root, mut rows, .. } => {
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                let _ = cx.done_context(root.context(), serde_json::to_string(&rows).unwrap());
            }
            Pending::PublicCandles {
                context, mut rows, ..
            } => {
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                let _ = cx.done_context(&context, serde_json::to_string(&rows).unwrap());
            }
            Pending::Account(p) => self.account_read_response(cx, p, r),
            Pending::Orders(read) => self.orders_background_response(cx, read, r.body),
            Pending::Repair(_) => unreachable!("handled before native envelope"),
            Pending::PlannedTrade(_) => unreachable!("handled before native envelope"),
            Pending::History(p) => self.history_response(cx, p, r),
            Pending::PublicTrades(read) => self.public_trades_response(cx, *read, r),
            Pending::Income(p) => self.income_response(cx, p, r),
        }
    }
    fn on_signing_release(
        &mut self,
        target: safe_cex_engine::SigningTarget<'_, Pending>,
        release: safe_cex_engine::SigningRelease,
    ) {
        if let safe_cex_engine::SigningTarget::Http { request, pending } = target {
            match pending {
                Pending::Repair(_) => {}
                Pending::PlannedTrade(read) => {
                    self.trade_signing_release(request, &mut read.native, release)
                }
                _ => {}
            }
        }
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        self.settle_retired_trading(cx);
        match pending {
            Pending::Identity(command) => self.identity_failed(cx, command, error),
            Pending::Candles { root, .. } => {
                let _ = cx.fail_context(root.context(), error);
            }
            Pending::Repair(request) => self.repair_failed(cx, request, error),
            Pending::PlannedTrade(read) => self.planned_trade_failed(cx, read, error),
            Pending::History(read) => self.history_failed(cx, read, error),
            Pending::Income(read) => self.income_failed(cx, read, error),
            Pending::PublicTrades(read) => self.public_trades_failed(cx, *read, error),
            Pending::PublicCandles { context, .. } => {
                let _ = cx.fail_context(&context, error.outcome(safe_cex_engine::Outcome::NotSent));
            }
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        self.settle_retired_trading(cx);
        match pending {
            Pending::Identity(read) => self.identity_failed(cx, read, error),
            Pending::Repair(request) => self.repair_failed(cx, request, error),
            Pending::History(read) => self.history_failed(cx, read, error),
            Pending::Income(read) => self.income_failed(cx, read, error),
            Pending::PlannedTrade(read) => self.planned_trade_failed(cx, read, error),
            Pending::Candles { root, .. } => {
                let _ = cx.fail_context(root.context(), error);
            }
            Pending::PublicTrades(read) => self.public_trades_failed(cx, *read, error),
            Pending::PublicCandles { context, .. } => {
                let _ = cx.fail_context(&context, error.outcome(safe_cex_engine::Outcome::NotSent));
            }
            pending => self.on_request_failed(cx, pending, &error.message),
        }
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        self.settle_retired_trading(cx);
        match pending {
            Pending::Income(read) => {
                if read.validate_delivery(cx).is_ok() {
                    self.income_failed(
                        cx,
                        read,
                        format!(
                            "response did not contain the expected list: {}",
                            response.body
                        ),
                    );
                }
            }
            Pending::History(read) => {
                if read.validate_delivery(cx).is_ok() {
                    self.history_failed(
                        cx,
                        read,
                        format!(
                            "response did not contain the expected list: {}",
                            response.body
                        ),
                    );
                }
            }
            pending @ (Pending::Markets(..) | Pending::Tickers(..)) => {
                self.on_response(cx, pending, response)
            }
            pending => self.on_request_failed(
                cx,
                pending,
                &format!(
                    "response did not contain the expected list: {}",
                    response.body
                ),
            ),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: &str) {
        self.settle_retired_trading(cx);
        match p {
            Pending::Income(read) => {
                self.income_failed(cx, read, safe_cex_engine::VenueError::new(error))
            }
            p => self.fail_pending(cx, p, error),
        }
    }
    fn on_repair_notice(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        notice: safe_cex_engine::repair::RepairNotice<'_>,
    ) {
        self.late_repair_notice(cx, notice);
    }
    fn on_http_owner_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) {
        self.retire_trading(owner);
    }
    fn unproven_http_evidence(
        &self,
        pending: &Pending,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        if matches!(pending, Pending::PlannedTrade(_) | Pending::Repair(_)) {
            trading::unproven_diagnostics(response, evidence);
        }
    }
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        if !matches!(pending, Pending::PlannedTrade(_) | Pending::Repair(_)) {
            return Err(safe_cex_engine::request_context::Error::Invalid);
        }
        // Native late HTTP replies have no reducer authority; account polling
        // observes orders only after the actual signing-expiry fence.
        safe_cex_engine::request_context::ReconciliationData::new(String::new())
    }

    fn read_root_retired(
        &mut self,
        owner: safe_cex_engine::request_context::Owner,
        _: Vec<Pending>,
    ) {
        self.retire_history(owner);
        self.retire_income(owner);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.settle_retired_trading(cx);
        if let Some(tickers) = self.discovery.on_timer(timer) {
            if tickers {
                self.discovery_tickers(cx);
            } else {
                self.start(cx);
            }
            return;
        }
        if self.account_timer(cx, timer)
            || self.history_timer(cx, timer)
            || self.income_timer(cx, timer)
        {
            return;
        }
        if self.startup_retry == Some(timer) {
            self.startup_retry = None;
            self.start(cx)
        } else {
            let timed = self
                .books
                .iter()
                .find_map(|(s, b)| (b.deadline == Some(timer)).then(|| s.clone()));
            if let Some(s) = timed {
                self.book_failed(cx, &s);
                return;
            }
            let symbol = self.books.iter_mut().find_map(|(s, b)| {
                (b.retry == Some(timer)).then(|| {
                    b.retry = None;
                    s.clone()
                })
            });
            if let Some(s) = symbol {
                self.snapshot(cx, &s)
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        // Store cleanup precedes planning; recovery I/O must not spend the fresh entry.
        let restored = self.restore_retired_trading(cx);
        self.command(cx, command);
        if restored {
            self.account_refresh(cx);
            self.orders_reconcile(cx);
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|b| b.last.is_some())
            .map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}
