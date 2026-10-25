//! Session authentication, private streams and atomic REST reconciliation.
use crate::wire::{Number, Str, Text};
use crate::{Grvt, Job, Pending};
use safe_cex_engine::Level;
use safe_cex_engine::coverage::{RowReports, SnapshotBuilder};
use safe_cex_engine::json::{self, ArrayAt};
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Balance, CurrencyBalance, MarginType, Order, OrderSide, OrderStatus, OrderType, Position,
    PositionMode, PositionSide, StopTriggerType, TimeInForce,
};
use safe_cex_engine::request_context::Operation;
use safe_cex_engine::socket::{Config, Supervisor, Tick as SocketTick};
use safe_cex_engine::store::Store;
use safe_cex_engine::{Cx, Fill, Response, SocketChannel, SocketId, TimerId};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

const AUTH: &str = "/auth/api_key/login";
const SUMMARY: &str = "/lite/v1/aggregated_account_summary";
const POSITIONS: &str = "/lite/v1/positions";
const ORDERS: &str = "/lite/v1/open_orders";
const LEVERAGE: &str = "/lite/v1/get_all_initial_leverage";
const BUILDERS: &str = "/full/v1/get_authorized_builders";
const RECENT_LIMIT: usize = 1000;
pub(crate) const BUILDER_MAINNET: &str = "0xee8f3559f298fffa79b8253437969d15b5dccff3";
pub(crate) const BUILDER_TESTNET: &str = "0xb0108ae4c07218ce320e5e22dd911a01c2042162";

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuilderState {
    #[default]
    Unknown,
    Checking,
    Unauthorized,
    CapTooLow(u32),
    Authorized,
}
pub(crate) struct Session {
    pub cookie: String,
    pub account_id: String,
}
pub(crate) struct OrderProgress {
    pub native_id: Option<String>,
    pub created_ns: u64,
    pub updated_ns: u64,
    pub filled: f64,
    pub amount: f64,
    pub status: OrderStatus,
}
#[derive(Default)]
pub(crate) struct Account {
    pub session: Option<Session>,
    pub generation: u64,
    pub trading_account_id: Option<String>,
    pub builder: BuilderState,
    pub native_ids: HashMap<String, String>,
    pub orders_revision: u64,
    pub positions_revision: u64,
    pub settings_revision: u64,
    pub execution_revision: u64,
    progress: HashMap<String, OrderProgress>,
    pub socket: Option<SocketId>,
    started: bool,
    /// Engine session owning API-key logins. Not repeat-safe: a login that
    /// does not succeed retires it, so a reissue is a new generation and the
    /// engine drops late answers to the abandoned one.
    login_session: Option<safe_cex_engine::request_context::Session>,
    login_pending: bool,
    waiting: Vec<Pending>,
    summary_pending: bool,
    orders_pending: bool,
    positions_pending: bool,
    leverage_pending: bool,
    authenticated: bool,
    authentication_failed: bool,
    refresh_attempts: u8,
    healthy_since: Option<f64>,
    last_received: f64,
    ping_sent: bool,
    pong_ping: Option<(TimerId, SocketId, u64)>,
    request_id: u32,
    supervisor: Supervisor,
    tick: Option<TimerId>,
    login_retry: Option<TimerId>,
    builder_retry: Option<TimerId>,
    builder_attempt: u8,
    pub builder_error: Option<String>,
    orders_timer: Option<TimerId>,
    positions_timer: Option<TimerId>,
    fills: HashSet<String>,
    fill_queue: VecDeque<String>,
    terminal: HashMap<String, (u64, Option<String>)>,
    terminal_queue: VecDeque<String>,
    identity_queue: VecDeque<String>,
    position_times: HashMap<String, u64>,
    order_times: HashMap<String, u64>,
    order_created: HashMap<String, u64>,
    sequences: HashMap<(String, String), u64>,
    first_sequence: HashMap<(String, String), u64>,
    placement_created: HashMap<String, u64>,
    pending_placements: HashSet<String>,
    prior_lifecycle: HashMap<String, (u64, Option<String>)>,
    /// `position:`/`order:`/`coin:` + identity (empty when unreadable) of skips already logged.
    skip_logged: RowReports<String>,
}
impl Account {
    pub(crate) fn cancellation_evidence(&self) -> HashMap<String, (Option<String>, Option<u64>)> {
        self.order_created
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    (self.native_ids.get(id).cloned(), self.order_created(id)),
                )
            })
            .collect()
    }
    pub(crate) fn order_progress(&self, client: &str) -> Option<&OrderProgress> {
        self.progress.get(client)
    }
    pub(crate) fn order_created(&self, client: &str) -> Option<u64> {
        self.order_created
            .get(client)
            .copied()
            .filter(|time| *time > 0)
    }
    pub(crate) fn record_identity(&mut self, client: &str, native: Option<&str>, created_at: f64) {
        let created = nanos("", created_at);
        if native.is_some()
            && self
                .placement_created
                .get(client)
                .is_some_and(|previous| *previous != created)
        {
            return;
        }
        if native.is_none() {
            let prior_created = self.order_created.get(client).copied().unwrap_or(0);
            let prior_native = self.native_ids.get(client).cloned().or_else(|| {
                self.terminal
                    .get(client)
                    .and_then(|(_, native)| native.clone())
            });
            if prior_created > 0 || prior_native.is_some() {
                self.prior_lifecycle
                    .insert(client.to_owned(), (prior_created, prior_native));
            }
            self.terminal.remove(client);
            self.terminal_queue.retain(|id| id != client);
            self.native_ids.remove(client);
            self.progress.remove(client);
            self.order_times.remove(client);
            self.order_created.remove(client);
            self.identity_queue.retain(|id| id != client);
            self.placement_created.insert(client.to_owned(), created);
            self.pending_placements.insert(client.to_owned());
        }
        if !self.order_times.contains_key(client) {
            self.identity_queue.push_back(client.to_owned());
        }
        self.order_created.entry(client.to_owned()).or_insert(0);
        self.order_times.entry(client.to_owned()).or_insert(0);
        if let Some(native) = native {
            self.native_ids.insert(client.to_owned(), native.to_owned());
        }
        while self.identity_queue.len() > RECENT_LIMIT {
            if let Some(id) = self.identity_queue.pop_front() {
                self.native_ids.remove(&id);
                self.progress.remove(&id);
                self.order_times.remove(&id);
                self.order_created.remove(&id);
                self.placement_created.remove(&id);
                self.pending_placements.remove(&id);
                self.prior_lifecycle.remove(&id);
            }
        }
    }
}
/// Logs a skipped row once per `key` until it maps again; polls would otherwise repeat it.
fn log_skip(
    cx: &mut Cx<'_, Pending>,
    logged: &mut RowReports<String>,
    key: String,
    level: Level,
    message: String,
) {
    logged.report(cx, key, level, message);
}
/// A rejected row with no stored copy kept is an error: it commits as absent.
fn skip_level(stored: bool) -> Level {
    if stored { Level::Warn } else { Level::Error }
}
/// Re-arms the skip logs of `prefix`: all of them after a clean read, else those `ids` mapped.
fn rearm_skips<'a>(
    logged: &mut RowReports<String>,
    prefix: &str,
    clean: bool,
    ids: impl IntoIterator<Item = &'a str>,
) {
    logged.rearm_namespace(prefix, clean, ids);
}
impl Account {
    pub(crate) fn new() -> Self {
        Self {
            supervisor: Supervisor::new(Config {
                auth_timeout_ms: Some(15_000.),
                idle_timeout_ms: 90_000.,
                ping_interval_ms: Some(15_000.),
                ..Config::default()
            }),
            ..Self::default()
        }
    }
}
pub(crate) enum Request {
    Login,
    Summary,
    Positions {
        revision: u64,
        issued_at: f64,
        rows: Vec<PositionRecord>,
        /// Rejections only; accepted rows are pushed once the response passes its fence.
        positions: SnapshotBuilder<String, Position>,
    },
    Orders {
        revision: u64,
        issued_at: f64,
        rows: Vec<DecodedOrder>,
        /// Rejections by stored (canonical client) id; accepted rows are pushed on response.
        orders: SnapshotBuilder<String, Order>,
    },
    Leverage {
        revision: u64,
        command: Option<(u32, String)>,
        rows: Vec<Leverage>,
        failed: bool,
    },
    Builders {
        found: Option<BuilderState>,
        failed: bool,
    },
}
impl Request {
    pub(crate) fn array_at(&self) -> Option<ArrayAt> {
        match self {
            Self::Positions { .. }
            | Self::Orders { .. }
            | Self::Leverage { .. }
            | Self::Builders { .. } => {
                Some(ArrayAt::Envelope(&[&["r"], &["result"], &["results"]]))
            }
            _ => None,
        }
    }
    pub(crate) fn reset(&mut self) {
        match self {
            Self::Positions {
                rows, positions, ..
            } => {
                rows.clear();
                *positions = SnapshotBuilder::default();
            }
            Self::Orders { rows, orders, .. } => {
                rows.clear();
                *orders = SnapshotBuilder::default();
            }
            Self::Leverage { rows, failed, .. } => {
                rows.clear();
                *failed = false;
            }
            Self::Builders { found, failed } => {
                *found = None;
                *failed = false;
            }
            _ => {}
        }
    }
}
#[derive(Deserialize)]
struct LoginBody<'a> {
    #[serde(borrow, default)]
    sub_account_id: Text<'a>,
}
#[derive(Deserialize)]
struct SummaryEnvelope<'a> {
    #[serde(borrow, rename = "r", alias = "result")]
    result: Summary<'a>,
}
#[derive(Deserialize)]
struct Summary<'a> {
    #[serde(rename = "te", alias = "total_equity")]
    equity: Number,
    #[serde(rename = "ts2", alias = "total_sub_account_available_balance", default)]
    available: Option<Number>,
    #[serde(rename = "tu", alias = "total_upnl", default)]
    upnl: Option<Number>,
    #[serde(borrow, rename = "sas", alias = "sub_account_summaries", default)]
    subaccounts: Vec<Subaccount<'a>>,
    #[serde(borrow, rename = "sb", alias = "spot_balances", default)]
    spot_balances: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct SpotCurrency<'a> {
    #[serde(borrow, rename = "c", alias = "currency")]
    currency: Text<'a>,
}
#[derive(Deserialize)]
struct SpotBalance<'a> {
    #[serde(borrow, rename = "c", alias = "currency")]
    currency: Text<'a>,
    #[serde(rename = "b", alias = "balance")]
    balance: Number,
}
impl Summary<'_> {
    /// Spot rows are summed per currency and parsed apart from the summary, so a bad row never
    /// costs the aggregate; its currency keeps the stored coin rather than a partial sum, and
    /// one without a currency keeps every stored coin that no summed row replaces. `None` (unreadable list)
    /// keeps the previous coins.
    fn currencies(
        &self,
        cx: &mut Cx<'_, Pending>,
        logged: &mut RowReports<String>,
    ) -> Option<Vec<CurrencyBalance>> {
        let mut totals = BTreeMap::<String, f64>::new();
        let mut coins = SnapshotBuilder::default();
        let mut rejected = HashSet::new();
        if let Some(raw) = self.spot_balances.filter(|raw| raw.get() != "null") {
            json::each::<&RawValue>(raw.get(), |item| {
                match serde_json::from_str::<SpotBalance>(item.get()) {
                    Ok(row) if !row.currency.is_empty() => {
                        *totals.entry(row.currency.to_string()).or_default() += row.balance.0;
                    }
                    _ => {
                        let currency = serde_json::from_str::<SpotCurrency>(item.get())
                            .ok()
                            .map(|row| row.currency.to_string())
                            .filter(|currency| !currency.is_empty());
                        let message = match &currency {
                            Some(c) => format!("Skipped malformed GRVT spot balance row {c}"),
                            None => {
                                "Skipped malformed GRVT spot balance row without a currency".into()
                            }
                        };
                        let key = format!("coin:{}", currency.as_deref().unwrap_or_default());
                        log_skip(cx, logged, key, Level::Warn, message);
                        rejected.extend(currency.clone());
                        coins.reject(currency);
                    }
                }
            })
            .ok()?;
        }
        rearm_skips(
            logged,
            "coin:",
            coins.rejected() == 0,
            totals.keys().map(String::as_str),
        );
        for (currency, total) in totals {
            if !rejected.contains(&currency) {
                coins.push(CurrencyBalance {
                    currency,
                    balance: Balance {
                        total,
                        free: total,
                        used: 0.,
                        upnl: 0.,
                    },
                });
            }
        }
        let current = cx.store.currency_balances().unwrap_or_default().to_vec();
        Some(coins.commit(current, |c| c.currency.clone()))
    }
}
#[derive(Deserialize)]
struct Subaccount<'a> {
    #[serde(borrow, rename = "si", alias = "sub_account_id")]
    id: Text<'a>,
}
#[derive(Deserialize)]
struct PositionWire<'a> {
    #[serde(borrow, rename = "i", alias = "instrument")]
    instrument: Str<'a>,
    #[serde(rename = "s", alias = "size")]
    size: Number,
    #[serde(rename = "ep", alias = "entry_price", default)]
    entry: Option<Number>,
    #[serde(rename = "n", alias = "notional", default)]
    notional: Option<Number>,
    #[serde(rename = "up", alias = "unrealized_pnl", default)]
    upnl: Option<Number>,
    #[serde(rename = "rp", alias = "realized_pnl", default)]
    realized: Option<Number>,
    #[serde(rename = "l", alias = "leverage", default)]
    leverage: Option<Number>,
    #[serde(
        rename = "el",
        alias = "est_liquidation_price",
        alias = "liquidation_price",
        alias = "lp",
        default
    )]
    liquidation: Option<Number>,
    #[serde(
        borrow,
        rename = "et",
        alias = "event_time",
        alias = "ut",
        alias = "update_time",
        default
    )]
    update_time: Text<'a>,
    #[serde(borrow, rename = "mt", alias = "margin_type", default)]
    margin: Option<Str<'a>>,
}
#[derive(Deserialize)]
struct LeverageWire<'a> {
    #[serde(borrow, rename = "i", alias = "instrument")]
    instrument: Str<'a>,
    #[serde(rename = "l", alias = "leverage")]
    leverage: Number,
    #[serde(rename = "ml", alias = "min_leverage", default)]
    min: Option<Number>,
    #[serde(rename = "ml1", alias = "max_leverage", default)]
    max: Option<Number>,
    #[serde(borrow, rename = "mt", alias = "margin_type", default)]
    margin: Option<Str<'a>>,
}
pub(crate) struct Leverage {
    symbol: String,
    value: f64,
    min: Option<f64>,
    max: Option<f64>,
    margin: Option<MarginType>,
}
#[derive(Deserialize)]
struct BuilderWire<'a> {
    #[serde(borrow, rename = "ba", alias = "builder_account_id")]
    address: Str<'a>,
    #[serde(
        rename = "mfr",
        alias = "max_futures_fee_rate",
        alias = "max_fee_rate",
        default,
        deserialize_with = "lenient_cap"
    )]
    cap: Option<f64>,
}
/// An unreadable cap (null, "", text) reads as absent, which TS treats as authorized.
fn lenient_cap<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Cap {
        Number(Number),
        Ignored(serde::de::IgnoredAny),
    }
    Ok(match Cap::deserialize(d)? {
        Cap::Number(n) => Some(n.0),
        Cap::Ignored(_) => None,
    })
}
#[derive(Deserialize)]
struct RowInstrument<'a> {
    #[serde(borrow, rename = "i", alias = "instrument")]
    instrument: Str<'a>,
}
#[derive(Deserialize)]
struct RowIdentity<'a> {
    #[serde(borrow, rename = "oi", alias = "order_id", default)]
    id: Text<'a>,
    #[serde(borrow, rename = "m", alias = "metadata", default)]
    metadata: Option<RowClient<'a>>,
}
/// The client id, else the native id, of an order row that failed to map; empty when unreadable.
fn order_label(item: &str) -> String {
    serde_json::from_str::<RowIdentity<'_>>(item)
        .ok()
        .map(|row| {
            let client = row.metadata.map(|m| m.client_id).unwrap_or_default();
            if client.is_empty() {
                row.id.to_string()
            } else {
                client.to_string()
            }
        })
        .unwrap_or_default()
}
#[derive(Deserialize)]
struct RowClient<'a> {
    #[serde(borrow, rename = "co", alias = "client_order_id", default)]
    client_id: Text<'a>,
}
#[derive(Deserialize)]
struct OrderWire<'a> {
    #[serde(borrow, rename = "oi", alias = "order_id", default)]
    id: Text<'a>,
    #[serde(rename = "im", alias = "is_market", default)]
    market: bool,
    #[serde(rename = "ro", alias = "reduce_only", default)]
    reduce_only: bool,
    #[serde(rename = "po", alias = "post_only", default)]
    post_only: bool,
    #[serde(borrow, rename = "ti", alias = "time_in_force", default)]
    tif: Str<'a>,
    #[serde(borrow, rename = "l", alias = "legs")]
    legs: Vec<Leg<'a>>,
    #[serde(borrow, rename = "m", alias = "metadata", default)]
    metadata: Option<Metadata<'a>>,
    #[serde(borrow, rename = "s1", alias = "state")]
    state: OrderState<'a>,
}
#[derive(Deserialize)]
struct Leg<'a> {
    #[serde(borrow, rename = "i", alias = "instrument")]
    instrument: Str<'a>,
    #[serde(rename = "s", alias = "size")]
    size: Number,
    #[serde(rename = "lp", alias = "limit_price", default)]
    price: Number,
    #[serde(rename = "ib", alias = "is_buying_asset")]
    buying: bool,
}
#[derive(Default, Deserialize)]
struct Metadata<'a> {
    #[serde(borrow, rename = "co", alias = "client_order_id", default)]
    client_id: Text<'a>,
    #[serde(borrow, rename = "ct", alias = "create_time", default)]
    create_time: Text<'a>,
    #[serde(borrow, rename = "t", alias = "trigger", default)]
    trigger: Option<Trigger<'a>>,
}
#[derive(Deserialize)]
struct Trigger<'a> {
    #[serde(borrow, rename = "tt", alias = "trigger_type")]
    kind: Str<'a>,
    #[serde(borrow, rename = "t", alias = "tpsl")]
    tpsl: TriggerDetails<'a>,
}
#[derive(Deserialize)]
struct TriggerDetails<'a> {
    #[serde(rename = "tp", alias = "trigger_price")]
    price: Number,
    #[serde(borrow, rename = "tb", alias = "trigger_by")]
    by: Str<'a>,
    #[serde(rename = "cp", alias = "close_position", default)]
    close: Option<bool>,
}
#[derive(Deserialize)]
struct OrderState<'a> {
    #[serde(borrow, rename = "s", alias = "status")]
    status: Str<'a>,
    #[serde(rename = "ts", alias = "traded_size")]
    traded: Vec<Number>,
    #[serde(borrow, rename = "ut", alias = "update_time", default)]
    update_time: Text<'a>,
}
pub(crate) struct DecodedOrder {
    pub order: Order,
    pub native_id: Option<String>,
    pub updated_ns: u64,
    pub created_ns: u64,
}
pub(crate) struct PositionRecord {
    position: Position,
    time: u64,
}
fn market<'a>(store: &'a Store, instrument: &str) -> Option<&'a safe_cex_engine::model::Market> {
    store
        .market_by_id(instrument)
        .or_else(|| store.market(&instrument.to_ascii_uppercase()))
}
fn timestamp(text: &str, fallback: f64) -> f64 {
    text.parse::<u64>()
        .ok()
        .map_or(fallback, |n| (n / 1_000_000) as f64)
}
fn nanos(text: &str, fallback: f64) -> u64 {
    text.parse::<u64>()
        .unwrap_or_else(|_| (fallback.max(0.) as u64).saturating_mul(1_000_000))
}
fn margin_type(text: Option<&str>) -> Result<Option<MarginType>, String> {
    match text {
        Some("CROSS") => Ok(Some(MarginType::Cross)),
        Some("ISOLATED") => Ok(Some(MarginType::Isolated)),
        None => Ok(None),
        _ => Err("Unknown GRVT margin type".into()),
    }
}
pub(crate) fn decode_order(text: &str, store: &Store, now: f64) -> Result<DecodedOrder, String> {
    let wire: OrderWire<'_> = serde_json::from_str(text).map_err(|_| "Invalid GRVT order")?;
    if wire.legs.len() != 1 || wire.state.traded.len() != 1 {
        return Err("Unsupported GRVT multi-leg order".into());
    }
    let leg = &wire.legs[0];
    let market = market(store, &leg.instrument)
        .ok_or_else(|| format!("Unknown GRVT order instrument {}", leg.instrument))?;
    let metadata = wire.metadata.unwrap_or_default();
    let native_id = (!wire.id.is_empty() && !matches!(&*wire.id, "0x00" | "0x0" | "0"))
        .then(|| wire.id.to_string());
    let client_id = (!metadata.client_id.is_empty() && &*metadata.client_id != "0")
        .then(|| metadata.client_id.to_string());
    let id = client_id
        .clone()
        .or_else(|| native_id.clone())
        .ok_or("GRVT order has no identity")?;
    let status = match &*wire.state.status {
        "OPEN" | "PENDING" => OrderStatus::Open,
        "FILLED" => OrderStatus::Closed,
        "CANCELLED" | "REJECTED" => OrderStatus::Canceled,
        _ => return Err("Unknown GRVT order status".into()),
    };
    let filled = wire.state.traded[0].0;
    if leg.size.0 <= 0. || filled < 0. || filled > leg.size.0 || leg.price.0 < 0. {
        return Err("Invalid GRVT order amounts".into());
    }
    let tif = match (&*wire.tif, wire.post_only) {
        ("GOOD_TILL_TIME", true) => Some(TimeInForce::PostOnly),
        ("GOOD_TILL_TIME", false) => Some(TimeInForce::GoodTillCancel),
        ("IMMEDIATE_OR_CANCEL", false) => Some(TimeInForce::ImmediateOrCancel),
        ("FILL_OR_KILL", false) => Some(TimeInForce::FillOrKill),
        _ => None,
    };
    let mut kind = if wire.market {
        OrderType::Market
    } else {
        OrderType::Limit
    };
    let mut price = leg.price.0;
    let mut trigger_type = None;
    let mut close = None;
    if let Some(trigger) = metadata.trigger {
        kind = match &*trigger.kind {
            "STOP_LOSS" => OrderType::StopLoss,
            "TAKE_PROFIT" => OrderType::TakeProfit,
            _ => return Err("Unknown GRVT trigger".into()),
        };
        if trigger.tpsl.price.0 <= 0. {
            return Err("Invalid GRVT trigger price".into());
        }
        price = trigger.tpsl.price.0;
        trigger_type = match &*trigger.tpsl.by {
            "LAST" => Some(StopTriggerType::LastPrice),
            "MARK" => Some(StopTriggerType::MarkPrice),
            "INDEX" => Some(StopTriggerType::IndexPrice),
            _ => None,
        };
        close = trigger.tpsl.close;
    }
    Ok(DecodedOrder {
        native_id,
        created_ns: metadata.create_time.parse().unwrap_or(0),
        updated_ns: wire
            .state
            .update_time
            .parse::<u64>()
            .unwrap_or_else(|_| nanos(&metadata.create_time, now)),
        order: Order {
            id,
            parent_id: None,
            status,
            symbol: market.symbol.clone(),
            kind,
            side: if leg.buying {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            },
            price,
            amount: leg.size.0,
            filled,
            remaining: (leg.size.0 - filled).max(0.),
            reduce_only: wire.reduce_only,
            close,
            client_order_id: client_id,
            time_in_force: tif,
            last_update: timestamp(
                &wire.state.update_time,
                timestamp(&metadata.create_time, now),
            ),
            trigger_type,
            take_profit: None,
            stop_loss: None,
            extra: None,
        },
    })
}

