//! Independent settlement-account reconciliation and authenticated AOP stream.
pub use crate::account_wire::Context;
use crate::account_wire::{AccountRow, OrderRow, PositionRow, integer, order_side};
use crate::read_owners::Background;
use crate::{Pending, PhemexPerpetuals};
use safe_cex_engine::Level;
use safe_cex_engine::backoff::Backoff;
use safe_cex_engine::coverage::{RowReports, SnapshotBuilder};
use safe_cex_engine::credential_selection::PrimaryReason;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Context as ReadContext, Error as ReadError};
use safe_cex_engine::selected_read::{ReadRequest, ReadRole};
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthEvidence, AuthMode, AuthRecovery, Control, ControlPolicy, EncodedBatch, FeedId, FeedPolicy,
    Interest, Invalidation, LifecycleEvent, RetireReason, SocketError, TrafficPolicy,
};
use safe_cex_engine::{Cx, Fill, Method, Response, SocketId, TimerId};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::cell::Cell;
use std::collections::{HashMap, HashSet, VecDeque};

const CURRENCIES: [&str; 2] = ["USDT", "USDC"];
const RETAINED_EVENTS: usize = 20_000;
const SNAPSHOT_DEADLINE_MS: f64 = 20_000.;
const SOCKET: Config = Config {
    connect_timeout_ms: 15_000.,
    auth_timeout_ms: Some(SNAPSHOT_DEADLINE_MS),
    idle_timeout_ms: 12_500.,
    ping_interval_ms: Some(5_000.),
    backoff_base_ms: 1_000.,
    backoff_cap_ms: 16_000.,
    stable_ms: 30_000.,
};
#[derive(Default)]
struct Slice {
    balance: Balance,
    revision: u64,
    last_timestamp: u64,
}
pub struct Request {
    cycle: u64,
    currency: usize,
    write_revision: u64,
    slice_revision: u64,
    positions: SnapshotBuilder<String, Position>,
    contexts: HashMap<String, Context>,
    versions: HashMap<String, u64>,
    /// An owned row sits outside a catalogue still loading; the slice keeps its last snapshot.
    hold: bool,
}
impl Request {
    /// Rejects every position id a row on `symbol` may carry; its legs keep their stored copies.
    /// An owned row with no stored copy is reported as an error: it commits as flat.
    fn reject(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        logged: &mut RowReports<String>,
        symbol: &str,
        size: f64,
        message: String,
    ) {
        let stored = cx.store.positions().iter().any(|p| p.symbol == symbol);
        let level = if size != 0. && !stored {
            Level::Error
        } else {
            Level::Warn
        };
        let key = format!("{}:{symbol}", CURRENCIES[self.currency]);
        log_skip(cx, logged, key, level, message);
        for id in [
            symbol.to_owned(),
            format!("{symbol}:long"),
            format!("{symbol}:short"),
        ] {
            self.positions.reject(Some(id));
        }
    }
}
/// Logs a skipped row once per `key` until it maps again; polls would otherwise repeat it.
fn log_skip(
    cx: &mut Cx<'_, Pending>,
    logged: &mut RowReports<String>,
    key: String,
    level: Level,
    message: impl Into<String>,
) {
    logged.report(cx, key, level, message);
}
/// A per-symbol open-order read issued after a placement outcome went unknown.
/// Only an HTTP read started after the failure can prove which of its orders exist.
pub struct OrdersRead {
    symbol: String,
    generation: u64,
    snapshot: Option<safe_cex_engine::store::OrderSnapshotToken>,
}
#[derive(Default)]
struct Reconcile {
    generation: u64,
    reading: bool,
    context: Option<ReadContext>,
    not_before: f64,
    failures: u32,
    incomplete: u32,
    abandoned: bool,
    /// clOrdIDs of unknown-outcome placements, tagged with the generation that owes their read.
    lost: Vec<(String, u64)>,
    /// Lost placements a later cancel-all must still cancel if the read reveals them.
    cancel: Vec<(String, u64)>,
}
/// Reads with rows this engine cannot map before giving up on a symbol.
const INCOMPLETE_READS: u32 = 3;
#[derive(Deserialize)]
struct OrdersEnvelope<'a> {
    code: i64,
    #[serde(default, borrow)]
    data: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct OrdersPage<'a> {
    #[serde(borrow)]
    rows: Vec<&'a RawValue>,
}
#[derive(Default)]
pub struct State {
    started: bool,
    cycle: u64,
    pending: u8,
    reads: [Option<ReadContext>; 2],
    failed: bool,
    failures: u32,
    deadline: Option<TimerId>,
    refresh: Option<TimerId>,
    write_revision: u64,
    order_revision: u64,
    slices: [Slice; 2],
    contexts: HashMap<String, Context>,
    confirmed_modes: HashMap<String, PositionMode>,
    order_sides: HashMap<String, String>,
    feed: Option<FeedId>,
    open: bool,
    authenticated: bool,
    snapshot: bool,
    snapshot_revision: u64,
    sequence: Option<u64>,
    next_id: Cell<u64>,
    terminals: HashMap<String, u64>,
    terminal_queue: VecDeque<(String, u64)>,
    terminal_floor: u64,
    order_versions: HashMap<String, u64>,
    position_versions: HashMap<String, u64>,
    position_absent: HashSet<String>,
    fills: HashSet<String>,
    fill_queue: VecDeque<(String, u64)>,
    fill_floor: u64,
    uncertain_fills: HashMap<String, u64>,
    uncertainty_overflow: bool,
    reconcile: HashMap<String, Reconcile>,
    reconcile_timer: Option<TimerId>,
    /// `{currency}:{symbol}` (or `{currency}:` for an unreadable row) of skips already logged.
    skip_logged: RowReports<String>,
    /// Frame-fed: the data source's unknown-outcome clOrdIDs and fill uncertainty.
    source_lost: HashSet<String>,
    source_unreconciled_fills: bool,
}
impl State {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn apply_settings(
        &mut self,
        symbol: &str,
        leverage: Option<f64>,
        side: Option<PositionSide>,
        margin: Option<MarginType>,
        mode: Option<PositionMode>,
    ) {
        if let Some(context) = self.contexts.get_mut(symbol) {
            if let Some(leverage) = leverage {
                if side != Some(PositionSide::Short) {
                    context.long_leverage = leverage;
                }
                if side != Some(PositionSide::Long) {
                    context.short_leverage = leverage;
                }
            }
            if let Some(margin) = margin {
                context.margin_type = margin;
            }
            if let Some(mode) = mode {
                context.mode = mode;
            }
        }
        if let Some(mode) = mode {
            // A mode-only ACK does not establish margin or either leverage rail.
            self.confirmed_modes.insert(symbol.to_string(), mode);
        }
    }
    pub fn terminal(&mut self, id: &str, now: f64) {
        self.remember_terminal(id.to_string(), (now * 1e6) as u64);
        self.order_revision += 1;
    }
    pub fn context(&self, symbol: &str) -> Option<Context> {
        self.contexts.get(symbol).copied()
    }
    pub fn any_hedged(&self) -> bool {
        self.contexts
            .values()
            .any(|context| context.mode == PositionMode::Hedged)
            || self
                .confirmed_modes
                .values()
                .any(|mode| *mode == PositionMode::Hedged)
    }
    pub fn mode(&self, symbol: &str) -> Option<PositionMode> {
        self.confirmed_modes
            .get(symbol)
            .copied()
            .or_else(|| self.context(symbol).map(|c| c.mode))
    }
    /// Data source: the write-path state a frame-fed engine needs (VENUE_STATE).
    pub(crate) fn export_venue_state(&self) -> crate::venue_state::VenueState {
        crate::venue_state::VenueState {
            contexts: self.contexts.iter().map(|(s, c)| (s.clone(), *c)).collect(),
            confirmed: self
                .confirmed_modes
                .iter()
                .map(|(s, m)| (s.clone(), *m))
                .collect(),
            sides: self
                .order_sides
                .iter()
                .map(|(i, s)| (i.clone(), s.clone()))
                .collect(),
            lost: self
                .reconcile
                .values()
                .flat_map(|entry| entry.lost.iter().chain(&entry.cancel))
                .map(|(id, _)| id.clone())
                .collect(),
            unreconciled_fills: self.has_unreconciled_fills(),
        }
    }
    /// Frame-fed: the source's state replaces what this engine would have read.
    pub(crate) fn apply_venue_state(&mut self, state: crate::venue_state::VenueState) {
        self.contexts = state.contexts.into_iter().collect();
        self.confirmed_modes = state.confirmed.into_iter().collect();
        self.order_sides = state.sides.into_iter().collect();
        self.source_lost = state.lost.into_iter().collect();
        self.source_unreconciled_fills = state.unreconciled_fills;
    }
    /// Frame-fed: the source's order snapshot is this engine's, once it has loaded;
    /// a lost placement the source's orders now show is no longer lost.
    pub(crate) fn frame_ingested(&mut self, orders_loaded: bool, client_ids: &HashSet<String>) {
        self.authenticated = orders_loaded;
        self.snapshot = orders_loaded;
        self.open = orders_loaded;
        for entry in self.reconcile.values_mut() {
            entry.lost.retain(|(id, _)| !client_ids.contains(id));
            entry.cancel.retain(|(id, _)| !client_ids.contains(id));
        }
        self.reconcile
            .retain(|_, entry| !entry.lost.is_empty() || !entry.cancel.is_empty());
    }
    pub fn orders_ready(&self) -> bool {
        self.authenticated && self.snapshot && self.open && !self.uncertainty_overflow
    }
    pub fn order_revision(&self) -> u64 {
        self.order_revision
    }
    /// Whether `id` is the clOrdID of a placement whose outcome is still unknown.
    pub fn lost_client(&self, id: &str) -> bool {
        self.source_lost.contains(id)
            || self.reconcile.values().any(|entry| {
                entry
                    .lost
                    .iter()
                    .chain(&entry.cancel)
                    .any(|(lost, _)| lost == id)
            })
    }
    pub fn has_unreconciled_fills(&self) -> bool {
        self.uncertainty_overflow
            || !self.uncertain_fills.is_empty()
            || self.source_unreconciled_fills
    }
    pub fn order_pos_side(&self, id: &str) -> Option<&str> {
        self.order_sides
            .get(id.split("__").next().unwrap_or(id))
            .map(String::as_str)
    }
    pub fn pos_side(&self, symbol: &str, side: OrderSide, reduce: bool) -> Option<&'static str> {
        Some(match (self.mode(symbol)?, side, reduce) {
            (PositionMode::Oneway, _, _) => "Merged",
            (PositionMode::Hedged, OrderSide::Buy, false)
            | (PositionMode::Hedged, OrderSide::Sell, true) => "Long",
            _ => "Short",
        })
    }
    fn remember_terminal(&mut self, id: String, version: u64) {
        if self.terminals.contains_key(&id) {
            return;
        }
        self.order_sides.remove(&id);
        self.terminals.insert(id.clone(), version);
        self.terminal_queue.push_back((id, version));
        while self.terminal_queue.len() > RETAINED_EVENTS {
            if let Some((id, version)) = self.terminal_queue.pop_front() {
                self.terminals.remove(&id);
                self.order_versions.remove(&id);
                self.terminal_floor = self.terminal_floor.max(version);
            }
        }
    }
}
#[derive(Deserialize)]
struct AccountEnvelope<'a> {
    code: i64,
    #[serde(borrow)]
    data: AccountData<'a>,
}
#[derive(Deserialize)]
struct AccountData<'a> {
    #[serde(borrow)]
    account: AccountRow<'a>,
    #[serde(borrow)]
    positions: &'a RawValue,
}
#[derive(Deserialize)]
struct Message<'a> {
    #[serde(default, deserialize_with = "optional_integer")]
    id: Option<u64>,
    #[serde(default, borrow)]
    result: Option<&'a RawValue>,
    #[serde(default, borrow)]
    method: &'a str,
    #[serde(rename = "type", default, borrow)]
    kind: &'a str,
    #[serde(default, deserialize_with = "integer")]
    timestamp: u64,
    #[serde(default, deserialize_with = "optional_integer")]
    sequence: Option<u64>,
    #[serde(default, borrow)]
    orders_p: Option<&'a RawValue>,
    #[serde(default, borrow)]
    positions_p: Option<&'a RawValue>,
    #[serde(default, borrow)]
    accounts_p: Option<&'a RawValue>,
}
fn optional_integer<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    struct Number(#[serde(deserialize_with = "integer")] u64);
    Ok(Option::<Number>::deserialize(d)?.map(|n| n.0))
}
impl PhemexPerpetuals {
    pub(crate) fn start_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.started || cx.frame_fed() {
            return;
        }
        self.account.started = true;
        if self.options.public_data_only || self.credentials.is_none() {
            cx.store.set_balance(Balance::default());
            cx.store.set_positions(vec![]);
            cx.store.set_orders(vec![]);
            return;
        }
        cx.store.set_hedged(true);
        self.account_cycle(cx);
        self.account_connect(cx);
    }
    pub(crate) fn account_write(&mut self, _cx: &mut Cx<'_, Pending>) {
        self.account.write_revision += 1;
        self.account.order_revision += 1;
    }
    pub(crate) fn account_refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        // Frame-fed: the data source's frames carry every account change; nothing is read here.
        if cx.frame_fed() {
            return;
        }
        if !self.account.started {
            self.start_account(cx);
            return;
        }
        if self.credentials.is_none() || self.options.public_data_only || self.account.pending != 0
        {
            return;
        }
        if let Some(timer) = self.account.refresh.take() {
            cx.clear_timer(timer);
        }
        self.account_cycle(cx);
    }
    fn account_cycle(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.pending != 0 || cx.frame_fed() {
            return;
        }
        if cx.now < self.paused_until {
            self.account.refresh = Some(cx.set_timer(self.paused_until - cx.now));
            return;
        }
        self.account.cycle += 1;
        self.account.pending = 3;
        self.account.failed = false;
        self.account.deadline = Some(cx.set_timer(20_000.));
        for (currency, name) in CURRENCIES.iter().enumerate() {
            let request = Request {
                cycle: self.account.cycle,
                currency,
                write_revision: self.account.write_revision,
                slice_revision: self.account.slices[currency].revision,
                positions: SnapshotBuilder::default(),
                contexts: HashMap::new(),
                versions: HashMap::new(),
                hold: false,
            };
            let read = match self.background_read(cx, ReadRole::Read { scope: None }) {
                Ok(read) => read,
                Err(error) => {
                    self.account_failed(cx, request, &error.message);
                    continue;
                }
            };
            self.account.reads[currency] = Some(read.context().clone());
            let path = "/g-accounts/accountPositions";
            let query = format!("currency={name}");
            let queued = read.queue(
                cx,
                ReadRequest {
                    method: Method::Get,
                    url: format!("{}{path}?{query}", self.rest()),
                    body: None,
                    stream: Some(ArrayAt::Path(&["data", "positions"])),
                },
                |identity| {
                    Box::new(crate::dispatch_signing::Http::new(
                        identity.clone(),
                        self.options.testnet,
                        Method::Get,
                        path.into(),
                        query,
                        None,
                    ))
                },
                |read| {
                    Pending::Account(Background {
                        read,
                        native: request,
                    })
                },
            );
            if let Err((Pending::Account(read), error)) = queued {
                self.account_read_failed(cx, read, &error.read_venue_error().message);
            }
        }
    }
    fn account_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Err(error) = self.connect_account_socket(cx) {
            if let Some(feed) = self.account.feed {
                cx.stop_socket_feed(feed);
            }
            cx.notice(
                Level::Error,
                code::SUBSCRIPTION_REJECTED,
                format!("Phemex private socket admission failed: {error:?}"),
            );
        }
    }
    fn connect_account_socket(&mut self, cx: &mut Cx<'_, Pending>) -> Result<(), SocketError> {
        let feed = match self.account.feed {
            Some(feed) => {
                if cx.socket_epoch(feed).is_some() {
                    return Ok(());
                }
                feed
            }
            None => {
                let feed = cx.register_socket_feed_with_snapshot(FeedPolicy {
                    role: safe_cex_engine::SocketChannel::Private,
                    supervision: SOCKET,
                    auth: AuthMode::Required,
                    recovery: AuthRecovery::Reconnect,
                    permanent_refusal_group: None,
                    traffic: TrafficPolicy::PongOnly,
                    controls: ControlPolicy::Native,
                    invalidation: Invalidation::Required,
                    subscription_interval_ms: 0.,
                    close_floors: &[],
                })?;
                self.account.feed = Some(feed);
                feed
            }
        };
        cx.connect_socket(feed, self.ws())?;
        self.account.authenticated = false;
        self.account.open = false;
        self.account.snapshot = false;
        self.account.snapshot_revision = self.account.order_revision;
        self.account.sequence = None;
        Ok(())
    }
    fn account_finish(&mut self, cx: &mut Cx<'_, Pending>, currency: usize, failed: bool) {
        self.account.pending &= !(1 << currency);
        self.account.failed |= failed;
        if self.account.pending != 0 {
            return;
        }
        if let Some(timer) = self.account.deadline.take() {
            cx.clear_timer(timer);
        }
        self.account.failures = if self.account.failed {
            self.account.failures.saturating_add(1)
        } else {
            0
        };
        let wait = if self.account.failed {
            cx.jittered(Backoff::new(1_000.0, 4, f64::INFINITY).delay(self.account.failures))
        } else {
            2_000.
        };
        self.account.refresh = Some(cx.set_poll_timer_with_bounds(
            2_000.,
            safe_cex_engine::PollTimerBounds {
                not_before_ms: self.paused_until.max(if self.account.failed {
                    cx.now + wait
                } else {
                    f64::NEG_INFINITY
                }),
                ..Default::default()
            },
        ));
    }
    fn currency_read_current(&self, read: &Background<Request>) -> bool {
        read.native.cycle == self.account.cycle
            && self.account.pending & (1 << read.native.currency) != 0
            && self.account.reads[read.native.currency].as_ref() == Some(read.read.context())
    }
    pub(crate) fn currency_read_delivery(
        &self,
        cx: &Cx<'_, Pending>,
        read: &Background<Request>,
    ) -> Result<(), ReadError> {
        if !self.currency_read_current(read) {
            return Err(ReadError::Revoked);
        }
        cx.validate_background_read_http_delivery(read.read.context())
    }
    pub(crate) fn account_read_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Background<Request>,
        response: Response<'_>,
    ) {
        let current = self.currency_read_current(&read);
        let _ = cx.finish_http_context(read.read.context());
        if current {
            self.account.reads[read.native.currency] = None;
            self.account_response(cx, read.native, response);
        }
    }
    pub(crate) fn account_read_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Background<Request>,
        error: &str,
    ) {
        let current = self.currency_read_current(&read);
        let _ = cx.finish_http_context(read.read.context());
        if current {
            self.account.reads[read.native.currency] = None;
            self.account_failed(cx, read.native, error);
        }
    }
    fn orders_read_current(&self, read: &Background<OrdersRead>) -> bool {
        self.account
            .reconcile
            .get(&read.native.symbol)
            .is_some_and(|entry| {
                entry.reading && entry.context.as_ref() == Some(read.read.context())
            })
    }
    pub(crate) fn orders_read_delivery(
        &self,
        cx: &Cx<'_, Pending>,
        read: &Background<OrdersRead>,
    ) -> Result<(), ReadError> {
        if !self.orders_read_current(read) {
            return Err(ReadError::Revoked);
        }
        cx.validate_background_read_http_delivery(read.read.context())
    }
    pub(crate) fn orders_background_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Background<OrdersRead>,
        body: &str,
    ) {
        let current = self.orders_read_current(&read);
        if current {
            self.account
                .reconcile
                .get_mut(&read.native.symbol)
                .expect("current read")
                .context = None;
            self.orders_read_response(cx, read.native, read.read.context(), body);
        }
        let _ = cx.finish_http_context(read.read.context());
    }
    pub(crate) fn orders_background_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Background<OrdersRead>,
        error: &str,
    ) {
        let current = self.orders_read_current(&read);
        let _ = cx.finish_http_context(read.read.context());
        if current {
            self.account
                .reconcile
                .get_mut(&read.native.symbol)
                .expect("current read")
                .context = None;
            self.orders_read_failed(cx, read.native, error);
        }
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        if request.cycle != self.account.cycle {
            return;
        }
        // A bad row keeps its positions' stored copies and never blocks the slice's balance,
        // unless it is owned on a market that unsettled discovery may still list.
        let Ok(row) = serde_json::from_str::<PositionRow<'_>>(item) else {
            let currency = CURRENCIES[request.currency];
            let stored = cx.store.positions().iter().any(|p| {
                cx.store
                    .market(&p.symbol)
                    .is_some_and(|m| m.quote == currency)
            });
            log_skip(
                cx,
                &mut self.account.skip_logged,
                format!("{currency}:"),
                if stored { Level::Warn } else { Level::Error },
                "Phemex skipped an unreadable position row",
            );
            request.positions.reject(None);
            return;
        };
        let symbol = row.symbol.to_ascii_uppercase();
        let Some(market) = cx.store.market(&symbol) else {
            if row.size != 0.0 && self.discovery_settled() {
                let message = format!("Phemex skipped {symbol} position on an unavailable market");
                request.reject(
                    cx,
                    &mut self.account.skip_logged,
                    &symbol,
                    row.size,
                    message,
                );
            } else if row.size != 0.0 {
                cx.notice(
                    Level::Warn,
                    code::SNAPSHOT_DISCARDED,
                    "Phemex owned position market unavailable; preserving previous snapshot",
                );
                request.hold = true;
            }
            return;
        };
        if market.quote != CURRENCIES[request.currency]
            || (!row.currency.is_empty() && row.currency != CURRENCIES[request.currency])
        {
            let message = format!("Phemex skipped {symbol} position with mismatched settlement");
            request.reject(
                cx,
                &mut self.account.skip_logged,
                &symbol,
                row.size,
                message,
            );
            return;
        }
        let prior = request
            .contexts
            .get(row.symbol.as_ref())
            .copied()
            .or_else(|| self.account.context(&row.symbol.to_ascii_uppercase()));
        let Some((context, position)) = row.map(cx.store, prior) else {
            let message = format!("Phemex skipped unmappable {symbol} position row");
            request.reject(
                cx,
                &mut self.account.skip_logged,
                &symbol,
                row.size,
                message,
            );
            return;
        };
        request
            .versions
            .insert(position_key(&row, context), row.transact_time_ns);
        request
            .contexts
            .insert(row.symbol.to_ascii_uppercase(), context);
        if let Some(position) = position {
            request.positions.push(position);
        }
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if request.cycle != self.account.cycle {
            return;
        }
        let Ok(envelope) = serde_json::from_str::<AccountEnvelope<'_>>(response.body) else {
            self.account_failed(cx, request, "invalid account envelope");
            return;
        };
        if !(200..300).contains(&response.status)
            || envelope.code != 0
            || envelope.data.account.currency != CURRENCIES[request.currency]
            || request.hold
            || envelope.data.account.used < 0.
        {
            self.account_failed(cx, request, "invalid account-position snapshot");
            return;
        }
        // The engine validates/streams the positions array, leaving an empty array in its envelope.
        if !matches!(envelope.data.positions.get(), "[]" | "null") {
            self.account_failed(cx, request, "account positions were not streamed");
            return;
        }
        let currency = request.currency;
        if request.write_revision == self.account.write_revision
            && request.slice_revision == self.account.slices[currency].revision
        {
            let (mut positions, slice): (Vec<_>, Vec<_>) =
                cx.store.positions().iter().cloned().partition(|p| {
                    cx.store
                        .market(&p.symbol)
                        .is_none_or(|m| m.quote != CURRENCIES[currency])
                });
            // A successful full slice also versions omissions. Keep exact native versions
            // for present rows: fresh authoritative REST may contain a long-lived position.
            let present: HashSet<_> = request
                .positions
                .rows()
                .iter()
                .map(|p| p.id.clone())
                .collect();
            let mut known: HashSet<_> = self.account.position_versions.keys().cloned().collect();
            known.extend(cx.store.positions().iter().map(|p| p.id.clone()));
            known.extend(request.versions.keys().cloned());
            for key in known {
                let symbol = key.split(':').next().unwrap_or(&key);
                if !cx
                    .store
                    .market(symbol)
                    .is_some_and(|m| m.quote == CURRENCIES[currency])
                    || !request.positions.allows_absence(&key)
                {
                    continue;
                }
                if present.contains(&key) {
                    self.account.position_absent.remove(&key);
                    self.account.position_versions.insert(
                        key.clone(),
                        request.versions.get(&key).copied().unwrap_or(0),
                    );
                } else {
                    let version = self
                        .account
                        .position_versions
                        .get(&key)
                        .copied()
                        .unwrap_or(0)
                        .max(request.versions.get(&key).copied().unwrap_or(0));
                    self.account.position_versions.insert(key.clone(), version);
                    self.account.position_absent.insert(key);
                }
            }
            // A clean slice re-arms its skip logs; otherwise only the symbols it mapped.
            let prefix = format!("{}:", CURRENCIES[currency]);
            self.account.skip_logged.rearm_namespace(
                &prefix,
                request.positions.rejected() == 0,
                request
                    .positions
                    .rows()
                    .iter()
                    .filter(|p| request.positions.allows_absence(&p.id))
                    .map(|p| p.symbol.as_str()),
            );
            positions.extend(request.positions.commit(slice, |p| p.id.clone()));
            cx.store.set_positions(positions);
            for (symbol, context) in request.contexts {
                self.account.install_context(cx, &symbol, context, true);
            }
            // Derived over the committed slice, retained copies included, as WS pushes do.
            let upnl = currency_upnl(cx, currency);
            self.account.slices[currency].balance = Balance {
                // Account pushes use the same raw field and subtract this currency's UPnL.
                total: envelope.data.account.total - upnl,
                used: envelope.data.account.used,
                free: envelope.data.account.total - envelope.data.account.used,
                upnl,
            };
            self.account.slices[currency].revision += 1;
            self.account_publish_balance(cx);
        }
        self.account_finish(cx, currency, false);
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        if request.cycle != self.account.cycle {
            return;
        }
        cx.notice(
            Level::Warn,
            code::REQUEST_FAILED,
            format!(
                "Phemex {} account read: {error}",
                CURRENCIES[request.currency]
            ),
        );
        self.account_finish(cx, request.currency, true);
    }
    fn account_publish_balance(&self, cx: &mut Cx<'_, Pending>) {
        let mut balance = Balance::default();
        for slice in &self.account.slices {
            balance.total += slice.balance.total;
            balance.used += slice.balance.used;
            balance.free += slice.balance.free;
            balance.upnl += slice.balance.upnl;
        }
        balance.upnl = (balance.upnl * 100.).round() / 100.;
        cx.store.set_balance(balance);
        // Unread slices publish as zeros, matching TS failing closed per currency.
        let mut currencies: Vec<CurrencyBalance> = CURRENCIES
            .iter()
            .zip(&self.account.slices)
            .map(|(currency, slice)| CurrencyBalance {
                currency: (*currency).to_owned(),
                balance: slice.balance.clone(),
            })
            .collect();
        currencies.sort_by(|a, b| a.currency.cmp(&b.currency));
        cx.store.set_currency_balances(currencies);
    }
    fn record_write_order(&mut self, row: &OrderRow<'_>) {
        if matches!(row.pos_side.as_ref(), "Long" | "Short" | "Merged") {
            self.account
                .order_sides
                .insert(row.order_id.to_string(), row.pos_side.to_string());
        }
        if row.version() > 0 {
            self.account
                .order_versions
                .insert(row.order_id.to_string(), row.version());
        }
        if row.terminal() {
            self.account
                .remember_terminal(row.order_id.to_string(), row.version());
        }
        self.account.order_revision += 1;
    }
    pub(crate) fn accept_write_order(&mut self, cx: &mut Cx<'_, Pending>, row: &OrderRow<'_>) {
        self.record_write_order(row);
        if let Some(orders) = row.orders(cx.now) {
            for order in orders {
                cx.store.upsert_order(order);
            }
        }
    }
    pub(crate) fn is_account_feed(&self, feed: FeedId) -> bool {
        self.account.feed == Some(feed)
    }
    fn account_frame(&self, method: &str) -> Result<String, SocketError> {
        let id = self
            .account
            .next_id
            .get()
            .checked_add(1)
            .ok_or(SocketError::Capacity)?;
        self.account.next_id.set(id);
        Ok(format!(r#"{{"id":{id},"method":"{method}","params":[]}}"#))
    }
    pub(crate) fn account_socket_control(&self, text: &str) -> Control {
        #[derive(Deserialize)]
        struct Header<'a> {
            #[serde(default, deserialize_with = "optional_integer")]
            id: Option<u64>,
            #[serde(default, borrow)]
            error: Option<&'a RawValue>,
            #[serde(default, borrow)]
            result: Option<&'a RawValue>,
            #[serde(default, borrow)]
            method: &'a str,
        }
        let Ok(header) = serde_json::from_str::<Header<'_>>(text) else {
            return Control::Other;
        };
        if header.id == Some(999) {
            #[derive(Deserialize)]
            struct Auth<'a> {
                #[serde(borrow)]
                status: &'a str,
            }
            let success = header.error.is_none()
                && header
                    .result
                    .and_then(|r| serde_json::from_str::<Auth<'_>>(r.get()).ok())
                    .is_some_and(|r| r.status == "success");
            return Control::Auth(if success {
                AuthEvidence::Accepted
            } else {
                AuthEvidence::Rejected(safe_cex_engine::VenueError::new(
                    "Phemex private websocket authentication rejected",
                ))
            });
        }
        if header.method == "server.ping" || header.result.is_some_and(|r| r.get() == r#""pong""#) {
            Control::Pong
        } else {
            Control::Other
        }
    }
    pub(crate) fn account_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Opened(epoch) => {
                self.account.open = true;
                if let Some(credentials) = &self.credentials {
                    let recipe = crate::dispatch_signing::WsAuth::new(
                        credentials.clone(),
                        epoch.socket(),
                        999,
                    );
                    cx.send_signed(epoch.socket(), recipe.unsigned(), Box::new(recipe));
                    self.account.next_id.set(1200);
                    cx.send_after_socket_interests(
                        epoch.feed(),
                        self.account_frame("server.ping")?,
                    )?;
                }
            }
            LifecycleEvent::Authenticated { epoch, first: true } => {
                self.account.authenticated = true;
                cx.set_socket_interests(
                    epoch.feed(),
                    vec![Interest::from_wire_args("aop_p.subscribe")],
                )?;
            }
            LifecycleEvent::Ping(epoch) => {
                cx.send(epoch.socket(), self.account_frame("server.ping")?)
            }
            LifecycleEvent::Reconnect(_) => self.connect_account_socket(cx)?,
            LifecycleEvent::Retired { .. } => {
                self.account.open = false;
                self.account.authenticated = false;
                self.account.snapshot = false;
            }
            LifecycleEvent::AuthFailure { .. } => cx.notice(
                Level::Error,
                code::AUTH_REJECTED,
                "Phemex private websocket authentication rejected",
            ),
            LifecycleEvent::InterestsRefused { error, .. } => return Err(error),
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn encode_account_interests(
        &self,
        subscribe: bool,
        interests: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        for interest in interests {
            if interest.key.as_ref() != "aop_p.subscribe"
                || interest.wire_args.as_ref() != "aop_p.subscribe"
            {
                return Err(SocketError::InvalidInterest);
            }
            emit(EncodedBatch {
                text: self.account_frame(if subscribe {
                    "aop_p.subscribe"
                } else {
                    "aop_p.unsubscribe"
                })?,
                covered_keys: vec![interest.key.clone()],
            })?;
        }
        Ok(())
    }
    pub(crate) fn account_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self
            .account
            .feed
            .and_then(|feed| cx.socket_epoch(feed))
            .map(|epoch| epoch.socket())
            != Some(socket)
        {
            return false;
        }
        let Ok(message) = serde_json::from_str::<Message<'_>>(text) else {
            cx.notice(
                Level::Warn,
                code::STREAM_MALFORMED,
                "Phemex malformed private frame",
            );
            return true;
        };
        if message.id == Some(999)
            || message.method == "server.ping"
            || message.result.is_some_and(|r| r.get() == r#""pong""#)
        {
            return true;
        }
        if !self.account.authenticated {
            return true;
        }
        let has_data = message.orders_p.is_some()
            || message.positions_p.is_some()
            || message.accounts_p.is_some();
        if !has_data {
            return true;
        }
        if message
            .sequence
            .is_some_and(|s| self.account.sequence.is_some_and(|previous| s <= previous))
        {
            return true;
        }
        let snapshot = message.kind == "snapshot";
        if snapshot && self.account.snapshot {
            cx.notice(
                Level::Warn,
                code::SNAPSHOT_DISCARDED,
                "Phemex duplicate private snapshot ignored",
            );
            return true;
        }
        // Decode every row before mutating; malformed arrays cannot erase prior state.
        let orders = message
            .orders_p
            .map(|r| serde_json::from_str::<Vec<OrderRow<'_>>>(r.get()))
            .transpose();
        let positions = message
            .positions_p
            .map(|r| serde_json::from_str::<Vec<PositionRow<'_>>>(r.get()))
            .transpose();
        let accounts = message
            .accounts_p
            .map(|r| serde_json::from_str::<Vec<AccountRow<'_>>>(r.get()))
            .transpose();
        let orders = match orders {
            Ok(rows) => rows,
            Err(error) => {
                cx.notice(
                    Level::Warn,
                    code::STREAM_MALFORMED,
                    format!("Phemex invalid private orders: {error}"),
                );
                return true;
            }
        };
        let positions = match positions {
            Ok(rows) => rows,
            Err(error) => {
                cx.notice(
                    Level::Warn,
                    code::STREAM_MALFORMED,
                    format!("Phemex invalid private positions: {error}"),
                );
                return true;
            }
        };
        let accounts = match accounts {
            Ok(rows) => rows,
            Err(error) => {
                cx.notice(
                    Level::Warn,
                    code::STREAM_MALFORMED,
                    format!("Phemex invalid private accounts: {error}"),
                );
                return true;
            }
        };
        let skipped = orders.iter().flatten().filter(|row| {
            cx.store.market(&row.symbol.to_ascii_uppercase()).is_some() && row.unrecognized()
        });
        let skipped = skipped
            .map(|row| {
                format!(
                    "Phemex skipped order {} with unsupported status {} or type {}",
                    row.order_id, row.status, row.kind
                )
            })
            .collect::<Vec<_>>();
        for message in skipped {
            cx.notice(Level::Warn, code::ROW_SKIPPED, message);
        }
        if orders.as_ref().is_some_and(|rows| {
            rows.iter().any(|row| {
                (row.symbol.ends_with("USDT") || row.symbol.ends_with("USDC"))
                    && row.status() == Some(OrderStatus::Open)
                    && cx.store.market(&row.symbol.to_ascii_uppercase()).is_none()
            })
        }) {
            cx.notice(
                Level::Warn,
                code::SNAPSHOT_DISCARDED,
                "Phemex working order market unavailable; preserving previous orders",
            );
            return true;
        }
        if orders.as_ref().is_some_and(|rows| {
            rows.iter().any(|row| {
                cx.store.market(&row.symbol.to_ascii_uppercase()).is_some()
                    && !row.unrecognized()
                    && row.orders(cx.now).is_none()
            })
        }) {
            cx.notice(
                Level::Warn,
                code::SNAPSHOT_DISCARDED,
                "Phemex malformed order mapping; retaining store and private caches",
            );
            return true;
        }
        // Validate every known semantic mapping before granting snapshot authority or
        // consuming the sequence; a corrected retry with the same sequence must work.
        let mut staged_positions = Vec::new();
        let mut staged_contexts = HashMap::new();
        let mut staged_versions: HashMap<String, (u64, bool)> = HashMap::new();
        if let Some(rows) = positions {
            for row in rows {
                let symbol = row.symbol.to_ascii_uppercase();
                let Some(market) = cx.store.market(&symbol) else {
                    if row.size != 0.0 {
                        cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED, "Phemex owned position market unavailable; preserving previous snapshot");
                        return true;
                    }
                    continue;
                };
                let Some(currency) = CURRENCIES.iter().position(|c| *c == market.quote) else {
                    continue;
                };
                if let Some(confirmed) = self.account.confirmed_modes.get(&symbol) {
                    let conflicting_mode = match row.pos_mode.as_ref() {
                        "Hedged" => *confirmed != PositionMode::Hedged,
                        "OneWay" => *confirmed != PositionMode::Oneway,
                        _ => false,
                    };
                    let conflicting_side = match confirmed {
                        PositionMode::Hedged => row.pos_side == "Merged",
                        PositionMode::Oneway => matches!(row.pos_side.as_ref(), "Long" | "Short"),
                    };
                    // Delayed stream rows cannot supersede a confirmed mode write.
                    // The next account read started after that ACK resolves authority.
                    if conflicting_mode || conflicting_side {
                        continue;
                    }
                }
                let prior = staged_contexts
                    .get(&symbol)
                    .copied()
                    .or_else(|| self.account.context(&symbol));
                // A bad row keeps the store's copy; the rest of the frame still applies.
                let Some((context, position)) = row.map(cx.store, prior) else {
                    cx.notice(
                        Level::Warn,
                        code::ROW_SKIPPED,
                        format!("Phemex skipped unmappable {symbol} position row"),
                    );
                    continue;
                };
                if !row.currency.is_empty() && row.currency != CURRENCIES[currency] {
                    cx.notice(
                        Level::Warn,
                        code::ROW_SKIPPED,
                        format!("Phemex skipped {symbol} position with mismatched settlement"),
                    );
                    continue;
                }
                let version = if row.transact_time_ns > 0 {
                    row.transact_time_ns
                } else {
                    message.timestamp
                };
                let key = position_key(&row, context);
                let staged_version = staged_versions.get(&key).copied();
                let present = staged_version
                    .map_or_else(|| cx.store.position(&key).is_some(), |(_, absent)| !absent);
                let (last, absent) = staged_version.unwrap_or_else(|| {
                    (
                        self.account
                            .position_versions
                            .get(&key)
                            .copied()
                            .unwrap_or(0),
                        self.account.position_absent.contains(&key)
                            || cx.store.position(&key).is_none(),
                    )
                });
                // Simulate accepted rows before advancing sequence or publishing any
                // authority. Rejected rows cannot supply another leg's context.
                if (position.is_some() && absent && version <= last)
                    || (version == 0 && present && last > 0)
                    || (version > 0 && version < last)
                    || cx.store.position(&key).is_some_and(|position| {
                        position
                            .last_update
                            .is_some_and(|time| version > 0 && (version as f64 / 1e6) < time)
                    })
                {
                    continue;
                }
                staged_versions.insert(
                    key.clone(),
                    (
                        if position.is_some() {
                            version
                        } else {
                            last.max(version)
                        },
                        position.is_none(),
                    ),
                );
                staged_contexts.insert(symbol, context);
                staged_positions.push((row, currency, context, position, version, key, last));
            }
        }
        if accounts.as_ref().is_some_and(|rows| {
            rows.iter()
                .any(|row| CURRENCIES.contains(&row.currency.as_ref()) && row.used < 0.)
        }) {
            cx.notice(
                Level::Warn,
                code::ROW_SKIPPED,
                "Phemex skipped an invalid account row; applying the rest of the frame",
            );
        }
        if snapshot
            && orders.is_some()
            && let Err(error) = cx.complete_socket_snapshot()
        {
            cx.notice(
                Level::Warn,
                code::SNAPSHOT_DISCARDED,
                format!("Phemex private snapshot authority refused: {error:?}"),
            );
            return true;
        }
        if let Some(sequence) = message.sequence {
            self.account.sequence = Some(sequence);
        }
        if let Some(rows) = orders {
            let safe_snapshot =
                snapshot && self.account.snapshot_revision == self.account.order_revision;
            let mut staged = Vec::new();
            for row in &rows {
                if cx.store.market(&row.symbol.to_ascii_uppercase()).is_none() {
                    continue;
                }
                if row.unrecognized() {
                    // Executions stand alone; the store keeps its copy of the order.
                    if !snapshot {
                        let version = if row.version() > 0 {
                            row.version()
                        } else {
                            message.timestamp
                        };
                        self.account_fill(cx, row, version);
                    } else if safe_snapshot && let Some(order) = cx.store.order(&row.order_id) {
                        staged.push(order.clone());
                    }
                    continue;
                }
                let mapped = row.orders(cx.now).expect("validated order row");
                if snapshot && !safe_snapshot && cx.store.order(&row.order_id).is_some() {
                    continue;
                }
                let version = if row.version() > 0 {
                    row.version()
                } else {
                    message.timestamp
                };
                if !snapshot {
                    self.account_fill(cx, row, version);
                    if (row.cumulative().is_some() || row.leaves().is_some())
                        && self
                            .account
                            .uncertain_fills
                            .get(row.order_id.as_ref())
                            .is_some_and(|time| version >= *time)
                    {
                        self.account.uncertain_fills.remove(row.order_id.as_ref());
                    }
                }
                if self.account.terminals.contains_key(row.order_id.as_ref())
                    || !safe_snapshot
                        && version <= self.account.terminal_floor
                        && self.account.terminal_floor > 0
                {
                    continue;
                }
                if self
                    .account
                    .order_versions
                    .get(row.order_id.as_ref())
                    .is_some_and(|last| version > 0 && version < *last)
                {
                    continue;
                }
                self.account
                    .order_versions
                    .insert(row.order_id.to_string(), version);
                if matches!(row.pos_side.as_ref(), "Long" | "Short" | "Merged") {
                    self.account
                        .order_sides
                        .insert(row.order_id.to_string(), row.pos_side.to_string());
                }
                if row.terminal() {
                    self.account
                        .remember_terminal(row.order_id.to_string(), version);
                }
                if safe_snapshot {
                    staged.extend(mapped.into_iter().filter(|o| o.status == OrderStatus::Open));
                } else {
                    for order in mapped {
                        cx.store.upsert_order(order);
                    }
                }
            }
            if safe_snapshot {
                let active: HashSet<_> = staged.iter().map(|o| o.id.as_str()).collect();
                self.account.order_versions.retain(|id, _| {
                    active.contains(id.as_str()) || self.account.terminals.contains_key(id)
                });
                self.account
                    .order_sides
                    .retain(|id, _| active.contains(id.as_str()));
                cx.store.set_orders(staged);
                self.account.uncertain_fills.clear();
                self.account.uncertainty_overflow = false;
            }
            self.account.order_revision += 1;
            if snapshot {
                if !safe_snapshot {
                    cx.store.set_orders(cx.store.orders().to_vec());
                }
                self.account.snapshot = true;
            }
            let owed: Vec<_> = self
                .account
                .reconcile
                .iter()
                .filter(|(_, entry)| !entry.cancel.is_empty())
                .map(|(symbol, _)| symbol.clone())
                .collect();
            for symbol in owed {
                self.cancel_landed(cx, &symbol);
            }
            self.retire_late_subjects(cx, None);
        }
        if message.positions_p.is_some() {
            for (row, currency, context, position, version, key, last) in staged_positions {
                if position.is_some() {
                    self.account.position_absent.remove(&key);
                    self.account.position_versions.insert(key.clone(), version);
                } else {
                    self.account.position_absent.insert(key.clone());
                    self.account
                        .position_versions
                        .insert(key, last.max(version));
                }
                self.account
                    .install_context(cx, &row.symbol.to_ascii_uppercase(), context, false);
                if let Some(mut position) = position {
                    if version > 0 {
                        position.last_update = Some(version as f64 / 1e6);
                    }
                    cx.store.upsert_position(position);
                } else {
                    let side = match row.pos_side.as_ref() {
                        "Long" => Some(PositionSide::Long),
                        "Short" => Some(PositionSide::Short),
                        _ => None,
                    };
                    let keep = cx
                        .store
                        .positions()
                        .iter()
                        .filter(|p| {
                            p.symbol != row.symbol.to_ascii_uppercase()
                                || side.is_some_and(|s| p.side != s)
                        })
                        .cloned()
                        .collect();
                    cx.store.set_positions(keep);
                }
                self.account.slices[currency].revision += 1;
                self.account.slices[currency].balance.upnl = currency_upnl(cx, currency);
            }
            self.account_publish_balance(cx);
        }
        if let Some(rows) = accounts {
            for row in rows {
                let Some(currency) = CURRENCIES.iter().position(|c| *c == row.currency.as_ref())
                else {
                    continue;
                };
                // REST has no native account timestamp. Retain the latest observed
                // WS version across reads, rather than promoting the client clock.
                let last = self.account.slices[currency].last_timestamp;
                if row.used < 0. || (last > 0 && message.timestamp <= last) {
                    continue;
                }
                self.account.slices[currency].last_timestamp = message.timestamp;
                let upnl = currency_upnl(cx, currency);
                self.account.slices[currency].balance = Balance {
                    used: row.used,
                    free: row.total - row.used,
                    total: row.total - upnl,
                    upnl,
                };
                self.account.slices[currency].revision += 1;
            }
            self.account_publish_balance(cx);
        }
        true
    }
    /// Placements on `symbols` ended with an unknown outcome; read their open
    /// orders no earlier than `not_before`. `lost` is the unanswered order, if any.
    pub(crate) fn orders_uncertain(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbols: Vec<String>,
        lost: Option<&Order>,
        not_before: f64,
    ) {
        self.note_uncertain_orders(symbols, lost, not_before);
        self.orders_reconcile(cx);
    }
    pub(crate) fn note_uncertain_orders(
        &mut self,
        symbols: Vec<String>,
        lost: Option<&Order>,
        not_before: f64,
    ) {
        for symbol in symbols {
            let entry = self.account.reconcile.entry(symbol.clone()).or_default();
            entry.generation += 1;
            entry.not_before = entry.not_before.max(not_before);
            entry.incomplete = 0;
            entry.abandoned = false;
            if let Some(id) = lost
                .filter(|order| order.symbol == symbol)
                .and_then(|order| order.client_order_id.clone())
            {
                entry.lost.push((id, entry.generation));
            }
        }
    }
    /// A cancel-all on `symbol` was dispatched; a still-unknown placement may land later.
    pub(crate) fn orders_cancel_all(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &safe_cex_engine::request_context::Preflight,
        symbol: &str,
    ) {
        let Some(entry) = self.account.reconcile.get_mut(symbol) else {
            return;
        };
        // An order the store already shows is covered by this cancel-all itself.
        let lost: Vec<_> = entry
            .lost
            .iter()
            .filter(|(id, _)| {
                !cx.store.orders().iter().any(|order| {
                    order.symbol == symbol && order.client_order_id.as_ref() == Some(id)
                })
            })
            .cloned()
            .collect();
        if lost.is_empty() {
            return;
        }
        let ids: Vec<_> = lost.iter().map(|(id, _)| id.as_str()).collect();
        cx.notice(
            Level::Warn,
            code::ORDERS_REPLACED,
            format!(
                "Phemex cancel-all on {symbol} sent while placement {} has an unknown outcome; \
                 it is canceled if the open-order read after its signed expiry finds it",
                ids.join(", ")
            ),
        );
        let covered = self.mint_late_subjects(cx, preflight, symbol, &lost);
        let entry = self
            .account
            .reconcile
            .get_mut(symbol)
            .expect("existing lost placement");
        for item in covered {
            if !entry.cancel.contains(&item) {
                entry.cancel.push(item);
            }
        }
    }
    /// Cancels live orders of lost placements a cancel-all already covered.
    /// A complete read proves every lost placement it postdates.
    fn cancel_late_orders(&mut self, cx: &mut Cx<'_, Pending>, read: &OrdersRead, complete: bool) {
        self.cancel_landed(cx, &read.symbol);
        if complete && let Some(entry) = self.account.reconcile.get_mut(&read.symbol) {
            entry
                .cancel
                .retain(|(_, generation)| *generation > read.generation);
            entry
                .lost
                .retain(|(_, generation)| *generation > read.generation);
        }
    }
    /// Cancels any stored order a cancel-all owes on `symbol`, however it arrived.
    fn cancel_landed(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(entry) = self
            .account
            .reconcile
            .get_mut(symbol)
            .filter(|entry| !entry.cancel.is_empty())
        else {
            return;
        };
        let (mut late, mut handled, mut filled) = (Vec::new(), Vec::new(), Vec::new());
        for order in cx.store.orders() {
            let Some(client) = order.client_order_id.as_deref() else {
                continue;
            };
            if order.symbol != symbol
                || order.parent_id.is_some()
                || !entry.cancel.iter().any(|(id, _)| id == client)
            {
                continue;
            }
            let open = matches!(order.status, OrderStatus::Open | OrderStatus::Updating);
            if open {
                late.push(order.id.clone());
            }
            if order.filled > 0. {
                filled.push(format!("{} filled {}", order.id, order.filled));
            }
            if open || order.filled > 0. {
                handled.push(client.to_owned());
            }
        }
        entry.cancel.retain(|(id, _)| !handled.contains(id));
        if !filled.is_empty() {
            cx.notice(
                Level::Error,
                code::CHILD_LEG_FAILED,
                format!(
                    "Phemex order {} landed after cancel-all on {symbol}; check the position",
                    filled.join(", ")
                ),
            );
        }
        if !late.is_empty() {
            self.cancel_late(cx, symbol, late);
        }
    }
    pub(crate) fn reconcile_uncertain_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        self.settle_retired_trading(cx);
        if self.account.reconcile_timer.is_none() {
            self.orders_reconcile(cx);
        }
    }

    /// Starts each owed read once it is due and no placement on that symbol is in flight.
    pub(crate) fn orders_reconcile(&mut self, cx: &mut Cx<'_, Pending>) {
        // Frame-fed: the source's orders settle an unknown outcome; nothing is read here.
        if self.credentials.is_none() || cx.frame_fed() {
            return;
        }
        if let Some(timer) = self.account.reconcile_timer.take() {
            cx.clear_timer(timer);
        }
        let mut due = Vec::new();
        let mut wake: Option<f64> = None;
        for (symbol, read) in &self.account.reconcile {
            if read.reading || read.abandoned || self.trading.placing(symbol) {
                continue;
            }
            let at = read.not_before.max(self.paused_until);
            if at <= cx.now {
                due.push((symbol.clone(), read.generation));
            } else {
                wake = Some(wake.map_or(at, |wake| wake.min(at)));
            }
        }
        if let Some(at) = wake {
            self.account.reconcile_timer = Some(cx.set_timer(at - cx.now));
        }
        for (symbol, generation) in due {
            if let Some(read) = self.account.reconcile.get_mut(&symbol) {
                read.reading = true;
            }
            let query = format!(
                "symbol={}",
                crate::trading::encode(self.native_symbol(&symbol))
            );
            let request = OrdersRead {
                symbol: symbol.clone(),
                generation,
                snapshot: cx.store.begin_orders_snapshot(),
            };
            let read = match self.background_read(
                cx,
                ReadRole::Primary {
                    reason: PrimaryReason::PostUnknownOrderRead,
                    scope: None,
                },
            ) {
                Ok(read) => read,
                Err(error) => {
                    self.orders_read_failed(cx, request, &error.message);
                    continue;
                }
            };
            self.account
                .reconcile
                .get_mut(&symbol)
                .expect("reserved read")
                .context = Some(read.context().clone());
            let path = "/g-orders/activeList";
            let queued = read.queue(
                cx,
                ReadRequest {
                    method: Method::Get,
                    url: format!("{}{path}?{query}", self.rest()),
                    body: None,
                    stream: None,
                },
                |identity| {
                    Box::new(crate::dispatch_signing::Http::new(
                        identity.clone(),
                        self.options.testnet,
                        Method::Get,
                        path.into(),
                        query,
                        None,
                    ))
                },
                |read| {
                    Pending::Orders(Background {
                        read,
                        native: request,
                    })
                },
            );
            if let Err((Pending::Orders(read), error)) = queued {
                self.orders_background_failed(cx, read, &error.read_venue_error().message);
            }
        }
    }
    pub(crate) fn orders_read_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut read: OrdersRead,
        context: &ReadContext,
        body: &str,
    ) {
        let Ok(envelope) = serde_json::from_str::<OrdersEnvelope<'_>>(body) else {
            self.orders_read_failed(cx, read, "malformed open-order envelope");
            return;
        };
        let data = envelope
            .data
            .map(RawValue::get)
            .unwrap_or("null")
            .trim_start();
        // Phemex answers an empty open-order list with 10002 OM_ORDER_NOT_FOUND.
        let rows = match envelope.code {
            10002 if data == "null" => Some(vec![]),
            0 if data.starts_with('[') => serde_json::from_str::<Vec<&RawValue>>(data).ok(),
            0 => serde_json::from_str::<OrdersPage<'_>>(data)
                .ok()
                .map(|page| page.rows),
            _ => None,
        };
        let Some(rows) = rows else {
            let error = format!("open-order read failed with code {}", envelope.code);
            self.orders_read_failed(cx, read, &error);
            return;
        };
        let mut complete = true;
        let mut published = Vec::new();
        for raw in rows {
            let Ok(row) = serde_json::from_str::<OrderRow<'_>>(raw.get()) else {
                cx.notice(
                    Level::Warn,
                    code::ROW_SKIPPED,
                    format!(
                        "Phemex skipped an unreadable {} open-order row",
                        read.symbol
                    ),
                );
                complete = false;
                continue;
            };
            if row.symbol.to_ascii_uppercase() != read.symbol || row.terminal() {
                continue;
            }
            if row.unrecognized() {
                cx.notice(
                    Level::Warn,
                    code::ROW_SKIPPED,
                    format!(
                        "Phemex skipped open order {} with unsupported status {} or type {}",
                        row.order_id, row.status, row.kind
                    ),
                );
                complete = false;
                continue;
            }
            if row.orders(cx.now).is_none() {
                cx.notice(
                    Level::Warn,
                    code::ROW_SKIPPED,
                    format!("Phemex skipped unmappable open order {}", row.order_id),
                );
                complete = false;
                continue;
            }
            // The private stream stays authoritative for every order it already reported.
            if cx.store.order(&row.order_id).is_some()
                || published
                    .iter()
                    .any(|order: &Order| order.id == row.order_id)
                || self.account.terminals.contains_key(row.order_id.as_ref())
            {
                continue;
            }
            published.extend(row.orders(cx.now).expect("validated native mapping"));
            self.record_write_order(&row);
        }
        let receipt = if self.has_late_subjects(&read.symbol) && cx.store.loaded().orders {
            read.snapshot.take().and_then(|token| {
                cx.commit_repair_snapshot(context, [token], |current| {
                    let mut merged = current.to_vec();
                    for order in &published {
                        if let Some(old) = merged.iter_mut().find(|old| old.id == order.id) {
                            *old = order.clone();
                        } else {
                            merged.push(order.clone());
                        }
                    }
                    merged
                })
            })
        } else {
            None
        };
        if receipt.is_none() {
            for order in published {
                cx.store.upsert_order(order);
            }
        }
        if complete && let Some(receipt) = &receipt {
            self.retire_late_subjects(cx, Some((&read.symbol, read.generation, receipt)));
        }
        self.cancel_late_orders(cx, &read, complete);
        let symbol = read.symbol.clone();
        if !complete {
            let Some(entry) = self.account.reconcile.get_mut(&symbol) else {
                return;
            };
            entry.incomplete += 1;
            if entry.incomplete < INCOMPLETE_READS {
                self.orders_read_failed(cx, read, "incomplete open-order read");
                return;
            }
            // Retrying cannot fix a row this engine cannot map, and skipping it
            // could hide the lost placement; stay fenced and say so once.
            entry.reading = false;
            entry.abandoned = true;
            cx.notice(Level::Error, code::CHILD_LEG_FAILED,
                format!(
                    "Phemex {symbol} open orders include rows this engine cannot read; \
                     an order from the unknown placement may stay open, cancel those orders individually"
                ),
            );
            self.orders_reconcile(cx);
            return;
        }
        if let Some(entry) = self.account.reconcile.get_mut(&symbol) {
            entry.reading = false;
            entry.failures = 0;
        }
        if !self.trading.placing(&symbol) {
            cx.orders_reconciled(Some(&symbol));
            if self
                .account
                .reconcile
                .get(&symbol)
                .is_some_and(|entry| entry.generation == read.generation)
            {
                self.account.reconcile.remove(&symbol);
            }
        }
        self.orders_reconcile(cx);
    }
    pub(crate) fn orders_read_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: OrdersRead,
        error: &str,
    ) {
        if let Some(entry) = self.account.reconcile.get_mut(&read.symbol) {
            entry.reading = false;
            entry.failures = entry.failures.saturating_add(1);
            let wait = cx.jittered(Backoff::new(1_000.0, 5, f64::INFINITY).delay(entry.failures));
            entry.not_before = entry.not_before.max(cx.now + wait);
        }
        // Keep the cause, never the response body.
        let cause = error.split_once(": ").map_or(error, |(cause, _)| cause);
        cx.notice(
            Level::Warn,
            code::REQUEST_FAILED,
            format!(
                "Phemex {} open-order reconciliation failed: {cause}; retrying",
                read.symbol
            ),
        );
        self.orders_reconcile(cx);
    }
    fn account_fill(&mut self, cx: &mut Cx<'_, Pending>, row: &OrderRow<'_>, version: u64) {
        let id = row.execution_id.as_ref();
        if id.is_empty()
            || id == "00000000-0000-0000-0000-000000000000"
            || self.account.fills.contains(id)
            || row.exec_qty <= 0.
            || row.exec_price_rp <= 0.
            || version <= self.account.fill_floor && self.account.fill_floor > 0
        {
            return;
        }
        if row.trade_type != "Trade"
            && !matches!(row.exec_status.as_ref(), "TakerFill" | "MakerFill")
        {
            return;
        }
        let Some(side) = order_side(&row.side) else {
            return;
        };
        self.account.fills.insert(id.to_string());
        self.account.fill_queue.push_back((id.to_string(), version));
        while self.account.fill_queue.len() > RETAINED_EVENTS {
            if let Some((id, version)) = self.account.fill_queue.pop_front() {
                self.account.fills.remove(&id);
                self.account.fill_floor = self.account.fill_floor.max(version);
            }
        }
        if row.cumulative().is_none() && row.leaves().is_none() {
            if self.account.uncertain_fills.len() < RETAINED_EVENTS
                || self
                    .account
                    .uncertain_fills
                    .contains_key(row.order_id.as_ref())
            {
                self.account
                    .uncertain_fills
                    .entry(row.order_id.to_string())
                    .and_modify(|time| *time = (*time).max(version))
                    .or_insert(version);
            } else {
                self.account.uncertainty_overflow = true;
                cx.notice(Level::Warn, code::STREAM_UNAVAILABLE,"Phemex execution reconciliation capacity exceeded; reconnecting for an authoritative snapshot");
                if let Some(feed) = self.account.feed {
                    cx.retire_socket(feed, RetireReason::Closed);
                }
            }
        }
        cx.fill(Fill {
            id: None,
            order_id: row.order_id.to_string(),
            symbol: row.symbol.to_string(),
            side,
            price: row.exec_price_rp,
            amount: row.exec_qty,
        });
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.reconcile_timer == Some(timer) {
            self.account.reconcile_timer = None;
            self.orders_reconcile(cx);
            return true;
        }
        if self.account.refresh == Some(timer) {
            self.account.refresh = None;
            self.account_cycle(cx);
            return true;
        }
        if self.account.deadline == Some(timer) {
            self.account.deadline = None;
            for context in &mut self.account.reads {
                if let Some(context) = context.take() {
                    let _ = cx.finish_http_context(&context);
                }
            }
            self.account.cycle += 1;
            self.account.pending = 0;
            self.account.failures = self.account.failures.saturating_add(1);
            cx.notice(
                Level::Warn,
                code::REQUEST_FAILED,
                "Phemex private reconciliation exceeded 20-second deadline; retaining state",
            );
            self.account.refresh = Some(cx.set_poll_timer_with_bounds(
                2_000.,
                safe_cex_engine::PollTimerBounds {
                    not_before_ms: self.paused_until.max(cx.now + 2_000.),
                    ..Default::default()
                },
            ));
            return true;
        }
        false
    }
}
impl State {
    fn install_context(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        context: Context,
        authoritative: bool,
    ) {
        if authoritative {
            self.confirmed_modes.remove(symbol);
        }
        self.contexts.insert(symbol.to_string(), context);
        let leverage = if context.long_leverage > 0. {
            context.long_leverage
        } else {
            context.short_leverage
        };
        if leverage > 0. {
            cx.store.update_setting(symbol, |s| {
                s.leverage = leverage;
                s.buy_leverage = (context.long_leverage > 0.).then_some(context.long_leverage);
                s.sell_leverage = (context.short_leverage > 0.).then_some(context.short_leverage);
                s.margin_type = Some(context.margin_type)
            });
        }
        cx.store.set_hedged(self.any_hedged());
    }
}
fn currency_upnl(cx: &Cx<'_, Pending>, currency: usize) -> f64 {
    let upnl: f64 = cx
        .store
        .positions()
        .iter()
        .filter(|p| {
            cx.store
                .market(&p.symbol)
                .is_some_and(|m| m.quote == CURRENCIES[currency])
        })
        .map(|p| p.unrealized_pnl)
        .sum();
    (upnl * 100.).round() / 100.
}

fn position_key(row: &PositionRow<'_>, context: Context) -> String {
    let symbol = row.symbol.to_ascii_uppercase();
    if context.mode == PositionMode::Oneway {
        symbol
    } else {
        format!(
            "{symbol}:{}",
            if row.pos_side == "Long" || row.side == "Buy" {
                "long"
            } else {
                "short"
            }
        )
    }
}
