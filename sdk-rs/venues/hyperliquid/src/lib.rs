//! Hyperliquid perpetuals: typed, deterministic state machines without I/O.
mod account;
mod history;
pub mod sign;
mod socket_codec;
mod trading;
mod wire;

use base64::Engine as _;
use safe_cex_engine::SubscriptionTopic;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::{self, ArrayAt};
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{FeedId, SocketError};
use safe_cex_engine::{
    Command, CommandId, Cx, Method, PublicFeedPolicy, Response, SocketId, TimerId, Venue,
    VenueError,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use wire::*;

#[derive(Clone, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub credentials: Option<safe_cex_engine::signing::Identity>,
    pub application_id: Option<String>,
    pub vault: Option<String>,
    pub builder: Option<Builder>,
    /// TS `enableSpotMarkets`: spot pairs, holdings and orders in the perp account.
    pub enable_spot_markets: bool,
    /// Extra fast depth topic per book; callers must budget aggregate per-IP subscriptions.
    pub fast_book: bool,
}
safe_cex_crypto::redacted_debug!(Options);

#[derive(Clone, Debug, Serialize)]
pub struct Builder {
    pub b: String,
    pub f: u32,
}

/// Discovery retry key for the merged spot catalogue; no dex is named `@`.
const SPOT_SCOPE: &str = "@spot";

/// TS `resolveSpotDisplayName`: Unit-bridged tokens (`UBTC`, fullName `Unit …`)
/// display without the `U`, unless another token already owns that name.
fn spot_display_name(token: &SpotToken, tokens: &[&SpotToken]) -> String {
    let bridged = token
        .full_name
        .as_deref()
        .is_some_and(|name| name.starts_with("Unit "));
    let mut chars = token.name.chars();
    let prefixed = chars.next() == Some('U')
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if !bridged || !prefixed {
        return token.name.clone();
    }
    let stripped = &token.name[1..];
    if tokens.iter().any(|t| t.name == stripped) {
        token.name.clone()
    } else {
        stripped.to_owned()
    }
}

struct SpotCatalog {
    meta: SpotMeta,
    markets: Vec<Market>,
    tickers: Vec<Ticker>,
    excluded: Vec<String>,
}

pub enum Pending {
    SpotMeta {
        attempt: u8,
        meta: Option<SpotMeta>,
        markets: Vec<Market>,
        tickers: Vec<Ticker>,
        /// Well-formed rows TS deliberately hides (null mid price).
        excluded: Vec<String>,
        valid: bool,
        stage: u8,
    },
    Account(account::Request),
    Trade(trading::Request),
    History(Box<history::Request>),
    Meta {
        dex: String,
        offset: u32,
        instruments: Vec<Option<Instrument>>,
        markets: Vec<Market>,
        tickers: Vec<Ticker>,
        /// Well-formed rows TS deliberately hides (null mid, delisted HIP-3).
        excluded: Vec<String>,
        valid: bool,
        stage: u8,
        attempt: u8,
    },
    Dexs {
        dexs: Vec<(String, u32)>,
        index: u32,
    },
    ManagedCandles(Box<ManagedCandleRead>),
    ManagedInfo(Box<ManagedInfoRead>),
    Leverage(Box<LeverageRead>),
    SocketRefresh {
        mode_ready: bool,
    },
}