fn position(text: &str, store: &Store, now: f64) -> Result<PositionRecord, String> {
    let wire: PositionWire<'_> = serde_json::from_str(text).map_err(|_| "Invalid GRVT position")?;
    let market = market(store, &wire.instrument)
        .ok_or_else(|| format!("Unknown GRVT position instrument {}", wire.instrument))?;
    if wire.size.0 != 0.
        && (wire.entry.is_none()
            || wire.notional.is_none()
            || wire.upnl.is_none()
            || wire.leverage.is_none()
            || wire.liquidation.is_none())
    {
        return Err("Incomplete GRVT position".into());
    }
    let entry = wire.entry.unwrap_or_default().0;
    let notional = wire.notional.unwrap_or_default().0;
    let upnl = wire.upnl.unwrap_or_default().0;
    let liquidation = wire.liquidation.unwrap_or_default().0;
    let leverage = if wire.leverage.is_some_and(|n| n.0 > 0.) {
        wire.leverage.unwrap_or_default().0
    } else {
        store
            .setting(&market.symbol)
            .map_or(10., |s| s.leverage)
            .max(1.)
    };
    if entry < 0. || liquidation < 0. {
        return Err("Invalid GRVT position amounts".into());
    }
    let margin = margin_type(wire.margin.as_deref())?
        .or_else(|| store.setting(&market.symbol).and_then(|s| s.margin_type));
    Ok(PositionRecord {
        time: nanos(&wire.update_time, now),
        position: Position {
            id: market.symbol.clone(),
            symbol: market.symbol.clone(),
            side: if wire.size.0 < 0. {
                PositionSide::Short
            } else {
                PositionSide::Long
            },
            entry_price: entry,
            notional: notional.abs(),
            leverage,
            margin_type: margin.or(Some(MarginType::Isolated)),
            position_mode: Some(PositionMode::Oneway),
            unrealized_pnl: upnl,
            realized_pnl: wire.realized.map(|n| n.0),
            contracts: wire.size.0.abs(),
            liquidation_price: liquidation,
            margin_size: None,
            last_update: Some(timestamp(&wire.update_time, now)),
        },
    })
}
#[derive(Default, Deserialize)]
struct PrivateMessage<'a> {
    #[serde(borrow, rename = "type", default)]
    kind: Str<'a>,
    #[serde(borrow, rename = "s", alias = "stream", default)]
    stream: Str<'a>,
    #[serde(borrow, rename = "s1", alias = "selector", default)]
    selector: Str<'a>,
    #[serde(borrow, rename = "sn", alias = "sequence_number", default)]
    sequence: Text<'a>,
    #[serde(borrow, rename = "f", alias = "feed", default)]
    feed: Option<&'a RawValue>,
    #[serde(borrow, rename = "e", alias = "error", default)]
    error: Option<crate::wire::Error<'a>>,
    #[serde(borrow, rename = "c", alias = "code", default)]
    code: Text<'a>,
    #[serde(borrow, rename = "r", alias = "result", default)]
    result: Option<crate::wire::SubscriptionResult<'a>>,
    #[serde(borrow, rename = "subs", default)]
    subs: Vec<Str<'a>>,
    #[serde(borrow, rename = "first_sequence_number", default)]
    first: Vec<Text<'a>>,
}
#[derive(Deserialize)]
struct FillWire<'a> {
    #[serde(borrow, rename = "i", alias = "instrument")]
    instrument: Str<'a>,
    #[serde(borrow, rename = "ti", alias = "trade_id")]
    trade_id: Text<'a>,
    #[serde(borrow, rename = "co", alias = "client_order_id", default)]
    client_id: Text<'a>,
    #[serde(borrow, rename = "oi", alias = "order_id", default)]
    native_id: Text<'a>,
    #[serde(rename = "ib", alias = "is_buyer")]
    buyer: bool,
    #[serde(rename = "s", alias = "size")]
    size: Number,
    #[serde(rename = "p", alias = "price")]
    price: Number,
}
impl Grvt {
    pub(crate) fn account_headers(&self) -> Vec<(&'static str, String)> {
        let mut headers = vec![("Content-Type", "application/json".into())];
        if let Some(session) = &self.account.session {
            headers.push(("X-Ts-Grvt-Session", session.cookie.clone()));
            headers.push(("Cookie", format!("rm=true; gravity={}", session.cookie)));
            headers.push(("X-Grvt-Account-Id", session.account_id.clone()));
        }
        headers
    }
    pub(crate) fn private_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.options.public_data_only {
            return;
        }
        self.account.trading_account_id = self.options.trading_account_id.clone();
        if !self.account.login_pending {
            self.login(cx);
        }
    }
    fn login(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.login_pending
            || self.account.authentication_failed
            || self.account.login_retry.is_some()
        {
            return;
        }
        let Some(key) = self.options.key.as_deref() else {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "GRVT private account requires an API key",
            );
            return;
        };
        #[derive(Serialize)]
        struct Body<'a> {
            api_key: &'a str,
        }
        let body = serde_json::to_string(&Body { api_key: key }).expect("serialize login");
        self.account.login_pending = true;
        self.send_login(cx, body, 0);
    }
    /// One session operation per physical login; reissues are new operations.
    fn send_login(&mut self, cx: &mut Cx<'_, Pending>, body: String, attempts: u8) {
        use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
        use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy, SessionPolicy};
        const HEADERS: &[(&str, &str)] = &[
            ("Content-Type", "application/json"),
            ("X-Grvt-Remember-Me", "true"),
            ("Cookie", "rm=true"),
        ];
        let session = match self.account.login_session.take() {
            Some(session) if cx.session_live(&session) => Ok(session),
            _ => {
                // The body carries the API key: keep it on the primary egress.
                let slots = Slots::new(self.options.key.as_ref(), None, SlotPolicy::default());
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
        let queued = session.and_then(|session| {
            let context = cx.session_context(
                &session,
                Policy {
                    operation: Operation::Mutation,
                    credential: None,
                    // The host default the unmanaged login relied on.
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            );
            self.account.login_session = Some(session);
            context
        });
        let pending = Pending {
            path: AUTH,
            body: body.clone(),
            job: Job::Account(Request::Login),
            attempts,
            auth_resent: false,
            auth_generation: self.account.generation,
            context: None,
        };
        let refused = match queued {
            Ok(context) => cx
                .request_managed_with_headers(
                    context.clone(),
                    None,
                    safe_cex_engine::Method::Post,
                    format!("{}{AUTH}", self.edge()),
                    HEADERS,
                    Some(body),
                    Pending {
                        context: Some(context),
                        ..pending
                    },
                )
                .err()
                .map(|(pending, _)| pending),
            Err(_) => Some(pending),
        };
        if refused.is_some() {
            self.retire_login_session(cx);
            self.account_login_unavailable(cx);
        }
    }
    fn retire_login_session(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(session) = self.account.login_session.take() {
            let _ = cx.retire_session(&session);
        }
    }
    /// A login that never produced a response: retire its session and reissue
    /// on a new generation, up to the legacy budget of five.
    pub(crate) fn login_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending) {
        self.retire_login_session(cx);
        if pending.attempts < 5 && !self.account.authentication_failed {
            self.send_login(cx, pending.body, pending.attempts + 1);
        } else {
            self.account_login_unavailable(cx);
        }
    }
    /// Record the answered login before its continuation runs.
    pub(crate) fn login_answered(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: safe_cex_engine::request_context::Context,
        response: &Response<'_>,
    ) {
        let (cookie, account_id) = session_headers(response);
        if crate::wire::check(response).is_ok() && cookie.is_some() && account_id.is_some() {
            let _ = cx.http_write_fact(
                &context,
                safe_cex_engine::classify::WriteFact::Accepted { order_ids: vec![] },
                None,
            );
            let _ = cx.finish_http_context(&context);
        } else {
            // Unproven: the next login must be a new generation.
            self.retire_login_session(cx);
        }
    }
    pub(crate) fn account_refresh(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending) {
        if self.account.authentication_failed {
            self.fail_pending(cx, pending, "GRVT authentication failed closed");
            return;
        }
        pending.auth_resent = true;
        if let Job::Account(request) = &mut pending.job {
            request.reset();
        }
        if pending.auth_generation != self.account.generation && self.account.session.is_some() {
            self.send_request(cx, pending);
            return;
        }
        self.account.waiting.push(pending);
        self.cancel_pong_ping(cx);
        self.account.session = None;
        self.account.authenticated = false;
        if let Some(socket) = self.account.socket.take() {
            cx.close(socket);
        }
        self.account.supervisor.cancel(cx);
        self.login(cx);
    }
    pub(crate) fn account_defer(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending) {
        if self.account.authentication_failed {
            self.fail_pending(cx, pending, "GRVT authentication failed closed");
            return;
        }
        self.account.waiting.push(pending);
        self.login(cx);
    }
    pub(crate) fn account_login_unavailable(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.login_pending = false;
        if self.account.login_retry.is_none() {
            self.account.login_retry = Some(cx.set_timer(5_000.));
        }
        cx.notice(
            Level::Warn,
            code::STREAM_UNAVAILABLE,
            "GRVT login temporarily unavailable after network retries; retrying shortly",
        );
        self.fail_waiting(cx);
    }
    fn connect_private(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.authentication_failed
            || self.account.socket.is_some()
            || self.account.supervisor.is_waiting()
            || self.account.trading_account_id.is_none()
        {
            return;
        }
        let Some(session) = self.account.session.as_ref() else {
            return;
        };
        let url = if self.options.testnet {
            "wss://trades.testnet.grvt.io/ws"
        } else {
            "wss://trades.grvt.io/ws"
        };
        let socket = if self.options.session_proxy {
            cx.connect(safe_cex_engine::SocketSpec::new(url).channel(SocketChannel::Private))
        } else {
            cx.connect(
                safe_cex_engine::SocketSpec::new(url)
                    .headers(vec![
                        ("Cookie", format!("rm=true; gravity={}", session.cookie)),
                        ("X-Grvt-Account-Id", session.account_id.clone()),
                    ])
                    .channel(SocketChannel::Private),
            )
        };
        self.account.socket = Some(socket);
        self.account.last_received = cx.now;
        self.account.authenticated = false;
        self.account.supervisor.connecting(cx);
    }
    pub(crate) fn account_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.cancel_pong_ping(cx);
        self.account.last_received = cx.now;
        self.account.supervisor.opened(cx);
        self.account.healthy_since = None;
        self.account.sequences.clear();
        self.account.first_sequence.clear();
        if self.options.session_proxy {
            #[derive(Serialize)]
            struct Auth<'a> {
                r#type: &'static str,
                session_cookie: String,
                account_id: &'a str,
                testnet: bool,
            }
            if let Some(session) = &self.account.session {
                cx.send(
                    socket,
                    serde_json::to_string(&Auth {
                        r#type: "auth",
                        session_cookie: format!("rm=true; gravity={}", session.cookie),
                        account_id: &session.account_id,
                        testnet: self.options.testnet,
                    })
                    .expect("serialize auth"),
                );
            }
        } else {
            self.subscribe_private(cx);
        }
        true
    }
    fn subscribe_private(&mut self, cx: &mut Cx<'_, Pending>) {
        let (Some(socket), Some(subaccount)) = (
            self.account.socket,
            self.account.trading_account_id.as_deref(),
        ) else {
            return;
        };
        #[derive(Serialize)]
        struct Subscribe<'a> {
            request_id: u32,
            method: &'static str,
            stream: &'static str,
            feed: [&'a str; 1],
            is_full: bool,
        }
        self.account.authenticated = true;
        self.account.supervisor.authenticated(cx);
        for stream in ["v1.order", "v1.position", "v1.fill"] {
            self.account.request_id += 1;
            cx.send(
                socket,
                serde_json::to_string(&Subscribe {
                    request_id: self.account.request_id,
                    method: "subscribe",
                    stream,
                    feed: [subaccount],
                    is_full: false,
                })
                .expect("serialize subscription"),
            );
        }
        self.refresh_orders(cx);
        self.refresh_positions(cx);
    }
    fn cancel_pong_ping(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some((timer, _, _)) = self.account.pong_ping.take() {
            cx.clear_timer(timer);
        }
    }
    /// `code` is `auth.rejected` only when GRVT refused the credentials.
    fn fail_authentication(&mut self, cx: &mut Cx<'_, Pending>, code: &'static str) {
        self.cancel_pong_ping(cx);
        self.account.authentication_failed = true;
        self.account.authenticated = false;
        self.account.session = None;
        if let Some(socket) = self.account.socket.take() {
            cx.close(socket);
        }
        self.account.supervisor.cancel(cx);
        if let Some(timer) = self.account.login_retry.take() {
            cx.clear_timer(timer);
        }
        self.retire_login_session(cx);
        cx.notice(Level::Error, code,"GRVT private WebSocket authentication failed closed. Check the API key and account authorization, then reconnect.");
    }
    fn refresh_stream_session(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.login_pending || self.account.authentication_failed {
            return;
        }
        if self
            .account
            .healthy_since
            .is_some_and(|at| cx.now - at >= 300_000.)
        {
            self.account.refresh_attempts = 0;
            self.account.healthy_since = None;
        }
        if self.account.refresh_attempts >= 1 {
            self.fail_authentication(cx, code::STREAM_UNAVAILABLE);
            return;
        }
        self.account.refresh_attempts += 1;
        self.cancel_pong_ping(cx);
        self.account.authenticated = false;
        self.account.session = None;
        if let Some(socket) = self.account.socket.take() {
            cx.close(socket);
        }
        self.account.supervisor.cancel(cx);
        self.login(cx);
    }
    pub(crate) fn account_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.cancel_pong_ping(cx);
        self.account.socket = None;
        self.account.authenticated = false;
        self.account.orders_revision += 1;
        self.account.positions_revision += 1;
        self.account.sequences.clear();
        self.account.first_sequence.clear();
        if !self.account.authentication_failed {
            self.account.supervisor.retry(cx);
        }
        true
    }
    fn summary(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.summary_pending || self.account.session.is_none() {
            return;
        }
        self.account.summary_pending = true;
        self.request(cx, SUMMARY, "{}".into(), Job::Account(Request::Summary));
    }
    pub(crate) fn refresh_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.orders_pending || self.account.session.is_none() {
            return;
        }
        let Some(subaccount) = self.account.trading_account_id.as_deref() else {
            return;
        };
        self.account.orders_pending = true;
        let body = serde_json::to_string(&SubaccountRequest { sa: subaccount })
            .expect("serialize account");
        self.request(
            cx,
            ORDERS,
            body,
            Job::Account(Request::Orders {
                revision: self.account.orders_revision,
                issued_at: cx.now,
                rows: vec![],
                orders: SnapshotBuilder::default(),
            }),
        );
    }
    pub(crate) fn refresh_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.positions_pending || self.account.session.is_none() {
            return;
        }
        let Some(subaccount) = self.account.trading_account_id.as_deref() else {
            return;
        };
        self.account.positions_pending = true;
        let body = serde_json::to_string(&SubaccountRequest { sa: subaccount })
            .expect("serialize account");
        self.request(
            cx,
            POSITIONS,
            body,
            Job::Account(Request::Positions {
                revision: self.account.positions_revision,
                issued_at: cx.now,
                rows: vec![],
                positions: SnapshotBuilder::default(),
            }),
        );
    }
    pub(crate) fn refresh_leverage(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.leverage_pending {
            return;
        }
        let Some(subaccount) = self.account.trading_account_id.as_deref() else {
            return;
        };
        self.account.leverage_pending = true;
        let body = serde_json::to_string(&SubaccountRequest { sa: subaccount })
            .expect("serialize account");
        self.request(
            cx,
            LEVERAGE,
            body,
            Job::Account(Request::Leverage {
                revision: self.account.settings_revision,
                command: None,
                rows: vec![],
                failed: false,
            }),
        );
    }
    pub(crate) fn fetch_leverage_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: u32,
        symbol: String,
    ) {
        if cx.store.market(&symbol).is_none() {
            cx.refuse_command(Operation::Read, "Unknown GRVT leverage market");
            return;
        }
        let Some(account) = self.account.trading_account_id.as_deref() else {
            cx.refuse_command(Operation::Read, "GRVT private account is not authenticated");
            return;
        };
        let body =
            serde_json::to_string(&SubaccountRequest { sa: account }).expect("serialize account");
        self.request(
            cx,
            LEVERAGE,
            body,
            Job::Account(Request::Leverage {
                revision: self.account.settings_revision,
                command: Some((command, symbol)),
                rows: vec![],
                failed: false,
            }),
        );
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::Positions {
                rows, positions, ..
            } => match position(item, cx.store, cx.now) {
                Ok(p) => rows.push(p),
                Err(_) => {
                    let listed = serde_json::from_str::<RowInstrument<'_>>(item)
                        .ok()
                        .map(|row| match market(cx.store, &row.instrument) {
                            Some(m) => (m.symbol.clone(), true),
                            None => (row.instrument.to_string(), false),
                        });
                    let symbol = listed.as_ref().map(|(symbol, _)| symbol.clone());
                    let message = match &symbol {
                        Some(symbol) => format!("Skipped malformed GRVT position row {symbol}"),
                        None => "Skipped malformed GRVT position row without an instrument".into(),
                    };
                    let key = format!("position:{}", symbol.as_deref().unwrap_or_default());
                    // An unlisted instrument hides no position the store could show.
                    let stored = match &listed {
                        Some((symbol, true)) => cx.store.position(symbol).is_some(),
                        Some((_, false)) => true,
                        None => !cx.store.positions().is_empty(),
                    };
                    let level = skip_level(stored);
                    log_skip(cx, &mut self.account.skip_logged, key, level, message);
                    positions.reject(symbol);
                }
            },
            Request::Orders { rows, orders, .. } => match decode_order(item, cx.store, cx.now) {
                Ok(o) => rows.push(o),
                Err(_) => {
                    let label = order_label(item);
                    match self.stored_order_id(cx.store, item) {
                        Ok(Some(id)) => orders.reject(Some(id)),
                        // A foreign row's own id matches no stored order.
                        Ok(None) => orders.reject(Some(label.clone())),
                        Err(()) => orders.reject(None),
                    }
                    let message = if label.is_empty() {
                        "Skipped malformed GRVT order row without an id".into()
                    } else {
                        format!("Skipped malformed GRVT order row {label}")
                    };
                    log_skip(
                        cx,
                        &mut self.account.skip_logged,
                        format!("order:{label}"),
                        Level::Warn,
                        message,
                    );
                }
            },
            Request::Leverage { rows, failed, .. } => {
                match serde_json::from_str::<LeverageWire<'_>>(item) {
                    Ok(w)
                        if w.leverage.0 > 0.
                            && w.min.is_none_or(|n| n.0 > 0.)
                            && w.max.is_none_or(|n| n.0 > 0.) =>
                    {
                        if let Some(m) = market(cx.store, &w.instrument) {
                            let Ok(margin) = margin_type(w.margin.as_deref()) else {
                                *failed = true;
                                return;
                            };
                            rows.push(Leverage {
                                symbol: m.symbol.clone(),
                                value: w.leverage.0,
                                min: w.min.map(|n| n.0),
                                max: w.max.map(|n| n.0),
                                margin,
                            });
                        }
                    }
                    _ => *failed = true,
                }
            }
            Request::Builders { found, failed } => {
                match serde_json::from_str::<BuilderWire<'_>>(item) {
                    Ok(w) => {
                        let expected = if self.options.testnet {
                            BUILDER_TESTNET
                        } else {
                            BUILDER_MAINNET
                        };
                        if found.is_none() && w.address.eq_ignore_ascii_case(expected) {
                            // As in TS, a matching row without a readable cap is authorized;
                            // GRVT itself rejects an order whose builder fee exceeds the cap.
                            *found = Some(match w.cap.map(|n| (n * 10_000.).round()) {
                                Some(raw) if raw < 300. => {
                                    BuilderState::CapTooLow(raw.max(0.) as u32)
                                }
                                _ => BuilderState::Authorized,
                            });
                        }
                    }
                    Err(_) => *failed = true,
                }
            }
            _ => {}
        }
    }
    fn stored_order_id(&self, store: &Store, item: &str) -> Result<Option<String>, ()> {
        let row = serde_json::from_str::<RowIdentity<'_>>(item).map_err(|_| ())?;
        let client = row.metadata.map(|m| m.client_id).unwrap_or_default();
        let native = &*row.id;
        if client.is_empty() && native.is_empty() {
            return Err(());
        }
        if !client.is_empty() && store.order(&client).is_some() {
            return Ok(Some(client.to_string()));
        }
        let matched = (!native.is_empty())
            .then(|| {
                store.orders().iter().find(|o| {
                    o.id == native
                        || self.account.native_ids.get(&o.id).map(String::as_str) == Some(native)
                })
            })
            .flatten();
        if let Some(order) = matched {
            return Ok(Some(order.id.clone()));
        }
        // Unmatched is a foreign order only if every client-keyed order's native id is known.
        let unmapped = store
            .orders()
            .iter()
            .any(|o| o.client_order_id.is_some() && !self.account.native_ids.contains_key(&o.id));
        if unmapped { Err(()) } else { Ok(None) }
    }
    fn remember_identity(&mut self, decoded: &DecodedOrder) {
        let previous = self.account.progress.get(&decoded.order.id);
        let same_lifecycle = previous.is_some_and(|old| {
            old.native_id == decoded.native_id
                && (old.created_ns == 0
                    || decoded.created_ns == 0
                    || old.created_ns == decoded.created_ns)
        });
        let prior_filled = previous
            .filter(|_| same_lifecycle)
            .map_or(0., |old| old.filled);
        let prior_closed = previous
            .filter(|_| same_lifecycle)
            .is_some_and(|old| old.status == OrderStatus::Closed);
        if decoded.order.filled > prior_filled
            || (decoded.order.status == OrderStatus::Closed && !prior_closed)
        {
            self.account.execution_revision += 1;
        }
        self.account.progress.insert(
            decoded.order.id.clone(),
            OrderProgress {
                native_id: decoded.native_id.clone(),
                created_ns: decoded.created_ns,
                updated_ns: decoded.updated_ns,
                filled: decoded.order.filled,
                amount: decoded.order.amount,
                status: decoded.order.status,
            },
        );
        self.account.pending_placements.remove(&decoded.order.id);
        self.account.prior_lifecycle.remove(&decoded.order.id);
        if !self.account.order_times.contains_key(&decoded.order.id) {
            self.account
                .identity_queue
                .push_back(decoded.order.id.clone());
        }
        self.account
            .order_times
            .insert(decoded.order.id.clone(), decoded.updated_ns);
        if decoded.created_ns > 0 {
            self.account
                .order_created
                .insert(decoded.order.id.clone(), decoded.created_ns);
        }
        if let Some(native) = &decoded.native_id {
            self.account
                .native_ids
                .insert(decoded.order.id.clone(), native.clone());
        }
        while self.account.identity_queue.len() > RECENT_LIMIT {
            if let Some(id) = self.account.identity_queue.pop_front() {
                self.account.native_ids.remove(&id);
                self.account.progress.remove(&id);
                self.account.order_times.remove(&id);
                self.account.order_created.remove(&id);
                self.account.placement_created.remove(&id);
                self.account.pending_placements.remove(&id);
                self.account.prior_lifecycle.remove(&id);
            }
        }
    }
    fn accept_order(&mut self, store: &Store, decoded: &DecodedOrder) -> bool {
        let id = &decoded.order.id;
        if self.account.pending_placements.contains(id)
            && self
                .account
                .prior_lifecycle
                .get(id)
                .is_some_and(|(created, native)| {
                    (native.is_some() && native == &decoded.native_id)
                        || (decoded.created_ns > 0 && decoded.created_ns <= *created)
                })
        {
            return false;
        }
        let created = self.account.order_created.get(id).copied().unwrap_or(0);
        if decoded.created_ns > 0 && decoded.created_ns < created {
            return false;
        }
        let mut new_lifecycle = created > 0 && decoded.created_ns > created;
        if let Some((time, native)) = self.account.terminal.get(id) {
            if new_lifecycle
                || (decoded.native_id.is_some()
                    && &decoded.native_id != native
                    && decoded.updated_ns > *time
                    && decoded.created_ns >= created)
            {
                new_lifecycle = true;
            } else {
                return false;
            }
        }
        if new_lifecycle {
            self.account.terminal.remove(id);
            self.account.terminal_queue.retain(|old| old != id);
            self.account.order_times.remove(id);
            self.account.native_ids.remove(id);
            self.account.identity_queue.retain(|old| old != id);
        } else {
            if self
                .account
                .order_times
                .get(id)
                .is_some_and(|old| *old > decoded.updated_ns)
            {
                return false;
            }
            if let (Some(known), Some(incoming)) =
                (self.account.native_ids.get(id), decoded.native_id.as_ref())
                && known != incoming
            {
                return false;
            }
            if store
                .order(id)
                .is_some_and(|old| old.filled > decoded.order.filled)
            {
                return false;
            }
        }
        true
    }
    fn preserve_local_order_fields(&self, store: &Store, decoded: &mut DecodedOrder) {
        if let Some(previous) = store.order(&decoded.order.id)
            && (self.account.pending_placements.contains(&decoded.order.id)
                || self
                    .account
                    .order_created
                    .get(&decoded.order.id)
                    .is_some_and(|created| *created == decoded.created_ns)
                || decoded.native_id.as_ref().is_some_and(|native| {
                    self.account.native_ids.get(&decoded.order.id) == Some(native)
                }))
            && decoded.order.parent_id.is_none()
        {
            decoded.order.parent_id = previous.parent_id.clone();
        }
    }
    fn remember_terminal(&mut self, id: String, time: u64, native: Option<String>) {
        if !self.account.terminal.contains_key(&id) {
            self.account.terminal_queue.push_back(id.clone());
        }
        self.account.terminal.insert(id, (time, native));
        while self.account.terminal_queue.len() > RECENT_LIMIT {
            if let Some(id) = self.account.terminal_queue.pop_front() {
                self.account.terminal.remove(&id);
            }
        }
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if let Err(error) = crate::wire::check(&response) {
            self.account_failed(cx, request, &error);
            return;
        }
        match request {
            Request::Login => {
                self.account.login_pending = false;
                let (cookie, account_id) = session_headers(&response);
                let (Some(cookie), Some(account_id)) = (cookie, account_id) else {
                    self.fail_authentication(cx, code::STREAM_UNAVAILABLE);
                    self.fail_waiting(cx);
                    return;
                };
                self.cancel_pong_ping(cx);
                self.account.session = Some(Session { cookie, account_id });
                self.account.generation += 1;
                if self.account.trading_account_id.is_none()
                    && let Ok(login) = serde_json::from_str::<LoginBody<'_>>(response.body)
                    && valid_account(&login.sub_account_id)
                {
                    self.account.trading_account_id = Some(login.sub_account_id.to_string());
                }
                for pending in std::mem::take(&mut self.account.waiting) {
                    self.send_request(cx, pending);
                }
                if self.account.started {
                    self.connect_private(cx);
                } else {
                    self.summary(cx);
                }
            }
            Request::Summary => {
                self.account.summary_pending = false;
                match serde_json::from_str::<SummaryEnvelope<'_>>(response.body) {
                    Ok(summary) => {
                        let summary = summary.result;
                        if self.account.trading_account_id.is_none() {
                            self.account.trading_account_id = summary
                                .subaccounts
                                .iter()
                                .find(|a| valid_account(&a.id))
                                .map(|a| a.id.to_string());
                        }
                        let free = summary.available.map_or(summary.equity.0, |n| n.0);
                        let upnl = summary.upnl.map_or_else(
                            || cx.store.positions().iter().map(|p| p.unrealized_pnl).sum(),
                            |n| n.0,
                        );
                        cx.store.set_balance(Balance {
                            total: summary.equity.0,
                            free,
                            used: summary.equity.0 - free,
                            upnl,
                        });
                        if let Some(currencies) =
                            summary.currencies(cx, &mut self.account.skip_logged)
                        {
                            cx.store.set_currency_balances(currencies);
                        }
                    }
                    Err(_) => {
                        cx.notice(
                            Level::Error,
                            code::SNAPSHOT_DISCARDED,
                            "Malformed GRVT account summary; retaining previous balance",
                        );
                        cx.store.set_balance(cx.store.balance().clone());
                    }
                }
                self.start_private_account(cx);
                if cx.store.loaded().positions {
                    self.sync_balance_upnl(cx);
                }
                if self.account.tick.is_none() {
                    self.account.tick = Some(cx.set_poll_timer(60_000.));
                }
            }
            Request::Positions {
                revision,
                issued_at,
                rows,
                mut positions,
            } => {
                self.account.positions_pending = false;
                if revision == self.account.positions_revision {
                    let mut seen: HashSet<_> = rows.iter().map(|p| p.position.id.clone()).collect();
                    rearm_skips(
                        &mut self.account.skip_logged,
                        "position:",
                        positions.rejected() == 0,
                        rows.iter()
                            .map(|p| p.position.symbol.as_str())
                            .filter(|symbol| positions.allows_absence(&symbol.to_string())),
                    );
                    for record in rows {
                        let time = record.time;
                        let position = record.position;
                        if self
                            .account
                            .position_times
                            .get(&position.id)
                            .is_some_and(|old| *old > time)
                        {
                            if let Some(previous) = cx.store.position(&position.id) {
                                positions.push(previous.clone());
                            }
                        } else {
                            self.account
                                .position_times
                                .insert(position.id.clone(), time);
                            if cx.store.position(&position.id).map_or_else(
                                || {
                                    cx.store.setting(&position.symbol).is_some_and(|s| {
                                        s.margin_type != position.margin_type
                                            || s.leverage != position.leverage
                                    })
                                },
                                |old| {
                                    old.margin_type != position.margin_type
                                        || old.leverage != position.leverage
                                },
                            ) {
                                self.account.settings_revision += 1;
                            }
                            if let Some(margin) = position.margin_type {
                                cx.store.update_setting(&position.symbol, |s| {
                                    s.margin_type = Some(margin)
                                });
                            }
                            if position.contracts > 0. {
                                positions.push(position);
                            }
                        }
                    }
                    let accepted =
                        positions.commit(cx.store.positions().iter().cloned(), |p| p.id.clone());
                    seen.extend(accepted.iter().map(|p| p.id.clone()));
                    for previous in cx.store.positions() {
                        if !seen.contains(&previous.id) {
                            let time = self
                                .account
                                .position_times
                                .entry(previous.id.clone())
                                .or_default();
                            *time = (*time).max(nanos("", issued_at));
                        }
                    }
                    cx.store.set_positions(accepted);
                    self.sync_balance_upnl(cx);
                } else {
                    cx.store.set_positions(cx.store.positions().to_vec());
                    if self.account.positions_timer.is_none() {
                        self.account.positions_timer = Some(cx.set_timer(500.));
                    }
                }
            }
            Request::Orders {
                revision,
                issued_at,
                rows,
                orders: mut snapshot,
            } => {
                self.account.orders_pending = false;
                if revision == self.account.orders_revision {
                    rearm_skips(
                        &mut self.account.skip_logged,
                        "order:",
                        snapshot.rejected() == 0,
                        rows.iter()
                            .map(|d| d.order.id.as_str())
                            .filter(|id| snapshot.allows_absence(&id.to_string())),
                    );
                    for mut decoded in rows {
                        self.preserve_local_order_fields(cx.store, &mut decoded);
                        if !self.accept_order(cx.store, &decoded) {
                            if let Some(previous) = cx.store.order(&decoded.order.id) {
                                snapshot.push(previous.clone());
                            }
                        } else if decoded.order.status == OrderStatus::Open {
                            self.remember_identity(&decoded);
                            snapshot.push(decoded.order);
                        }
                    }
                    let orders =
                        snapshot.commit(cx.store.orders().iter().cloned(), |o| o.id.clone());
                    for previous in cx.store.orders() {
                        if !orders.iter().any(|order| order.id == previous.id) {
                            self.remember_terminal(
                                previous.id.clone(),
                                nanos("", issued_at),
                                self.account.native_ids.get(&previous.id).cloned(),
                            );
                        }
                    }
                    cx.store.set_orders(orders);
                } else {
                    cx.store.set_orders(cx.store.orders().to_vec());
                    if self.account.orders_timer.is_none() {
                        self.account.orders_timer = Some(cx.set_timer(1_000.));
                    }
                }
            }
            Request::Leverage {
                revision,
                command,
                rows,
                failed,
            } => {
                if command.is_none() {
                    self.account.leverage_pending = false;
                }
                let answer = command
                    .as_ref()
                    .and_then(|(_, symbol)| rows.iter().find(|row| &row.symbol == symbol))
                    .map(|row| row.value);
                if !failed && revision == self.account.settings_revision {
                    for row in rows {
                        cx.store.update_setting(&row.symbol, |s| {
                            s.leverage = row.value;
                            if let Some(margin) = row.margin {
                                s.margin_type = Some(margin);
                            }
                        });
                        cx.store.update_positions(|p| {
                            if p.symbol == row.symbol {
                                p.leverage = row.value;
                                if row.margin.is_some() {
                                    p.margin_type = row.margin;
                                }
                            }
                        });
                        cx.store.update_markets(|m| {
                            if m.symbol == row.symbol {
                                if let Some(min) = row.min {
                                    m.limits.leverage.min = min;
                                }
                                if let Some(max) = row.max {
                                    m.limits.leverage.max = max;
                                }
                            }
                        });
                    }
                } else {
                    let message = "Malformed GRVT leverage settings; retaining previous settings";
                    if let Some((command, _)) = &command {
                        cx.command_error(*command, message);
                    } else {
                        cx.notice(Level::Error, code::SNAPSHOT_DISCARDED, message);
                    }
                }
                if let Some((command, _)) = command {
                    #[derive(Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Fetched {
                        buy_leverage: f64,
                        sell_leverage: f64,
                    }
                    let result: Result<String, String> = if failed {
                        Err("Malformed GRVT leverage response".into())
                    } else if revision != self.account.settings_revision {
                        Err("GRVT leverage settings changed during read; retry query".into())
                    } else {
                        answer
                            .ok_or_else(|| {
                                "GRVT response omitted requested instrument leverage".into()
                            })
                            .map(|value| {
                                serde_json::to_string(&Fetched {
                                    buy_leverage: value,
                                    sell_leverage: value,
                                })
                                .expect("serialize leverage")
                            })
                    };
                    match result {
                        Ok(data) => cx.done(command, data),
                        Err(error) => cx.fail(command, error),
                    };
                }
                if !failed && revision != self.account.settings_revision {
                    self.refresh_leverage(cx);
                }
            }
            Request::Builders { found, failed } => match (found, failed) {
                (None, true) => {
                    self.builder_failed(cx, "malformed GRVT builder authorization list");
                }
                (found, _) => {
                    self.account.builder = found.unwrap_or(BuilderState::Unauthorized);
                    self.account.builder_error = None;
                    self.builder_checked(cx);
                }
            },
        }
    }
    pub(crate) fn account_response_headers(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        generation: u64,
        response: &Response<'_>,
    ) {
        if generation != self.account.generation {
            return;
        }
        let (cookie, account_id) = session_headers(response);
        if let Some(session) = &mut self.account.session {
            let mut changed = false;
            if let Some(cookie) = cookie
                && cookie != session.cookie
            {
                session.cookie = cookie;
                changed = true;
            }
            if let Some(account_id) = account_id
                && account_id != session.account_id
            {
                session.account_id = account_id;
                changed = true;
            }
            if changed {
                self.cancel_pong_ping(cx);
                self.account.generation += 1;
                self.account.authenticated = false;
                if let Some(socket) = self.account.socket.take() {
                    cx.close(socket);
                }
                self.account.supervisor.cancel(cx);
                if self.account.started {
                    self.connect_private(cx);
                }
            }
        }
    }
    pub(crate) fn check_builder(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.builder == BuilderState::Checking {
            return;
        }
        self.account.builder = BuilderState::Checking;
        self.account.builder_attempt = 0;
        self.send_builder_check(cx);
    }
    // TS budget: three tries with 2s/4s backoff, so queued writes are not held for long.
    fn builder_failed(&mut self, cx: &mut Cx<'_, Pending>, error: &str) {
        if self.account.builder_attempt < 2 && !self.account.authentication_failed {
            self.account.builder_attempt += 1;
            let delay = 2_000. * f64::from(self.account.builder_attempt);
            self.account.builder_retry = Some(cx.set_timer(delay));
            return;
        }
        cx.notice(
            Level::Warn,
            code::REQUEST_FAILED,
            format!(
                "GRVT builder authorization check failed; rechecking before next order: {error}"
            ),
        );
        self.account.builder = BuilderState::Unknown;
        self.account.builder_error = Some(error.to_owned());
        self.builder_checked(cx);
    }
    fn send_builder_check(&mut self, cx: &mut Cx<'_, Pending>) {
        self.request(
            cx,
            BUILDERS,
            "{}".into(),
            Job::Account(Request::Builders {
                found: None,
                failed: false,
            }),
        );
    }
    fn fail_waiting(&mut self, cx: &mut Cx<'_, Pending>) {
        for pending in std::mem::take(&mut self.account.waiting) {
            self.fail_pending(cx, pending, "GRVT session refresh failed");
        }
    }
    fn start_private_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.started {
            return;
        }
        cx.store.set_hedged(false);
        let symbols: Vec<_> = cx
            .store
            .markets()
            .iter()
            .map(|m| m.symbol.clone())
            .collect();
        for symbol in symbols {
            cx.store.update_setting(&symbol, |s| {
                if !s.leverage.is_finite() {
                    s.leverage = 10.;
                }
                s.margin_type = Some(MarginType::Isolated);
            });
        }
        if self.account.trading_account_id.is_some() {
            self.account.started = true;
            self.refresh_leverage(cx);
            self.check_builder(cx);
            self.connect_private(cx);
            self.refresh_positions(cx);
            self.refresh_orders(cx);
        } else {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "GRVT trading account ID was not returned; configure a subaccount",
            );
            cx.store.set_orders(cx.store.orders().to_vec());
            cx.store.set_positions(cx.store.positions().to_vec());
        }
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        match request {
            Request::Login => {
                self.account.login_pending = false;
                let refused = error.contains("(HTTP 401") || error.contains("(HTTP 403");
                self.fail_authentication(
                    cx,
                    if refused {
                        code::AUTH_REJECTED
                    } else {
                        code::STREAM_UNAVAILABLE
                    },
                );
                self.fail_waiting(cx);
            }
            Request::Summary => {
                self.account.summary_pending = false;
                cx.store.set_balance(cx.store.balance().clone());
                self.start_private_account(cx);
                if self.account.tick.is_none() {
                    self.account.tick = Some(cx.set_poll_timer(60_000.));
                }
            }
            Request::Positions { .. } => {
                self.account.positions_pending = false;
                cx.store.set_positions(cx.store.positions().to_vec());
            }
            Request::Orders { .. } => {
                self.account.orders_pending = false;
                cx.store.set_orders(cx.store.orders().to_vec());
            }
            Request::Leverage { command, .. } => {
                if let Some((command, _)) = command {
                    cx.command_error(
                        command,
                        "GRVT private request failed; retaining previous account state",
                    );
                    cx.fail(command, "GRVT leverage request failed");
                    return;
                } else {
                    self.account.leverage_pending = false;
                }
            }
            Request::Builders { .. } => {
                self.builder_failed(cx, error);
                return;
            }
        }
        cx.retried_failure(
            "grvt.account:1815",
            "GRVT private request failed; retaining previous account state",
        );
    }
    pub(crate) fn account_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.account.last_received = cx.now;
        self.account.supervisor.received(cx.now);
        self.account.ping_sent = false;
        if text == "pong" {
            self.cancel_pong_ping(cx);
            if self.account.authenticated && !self.account.authentication_failed {
                self.account.pong_ping =
                    Some((cx.set_timer(5_000.), socket, self.account.generation));
            }
            return true;
        }
        if text == "ping" {
            cx.send(socket, "pong");
            return true;
        }
        let Ok(message) = serde_json::from_str::<PrivateMessage<'_>>(text) else {
            cx.notice(
                Level::Warn,
                code::STREAM_MALFORMED,
                "Malformed GRVT private message",
            );
            return true;
        };
        let code = message.error.as_ref().map_or(&*message.code, |e| &*e.code);
        if message.kind == "error" || code == "1000" {
            self.refresh_stream_session(cx);
            return true;
        }
        if message.kind == "auth_success" {
            if !self.account.authenticated {
                self.subscribe_private(cx);
            }
            return true;
        }
        if code == "1008" {
            cx.notice(Level::Error, code::AUTH_REJECTED,
                "GRVT IP is not whitelisted. Please re-add your API key with correct IP whitelisting (18.176.98.246).",
            );
            return true;
        }
        if !code.is_empty() && code != "0" {
            cx.notice(
                Level::Error,
                code::STREAM_UNAVAILABLE,
                format!("GRVT private stream rejected (code {code})"),
            );
            return true;
        }
        if !self.account.authenticated {
            return true;
        }
        if let Some(ack) = message.result {
            self.private_ack(cx, &ack.stream, &ack.subs, &ack.first);
            return true;
        }
        if !message.first.is_empty() {
            self.private_ack(cx, &message.stream, &message.subs, &message.first);
            return true;
        }
        let Some(feed) = message.feed else {
            return true;
        };
        if matches!(
            &*message.stream,
            "v1.order" | "v1.state" | "v1.cancel" | "v1.position" | "v1.fill"
        ) && !message.sequence.is_empty()
        {
            let Ok(sequence) = message.sequence.parse::<u64>() else {
                self.private_gap(cx, socket);
                return true;
            };
            if sequence != 0 {
                let selector = self.private_selector(&message.selector).to_owned();
                let key = (message.stream.to_string(), selector);
                let previous = self.account.sequences.get(&key).copied();
                if previous.is_some_and(|old| sequence <= old) {
                    return true;
                }
                let expected = previous
                    .and_then(|old| old.checked_add(1))
                    .or_else(|| self.account.first_sequence.get(&key).copied())
                    .or_else(|| {
                        self.account
                            .first_sequence
                            .get(&(message.stream.to_string(), String::new()))
                            .copied()
                    });
                if expected.is_some_and(|expected| sequence != expected)
                    || self.account.sequences.len() > cx.store.markets().len() * 6 + 6
                {
                    self.private_gap(cx, socket);
                    return true;
                }
                self.account.sequences.insert(key, sequence);
            }
        }
        match &*message.stream {
            "v1.order" | "v1.state" | "v1.cancel" => {
                match decode_order(feed.get(), cx.store, cx.now) {
                    Ok(mut decoded) => {
                        self.account.healthy_since.get_or_insert(cx.now);
                        self.account.orders_revision += 1;
                        self.preserve_local_order_fields(cx.store, &mut decoded);
                        if !self.accept_order(cx.store, &decoded) {
                            return true;
                        }
                        self.remember_identity(&decoded);
                        let updated_ns = decoded.updated_ns;
                        let native = decoded.native_id;
                        let order = decoded.order;
                        if matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
                            self.remember_terminal(order.id.clone(), updated_ns, native);
                            cx.store.remove_orders(&[&order.id]);
                        } else if !self.account.terminal.contains_key(&order.id)
                            && cx.store.order(&order.id).is_none_or(|old| {
                                old.last_update <= order.last_update && old.filled <= order.filled
                            })
                        {
                            cx.store.upsert_order(order);
                        }
                    }
                    Err(_) => cx.notice(
                        Level::Warn,
                        code::STREAM_MALFORMED,
                        "Malformed GRVT order event; reconciling",
                    ),
                }
                if let Some(timer) = self.account.orders_timer.take() {
                    cx.clear_timer(timer);
                }
                self.account.orders_timer = Some(cx.set_timer(1_000.));
            }
            "v1.position" => {
                match position(feed.get(), cx.store, cx.now) {
                    Ok(record) => {
                        self.account.healthy_since.get_or_insert(cx.now);
                        self.account.positions_revision += 1;
                        let time = record.time;
                        let position = record.position;
                        if self
                            .account
                            .position_times
                            .get(&position.id)
                            .is_none_or(|old| *old <= time)
                        {
                            self.account
                                .position_times
                                .insert(position.id.clone(), time);
                            if cx.store.position(&position.id).map_or_else(
                                || {
                                    cx.store.setting(&position.symbol).is_some_and(|s| {
                                        s.margin_type != position.margin_type
                                            || s.leverage != position.leverage
                                    })
                                },
                                |old| {
                                    old.margin_type != position.margin_type
                                        || old.leverage != position.leverage
                                },
                            ) {
                                self.account.settings_revision += 1;
                            }
                            if let Some(margin) = position.margin_type {
                                cx.store.update_setting(&position.symbol, |s| {
                                    s.margin_type = Some(margin)
                                });
                            }
                            cx.store.upsert_position(position);
                        }
                        self.sync_balance_upnl(cx);
                    }
                    Err(_) => cx.notice(
                        Level::Warn,
                        code::STREAM_MALFORMED,
                        "Malformed GRVT position event; reconciling",
                    ),
                }
                if self.account.positions_timer.is_none() {
                    self.account.positions_timer = Some(cx.set_timer(500.));
                }
            }
            "v1.fill" => {
                if let Ok(fill) = serde_json::from_str::<FillWire<'_>>(feed.get())
                    && fill.size.0 > 0.
                    && fill.price.0 > 0.
                    && !fill.trade_id.is_empty()
                    && let Some(market) = market(cx.store, &fill.instrument)
                {
                    self.account.healthy_since.get_or_insert(cx.now);
                    let key = format!("{}:{}", market.id, fill.trade_id);
                    if self.account.fills.insert(key.clone()) {
                        self.account.execution_revision += 1;
                        self.account.fill_queue.push_back(key);
                        while self.account.fill_queue.len() > RECENT_LIMIT {
                            if let Some(old) = self.account.fill_queue.pop_front() {
                                self.account.fills.remove(&old);
                            }
                        }
                        let id = if !fill.client_id.is_empty() && &*fill.client_id != "0" {
                            fill.client_id.to_string()
                        } else {
                            self.account
                                .native_ids
                                .iter()
                                .find(|(_, native)| native.as_str() == &*fill.native_id)
                                .map_or_else(|| fill.native_id.to_string(), |(id, _)| id.clone())
                        };
                        cx.fill(Fill {
                            id: None,
                            order_id: id,
                            symbol: market.symbol.clone(),
                            side: if fill.buyer {
                                OrderSide::Buy
                            } else {
                                OrderSide::Sell
                            },
                            price: fill.price.0,
                            amount: fill.size.0,
                        });
                    }
                }
            }
            _ => {}
        }
        true
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.builder_retry == Some(timer) {
            self.account.builder_retry = None;
            self.send_builder_check(cx);
            return true;
        }
        if self
            .account
            .pong_ping
            .is_some_and(|(pending, _, _)| pending == timer)
        {
            let (_, socket, generation) =
                self.account.pong_ping.take().expect("matched pong timer");
            if self.account.socket == Some(socket)
                && self.account.generation == generation
                && self.account.authenticated
                && !self.account.authentication_failed
            {
                cx.send(socket, "ping");
            }
        } else if self.account.login_retry == Some(timer) {
            self.account.login_retry = None;
            self.login(cx);
        } else if self.account.supervisor.owns_timer(timer) {
            match self.account.supervisor.on_timer(cx, timer) {
                SocketTick::Reconnect => self.connect_private(cx),
                SocketTick::TimedOut(_) => {
                    if let Some(socket) = self.account.socket {
                        cx.close(socket);
                        self.account_close(cx, socket);
                    }
                }
                SocketTick::Ping
                    if self.account.authenticated
                        && cx.now - self.account.last_received >= 60_000.
                        && !self.account.ping_sent =>
                {
                    if let Some(socket) = self.account.socket {
                        cx.send(socket, "ping");
                        self.account.ping_sent = true;
                    }
                }
                _ => {}
            }
        } else if self.account.tick == Some(timer) {
            self.account.tick = None;
            self.summary(cx);
        } else if self.account.orders_timer == Some(timer) {
            self.account.orders_timer = None;
            self.refresh_orders(cx);
        } else if self.account.positions_timer == Some(timer) {
            self.account.positions_timer = None;
            self.refresh_positions(cx);
        } else {
            return false;
        }
        true
    }
    fn private_selector<'a>(&self, selector: &'a str) -> &'a str {
        let Some(account) = self.account.trading_account_id.as_deref() else {
            return selector;
        };
        if selector == account {
            ""
        } else {
            selector
                .strip_prefix(&format!("{account}-"))
                .unwrap_or(selector)
        }
    }
    fn private_ack(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        stream: &str,
        subs: &[Str<'_>],
        first: &[Text<'_>],
    ) {
        for (selector, number) in subs.iter().zip(first) {
            if self.account.first_sequence.len() > cx.store.markets().len() * 6 + 6 {
                break;
            }
            if let Ok(number) = number.parse::<u64>() {
                let key = (
                    stream.to_owned(),
                    self.private_selector(selector).to_owned(),
                );
                self.account.first_sequence.entry(key).or_insert(number);
            }
        }
    }
    fn private_gap(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        cx.notice(
            Level::Warn,
            code::WS_DISCONNECTED,
            "GRVT private sequence gap; reconnecting and reconciling account state",
        );
        cx.close(socket);
        self.account_close(cx, socket);
        self.refresh_orders(cx);
        self.refresh_positions(cx);
    }
    pub(crate) fn sync_balance_upnl(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.account.started {
            return;
        }
        let updates: Vec<_> = cx
            .store
            .positions()
            .iter()
            .filter_map(|position| {
                let last = cx.store.ticker(&position.symbol)?.last;
                if last <= 0. || position.entry_price <= 0. {
                    return None;
                }
                let direction = if position.side == PositionSide::Long {
                    1.
                } else {
                    -1.
                };
                let upnl = direction * (last - position.entry_price) * position.contracts;
                upnl.is_finite().then(|| (position.id.clone(), upnl))
            })
            .collect();
        if !updates.is_empty() {
            cx.store.update_positions(|p| {
                if let Some((_, upnl)) = updates.iter().find(|(id, _)| id == &p.id) {
                    p.unrealized_pnl = *upnl;
                }
            });
        }
        let mut balance = cx.store.balance().clone();
        balance.upnl = cx.store.positions().iter().map(|p| p.unrealized_pnl).sum();
        cx.store.set_balance(balance);
    }
}
#[derive(Serialize)]
struct SubaccountRequest<'a> {
    sa: &'a str,
}
fn valid_account(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) && text.parse::<u64>().is_ok()
}
fn session_headers(response: &Response<'_>) -> (Option<String>, Option<String>) {
    let cookie = response
        .header("x-ts-grvt-session")
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            response
                .headers
                .lines()
                .filter_map(|line| line.split_once(':'))
                .filter(|(name, _)| name.trim().eq_ignore_ascii_case("set-cookie"))
                .find_map(|(_, value)| {
                    value
                        .trim()
                        .strip_prefix("gravity=")
                        .and_then(|v| v.split(';').next())
                        .filter(|v| !v.is_empty())
                        .map(str::to_owned)
                })
        });
    let account = response
        .header("x-grvt-account-id")
        .filter(|v| !v.is_empty())
        .map(str::to_owned);
    (cookie, account)
}

#[cfg(test)]
#[path = "login_preparation_tests.rs"]
mod login_preparation_tests;