/// A public candle read owns its exact root and physical HTTP delivery.
pub struct ManagedCandleRead {
    context: Context,
    candles: Vec<Candle>,
    valid: bool,
}
pub struct LeverageRead {
    context: Context,
    symbol: String,
}
pub struct ManagedInfoRead {
    context: Context,
    pending: Pending,
    background: bool,
    read_class: safe_cex_engine::HttpReadClass,
}
struct InfoSuccessor {
    body: String,
    pending: Pending,
    streamed: Option<ArrayAt>,
    background: bool,
    read_class: safe_cex_engine::HttpReadClass,
}
#[derive(Default)]
struct Connection {
    feed: Option<FeedId>,
    feed_refused: bool,
    // Native topic eligibility, updated only by accepted shared lifecycle events.
    open: bool,
    private_bootstrap: socket_codec::PrivateBootstrap,
    // Transport liveness and epoch retirement cannot forgive a missing private bootstrap.
    private_bootstrap_misses: u8,
    /// Subscribe (true) and unsubscribe frames encoded for the current socket and not
    /// yet answered, oldest first, and whether the engine's ledger shows each was sent.
    /// The venue answers every sent frame, in order, with an ack or an error.
    unanswered: std::cell::RefCell<VecDeque<socket_codec::Unanswered>>,
    /// Interest keys the venue refused; left out until the socket or the topic is renewed.
    refused: HashSet<String>,
    refusal_notices: u8,
    // Successful proposal admission persists across epochs and catalogue growth.
    admitted_interests: HashSet<std::sync::Arc<str>>,
    /// The last subscription count reported, and when.
    reported_subscriptions: Option<(usize, f64)>,
    capacity_excess: usize,
}
#[derive(Default)]
struct Ladder {
    subscribers: usize,
    depth: [Vec<Level>; 2],
    fast_depth: Option<[Vec<Level>; 2]>,
    depth_time: Option<f64>,
    spot_depth: [Option<[Vec<Level>; 2]>; 7],
    bbo: Option<(f64, [Option<Level>; 2])>,
    book: Book,
}
pub struct Hyperliquid {
    repair_boot_sequence: u64,
    options: Options,
    /// The standalone `hyperliquidspot` venue: spot is the whole product.
    spot: bool,
    /// Spot markets merged into the perp account (`enable_spot_markets`).
    spot_markets: bool,
    /// A merged spot catalogue read before the perp and HIP-3 catalogue was
    /// published; TS lists spot after both.
    spot_catalog: Option<SpotCatalog>,
    perps_published: bool,
    /// The merged spot catalogue is in the store; until then spot rows are skipped.
    pub(crate) spot_committed: bool,
    /// Merged spot pair symbol -> venue token (name, fullName), published as the
    /// TS `info.hlTokenName`/`hlTokenFullName` disclosure.
    underlying: HashMap<String, (String, Option<String>)>,
    spot_tokens: HashMap<u32, String>,
    account: account::Account,
    trading: trading::Trading,
    credentials: safe_cex_engine::credential_selection::Slots<safe_cex_engine::signing::Identity>,
    connection: Connection,
    books: HashMap<String, Ladder>,
    candles: HashMap<(String, Interval), usize>,
    /// Newest trade time per watched symbol whose `last` is now its trade price;
    /// until its first trade, and for every other symbol, `last` is the mid.
    trade_last: HashMap<String, f64>,
    assets: HashMap<String, u32>,
    isolated_only: HashSet<String>,
    coins: HashMap<String, String>,
    dex_queue: std::collections::VecDeque<(String, u32)>,
    dex_names: Vec<String>,
    advanced_scopes: HashSet<String>,
    core_discovery_incomplete: bool,
    http_429_count: u32,
    /// Lowercase coins of catalogue rows excluded by design: never published,
    /// and owned exposure on them is hidden rather than freezing the account.
    excluded: HashSet<String>,
    startup: safe_cex_engine::startup::Backoff<(String, u32)>,
    info_completing: bool,
    info_successors: VecDeque<InfoSuccessor>,
}
impl Hyperliquid {
    pub fn new(options: Options) -> Self {
        let spot_markets = options.enable_spot_markets;
        let credentials = safe_cex_engine::credential_selection::Slots::new(
            options.credentials.clone(),
            None,
            Default::default(),
        );
        Self {
            repair_boot_sequence: socket_codec::next_repair_boot_sequence(),
            options,
            spot: false,
            spot_markets,
            spot_catalog: None,
            perps_published: false,
            spot_committed: false,
            underlying: HashMap::new(),
            spot_tokens: HashMap::new(),
            account: account::Account::default(),
            trading: trading::Trading::default(),
            credentials,
            connection: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            trade_last: HashMap::new(),
            assets: HashMap::new(),
            isolated_only: HashSet::new(),
            coins: HashMap::new(),
            dex_queue: Default::default(),
            dex_names: Vec::new(),
            advanced_scopes: HashSet::new(),
            core_discovery_incomplete: true,
            http_429_count: 0,
            excluded: HashSet::new(),
            startup: Default::default(),
            info_completing: false,
            info_successors: VecDeque::new(),
        }
    }
    pub fn new_spot(mut options: Options) -> Self {
        options.public_data_only |= options.key.as_deref().is_none_or(str::is_empty)
            || options
                .credentials
                .as_ref()
                .is_none_or(|identity| identity.signing_ref().is_err());
        Self {
            spot: true,
            spot_markets: false,
            ..Self::new(options)
        }
    }
    fn spot_meta(&mut self, cx: &mut Cx<'_, Pending>) {
        self.spot_meta_attempt(cx, 0);
    }
    fn spot_meta_attempt(&mut self, cx: &mut Cx<'_, Pending>, attempt: u8) {
        self.info(
            cx,
            "{\"type\":\"spotMetaAndAssetCtxs\"}".into(),
            ArrayAt::Root,
            Pending::SpotMeta {
                attempt,
                meta: None,
                markets: Vec::new(),
                tickers: Vec::new(),
                excluded: Vec::new(),
                valid: true,
                stage: 0,
            },
        );
    }
    /// Spot asset ids, coin handles and each token's valuation pair.
    fn register_spot(&mut self, cx: &Cx<'_, Pending>, meta: &SpotMeta, markets: &[Market]) {
        for native in meta.universe.iter().filter_map(|row| row.0.as_ref()) {
            if let Some(m) = markets.iter().find(|m| m.id == native.name) {
                self.assets.insert(m.symbol.clone(), 10_000 + native.index);
                if let Some(token) = native.tokens.first() {
                    let replace = self
                        .spot_tokens
                        .get(token)
                        .and_then(|s| {
                            markets
                                .iter()
                                .find(|m| &m.symbol == s)
                                .or_else(|| cx.store.market(s))
                        })
                        .is_none_or(|existing| existing.quote != "USDC" && m.quote == "USDC");
                    if replace {
                        self.spot_tokens.insert(*token, m.symbol.clone());
                    }
                }
                self.coins
                    .insert(m.id.to_ascii_lowercase(), m.symbol.clone());
            }
        }
    }
    /// Spot pairs merged into the perp account, as TS `fetchSpotMarketsAndTickers`:
    /// best effort, so a failure never holds back the perp catalogue.
    fn merged_spot_meta(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        meta: Option<SpotMeta>,
        mut markets: Vec<Market>,
        mut tickers: Vec<Ticker>,
        excluded: Vec<String>,
    ) {
        let Some(meta) = meta else {
            self.spot_markets_failed(cx, "Malformed Hyperliquid spot metadata");
            return;
        };
        let tokens: Vec<&SpotToken> = meta
            .tokens
            .iter()
            .filter_map(|row| row.0.as_ref())
            .collect();
        for market in &mut markets {
            let Some(token) = tokens.iter().find(|t| t.name == market.base) else {
                continue;
            };
            self.underlying.insert(
                market.symbol.clone(),
                (token.name.clone(), token.full_name.clone()),
            );
            let display = spot_display_name(token, &tokens);
            if display != token.name {
                market.pretty_symbol = Some(format!("{display}/{}", market.quote));
                market.base = display;
            }
        }
        // Spot publishes no oracle or funding (TS zeroes both).
        for ticker in &mut tickers {
            ticker.index = 0.0;
            ticker.funding_rate = 0.0;
        }
        self.startup.succeeded(cx, &(SPOT_SCOPE.into(), 0));
        let catalog = SpotCatalog {
            meta,
            markets,
            tickers,
            excluded,
        };
        if self.perps_published {
            self.commit_spot_catalog(cx, catalog);
        } else {
            self.spot_catalog = Some(catalog);
        }
    }
    fn commit_spot_catalog(&mut self, cx: &mut Cx<'_, Pending>, catalog: SpotCatalog) {
        let SpotCatalog {
            meta,
            markets,
            tickers,
            excluded,
        } = catalog;
        self.register_spot(cx, &meta, &markets);
        let count = markets.len();
        self.commit_discovery(cx, markets, tickers, excluded);
        self.spot_committed = true;
        cx.notice(
            safe_cex_engine::Level::Info,
            code::STARTUP_PROGRESS,
            format!("Loaded {count} Hyperliquid spot markets"),
        );
        self.account_prices(cx);
        // A late catalogue: spot rows read before it were skipped. A scheduled
        // refresh cannot be swallowed by an orders read already in flight.
        if self.account.started && !self.options.public_data_only && self.user().is_some() {
            self.schedule_orders(cx);
            self.refresh_spot_holdings(cx);
        }
    }
    fn spot_markets_failed(&mut self, cx: &mut Cx<'_, Pending>, error: &str) {
        cx.notice(
            safe_cex_engine::Level::Warn,
            code::REQUEST_FAILED,
            format!("Failed to load Hyperliquid spot markets; retrying: {error}"),
        );
        self.startup.failed(cx, (SPOT_SCOPE.into(), 0));
    }
    fn rest(&self) -> &'static str {
        if self.options.testnet {
            "https://api.hyperliquid-testnet.xyz"
        } else {
            "https://api.hyperliquid.xyz"
        }
    }
    fn ws(&self) -> &'static str {
        if self.options.testnet {
            "wss://api.hyperliquid-testnet.xyz/ws"
        } else if self.spot {
            "wss://api.hyperliquid.xyz/ws"
        } else {
            "wss://api-ui.hyperliquid.xyz/ws"
        }
    }
    fn discard_retired_amend_read(&mut self, cx: &mut Cx<'_, Pending>, pending: &Pending) -> bool {
        let inner = match pending {
            Pending::ManagedInfo(read) => &read.pending,
            other => other,
        };
        let Pending::Account(request) = inner else {
            return false;
        };
        let Some((id, amend)) = request.amend_read() else {
            return false;
        };
        if self.amend_read_current(id, amend) {
            return false;
        }
        if let Pending::ManagedInfo(read) = pending {
            let _ = cx.finish_http_context(&read.context);
            self.finish_background_read(cx, read.background);
        }
        true
    }
    fn info(&mut self, cx: &mut Cx<'_, Pending>, body: String, at: ArrayAt, pending: Pending) {
        self.info_read(
            cx,
            body,
            pending,
            Some(at),
            false,
            safe_cex_engine::HttpReadClass::Normal,
        );
    }
    fn info_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        body: String,
        pending: Pending,
        streamed: Option<ArrayAt>,
        background: bool,
        read_class: safe_cex_engine::HttpReadClass,
    ) {
        if self.info_completing {
            self.info_successors.push_back(InfoSuccessor {
                body,
                pending,
                streamed,
                background,
                read_class,
            });
            return;
        }
        let context = match cx.http_context(
            safe_cex_engine::request_context::Scope::Background,
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
                self.on_request_failed(cx, pending, &error.read_venue_error().message);
                self.finish_background_read(cx, background);
                return;
            }
        };
        let pending = Pending::ManagedInfo(Box::new(ManagedInfoRead {
            context: context.clone(),
            pending,
            background,
            read_class,
        }));
        match cx.request_managed_with_headers(
            context,
            None,
            Method::Post,
            format!("{}/info", self.rest()),
            &[("Content-Type", "application/json")],
            Some(body),
            pending,
        ) {
            Ok(id) => {
                if let Some(at) = streamed {
                    cx.stream_response(id, at);
                }
            }
            Err((pending, error)) => {
                self.on_request_failed(cx, pending, &error.read_venue_error().message);
            }
        }
    }
    fn meta(&mut self, cx: &mut Cx<'_, Pending>, dex: String, offset: u32) {
        self.meta_attempt(cx, dex, offset, 0);
    }
    fn meta_attempt(&mut self, cx: &mut Cx<'_, Pending>, dex: String, offset: u32, attempt: u8) {
        #[derive(Serialize)]
        struct Read<'a> {
            r#type: &'static str,
            #[serde(skip_serializing_if = "str::is_empty")]
            dex: &'a str,
        }
        self.info(
            cx,
            serde_json::to_string(&Read {
                r#type: "metaAndAssetCtxs",
                dex: &dex,
            })
            .unwrap(),
            ArrayAt::Root,
            Pending::Meta {
                dex,
                offset,
                instruments: Vec::new(),
                markets: Vec::new(),
                tickers: Vec::new(),
                excluded: Vec::new(),
                valid: true,
                stage: 0,
                attempt,
            },
        );
    }
    fn next_dex(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some((dex, offset)) = self.dex_queue.pop_front() {
            self.meta(cx, dex, offset);
        } else if !self.perps_published {
            self.perps_published = true;
            if let Some(catalog) = self.spot_catalog.take() {
                self.commit_spot_catalog(cx, catalog);
            }
            self.account_start(cx);
            self.propose_socket_interests(cx);
        }
    }
    fn start_socket(&mut self, cx: &mut Cx<'_, Pending>) {
        // Frame-fed engines stream nothing; their data source does.
        if cx.frame_fed() {
            return;
        }
        if let Err(error) = self.managed_connect(cx) {
            Self::socket_refused(cx, error);
        }
    }
    // A context retry must not advance discovery again or open a second session.
    fn advance_scope(&mut self, cx: &mut Cx<'_, Pending>, dex: &str) {
        if !self.advanced_scopes.insert(dex.into()) {
            return;
        }
        // Streams start with the first catalogue (5b15cb sat 2 min with none while HIP-3
        // dexes loaded); each later scope adds its topics to the open socket.
        if self.connection.feed.is_none() {
            self.start_socket(cx);
        } else {
            self.propose_socket_interests(cx);
        }
        if !self.spot && dex.is_empty() {
            self.info(
                cx,
                "{\"type\":\"perpDexs\"}".into(),
                ArrayAt::Root,
                Pending::Dexs {
                    dexs: Vec::new(),
                    index: 0,
                },
            );
        } else {
            self.next_dex(cx);
        }
    }
    // Keep table indices and known rails for omitted contexts. Fresh rows use
    // native identity, so retries neither duplicate peers nor move settings.
    fn commit_discovery(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        markets: Vec<Market>,
        tickers: Vec<Ticker>,
        excluded: Vec<String>,
    ) {
        for coin in excluded {
            self.excluded.insert(coin.to_ascii_lowercase());
        }
        for market in &markets {
            self.excluded.remove(&market.id.to_ascii_lowercase());
            // Account start seeded only the markets known then.
            if self.account.started
                && self.has_setting(market)
                && cx.store.setting(&market.symbol).is_none()
            {
                let seed = self.seed_setting(market);
                cx.store.update_setting(&market.symbol, |s| *s = seed);
            }
        }
        let mut all = cx.store.markets().to_vec();
        for market in markets {
            if let Some(old) = all.iter_mut().find(|old| old.id == market.id) {
                *old = market;
            } else {
                all.push(market);
            }
        }
        cx.store.set_markets(all);
        // `set_markets` clears the table; its rows index the new market snapshot.
        if !self.underlying.is_empty() {
            let rows = cx
                .store
                .markets()
                .iter()
                .enumerate()
                .filter_map(|(index, market)| {
                    let (name, full_name) = self.underlying.get(&market.symbol)?;
                    Some(safe_cex_engine::model::UnderlyingToken {
                        market: index as u32,
                        name: name.clone(),
                        full_name: full_name.clone(),
                    })
                })
                .collect();
            cx.store.set_underlying_tokens(rows);
        }
        let mut all = cx.store.tickers().to_vec();
        for ticker in tickers {
            // Discovery retries hydrate missing contexts, preserving live quotes.
            if !all.iter().any(|old| old.id == ticker.id) {
                all.push(ticker);
            }
        }
        cx.store.set_tickers(all);
    }
    fn propose_socket_interests(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.connection.open
            && self.account.mode_refresh_due
            && !self.private_socket_interests(cx.store).is_empty()
        {
            self.schedule_socket_refresh(cx, true);
            return;
        }
        if let Some(feed) = self.connection.feed {
            let desired = self.bounded_socket_interests(cx);
            self.report_subscriptions(cx, &desired);
            let admitted = desired.iter().map(|i| i.key.clone()).collect();
            self.forget_unsent(cx, feed);
            if let Err(error) = cx.set_socket_interests(feed, desired) {
                Self::socket_refused(cx, error);
            } else {
                self.connection.admitted_interests = admitted;
            }
        }
    }
    fn coin<'a>(&'a self, cx: &'a Cx<'_, Pending>, symbol: &str) -> Option<&'a str> {
        cx.store.market(symbol).map(|m| m.id.as_str())
    }
    /// Every catalogue read has succeeded, so an unknown coin is not a pending retry.
    pub(crate) fn discovery_settled(&self) -> bool {
        !self.core_discovery_incomplete
            && self.startup.idle()
            && self
                .dex_names
                .iter()
                .all(|d| self.advanced_scopes.contains(d))
    }
    /// A coin TS hides by design (delisted or unquoted), as opposed to unknown.
    pub(crate) fn excluded_coin(&self, coin: &str) -> bool {
        self.symbol(coin).is_none() && self.excluded.contains(&coin.to_ascii_lowercase())
    }
    fn symbol(&self, coin: &str) -> Option<&str> {
        self.coins
            .get(&coin.to_ascii_lowercase())
            .map(String::as_str)
    }
    fn public_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Ok(frame) = serde_json::from_str::<Envelope>(text) else {
            return;
        };
        let raw = frame.data.get();
        match frame.channel.as_ref() {
            "allMids" => {
                cx.record_socket_topic(&["@allMids|||||||"]);
                if let Ok(mids) = serde_json::from_str::<Mids>(raw) {
                    for (coin, price) in mids.mids {
                        if let (Some(symbol), Ok(mid)) = (self.symbol(&coin), price.parse::<f64>())
                            && mid.is_finite()
                            && mid > 0.0
                        {
                            let mid_is_last = !self.trade_last.contains_key(symbol);
                            cx.store.update_ticker(symbol, |t| {
                                if mid_is_last {
                                    t.last = mid;
                                }
                                t.mid_price = Some(mid);
                            });
                            // A repeated mid is still an observation of last.
                            if mid_is_last {
                                cx.store.observe_ticker_fields(symbol, 1 << 2);
                            }
                        }
                    }
                }
            }
            "fastAssetCtxs" => {
                cx.record_socket_topic(&["@fastAssetCtxs|||||||"]);
                if let Ok(encoded) = serde_json::from_str::<&str>(raw)
                    && let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded)
                    && let Ok(decoded) =
                        miniz_oxide::inflate::decompress_to_vec_with_limit(&bytes, 2_000_000)
                    && let Ok(contexts) = serde_json::from_slice::<HashMap<&str, FastCtx>>(&decoded)
                {
                    for (coin, context) in contexts {
                        if let Some(symbol) = self.symbol(coin) {
                            let usable =
                                |v: Option<f64>| v.is_some_and(|v| v.is_finite() && v > 0.0);
                            let mid_is_last = !self.trade_last.contains_key(symbol);
                            // The references this context carries, changed or not (bits: last, mark).
                            let observed = (u16::from(mid_is_last && usable(context.mid_px)) << 2)
                                | (u16::from(usable(context.mark_px)) << 3);
                            cx.store.update_ticker(symbol, |t| {
                                if let Some(mark) =
                                    context.mark_px.filter(|v| v.is_finite() && *v > 0.0)
                                {
                                    t.mark = mark;
                                }
                                if let Some(mid) =
                                    context.mid_px.filter(|v| v.is_finite() && *v > 0.0)
                                {
                                    if mid_is_last {
                                        t.last = mid;
                                    }
                                    t.mid_price = Some(mid);
                                }
                            });
                            cx.store.observe_ticker_fields(symbol, observed);
                        }
                    }
                }
            }
            "trades" => {
                if let Ok(rows) = serde_json::from_str::<Vec<TradeRow>>(raw) {
                    for row in rows {
                        cx.record_socket_topic(&["@trades|", &row.coin, "||||||"]);
                        let Some(symbol) = self.symbol(&row.coin).map(str::to_owned) else {
                            continue;
                        };
                        // A print after the last book or chart closed must not pin `last`.
                        if !self.watches(&symbol)
                            || !row.px.is_finite()
                            || row.px <= 0.0
                            || self
                                .trade_last
                                .get(&symbol)
                                .is_some_and(|at| row.time < *at)
                        {
                            continue;
                        }
                        self.trade_last.insert(symbol.clone(), row.time);
                        cx.store.update_ticker(&symbol, |t| t.last = row.px);
                        cx.store.observe_ticker_fields(&symbol, 1 << 2);
                    }
                }
            }
            "l2Book" => {
                if let Ok(depth) = serde_json::from_str::<Depth>(raw)
                    && let Some(symbol) = self.symbol(&depth.coin).map(str::to_owned)
                    && let Some(ladder) = self.books.get_mut(&symbol)
                {
                    if depth.fast {
                        cx.record_socket_topic(&["@l2Book|", &depth.coin, "||||true||"]);
                    } else if !self.spot {
                        cx.record_socket_topic(&["@l2Book|", &depth.coin, "||||||"]);
                    }
                    if self.spot {
                        let Some(market) = cx.store.market(&symbol) else {
                            return;
                        };
                        let group = spot_depth_tier(&depth.levels, market.precision.price);
                        ladder.spot_depth[group] = Some(depth.levels);
                        ladder.depth_time = Some(depth.time);
                        for (side, index) in [(BookSide::Bids, 0), (BookSide::Asks, 1)] {
                            let mut levels = Vec::new();
                            let mut last = if index == 0 {
                                f64::INFINITY
                            } else {
                                f64::NEG_INFINITY
                            };
                            for tier in ladder.spot_depth.iter().flatten() {
                                for l in &tier[index] {
                                    if if index == 0 { l.px < last } else { l.px > last } {
                                        levels.push((l.px, l.sz));
                                        last = l.px;
                                    }
                                }
                            }
                            ladder.book.load_sorted(side, levels);
                        }
                        cx.book_changed(&symbol);
                        return;
                    }
                    if !depth.time.is_finite()
                        || ladder.depth_time.is_some_and(|time| depth.time < time)
                        || !valid_perp_depth(&depth)
                    {
                        return;
                    }
                    if depth.fast {
                        ladder.fast_depth = Some(depth.levels);
                    } else {
                        ladder.depth = depth.levels;
                        if ladder.depth_time.is_none_or(|time| depth.time > time) {
                            ladder.fast_depth = None;
                        }
                    }
                    // Once shallow coverage removes a level it cannot return on a retreat.
                    if let Some(fast) = &ladder.fast_depth {
                        for (index, side) in ladder.depth.iter_mut().enumerate() {
                            side.retain(|level| {
                                fast[index].len() == 5
                                    && fast[index].last().is_some_and(|last| {
                                        if index == 0 {
                                            level.px < last.px
                                        } else {
                                            level.px > last.px
                                        }
                                    })
                            });
                        }
                    }
                    ladder.depth_time = Some(depth.time);
                    Self::combine(ladder);
                    cx.book_changed(&symbol);
                }
            }
            "bbo" => {
                if let Ok(bbo) = serde_json::from_str::<Bbo>(raw)
                    && let Some(symbol) = self.symbol(&bbo.coin).map(str::to_owned)
                    && let Some(ladder) = self.books.get_mut(&symbol)
                    && ladder
                        .bbo
                        .as_ref()
                        .is_none_or(|(time, _)| bbo.time >= *time)
                {
                    cx.record_socket_topic(&["@bbo|", &bbo.coin, "||||||"]);
                    ladder.bbo = Some((bbo.time, bbo.bbo));
                    if ladder.depth_time.is_some() {
                        Self::combine(ladder);
                        cx.book_changed(&symbol);
                    }
                }
            }
            "candle" => {
                let mut apply = |row: CandleRow<'_>| {
                    cx.record_socket_topic(&["@candle|", &row.s, "|", &row.i, "|||||"]);
                    if let Some(symbol) = self.symbol(&row.s)
                        && let Some(interval) = Interval::parse(&row.i)
                        && self.candles.contains_key(&(symbol.to_owned(), interval))
                    {
                        cx.candle(symbol, interval, row.candle());
                    }
                };
                if raw.starts_with('[') {
                    let _ = json::each(raw, &mut apply);
                } else if let Ok(row) = serde_json::from_str::<CandleRow>(raw) {
                    apply(row);
                }
            }
            _ => {}
        }
    }
    fn combine(ladder: &mut Ladder) {
        for (index, side) in [BookSide::Bids, BookSide::Asks].into_iter().enumerate() {
            let fast = ladder
                .fast_depth
                .as_ref()
                .map_or(&[][..], |fast| fast[index].as_slice());
            let depth = fast.iter().chain(ladder.depth[index].iter());
            if let Some((time, bbo)) = &ladder.bbo
                && *time >= ladder.depth_time.unwrap_or(0.0)
            {
                let mut levels = Vec::with_capacity(20);
                if let Some(best) = &bbo[index] {
                    levels.push((best.px, best.sz));
                    for level in depth {
                        if levels.len() == 20 {
                            break;
                        }
                        if if index == 0 {
                            level.px < best.px
                        } else {
                            level.px > best.px
                        } {
                            levels.push((level.px, level.sz));
                        }
                    }
                }
                ladder.book.load_sorted(side, levels);
            } else {
                ladder
                    .book
                    .load_sorted(side, depth.take(20).map(|l| (l.px, l.sz)));
            }
        }
    }
    /// A symbol with a live book or candle subscriber streams `trades`, its `last`.
    fn watches(&self, symbol: &str) -> bool {
        self.books.contains_key(symbol) || self.candles.keys().any(|(s, _)| s == symbol)
    }
    pub(crate) fn watched_symbols(&self) -> std::collections::BTreeSet<&str> {
        let candles = self.candles.keys().map(|(symbol, _)| symbol.as_str());
        self.books
            .keys()
            .map(String::as_str)
            .chain(candles)
            .collect()
    }
    /// An unwatched symbol's `last` returns to the mid on the next mids frame.
    fn forget_unwatched_trades(&mut self) {
        let (books, candles) = (&self.books, &self.candles);
        self.trade_last.retain(|symbol, _| {
            books.contains_key(symbol) || candles.keys().any(|(s, _)| s == symbol)
        });
    }
    fn book_sub(&mut self, cx: &mut Cx<'_, Pending>, symbol: String, add: bool) {
        let count = self.books.entry(symbol.clone()).or_default();
        if add {
            count.subscribers += 1;
            if count.subscribers > 1 {
                return;
            }
            self.forget_refusals(
                cx.store,
                &SubscriptionTopic::OrderBook {
                    symbol: symbol.clone(),
                },
            );
        } else {
            count.subscribers = count.subscribers.saturating_sub(1);
            if count.subscribers > 0 {
                return;
            }
            self.books.remove(&symbol);
            self.forget_unwatched_trades();
        }
        if add
            && self
                .desired_socket_interests(cx.store, cx.public_feed_policy())
                .len()
                > 1000
        {
            self.books.remove(&symbol);
            Self::socket_refused(cx, SocketError::Capacity);
            return;
        }
        if self.coin(cx, &symbol).is_some() {
            self.propose_socket_interests(cx);
        }
    }
}

fn valid_perp_depth(depth: &Depth<'_>) -> bool {
    for (index, side) in depth.levels.iter().enumerate() {
        if (depth.fast && side.len() > 5)
            || side
                .iter()
                .any(|l| !l.px.is_finite() || l.px <= 0. || !l.sz.is_finite() || l.sz <= 0.)
            || side.windows(2).any(|pair| {
                if index == 0 {
                    pair[0].px <= pair[1].px
                } else {
                    pair[0].px >= pair[1].px
                }
            })
        {
            return false;
        }
    }
    depth.levels[0]
        .first()
        .zip(depth.levels[1].first())
        .is_none_or(|(bid, ask)| bid.px < ask.px)
}
/// Spot frames do not say which nSigFigs subscription sent them. A tier's
/// prices are all multiples of its bucket, so the coarsest power-of-ten tick
/// multiple dividing every price names the tier; the native tier is 0.
fn spot_depth_tier(levels: &[Vec<Level>; 2], tick: f64) -> usize {
    let prices = || levels.iter().flatten().map(|l| l.px);
    (1..=6)
        .rev()
        .find(|&decade| {
            let step = tick * 10f64.powi(decade);
            prices().all(|px| {
                let steps = px / step;
                (steps - steps.round()).abs() < 1e-6
            })
        })
        .unwrap_or(0) as usize
}
fn price_decimals(price: f64, size: i32) -> i32 {
    let significant = if price == 0.0 {
        5
    } else {
        4 - price.abs().log10().floor() as i32
    };
    (6 - size).min(significant.max(0))
}
fn spot_price_decimals(price: f64, size: i32) -> i32 {
    let significant = if price > 0.0 {
        (4 - price.log10().floor() as i32).max(0)
    } else {
        5
    };
    (8 - size).min(significant)
}
fn market_rows(
    cx: &mut Cx<'_, Pending>,
    dex: &str,
    instruments: &[Option<Instrument>],
    raw: &str,
    markets: &mut Vec<Market>,
    tickers: &mut Vec<Ticker>,
    excluded: &mut Vec<String>,
) -> bool {
    let mut i = 0;
    json::each::<&serde_json::value::RawValue>(raw, |row| {
        let index = i;
        i += 1;
        let Some(meta) = instruments.get(index).and_then(Option::as_ref) else {
            return;
        };
        let Ok(ctx) = serde_json::from_str::<AssetCtx>(row.get()) else {
            safe_cex_engine::startup::skipped_row(cx);
            return;
        };
        let id = if dex.is_empty() || meta.name.contains(':') {
            meta.name.clone()
        } else {
            format!("{dex}:{}", meta.name)
        };
        // Like TS: HIP-3 delisted and null-mid rows are hidden, not malformed.
        let Some(price) = ctx
            .mid_px
            .filter(|p| p.is_finite() && *p > 0.0 && (dex.is_empty() || !meta.is_delisted))
        else {
            excluded.push(id);
            return;
        };
        let base = meta
            .name
            .split_once(':')
            .map_or(meta.name.as_str(), |(_, b)| b);
        let symbol = if dex.is_empty() {
            format!("{}-PERP", base.to_uppercase())
        } else {
            format!("{}|{}-PERP", dex.to_uppercase(), base.to_uppercase())
        };
        // Bounded first: the decimal arithmetic below overflows on extreme counts.
        if !(0..=18).contains(&meta.sz_decimals) {
            safe_cex_engine::startup::skipped_row(cx);
            return;
        }
        let step = 10f64.powi(-meta.sz_decimals);
        let precision = Precision {
            amount: step,
            price: 10f64.powi(-price_decimals(price, meta.sz_decimals)),
        };
        if !precision.is_tradable() {
            safe_cex_engine::startup::skipped_row(cx);
            return;
        }
        markets.push(Market {
            id: id.clone(),
            symbol: symbol.clone(),
            pretty_symbol: None,
            base: base.into(),
            quote: "USDC".into(),
            active: true,
            precision,
            limits: Limits {
                amount: MinMax {
                    min: step,
                    max: 9_007_199_254_740_991.0,
                },
                leverage: MinMax {
                    min: 1.0,
                    max: meta.max_leverage,
                },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: Some(0.00045),
            maker_fee: Some(0.00015),
        });
        let mark = if ctx.mark_px > 0.0 {
            ctx.mark_px
        } else {
            price
        };
        tickers.push(Ticker {
            id,
            symbol,
            bid: 0.0,
            ask: 0.0,
            last: price,
            mark: ctx.mark_px,
            index: ctx.oracle_px,
            mid_price: Some(price),
            percentage: if ctx.prev_day_px > 0.0 {
                (mark - ctx.prev_day_px) / ctx.prev_day_px * 100.0
            } else {
                0.0
            },
            open_interest: ctx.open_interest * mark,
            funding_rate: ctx.funding,
            funding_time: 0.0,
            volume: if ctx.day_base_vlm > 0.0 {
                ctx.day_base_vlm
            } else {
                ctx.day_ntl_vlm / price
            },
            quote_volume: ctx.day_ntl_vlm,
        });
    })
    .is_ok()
        && i == instruments.len()
}

fn spot_rows(
    cx: &mut Cx<'_, Pending>,
    meta: &SpotMeta,
    raw: &str,
    markets: &mut Vec<Market>,
    tickers: &mut Vec<Ticker>,
    excluded: &mut Vec<String>,
) -> bool {
    let Ok(contexts) = safe_cex_engine::startup::rows::<SpotCtx>(raw) else {
        return false;
    };
    let positional = contexts.len() == meta.universe.len()
        && contexts
            .iter()
            .filter_map(|row| row.0.as_ref())
            .all(|c| c.coin.is_empty());
    for row in &contexts {
        if row.0.is_none() {
            safe_cex_engine::startup::skipped_row(cx);
        }
    }
    for (position, native) in meta.universe.iter().enumerate() {
        let Some(native) = &native.0 else {
            safe_cex_engine::startup::skipped_row(cx);
            continue;
        };
        let Some(ctx) = contexts
            .iter()
            .filter_map(|row| row.0.as_ref())
            .find(|c| c.coin == native.name)
            .or_else(|| {
                positional
                    .then(|| contexts.get(position).and_then(|row| row.0.as_ref()))
                    .flatten()
            })
        else {
            continue;
        };
        let Some(token) = meta
            .tokens
            .iter()
            .filter_map(|row| row.0.as_ref())
            .find(|t| Some(&t.index) == native.tokens.first())
        else {
            continue;
        };
        let Some(quote) = meta
            .tokens
            .iter()
            .filter_map(|row| row.0.as_ref())
            .find(|t| Some(&t.index) == native.tokens.get(1))
        else {
            continue;
        };
        let Some(price) = ctx.context.mid_px.filter(|p| p.is_finite() && *p > 0.0) else {
            excluded.push(native.name.clone());
            continue;
        };
        let id = native.name.clone();
        let symbol = format!("{}/{}", token.name, quote.name);
        if !(0..=18).contains(&token.sz_decimals) {
            safe_cex_engine::startup::skipped_row(cx);
            continue;
        }
        let step = 10f64.powi(-token.sz_decimals);
        let precision = Precision {
            amount: step,
            price: 10f64.powi(-spot_price_decimals(price, token.sz_decimals)),
        };
        if !precision.is_tradable() {
            safe_cex_engine::startup::skipped_row(cx);
            continue;
        }
        let mark = if ctx.context.mark_px > 0.0 {
            ctx.context.mark_px
        } else {
            price
        };
        markets.push(Market {
            id: id.clone(),
            symbol: symbol.clone(),
            pretty_symbol: None,
            base: token.name.clone(),
            quote: quote.name.clone(),
            active: true,
            precision,
            limits: Limits {
                amount: MinMax {
                    min: step,
                    max: 9_007_199_254_740_991.0,
                },
                leverage: MinMax { min: 1.0, max: 1.0 },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: Some(true),
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        });
        let c = &ctx.context;
        tickers.push(Ticker {
            id,
            symbol,
            bid: 0.0,
            ask: 0.0,
            last: price,
            mark,
            index: c.oracle_px,
            mid_price: Some(price),
            percentage: if c.prev_day_px > 0.0 {
                (mark - c.prev_day_px) / c.prev_day_px * 100.0
            } else {
                0.0
            },
            open_interest: 0.0,
            funding_rate: c.funding,
            funding_time: 0.0,
            volume: if c.day_base_vlm > 0.0 {
                c.day_base_vlm
            } else {
                c.day_ntl_vlm / price
            },
            quote_volume: c.day_ntl_vlm,
        });
    }
    true
}

/// Frame-fed hold on this engine's own builder-approval read.
pub(crate) const BUILDER_HOLD: safe_cex_engine::setting_holds::SettingKey =
    safe_cex_engine::setting_holds::SettingKey::Venue("builder", String::new());

impl Venue for Hyperliquid {
    fn managed_sockets() -> bool {
        true
    }
    fn on_open(&mut self, _cx: &mut Cx<'_, Pending>, _socket: SocketId) {}
    fn on_close(&mut self, _cx: &mut Cx<'_, Pending>, _socket: SocketId, _code: u16) {}
    /// A take profit amended through the price is replaced as a stop (`trading.rs`),
    /// but only where mark, last and index agree enough for that projection to run.
    fn judges_protective_amend(
        &self,
        store: &safe_cex_engine::store::Store,
        order: &safe_cex_engine::model::Order,
    ) -> bool {
        order.kind == safe_cex_engine::model::OrderType::TakeProfit
            && store
                .ticker(&order.symbol)
                .is_some_and(|ticker| trading::tp_projection_reference(ticker).is_some())
    }
    fn cancel_needs_stored_order(&self) -> bool {
        true
    }

    fn guard_order_mutations(&self) -> bool {
        true
    }

    fn fence_uncertain_placements(&self) -> bool {
        true
    }
    fn enforces_reduce_only(&self, store: &safe_cex_engine::Store, symbol: &str) -> bool {
        !self.spot && store.market(symbol).is_none_or(|m| m.is_spot != Some(true))
    }

    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, symbols: Option<&[String]>) {
        self.reconcile_order_scopes(cx, symbols);
        self.reconcile_settings(cx);
    }
    // Every read reads every scope the account cover joins (H9).
    fn refreshes_orders(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn startup_reads(pending: &Pending) -> safe_cex_engine::startup::Sections {
        use safe_cex_engine::startup::Sections as S;
        match pending {
            Pending::ManagedInfo(read) => Self::startup_reads(&read.pending),
            Pending::Meta { .. } | Pending::SpotMeta { .. } => S::PUBLIC,
            Pending::Dexs { .. } => S::MARKETS,
            Pending::Account(request) => request.startup_sections(),
            _ => S::NONE,
        }
    }

    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        // Every cancel names an order id; there is no immediate cancel-all.
        if self.spot {
            safe_cex_engine::Capabilities {
                symbol_cancel_misses_unseen: true,
                order_read_covers: true,
                ..safe_cex_engine::Capabilities::NO_HISTORY
            }
        } else {
            safe_cex_engine::Capabilities {
                position_history: safe_cex_engine::PositionHistory::FillsSymbol,
                fill_history_time: safe_cex_engine::FillHistoryTime::SecondsOrMilliseconds,
                client_order_lookup: true,
                schedule_cancel: true,
                // Settings stream only for the selection; selecting a symbol reads its rails.
                leverage_query: safe_cex_engine::LeverageQuery::OnMetadata,
                fresh_positions: true,
                symbol_cancel_misses_unseen: true,
                order_read_covers: true,
                ..safe_cex_engine::Capabilities::BASE
            }
        }
    }
    fn address_nonce_domain(&self) -> Option<&'static safe_cex_engine::nonce::NonceDomain> {
        Some(&safe_cex_engine::nonce::HYPERLIQUID_AGENT)
    }
    fn public_address_slots(&self) -> safe_cex_engine::AddressSetupSlots {
        if self.options.public_data_only {
            safe_cex_engine::AddressSetupSlots::None
        } else {
            safe_cex_engine::AddressSetupSlots::Primary
        }
    }
    fn dispatch_nonce() -> bool {
        true
    }
    fn http_release() -> safe_cex_engine::HttpRelease<Self> {
        safe_cex_engine::HttpRelease::NonceUnscheduled(|venue, pending, release| {
            if let Pending::Trade(request) = pending {
                venue.check_trade_intent(request, release.time.raw_ms)?;
                request.check_cancel_deadline(release.time.raw_ms)?;
            }
            Ok(())
        })
    }
    fn http_release_veto(
        &self,
        store: &safe_cex_engine::Store,
        pending: &Pending,
        release: &safe_cex_engine::request_context::HttpRelease,
    ) -> Result<(), VenueError> {
        self.protective_release_veto(store, pending, release)
    }
    fn write_failure_text() -> Option<fn(&VenueError) -> String> {
        Some(trading::terminal_failure_text)
    }
    fn http_pacing(&self) -> &[safe_cex_engine::request_context::Pacing] {
        &[trading::PACING]
    }
    fn http_release_domain() -> Option<safe_cex_engine::request_context::ReleaseDomain> {
        Some(safe_cex_engine::request_context::ReleaseDomain {
            pacings: &[trading::PACING],
            members: safe_cex_engine::request_context::DomainMembers::Mutations,
        })
    }
    fn on_http_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        owner: &safe_cex_engine::request_context::Preflight,
    ) {
        self.trade_preflight_ready(cx, owner);
    }
    fn unproven_http_evidence(
        &self,
        pending: &Pending,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        if let Pending::Trade(request) = pending {
            self.unproven_trade_evidence(request, response, evidence);
        }
    }
    fn http_read_class(&self, pending: &Pending) -> safe_cex_engine::HttpReadClass {
        match pending {
            Pending::ManagedInfo(read) => read.read_class,
            Pending::ManagedCandles(_) | Pending::History(_) => {
                safe_cex_engine::HttpReadClass::Bulk
            }
            _ => safe_cex_engine::HttpReadClass::Normal,
        }
    }
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        match pending {
            Pending::Trade(request) => request.reconciliation(),
            _ => Err(safe_cex_engine::request_context::Error::Invalid),
        }
    }
    fn on_http_owner_retired_preflight(
        &mut self,
        retired: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) -> Option<safe_cex_engine::request_context::Preflight> {
        self.trade_owner_retired(retired)
    }
    fn on_http_plan_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        owner: &safe_cex_engine::request_context::Plan,
    ) {
        self.trade_plan_ready(cx, owner);
    }
    fn startup_diagnostics(
        &self,
        store: &safe_cex_engine::Store,
    ) -> Vec<(&'static str, safe_cex_engine::log::Value)> {
        let loaded = store.loaded();
        let stage = if !loaded.markets {
            if self.spot {
                "spotMetaAndAssetCtxs"
            } else {
                "metaAndAssetCtxs"
            }
        } else if self.options.public_data_only {
            "tickers"
        } else if self.user().is_none() {
            "userRole"
        } else if !loaded.orders {
            "frontendOpenOrders"
        } else if !loaded.balance || !loaded.positions {
            self.startup_private_stage()
        } else {
            "tickers"
        };
        let mut fields = vec![
            ("stage", stage.into()),
            ("readTimeoutMs", 20_000.0.into()),
            ("http429Count", f64::from(self.http_429_count).into()),
        ];
        if let Some(kind) = self.account.last_failed_private_read_kind {
            fields.push(("lastFailedPrivateReadKind", kind.as_str().into()));
        }
        fields
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.credentials = safe_cex_engine::credential_selection::Slots::new(
            cx.address_nonce_identities().primary.clone(),
            None,
            Default::default(),
        );
        if self.spot
            && !self.options.public_data_only
            && !self
                .sealed_agent()
                .zip(self.options.key.as_deref())
                .is_some_and(|(agent, key)| agent.eq_ignore_ascii_case(key))
        {
            cx.notice(
                safe_cex_engine::Level::Error,
                code::AUTH_REJECTED,
                "Hyperliquid spot invalid private key & wallet address pair",
            );
            cx.deactivate();
            return;
        }
        if self.spot {
            self.spot_meta(cx);
        } else {
            self.meta(cx, String::new(), 0);
            if self.spot_markets {
                self.spot_meta(cx);
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn rechecks_startup_reads() -> bool {
        true
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn recheck_startup_reads(&mut self, cx: &mut Cx<'_, Pending>) -> bool {
        self.recheck_account_startup(cx)
    }
    /// VENUE_STATE v1: `[1, builder]`, builder 0 unknown, 1 refused, 2 approved.
    /// The one write-path fact the store lacks: whether placements carry the builder.
    fn export_venue_state(&self) -> Option<Vec<u8>> {
        let builder = match self.account.builder_approved {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        };
        Some(vec![1, builder])
    }
    fn apply_venue_state(&mut self, cx: &mut Cx<'_, Pending>, state: &[u8]) {
        let source = match state {
            [1, 1, ..] => Some(false),
            [1, 2, ..] => Some(true),
            // The source has not read the approval yet.
            [1, 0, ..] => return,
            _ => {
                cx.venue_state_unreadable(
                    "Unreadable Hyperliquid venue state from the data source",
                );
                return;
            }
        };
        // A held approval (this engine's own read) wins until the source agrees.
        if cx.setting_held(&BUILDER_HOLD) {
            if source != self.account.builder_approved {
                return;
            }
            cx.release_setting(&BUILDER_HOLD);
        }
        self.account.builder_approved = source;
    }
    /// Source side: the venue's existing setting re-reads, never a write.
    fn refresh_settings(&mut self, cx: &mut Cx<'_, Pending>, symbol: Option<&str>) {
        if self.options.public_data_only || self.user().is_none() {
            return;
        }
        match symbol {
            Some(symbol) => self.refresh_uncertain_settings(cx, symbol),
            None => self.refresh_account(cx),
        }
        self.check_builder(cx);
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, Pending>) {
        // Public prices cannot establish a private snapshot or mark it loaded.
        self.account_prices(cx);
    }
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
        if let Pending::SocketRefresh { mode_ready } = pending {
            self.watch_private_snapshots(cx, epoch);
            self.finish_socket_refresh(cx, mode_ready);
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
    fn subscription_interests(
        &self,
        store: &safe_cex_engine::Store,
        topic: &SubscriptionTopic,
    ) -> Vec<(FeedId, String)> {
        self.topic_interests(store, topic)
    }
    fn repair_subscription(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        topic: &SubscriptionTopic,
    ) -> Option<Result<(), String>> {
        self.unnamed_candle_repair(cx, topic)
            .or_else(|| self.retry_refused(cx, topic))
    }
    fn socket_control(
        &self,
        _: &safe_cex_engine::Store,
        _: FeedId,
        _: bool,
        text: &str,
    ) -> safe_cex_engine::socket_lifecycle::Control {
        socket_codec::managed_control(text)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self
            .connection
            .feed
            .and_then(|feed| cx.socket_epoch(feed))
            .is_none_or(|epoch| epoch.socket() != socket)
        {
            return;
        }
        if !self.observe_private_snapshot(cx, text) || self.subscription_reply(cx, text) {
            return;
        }
        if cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.public_frame(cx, text);
        }
        self.account_frame(cx, text);
        self.live_balance(cx);
        if !socket_codec::public_market_frame(text) {
            self.refresh_asset_scope(cx);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if self.discard_retired_amend_read(cx, &pending) {
            return;
        }
        if let Pending::ManagedCandles(read) = &pending
            && cx
                .validate_read_http_delivery(&read.context, &read.context)
                .is_err()
        {
            return;
        }
        if let Pending::Leverage(read) = &pending
            && cx
                .validate_read_http_delivery(&read.context, &read.context)
                .is_err()
        {
            return;
        }
        if let Pending::History(read) = &pending
            && !read.valid_delivery(cx)
        {
            return;
        }
        // Trade answers keep their status so a refusal can be told from a lost write.
        if !(200..300).contains(&response.status) && !matches!(pending, Pending::Trade(_)) {
            let message = format!("Hyperliquid HTTP {}: {}", response.status, response.body);
            if response.status == 429 {
                self.http_429_count = self.http_429_count.saturating_add(1);
                let error = safe_cex_engine::VenueError::new(message.clone())
                    .kind(safe_cex_engine::ErrorKind::RateLimited)
                    .retry_after_ms(
                        safe_cex_engine::classify::retry_after_ms(&response).unwrap_or(5000.0),
                    );
                self.account.read_error = Some(error);
            }
            self.on_request_failed(cx, pending, &message);
            self.account.read_error = None;
            return;
        }
        match pending {
            Pending::ManagedInfo(read) => {
                if cx
                    .validate_background_read_http_delivery(&read.context)
                    .is_ok()
                {
                    self.info_completing = true;
                    self.on_response(cx, read.pending, response);
                    let _ = cx.finish_http_context(&read.context);
                    self.info_completing = false;
                    while let Some(next) = self.info_successors.pop_front() {
                        self.info_read(
                            cx,
                            next.body,
                            next.pending,
                            next.streamed,
                            next.background,
                            next.read_class,
                        );
                    }
                }
                self.finish_background_read(cx, read.background);
            }
            Pending::SocketRefresh { .. } => {}
            Pending::SpotMeta {
                meta,
                markets,
                tickers,
                excluded,
                valid,
                stage,
                ..
            } if !self.spot => {
                self.merged_spot_meta(
                    cx,
                    meta.filter(|_| valid && stage == 2),
                    markets,
                    tickers,
                    excluded,
                );
            }
            Pending::SpotMeta {
                meta,
                markets,
                tickers,
                excluded,
                valid,
                stage,
                attempt,
            } => {
                if !valid || stage != 2 {
                    self.on_request_failed(
                        cx,
                        Pending::SpotMeta {
                            meta,
                            markets,
                            tickers,
                            excluded,
                            valid,
                            stage,
                            attempt,
                        },
                        "Malformed Hyperliquid spot metadata",
                    );
                    return;
                }
                let complete = meta.as_ref().is_some_and(|meta| {
                    meta.universe.iter().all(|row| {
                        row.0.as_ref().is_some_and(|native| {
                            markets.iter().any(|m| m.id == native.name)
                                || excluded.contains(&native.name)
                        })
                    })
                });
                self.core_discovery_incomplete = !complete;
                if let Some(meta) = &meta {
                    self.register_spot(cx, meta, &markets);
                }
                if complete || !markets.is_empty() {
                    self.commit_discovery(cx, markets, tickers, excluded);
                }
                if complete {
                    self.startup.succeeded(cx, &(String::new(), 0));
                } else {
                    self.startup.failed(cx, (String::new(), 0));
                }
                self.account_prices(cx);
                if self.account.started && !self.options.public_data_only && self.user().is_some() {
                    self.refresh_account(cx);
                }
                // A complete but empty universe applies no catalogue (the store
                // never applies an empty one); later scopes are still discovered.
                if complete || cx.store.loaded().markets {
                    self.advance_scope(cx, "");
                }
            }
            Pending::Account(request) => {
                let builder = request.is_builder();
                self.account_response(cx, request, response);
                if builder {
                    self.builder_result(cx);
                }
                self.refresh_asset_scope(cx);
            }
            Pending::Trade(request) => {
                self.trade_response(cx, request, Ok(response));
                self.refresh_asset_scope(cx);
            }
            Pending::History(request) => self.history_response(cx, *request, response),
            Pending::Meta {
                dex,
                offset,
                instruments,
                markets,
                tickers,
                excluded,
                valid,
                stage,
                attempt,
            } => {
                if !valid || stage != 2 {
                    self.on_request_failed(
                        cx,
                        Pending::Meta {
                            dex,
                            offset,
                            instruments,
                            markets,
                            tickers,
                            excluded,
                            valid: false,
                            stage,
                            attempt,
                        },
                        "Malformed Hyperliquid metadata tuple",
                    );
                    return;
                }
                let complete = instruments.iter().all(|row| {
                    row.as_ref().is_some_and(|native| {
                        let id = if dex.is_empty() || native.name.contains(':') {
                            native.name.clone()
                        } else {
                            format!("{dex}:{}", native.name)
                        };
                        markets.iter().any(|market| market.id == id) || excluded.contains(&id)
                    })
                });
                if dex.is_empty() {
                    self.core_discovery_incomplete = !complete;
                }
                if complete {
                    self.startup.succeeded(cx, &(dex.clone(), offset));
                } else {
                    self.startup.failed(cx, (dex.clone(), offset));
                }
                // A changed shared identity/index may affect another scope. Ordinary
                // new DEX rows are scoped; remaps conservatively re-read every DEX.
                let refresh_all = markets.iter().any(|m| {
                    m.id.split_once(':').map_or("", |(scope, _)| scope) != dex
                        || self
                            .coins
                            .get(&m.id.to_ascii_lowercase())
                            .is_some_and(|old| old != &m.symbol)
                }) || instruments.iter().enumerate().any(|(index, native)| {
                    let Some(native) = native else {
                        return false;
                    };
                    let id = if dex.is_empty() || native.name.contains(':') {
                        native.name.clone()
                    } else {
                        format!("{dex}:{}", native.name)
                    };
                    let Some(market) = markets.iter().find(|m| m.id == id) else {
                        return false;
                    };
                    let asset = offset + index as u32;
                    self.assets
                        .get(&market.symbol)
                        .is_some_and(|old| *old != asset)
                        || self
                            .assets
                            .iter()
                            .any(|(symbol, old)| *old == asset && symbol != &market.symbol)
                }) || excluded
                    .iter()
                    .any(|coin| coin.split_once(':').map_or("", |(scope, _)| scope) != dex);
                if valid {
                    for (index, meta) in instruments.iter().enumerate() {
                        let Some(meta) = meta else {
                            continue;
                        };
                        let id = if dex.is_empty() || meta.name.contains(':') {
                            meta.name.clone()
                        } else {
                            format!("{dex}:{}", meta.name)
                        };
                        if let Some(m) = markets.iter().find(|m| m.id == id) {
                            self.assets.insert(m.symbol.clone(), offset + index as u32);
                            if meta.isolated_only() {
                                self.isolated_only.insert(m.symbol.clone());
                            }
                            self.coins
                                .insert(m.id.to_ascii_lowercase(), m.symbol.clone());
                        }
                    }
                    if complete || !markets.is_empty() {
                        self.commit_discovery(cx, markets, tickers, excluded);
                    }
                }
                self.account_prices(cx);
                if self.account.started && !self.options.public_data_only && self.user().is_some() {
                    if refresh_all {
                        self.refresh_account(cx);
                    } else {
                        self.refresh_account_scope(cx, &dex);
                    }
                }
                if complete || cx.store.loaded().markets {
                    self.advance_scope(cx, &dex);
                }
            }
            Pending::Dexs { dexs, .. } => {
                self.dex_names = dexs.iter().map(|(d, _)| d.clone()).collect();
                self.dex_queue = dexs.into();
                self.next_dex(cx);
            }
            Pending::ManagedCandles(read) => {
                if read.valid {
                    let _ = cx
                        .done_context(&read.context, serde_json::to_string(&read.candles).unwrap());
                } else {
                    let _ = cx.fail_context(&read.context, "Malformed Hyperliquid candles");
                }
            }
            Pending::Leverage(read) => self.leverage_read(cx, *read, response.body),
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::SocketRefresh { .. } | Pending::Leverage(_) => {}
            Pending::SpotMeta {
                meta,
                markets,
                tickers,
                excluded,
                valid,
                stage,
                ..
            } => {
                match *stage {
                    0 => match SpotMeta::parse(item) {
                        Ok(value) => {
                            for token in &value.tokens {
                                if token.0.is_none() {
                                    safe_cex_engine::startup::skipped_row(cx);
                                }
                            }
                            *meta = Some(value);
                        }
                        Err(_) => *valid = false,
                    },
                    1 => {
                        *valid &= meta.as_ref().is_some_and(|meta| {
                            spot_rows(cx, meta, item, markets, tickers, excluded)
                        })
                    }
                    _ => *valid = false,
                }
                *stage = stage.saturating_add(1);
            }
            Pending::Account(request) => self.account_item(cx, request, item),
            Pending::ManagedInfo(read) => {
                if cx
                    .validate_background_read_http_delivery(&read.context)
                    .is_ok()
                {
                    self.on_item(cx, &mut read.pending, item);
                }
            }
            Pending::Trade(_) => {}
            Pending::History(request) => self.history_item(cx, request, item),
            Pending::Meta {
                dex,
                instruments,
                markets,
                tickers,
                excluded,
                valid,
                stage,
                ..
            } => {
                match *stage {
                    0 => match serde_json::from_str::<Meta>(item) {
                        Ok(meta) => {
                            *instruments = meta
                                .universe
                                .into_iter()
                                .map(|row| {
                                    let parsed = serde_json::from_str::<Instrument>(row.get()).ok();
                                    if parsed.is_none() {
                                        safe_cex_engine::startup::skipped_row(cx);
                                    }
                                    parsed
                                })
                                .collect();
                        }
                        Err(_) => *valid = false,
                    },
                    1 => {
                        *valid &=
                            market_rows(cx, dex, instruments, item, markets, tickers, excluded)
                    }
                    _ => *valid = false,
                }
                *stage = stage.saturating_add(1);
            }
            Pending::Dexs { dexs, index } => {
                let position = *index;
                *index += 1;
                if let Ok(Some(dex)) = serde_json::from_str::<Option<Dex>>(item)
                    && position > 0
                    && (!self.options.testnet
                        || ["vntls", "xyz", "VOLMEX", "flx"].contains(&dex.name.as_str()))
                {
                    dexs.push((dex.name, 110_000 + (position - 1) * 10_000));
                }
            }
            Pending::ManagedCandles(read) => {
                if cx
                    .validate_read_http_delivery(&read.context, &read.context)
                    .is_err()
                {
                    return;
                }
                match serde_json::from_str::<CandleRow>(item) {
                    Ok(row) => read.candles.push(row.candle()),
                    Err(_) => read.valid = false,
                }
            }
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if self.discard_retired_amend_read(cx, &pending) {
            return;
        }
        if matches!(
            pending,
            Pending::Meta { .. } | Pending::SpotMeta { .. } | Pending::Dexs { .. }
        ) {
            self.private_read_failed(cx, account::PrivateReadKind::Discovery, error);
        }
        match pending {
            Pending::ManagedInfo(read) => {
                let _ = cx.finish_http_context(&read.context);
                self.on_request_failed(cx, read.pending, error);
                self.finish_background_read(cx, read.background);
            }
            Pending::SocketRefresh { .. } => {}
            Pending::ManagedCandles(read) => {
                let _ = cx.fail_context(&read.context, error);
            }
            Pending::Leverage(read) => {
                let _ = cx.fail_context(&read.context, error);
            }
            Pending::SpotMeta { .. } if !self.spot => {
                self.spot_markets_failed(cx, error);
            }
            Pending::SpotMeta { .. } => self.startup.failed(cx, (String::new(), 0)),
            Pending::Account(request) => {
                let builder = request.is_builder();
                self.account_failed(cx, request, error);
                if builder {
                    self.builder_result(cx);
                }
            }
            Pending::Trade(request) => self.trade_response(cx, request, Err(error)),
            Pending::History(request) => self.history_failed(cx, *request, error),
            Pending::Dexs { .. } => self.next_dex(cx),
            Pending::Meta { dex, offset, .. } => {
                let optional = !dex.is_empty();
                self.startup.failed(cx, (dex.clone(), offset));
                if optional {
                    self.advance_scope(cx, &dex);
                }
            }
        }
    }
    fn read_root_retired(
        &mut self,
        owner: safe_cex_engine::request_context::Owner,
        _pending: Vec<Pending>,
    ) {
        self.discard_history_owner(owner);
        self.account.fresh_positions.retired(owner);
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        cx.with_startup_request_failure(pending, error, |cx, pending, error| match pending {
            Pending::Trade(request) => self.trade_failed(cx, request, error),
            Pending::History(read) => self.history_failed(cx, *read, error),
            Pending::ManagedCandles(read) => {
                let _ = cx.fail_context(&read.context, error);
            }
            Pending::Leverage(read) => {
                let _ = cx.fail_context(&read.context, error);
            }
            pending => self.on_sign_failed(cx, pending, &error.message),
        });
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending {
            Pending::Trade(request) => self.trade_failed(cx, request, error),
            Pending::History(read) => self.history_failed(cx, *read, error),
            Pending::ManagedCandles(read) => {
                let _ = cx.fail_context(&read.context, error);
            }
            Pending::Leverage(read) => {
                let _ = cx.fail_context(&read.context, error);
            }
            pending => {
                self.account.read_error = Some(error.clone());
                self.on_request_failed(cx, pending, &error.message);
                self.account.read_error = None;
            }
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.private_snapshot_timer(cx, timer);
        self.account_timer(cx, timer);
        self.trade_timer(cx, timer);
        if let Some((dex, offset)) = self.startup.on_timer(timer) {
            if self.spot || dex == SPOT_SCOPE {
                self.spot_meta(cx);
            } else {
                self.meta(cx, dex, offset);
            }
        }
    }
    fn on_venue_action(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        action: safe_cex_engine::VenueAction,
    ) {
        match action {
            safe_cex_engine::VenueAction::ReadPositions => self.read_positions(cx),
            safe_cex_engine::VenueAction::ScheduleCancel { time_ms } => {
                self.schedule_cancel(cx, command, time_ms)
            }
            _ => safe_cex_engine::unsupported_venue_action(cx, command),
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => self.book_sub(cx, symbol, true),
            Command::UnsubscribeOrderBook { symbol } => self.book_sub(cx, symbol, false),
            Command::SubscribeCandles { symbol, interval } => {
                if !supported(interval) {
                    cx.notice(
                        safe_cex_engine::Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        interval.unsupported("Hyperliquid"),
                    );
                    return;
                }
                let count = self.candles.entry((symbol.clone(), interval)).or_default();
                *count += 1;
                if *count == 1 {
                    self.forget_refusals(
                        cx.store,
                        &SubscriptionTopic::Candles {
                            symbol: symbol.clone(),
                            interval,
                        },
                    );
                    if self
                        .desired_socket_interests(cx.store, cx.public_feed_policy())
                        .len()
                        > 1000
                    {
                        self.candles.remove(&(symbol, interval));
                        Self::socket_refused(cx, SocketError::Capacity);
                        return;
                    }
                    if self.coin(cx, &symbol).is_some() {
                        self.propose_socket_interests(cx);
                    }
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                if let Some(count) = self.candles.get_mut(&(symbol.clone(), interval)) {
                    *count -= 1;
                    if *count > 0 {
                        return;
                    }
                }
                self.candles.remove(&(symbol.clone(), interval));
                self.forget_unwatched_trades();
                if self.coin(cx, &symbol).is_some() {
                    self.propose_socket_interests(cx);
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
                let timeout_ms = match cx.command_wait() {
                    Some(safe_cex_engine::CommandWait::Remaining(left)) => left.min(20_000),
                    Some(safe_cex_engine::CommandWait::Expired) => {
                        let _ = cx.reject_expired_command(Operation::Read);
                        return;
                    }
                    None => 20_000,
                };
                let Ok(context) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) else {
                    return;
                };
                if !supported(interval) {
                    let _ = cx.fail_context(&context, interval.unsupported("Hyperliquid"));
                    return;
                }
                let Some(coin) = self.coin(cx, &symbol) else {
                    let _ = cx.fail_context(&context, "Market not found");
                    return;
                };
                let Some(body) =
                    candle_body(cx.protocol_now(), coin, interval, limit, from_ms, to_ms)
                else {
                    let _ = cx.fail_context(&context, "Invalid candle time range");
                    return;
                };
                self.queue_candles(
                    cx,
                    body,
                    Box::new(ManagedCandleRead {
                        context,
                        candles: Vec::new(),
                        valid: true,
                    }),
                );
            }
            Command::Query { command, query } => self.start_query(cx, command, query),
            Command::FetchLeverage { symbol, .. } if !self.spot => self.fetch_leverage(cx, symbol),
            trade => self.trade_command(cx, trade),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|l| l.depth_time.is_some())
            .map(|l| &l.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, l)| (s.as_str(), &l.book)))
    }
}
// Hyperliquid's candle codes are the TS spellings.
fn candle_body(
    now: f64,
    coin: &str,
    interval: Interval,
    limit: Option<u32>,
    from_ms: Option<f64>,
    to_ms: Option<f64>,
) -> Option<String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Req<'a> {
        coin: &'a str,
        interval: &'a str,
        start_time: u64,
        end_time: u64,
    }
    #[derive(Serialize)]
    struct Read<'a> {
        r#type: &'static str,
        req: Req<'a>,
    }
    let end = to_ms.unwrap_or(now).floor();
    let start = match (limit, from_ms) {
        (None, Some(from)) => from,
        _ => end - interval.ms() * f64::from(limit.unwrap_or(500).min(1500)),
    };
    if !start.is_finite()
        || !end.is_finite()
        || start > end
        || end > 9_007_199_254_740_991.0
        || end < 0.0
        || from_ms.is_some_and(|t| t < 0.0)
    {
        return None;
    }
    Some(
        serde_json::to_string(&Read {
            r#type: "candleSnapshot",
            req: Req {
                coin,
                interval: interval.as_str(),
                start_time: start.max(0.0).floor() as u64,
                end_time: end as u64,
            },
        })
        .unwrap(),
    )
}

fn supported(interval: Interval) -> bool {
    use Interval::*;
    matches!(
        interval,
        M1 | M3 | M5 | M15 | M30 | H1 | H2 | H4 | H8 | H12 | D1 | D3 | W1 | Mo1
    )
}

#[cfg(test)]
mod startup_tests;
