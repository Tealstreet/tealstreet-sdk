//! Account snapshots are committed only after every scoped read succeeds.
use crate::{Lighter, Pending, sign};
use safe_cex_engine::ControlFrame;
use safe_cex_engine::Level;
use safe_cex_engine::coverage::SnapshotCoverage;
use safe_cex_engine::de::number;
use safe_cex_engine::fresh_positions::{self, FreshPositions};
use safe_cex_engine::history::BackgroundCoverage;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::{Cx, ErrorKind, Fill, Method, Response, SocketId, TimerId, VenueError, json};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
};

#[derive(Clone)]
pub struct Lookup {
    by: &'static str,
    value: String,
    address: bool,
}
impl Lookup {
    pub(crate) fn path(&self) -> String {
        if self.address {
            format!("accountsByL1Address?l1_address={}", self.value)
        } else {
            format!("account?by={}&value={}", self.by, self.value)
        }
    }
    pub(crate) fn is_address(&self) -> bool {
        self.address
    }
}
pub enum Request {
    Limits {
        started_at: f64,
    },

    Lookup {
        candidates: VecDeque<Lookup>,
        index: Option<u64>,
    },
    Snapshot {
        serial: u64,
        revision: u64,
        account: Option<Snapshot>,
        valid: bool,
    },
    Orders {
        revision: u64,
        market: Option<String>,
        orders: Vec<Order>,
        coverage: SnapshotCoverage<String>,
        census: CensusRead,
        valid: bool,
    },
    Backfill {
        run: Backfill,
        executions: Vec<Execution>,
        oldest: f64,
        count: usize,
    },
}
/// What an open-order census (one read, or one per market) has proved so far.
#[derive(Default)]
pub struct CensusRead {
    /// The earliest release among its reads.
    read: Option<safe_cex_engine::order_read_cover::OrderReadMark>,
    /// A read answered outside a read response: the census proves no cover.
    lost: bool,
    started: f64,
    /// Rows naming a client order index (client, native, symbol), applied only
    /// once their symbol's rows are admitted.
    observed: Vec<(String, String, String)>,
    originals: Vec<(Order, String)>,
}
#[derive(Clone)]
pub struct Backfill {
    since: f64,
    epoch: u64,
    page: u32,
    cursor: Option<String>,
    retried: bool,
}
pub struct Execution {
    id: String,
    timestamp: f64,
    fill: Fill,
}
#[derive(Default)]
pub(crate) struct Connection {
    pub(crate) stream: crate::Connection,
    connected_at: f64,
    last_payload: Option<f64>,
    last_recovery: Option<f64>,
    recovery_started: Option<f64>,
    /// Early failures that read as an IP restriction; three switch to `?readonly=true`.
    read_only_failures: u32,
}
struct OrderBatch {
    revision: u64,
    markets: VecDeque<String>,
    orders: Vec<Order>,
}
const STREAMED_POLL_MS: f64 = 5_000.0;
/// The open-order census's read-cover scope: one census reads the whole account.
const ORDERS_SCOPE: &str = "orders";
/// A census the stream outran is read again no sooner than this after it began.
const CENSUS_FOLLOW_UP_MS: f64 = 2_000.0;
const RATE_LIMITED_POLL_HOLD_MS: f64 = 30_000.0;
/// How soon `readPositions` tries again after a read a newer stream frame held.
const FRESH_POSITIONS_RETRY_MS: f64 = 500.0;
const FRESH_PAUSED: &str = "Lighter account reads are paused after a rate limit";
#[derive(Default)]
pub struct Account {
    pub account_index: Option<u64>,
    pub api_key_index: Option<u8>,
    pub identity: Option<safe_cex_engine::signing::Identity>,
    setting_epochs: HashMap<String, u64>,
    pub records: HashMap<String, Order>,
    lookup_failed: bool,
    lookup_retry: Option<TimerId>,
    /// The lookup ended without an account index; parked reads cannot be answered.
    pub(crate) lookup_abandoned: bool,
    /// Private history reads that arrived while the account index lookup was pending.
    pub(crate) awaiting_identity: Vec<crate::history::Request>,
    /// Refuses the parked reads outside the lookup's own delivery, which cannot settle them.
    pub(crate) identity_refusal: Option<TimerId>,
    /// Client order index to native order index, as rows report them.
    pub(crate) client_orders: HashMap<String, String>,
    /// Placed orders the stream has not yet named: their client index can cancel them.
    pub(crate) unnamed_placements: HashMap<String, Order>,
    pub(crate) connection: Connection,
    started: bool,
    revision: u64,
    position_revision: HashMap<String, u64>,
    position_source: HashMap<String, (f64, bool)>,
    order_source: HashMap<String, f64>,
    order_revision: HashMap<String, u64>,
    settings_revision: HashMap<String, u64>,
    suppressed: HashSet<String>,
    pub snapshot_inflight: bool,
    snapshot_serial: u64,
    pub(crate) fresh_positions: FreshPositions,
    fresh_positions_timer: Option<TimerId>,
    /// The typed cause of the read failure being delivered, for its waiters.
    pub(crate) read_error: Option<VenueError>,
    orders_inflight: bool,
    /// An uncertainty reconcile arrived while a read was in flight; that read may
    /// predate the write, so another follows once it settles.
    orders_again: bool,
    batch: Option<OrderBatch>,
    census_follow_up: Option<TimerId>,
    sync: Option<TimerId>,
    polls_held_until: f64,
    polled_at: f64,
    heal: Option<TimerId>,
    seen_fills: HashSet<String>,
    fill_queue: VecDeque<String>,
    newest_fill: f64,
    live_after: Option<f64>,
    /// Earliest account-payload silence not yet covered by a completed backfill.
    outage: Option<f64>,
    outage_epoch: u64,
    session_start: Option<f64>,
    baselined: bool,
    backfill_retry: Option<(TimerId, Backfill)>,
}
#[derive(Default)]
pub struct Snapshot {
    positions: Vec<Position>,
    settings: Vec<(String, f64, MarginType)>,
    known_settings: HashSet<String>,
    unconfirmed_positions: HashSet<String>,
    equity: f64,
    free: f64,
    coverage: SnapshotCoverage<String>,
    /// Each skipped owned row's identity and native PnL, as far as they parsed.
    skipped: Vec<(Option<String>, Option<f64>)>,
}

#[derive(Default, Deserialize)]
#[serde(transparent)]
struct Num(#[serde(deserialize_with = "number")] f64);
#[derive(Default, Deserialize)]
#[serde(transparent)]
struct Atom<'a>(#[serde(borrow, deserialize_with = "atom")] Cow<'a, str>);
fn atom<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Cow<'de, str>, D::Error> {
    struct V;
    impl<'de> serde::de::Visitor<'de> for V {
        type Value = Cow<'de, str>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a string or integer")
        }
        fn visit_borrowed_str<E: serde::de::Error>(self, s: &'de str) -> Result<Self::Value, E> {
            Ok(Cow::Borrowed(s))
        }
        fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
            Ok(Cow::Owned(s.into()))
        }
        fn visit_u64<E: serde::de::Error>(self, n: u64) -> Result<Self::Value, E> {
            Ok(Cow::Owned(n.to_string()))
        }
        fn visit_i64<E: serde::de::Error>(self, n: i64) -> Result<Self::Value, E> {
            Ok(Cow::Owned(n.to_string()))
        }
    }
    d.deserialize_any(V)
}
#[derive(Deserialize)]
struct Status<'a> {
    #[serde(default, borrow)]
    code: Option<Atom<'a>>,
    #[serde(default)]
    success: Option<bool>,
    #[serde(default, borrow)]
    message: Cow<'a, str>,
}
impl Status<'_> {
    fn valid(&self) -> bool {
        self.code.as_ref().is_none_or(|c| c.0 == "200") && self.success != Some(false)
    }
}
#[derive(Deserialize)]
struct AccountWire<'a> {
    // Real rows carry both names; an alias would refuse them as a duplicate field.
    #[serde(default, borrow)]
    index: Option<Atom<'a>>,
    #[serde(default, borrow)]
    account_index: Option<Atom<'a>>,
    #[serde(default, borrow, rename = "accountIndex")]
    account_index_camel: Option<Atom<'a>>,
    #[serde(default, borrow)]
    positions: Option<&'a RawValue>,
    #[serde(default)]
    total_asset_value: Option<Num>,
    #[serde(default)]
    collateral: Option<Num>,
    #[serde(default)]
    cross_asset_value: Option<Num>,
    #[serde(default, alias = "availableBalance")]
    available_balance: Option<Num>,
}
impl<'a> AccountWire<'a> {
    fn identity(&self) -> Option<&Atom<'a>> {
        self.index
            .as_ref()
            .or(self.account_index.as_ref())
            .or(self.account_index_camel.as_ref())
    }
}
#[derive(Default, Deserialize)]
struct PositionWire<'a> {
    #[serde(default, borrow, alias = "marketId")]
    market_id: Atom<'a>,
    #[serde(default, borrow)]
    symbol: Cow<'a, str>,
    #[serde(default, alias = "position", alias = "qty")]
    size: Option<Num>,
    #[serde(default)]
    sign: Num,
    #[serde(default, borrow)]
    side: Cow<'a, str>,
    #[serde(
        default,
        alias = "avgEntryPrice",
        alias = "entryPrice",
        alias = "entry_price"
    )]
    avg_entry_price: Num,
    #[serde(default, alias = "positionValue")]
    position_value: Option<Num>,
    #[serde(default, alias = "unrealizedPnl")]
    unrealized_pnl: Option<Num>,
    #[serde(default, alias = "realizedPnl")]
    realized_pnl: Option<Num>,
    #[serde(default, alias = "initialLeverage", alias = "initial_leverage")]
    leverage: Option<Num>,
    #[serde(default, alias = "initialMarginFraction")]
    initial_margin_fraction: Option<MarginFraction>,
    #[serde(default, alias = "marginMode")]
    margin_mode: Option<Atom<'a>>,
    #[serde(default, alias = "liquidationPrice")]
    liquidation_price: Num,
    #[serde(
        default,
        alias = "allocatedMargin",
        alias = "marginSize",
        alias = "margin_size"
    )]
    allocated_margin: Option<Num>,
    #[serde(default, alias = "maintenanceMarginFraction")]
    maintenance_margin_fraction: Option<Num>,
    #[serde(default, alias = "updatedAt")]
    updated_at: Option<Num>,
    #[serde(default)]
    timestamp: Option<Num>,
    #[serde(default)]
    transaction_time: Option<Num>,
}
#[derive(Default, Deserialize)]
pub(crate) struct OrderWire<'a> {
    #[serde(default, borrow, alias = "orderId", alias = "OrderId")]
    order_id: Option<Atom<'a>>,
    #[serde(default, borrow, alias = "orderIndex", alias = "OrderIndex")]
    order_index: Option<Atom<'a>>,
    #[serde(
        default,
        borrow,
        alias = "marketId",
        alias = "marketIndex",
        alias = "MarketIndex",
        alias = "market_index"
    )]
    market_id: Atom<'a>,
    #[serde(default, borrow, alias = "marketSymbol", alias = "market_symbol")]
    symbol: Cow<'a, str>,
    #[serde(
        default,
        borrow,
        rename = "type",
        alias = "order_type",
        alias = "orderType",
        alias = "Type",
        alias = "orderTypeValue"
    )]
    kind: Atom<'a>,
    #[serde(
        default,
        borrow,
        alias = "timeInForce",
        alias = "TimeInForce",
        alias = "timeInForceValue"
    )]
    time_in_force: Atom<'a>,
    #[serde(default, borrow)]
    status: Cow<'a, str>,
    #[serde(default, borrow)]
    side: Cow<'a, str>,
    #[serde(default, alias = "isAsk")]
    is_ask: Option<Bool>,
    #[serde(default, alias = "isBuy")]
    is_buy: Option<Bool>,
    #[serde(default, alias = "reduceOnly")]
    reduce_only: Option<Bool>,
    #[serde(default, alias = "postOnly")]
    post_only: Option<Bool>,
    #[serde(
        default,
        alias = "initialBaseAmount",
        alias = "quantity",
        alias = "size",
        alias = "base_amount",
        alias = "baseAmount",
        alias = "qty",
        alias = "amount"
    )]
    initial_base_amount: Option<Num>,
    #[serde(
        default,
        alias = "filledBaseAmount",
        alias = "filledQuantity",
        alias = "filledAmount",
        alias = "filledSize",
        alias = "filled_quantity"
    )]
    filled_base_amount: Option<Num>,
    #[serde(
        default,
        alias = "remainingBaseAmount",
        alias = "remainingQuantity",
        alias = "remainingAmount",
        alias = "remainingSize",
        alias = "remaining_quantity"
    )]
    remaining_base_amount: Option<Num>,
    #[serde(default, alias = "limitPrice", alias = "limit_price", alias = "Price")]
    price: Option<Num>,
    #[serde(
        default,
        alias = "triggerPrice",
        alias = "stopPrice",
        alias = "stop_price"
    )]
    trigger_price: Option<Num>,
    #[serde(default, alias = "remainingQuoteAmount")]
    remaining_quote_amount: Option<Num>,
    #[serde(default, alias = "quoteAmount")]
    quote_amount: Option<Num>,
    #[serde(default, alias = "updatedAt")]
    updated_at: Option<Num>,
    #[serde(default)]
    timestamp: Option<Num>,
    #[serde(default)]
    transaction_time: Option<Num>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Bool {
    Flag(bool),
    Number(i64),
    Text(String),
}
impl Bool {
    fn get(&self) -> bool {
        match self {
            Self::Flag(b) => *b,
            Self::Number(n) => *n != 0,
            Self::Text(s) => s == "true" || s == "1",
        }
    }
}
#[derive(Default, Deserialize)]
struct TradeWire<'a> {
    #[serde(default, borrow, alias = "tradeId")]
    trade_id: Atom<'a>,
    #[serde(default, borrow, alias = "fillId")]
    fill_id: Atom<'a>,
    #[serde(default, borrow)]
    trade_id_str: Atom<'a>,
    #[serde(default, borrow, alias = "orderId")]
    order_id: Atom<'a>,
    #[serde(default, borrow)]
    ask_id: Atom<'a>,
    #[serde(default, borrow)]
    bid_id: Atom<'a>,
    #[serde(default, borrow)]
    ask_id_str: Atom<'a>,
    #[serde(default, borrow)]
    bid_id_str: Atom<'a>,
    #[serde(default, borrow)]
    ask_account_id: Atom<'a>,
    #[serde(default, borrow)]
    bid_account_id: Atom<'a>,
    #[serde(default, borrow, alias = "marketId")]
    market_id: Atom<'a>,
    #[serde(default, borrow)]
    symbol: Cow<'a, str>,
    #[serde(default, borrow)]
    side: Cow<'a, str>,
    #[serde(default, alias = "isAsk")]
    is_ask: Option<Bool>,
    #[serde(default)]
    price: Num,
    #[serde(default, alias = "amount")]
    size: Num,
    #[serde(default)]
    timestamp: Num,
}
#[derive(Default, Deserialize)]
struct Payload<'a> {
    #[serde(default, borrow)]
    orders: Option<&'a RawValue>,
    #[serde(default, borrow)]
    positions: Option<&'a RawValue>,
    #[serde(default, borrow)]
    trades: Option<&'a RawValue>,
    #[serde(default, borrow)]
    fills: Option<&'a RawValue>,
    #[serde(default, borrow)]
    account: Option<&'a RawValue>,
    #[serde(default, borrow)]
    balances: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct Frame<'a> {
    #[serde(default, borrow)]
    channel: Cow<'a, str>,
    #[serde(default, borrow, rename = "type")]
    kind: Cow<'a, str>,
    #[serde(default, borrow)]
    data: Option<&'a RawValue>,
    #[serde(default)]
    timestamp: Option<Num>,
    #[serde(default)]
    transaction_time: Option<Num>,
    #[serde(default, borrow)]
    orders: Option<&'a RawValue>,
    #[serde(default, borrow)]
    positions: Option<&'a RawValue>,
    #[serde(default, borrow)]
    trades: Option<&'a RawValue>,
    #[serde(default, borrow)]
    fills: Option<&'a RawValue>,
    #[serde(default, borrow)]
    account: Option<&'a RawValue>,
    #[serde(default, borrow)]
    balances: Option<&'a RawValue>,
}
fn each_row<'a, T: Deserialize<'a>>(
    raw: &'a RawValue,
    mut each: impl FnMut(T, Option<&str>),
) -> Result<(), String> {
    if raw.get().starts_with('[') {
        json::each(raw.get(), |r: T| each(r, None)).map_err(|e| e.to_string())
    } else {
        let map: BTreeMap<Cow<'a, str>, &'a RawValue> =
            serde_json::from_str(raw.get()).map_err(|e| e.to_string())?;
        for (market, rows) in map {
            if rows.get().starts_with('[') {
                json::each(rows.get(), |r: T| each(r, Some(&market))).map_err(|e| e.to_string())?;
            } else if rows.get().starts_with('{') {
                each(
                    serde_json::from_str(rows.get()).map_err(|e| e.to_string())?,
                    Some(&market),
                );
            }
        }
        Ok(())
    }
}
enum OrderRowError {
    /// A working order, by native id, on a market this session cannot map.
    Unmapped(String),
    Invalid(String),
}
impl OrderRowError {
    fn serde(error: serde_json::Error) -> Self {
        Self::Invalid(error.to_string())
    }
}
/// Native stream rejection with a readable post-only explanation.
/// No notice for the user's own cancellation.
fn rejection_error(raw: &RawValue, order: &Order) -> Option<VenueError> {
    #[derive(Deserialize)]
    struct Nested<'a> {
        #[serde(default, borrow)]
        order: Option<&'a RawValue>,
    }
    #[derive(Deserialize)]
    struct Row<'a> {
        #[serde(default, borrow)]
        status: Cow<'a, str>,
        #[serde(default, borrow, alias = "cancelReason")]
        cancel_reason: Cow<'a, str>,
    }
    let nested = serde_json::from_str::<Nested<'_>>(raw.get()).ok()?;
    let row = serde_json::from_str::<Row<'_>>(nested.order.unwrap_or(raw).get()).ok()?;
    let reason = row.cancel_reason.trim();
    if matches!(
        reason.to_ascii_uppercase().as_str(),
        "USER_CANCELLED" | "USER_CANCELED"
    ) {
        return None;
    }
    let reject_reason = order.extra.as_ref().and_then(|extra| extra.reject_reason);
    let native_message = if !reason.is_empty() {
        reason
    } else if reject_reason.is_some() {
        row.status.as_ref()
    } else {
        return None;
    };
    // Translate only the native post-only rejection, retaining its typed diagnostics.
    Some(if reject_reason == Some(RejectReason::PostOnly) {
        VenueError::new("Post-only order would execute immediately as taker; not placed.")
            .kind(ErrorKind::Rejected)
            .venue_message(native_message)
    } else {
        VenueError::new(native_message)
    })
}

fn client_order_index(raw: &RawValue) -> Option<String> {
    #[derive(Deserialize)]
    struct Nested<'a> {
        #[serde(default, borrow)]
        order: Option<&'a RawValue>,
    }
    #[derive(Deserialize)]
    struct Client<'a> {
        #[serde(default, borrow, alias = "clientOrderIndex")]
        client_order_index: Option<Atom<'a>>,
    }
    let nested: Nested<'_> = serde_json::from_str(raw.get()).ok()?;
    let row: Client<'_> = serde_json::from_str(nested.order.unwrap_or(raw).get()).ok()?;
    row.client_order_index
        .and_then(|a| index(&a.0))
        .filter(|n| *n > 0)
        .map(|n| n.to_string())
}
fn index(text: &str) -> Option<u64> {
    let n = text.parse::<u64>().ok()?;
    (n <= i64::MAX as u64).then_some(n)
}
fn address(text: &str) -> Option<String> {
    let at = text.find("0x")?;
    let candidate = text.get(at..at + 42)?;
    candidate[2..]
        .bytes()
        .all(|b| b.is_ascii_hexdigit())
        .then(|| candidate.into())
}
fn ratio(n: f64) -> f64 {
    if n > 1.0 { n / 10000.0 } else { n }
}
/// Position `initial_margin_fraction`: the venue sends a percent string
/// (`"20.00"` = 5x); a bare integer is the 1/10,000 tick form tx payloads use.
/// A percent above 100 is impossible, so it reads as ticks as in TS.
enum MarginFraction {
    Percent(f64),
    Ticks(f64),
}
impl<'de> Deserialize<'de> for MarginFraction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = MarginFraction;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a percent string or tick integer")
            }
            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
                s.trim()
                    .parse()
                    .map(MarginFraction::Percent)
                    .map_err(E::custom)
            }
            fn visit_u64<E: serde::de::Error>(self, n: u64) -> Result<Self::Value, E> {
                Ok(MarginFraction::Ticks(n as f64))
            }
            fn visit_i64<E: serde::de::Error>(self, n: i64) -> Result<Self::Value, E> {
                Ok(MarginFraction::Ticks(n as f64))
            }
            fn visit_f64<E: serde::de::Error>(self, n: f64) -> Result<Self::Value, E> {
                Ok(MarginFraction::Percent(n))
            }
        }
        d.deserialize_any(V)
    }
}
impl MarginFraction {
    fn ratio(&self) -> Option<f64> {
        let ratio = match *self {
            Self::Percent(n) if n <= 100.0 => n / 100.0,
            Self::Percent(n) | Self::Ticks(n) => n / 10000.0,
        };
        (ratio.is_finite() && ratio > 0.0 && ratio <= 1.0).then_some(ratio)
    }
}
fn margin(value: Option<&Atom<'_>>) -> Option<MarginType> {
    match value.map(|v| v.0.as_ref()) {
        Some("1" | "isolated" | "ISOLATED") => Some(MarginType::Isolated),
        Some("0" | "cross" | "CROSS") => Some(MarginType::Cross),
        _ => None,
    }
}
fn millis(time: f64) -> f64 {
    if time > 0.0 && time < 1_000_000_000_000.0 {
        time * 1000.0
    } else {
        time
    }
}

fn normalized(text: &str) -> String {
    text.to_ascii_uppercase().replace(['-', ' '], "_")
}
/// Seconds, milliseconds or microseconds to milliseconds.
fn venue_millis(time: f64) -> f64 {
    if time >= 100_000_000_000_000.0 {
        time / 1000.0
    } else {
        millis(time)
    }
}
fn event_time(updated: Option<&Num>, transaction: Option<&Num>) -> Option<f64> {
    transaction
        .filter(|n| n.0 > 0.0)
        .map(|n| venue_millis(n.0))
        .or_else(|| updated.filter(|n| n.0 > 0.0).map(|n| millis(n.0)))
}

impl Lighter {
    pub(crate) fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.started {
            return;
        }
        self.account.started = true;
        if cx.frame_fed() {
            // The data source owns the account rows, settings and lookup; the
            // account index arrives in VENUE_STATE. Only the own credential loads.
            if !self.options.public_data_only {
                self.load_credential();
            }
            return;
        }
        cx.store.set_hedged(false);
        for m in cx.store.markets().to_vec() {
            let mode = self.metadata.get(&m.symbol).map_or(0, |v| v.margin_mode);
            let leverage = self.default_leverage(&m);
            cx.store.update_setting(&m.symbol, |s| {
                s.leverage = leverage;
                s.margin_type = Some(if mode == 1 {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                });
            });
        }
        if self.options.public_data_only
            || (self.options.identity.is_none() && !self.options.remote_signer)
            || self
                .options
                .api_key_index
                .as_deref()
                .is_none_or(str::is_empty)
        {
            cx.store.set_positions(vec![]);
            cx.store.set_orders(vec![]);
            cx.store.set_balance(Balance::default());
            return;
        }
        self.load_credential();
        self.next_lookup(cx, self.lookup_candidates());
    }
    fn load_credential(&mut self) {
        self.account.identity = self
            .options
            .identity
            .clone()
            .filter(|identity| identity.signing_ref().is_ok());
        self.account.api_key_index = self
            .options
            .api_key_index
            .as_deref()
            .and_then(|s| s.parse::<u8>().ok());
    }
    /// Frame-fed: the data source's resolved account index, refused when it
    /// names another account than the configured Application ID.
    pub(crate) fn apply_account_index(&mut self, cx: &mut Cx<'_, Pending>, account: u64) {
        if let Some(expected) = self.options.application_id.as_deref().and_then(index)
            && expected != account
        {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "Lighter data source names a different account index",
            );
            return;
        }
        self.account.account_index = Some(account);
        self.release_awaiting_identity(cx, account);
    }
    pub(crate) fn lookup_candidates(&self) -> VecDeque<Lookup> {
        self.candidates(true)
    }
    /// Identity reads never fall back to the API key index: after a transient
    /// address-lookup failure it names someone else's account.
    pub(crate) fn identity_candidates(&self) -> VecDeque<Lookup> {
        self.candidates(false)
    }
    fn candidates(&self, key_index: bool) -> VecDeque<Lookup> {
        let mut candidates = VecDeque::new();
        let app = self
            .options
            .application_id
            .as_deref()
            .unwrap_or_default()
            .trim();
        if let Some(i) = index(app) {
            candidates.push_back(Lookup {
                by: "index",
                value: i.to_string(),
                address: false,
            });
        }
        let mut addresses = Vec::new();
        for source in [Some(app), self.options.l1_address.as_deref()]
            .into_iter()
            .flatten()
        {
            if let Some(a) = address(source)
                && !addresses.contains(&a)
            {
                addresses.push(a);
            }
        }
        for a in &addresses {
            candidates.push_back(Lookup {
                by: "l1_address",
                value: a.clone(),
                address: false,
            });
        }
        // The key index names a stranger's account when a selector is present.
        let selected = index(app).is_some() || !addresses.is_empty();
        if key_index
            && !selected
            && let Some(key) = self.options.api_key_index.as_deref().and_then(index)
        {
            candidates.push_back(Lookup {
                by: "index",
                value: key.to_string(),
                address: false,
            });
        }
        for a in addresses {
            candidates.push_back(Lookup {
                by: "l1_address",
                value: a,
                address: true,
            });
        }
        candidates
    }
    /// A placement answers with its client index; rows name the native one.
    pub(crate) fn note_client_order(&mut self, client: &str, native: &str) {
        if self.account.client_orders.len() >= 1_000 {
            self.account.client_orders.clear();
            self.account.unnamed_placements.clear();
        }
        if client != native {
            self.account.unnamed_placements.remove(client);
        }
        self.account
            .client_orders
            .insert(client.to_owned(), native.to_owned());
    }
    fn next_lookup(&mut self, cx: &mut Cx<'_, Pending>, mut candidates: VecDeque<Lookup>) {
        if let Some(candidate) = candidates.pop_front() {
            let path = candidate.path();
            self.account_request(
                cx,
                format!("{}/api/v1/{path}", self.host()),
                Some(json::ArrayAt::Envelope(&[
                    &["accounts"],
                    &["sub_accounts"],
                    &["data", "accounts"],
                    &["data", "sub_accounts"],
                ])),
                Request::Lookup {
                    candidates,
                    index: None,
                },
                None,
            );
        } else if std::mem::take(&mut self.account.lookup_failed) {
            // No lookup was answered, so nothing says the account is absent.
            cx.retried_failure(
                "lighter.account:lookup",
                "Lighter account lookup failed; retrying",
            );
            self.account.lookup_retry = Some(cx.set_timer(5_000.0));
        } else {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "Failed to resolve Lighter account index. Provide account index in Application ID.",
            );
            self.account.lookup_abandoned = true;
            self.schedule_identity_refusal(cx);
        }
    }
    fn identity_ready(&mut self, cx: &mut Cx<'_, Pending>, account: u64) {
        if let Some(expected) = self.options.application_id.as_deref().and_then(index)
            && expected != account
        {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "Lighter account lookup returned a different account index",
            );
            self.account.lookup_abandoned = true;
            self.schedule_identity_refusal(cx);
            return;
        }
        self.account.account_index = Some(account);
        self.release_awaiting_identity(cx, account);
        if self.account.identity.is_some() && self.account.connection.stream.idle() {
            self.open_feed(cx, crate::Channel::Private);
        }
        self.refresh_account(cx);
        self.refresh_orders(cx);
        self.account.sync = Some(cx.set_poll_timer(2_000.0));
    }

    pub(crate) fn auth_recipe(
        &self,
        identity: safe_cex_engine::signing::Identity,
        placement: sign::AuthPlacement,
    ) -> Result<Box<dyn safe_cex_engine::SignStep>, String> {
        let account = sign::Account::new(
            self.chain_id(),
            self.account
                .account_index
                .ok_or("Lighter account is not loaded yet")?,
            self.account
                .api_key_index
                .ok_or("Invalid Lighter API key index")?,
        )?;
        Ok(Box::new(sign::FormRecipe(sign::AuthRecipe::new(
            identity,
            account,
            self.host().into(),
            placement,
        ))))
    }
    pub(crate) fn account_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        url: String,
        at: Option<json::ArrayAt>,
        request: Request,
        placement: Option<sign::AuthPlacement>,
    ) {
        use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy, Scope};
        let identity = placement.as_ref().and(self.account.identity.clone());
        if placement.is_some() && identity.is_none() {
            self.account_failed(cx, request, "Lighter signer unavailable");
            return;
        }
        let context = match cx.http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: identity.as_ref().map(crate::primary_credential),
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) {
            Ok(context) => context,
            Err(error) => {
                self.account_failed(cx, request, &error.venue_error().message);
                return;
            }
        };
        let result = match (identity, placement) {
            (Some(identity), Some(placement)) => match self.auth_recipe(identity, placement) {
                Ok(recipe) => cx.request_signed_managed(
                    context.clone(),
                    None,
                    Method::Get,
                    url,
                    None,
                    recipe,
                    Pending::Account {
                        context: context.clone(),
                        request,
                    },
                ),
                Err(error) => {
                    let _ = cx.finish_http_context(&context);
                    self.account_failed(cx, request, &error);
                    return;
                }
            },
            _ => cx.request_managed(
                context.clone(),
                None,
                Method::Get,
                url,
                None,
                Pending::Account {
                    context: context.clone(),
                    request,
                },
            ),
        };
        match result {
            Ok(id) => {
                if let Some(at) = at {
                    cx.stream_response(id, at);
                }
            }
            Err((Pending::Account { request, .. }, error)) => {
                let _ = cx.finish_http_context(&context);
                self.account_failed(cx, request, &error.venue_error().message);
            }
            Err(_) => unreachable!(),
        }
    }
    pub(crate) fn account_snapshot_serial(&self) -> Option<u64> {
        self.account
            .snapshot_inflight
            .then_some(self.account.snapshot_serial)
    }
    pub(crate) fn refresh_account(&mut self, cx: &mut Cx<'_, Pending>) {
        // Frame-fed account rows and their reconcile arrive from the data source.
        if cx.frame_fed() || self.account.snapshot_inflight {
            return;
        }
        let Some(account) = self.account.account_index else {
            return;
        };
        self.account.snapshot_inflight = true;
        self.account.snapshot_serial = self.account.snapshot_serial.wrapping_add(1);
        self.account_request(
            cx,
            format!("{}/api/v1/account?by=index&value={account}", self.host()),
            Some(json::ArrayAt::Field("accounts")),
            Request::Snapshot {
                serial: self.account.snapshot_serial,
                revision: self.account.revision,
                account: None,
                valid: true,
            },
            None,
        );
    }
    pub(crate) fn refresh_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.frame_fed() || self.account.orders_inflight || self.account.identity.is_none() {
            return;
        }
        if self.account.account_index.is_none() {
            return;
        }
        self.account.orders_inflight = true;
        let census = CensusRead {
            started: cx.now,
            ..CensusRead::default()
        };
        self.request_orders(
            cx,
            None,
            self.account.revision,
            SnapshotCoverage::default(),
            census,
        );
    }
    /// An uncertainty reconcile needs a read requested after the write: one
    /// already in flight may predate it, so another follows once it settles.
    /// A re-ask (no new uncertain write) joins the running read instead.
    pub(crate) fn reconcile_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.orders_again |= self.account.orders_inflight && !cx.reconcile_reask();
        self.refresh_orders(cx);
    }
    fn orders_settled(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.orders_inflight = false;
        if std::mem::take(&mut self.account.orders_again) {
            self.refresh_orders(cx);
        }
    }
    fn request_orders(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        market: Option<String>,
        revision: u64,
        coverage: SnapshotCoverage<String>,
        census: CensusRead,
    ) {
        let id = market.as_deref().unwrap_or("255");
        let account = self.account.account_index.unwrap();
        self.account_request(
            cx,
            format!(
                "{}/api/v1/accountActiveOrders?account_index={account}&market_id={id}",
                self.host()
            ),
            Some(json::ArrayAt::Envelope(&[
                &["orders"],
                &["data", "orders"],
                &["result", "orders"],
                &["data"],
                &["result"],
                &[],
            ])),
            Request::Orders {
                revision,
                market,
                orders: Vec::new(),
                coverage,
                census,
                valid: true,
            },
            Some(sign::AuthPlacement::Query),
        );
    }
    pub(crate) fn account_order_terminal(&self, id: &str) -> bool {
        self.account.suppressed.contains(id)
    }
    pub(crate) fn record_cancel(&mut self, id: &str, _now: f64) {
        self.protection.originals.remove(id);
        self.protection.complex.remove(id);
        self.account.revision += 1;
        self.account
            .order_revision
            .insert(id.into(), self.account.revision);
        self.account.suppressed.insert(id.into());
        self.account.records.remove(id);
    }
    pub(crate) fn record_modify(&mut self, id: &str) {
        self.protection.originals.remove(id);
        self.account.revision += 1;
        self.account
            .order_revision
            .insert(id.into(), self.account.revision);
    }
    pub(crate) fn record_placement(&mut self, order: Order) {
        self.account.revision += 1;
        self.account
            .order_revision
            .insert(order.id.clone(), self.account.revision);
        self.account.records.insert(order.id.clone(), order);
    }
    pub(crate) fn account_setting_epoch(&self, symbol: &str) -> u64 {
        self.account
            .setting_epochs
            .get(symbol)
            .copied()
            .unwrap_or(0)
    }
    pub(crate) fn account_setting_write(&mut self, symbol: &str) {
        self.account.revision += 1;
        self.account
            .settings_revision
            .insert(symbol.into(), self.account.revision);
    }
    fn schedule_heal(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.heal.is_none() {
            self.account.heal = Some(cx.set_timer(500.0));
        }
    }
    fn subscribe_private(&self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let account = self.account.account_index.unwrap();
        cx.send(
            socket,
            ControlFrame::subscription(format!(
                r#"{{"type":"subscribe","channel":"account_all/{account}"}}"#
            )),
        );
        if let Some(identity) = self.account.identity.clone() {
            let channel = format!("account_all_orders/{account}");
            match self.auth_recipe(
                identity,
                sign::AuthPlacement::WsSubscribe {
                    socket,
                    url: self.ws_url().into(),
                    channel: channel.clone(),
                },
            ) {
                Ok(recipe) => cx.send_signed(
                    socket,
                    ControlFrame::authentication(format!(
                        r#"{{"type":"subscribe","channel":"{channel}"}}"#
                    )),
                    recipe,
                ),
                Err(error) => cx.notice(Level::Warn, code::STREAM_UNAVAILABLE, error),
            }
        }
    }
    pub(crate) fn account_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.account.connection.connected_at = cx.now;
        self.account.connection.last_payload = None;
        self.account.connection.last_recovery = None;
        self.account.connection.recovery_started = None;
        self.account.session_start.get_or_insert(cx.now);
        self.account.live_after = self.account.outage.map(|outage| self.cutoff(outage));
        self.subscribe_private(cx, socket);
        cx.send(
            socket,
            ControlFrame::heartbeat(format!(
                r#"{{"type":"ping","timestamp":{}}}"#,
                cx.protocol_now()
            )),
        );
        self.refresh_account(cx);
        self.refresh_orders(cx);
        if let Some(since) = self.account.live_after {
            if let Some((timer, _)) = self.account.backfill_retry.take() {
                cx.clear_timer(timer);
            }
            let run = Backfill {
                since,
                epoch: self.account.outage_epoch,
                page: 1,
                cursor: None,
                retried: false,
            };
            self.request_backfill(cx, run);
        }
    }
    /// The engine has closed the epoch and owns the retry.
    pub(crate) fn account_retired(&mut self, cx: &mut Cx<'_, Pending>) {
        let connection = &self.account.connection;
        self.arm_outage(connection.last_payload.unwrap_or(connection.connected_at));
        self.trade_stream_lost(cx);
    }
    /// Legacy lighter.ws-private.ts: a handshake that failed, or opened and closed
    /// within 10 s with no account stream, as 1006 or a 1008 restriction.
    pub(crate) fn note_private_close(&mut self, cx: &mut Cx<'_, Pending>, code: u16) {
        let connection = &mut self.account.connection;
        let early = connection.stream.open
            && connection.last_payload.is_none()
            && cx.now - connection.connected_at <= 10_000.0;
        let restricted = matches!(code, 1006 | 1008) && (!connection.stream.open || early);
        if !restricted || connection.stream.read_only || self.options.ws_routed {
            return;
        }
        connection.read_only_failures += 1;
        if connection.read_only_failures < 3 {
            return;
        }
        connection.stream.read_only = true;
        cx.notice(
            Level::Warn,
            code::STREAM_UNAVAILABLE,
            "Lighter georestrictions detected; connectivity status is degraded. Private updates may rely on read-only/REST fallback from this IP. Use a VPN or Tealstreet WS proxy; account tier changes will not fix this.",
        );
    }
    // Payload staleness is checked on the same 10s cadence as the heartbeat.
    pub(crate) fn private_heartbeat(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let connection = &self.account.connection;
        if connection
            .recovery_started
            .is_some_and(|at| cx.now - at >= 20_000.0)
        {
            self.retire(cx, crate::Channel::Private);
            return;
        }
        let payload_age = cx.now - connection.last_payload.unwrap_or(connection.connected_at);
        let stale_after = if connection.last_payload.is_some() {
            120_000.0
        } else {
            180_000.0
        };
        if payload_age >= stale_after
            && !connection.stream.read_only
            && connection.recovery_started.is_none()
            && connection
                .last_recovery
                .is_none_or(|at| cx.now - at >= 180_000.0)
        {
            let quiet_since = connection.last_payload.unwrap_or(connection.connected_at);
            self.account.connection.last_recovery = Some(cx.now);
            self.account.connection.recovery_started = Some(cx.now);
            self.arm_outage(quiet_since);
            self.account.live_after = self.account.outage.map(|outage| self.cutoff(outage));
            self.subscribe_private(cx, socket);
        }
        cx.send(
            socket,
            ControlFrame::heartbeat(format!(
                r#"{{"type":"ping","timestamp":{}}}"#,
                cx.protocol_now()
            )),
        );
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.identity_refusal == Some(timer) {
            self.account.identity_refusal = None;
            self.refuse_awaiting_identity(cx);
            return true;
        }
        if self.account.fresh_positions_timer == Some(timer) {
            self.account.fresh_positions_timer = None;
            self.retry_fresh_positions(cx);
            return true;
        }
        if self.account.lookup_retry == Some(timer) {
            self.account.lookup_retry = None;
            self.next_lookup(cx, self.lookup_candidates());
            return true;
        }
        if self.account.sync == Some(timer) {
            self.account.sync = Some(cx.set_poll_timer(2_000.0));
            // Two reads every 2 s fill Lighter's budget, and a close then gets refused.
            let cadence = if self.account.connection.stream.open {
                STREAMED_POLL_MS
            } else {
                0.0
            };
            if !self.trade_running()
                && cx.now >= self.account.polls_held_until
                && cx.now - self.account.polled_at >= cadence
            {
                self.account.polled_at = cx.now;
                self.refresh_account(cx);
                self.refresh_orders(cx);
            }
            self.refresh_limits(cx);
            return true;
        }
        if self
            .account
            .backfill_retry
            .as_ref()
            .is_some_and(|(t, _)| *t == timer)
        {
            let (_, run) = self.account.backfill_retry.take().unwrap();
            self.request_backfill(cx, run);
            return true;
        }
        if self.account.census_follow_up == Some(timer) {
            self.account.census_follow_up = None;
            // Within the poll's budget; a held poll reads it later.
            if !self.trade_running() && cx.now >= self.account.polls_held_until {
                self.refresh_orders(cx);
            }
            return true;
        }
        if self.account.heal == Some(timer) {
            self.account.heal = None;
            self.refresh_account(cx);
            self.refresh_orders(cx);
            return true;
        }
        false
    }
    fn position_row(
        &self,
        cx: &mut Cx<'_, Pending>,
        wire: PositionWire<'_>,
        market_hint: Option<&str>,
    ) -> Option<(Option<Position>, String, f64, MarginType)> {
        let id = if wire.market_id.0.is_empty() {
            market_hint.unwrap_or_default()
        } else {
            &wire.market_id.0
        };
        let symbol = if !wire.symbol.is_empty() {
            wire.symbol.as_ref()
        } else {
            self.symbol_for_id(id)?
        };
        let market = cx.store.market(symbol)?;
        let meta = self.metadata.get(symbol);
        let margin_type = margin(wire.margin_mode.as_ref())
            .or_else(|| cx.store.setting(symbol).and_then(|s| s.margin_type))
            .unwrap_or_else(|| {
                if meta.is_some_and(|m| m.margin_mode == 1) {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                }
            });
        let leverage = wire
            .leverage
            .as_ref()
            .map(|n| n.0)
            .filter(|n| *n > 0.0)
            .or_else(|| {
                wire.initial_margin_fraction
                    .as_ref()
                    .and_then(MarginFraction::ratio)
                    .map(|n| 1.0 / n)
            })
            .or_else(|| {
                cx.store
                    .setting(symbol)
                    .map(|s| s.leverage)
                    .filter(|n| *n > 0.0)
            })
            .or_else(|| {
                cx.store
                    .positions()
                    .iter()
                    .find(|p| p.symbol == symbol)
                    .map(|p| p.leverage)
            })
            .unwrap_or_else(|| self.default_leverage(market));
        let leverage = safe_cex_engine::num::clamp_leverage(
            leverage,
            market.limits.leverage.min,
            market.limits.leverage.max,
        )
        .ok()?;
        let Some(size) = wire.size else {
            return Some((
                cx.store
                    .positions()
                    .iter()
                    .find(|p| p.symbol == symbol)
                    .cloned()
                    .map(|mut p| {
                        p.leverage = leverage;
                        p.margin_type = Some(margin_type);
                        p
                    }),
                symbol.into(),
                leverage,
                margin_type,
            ));
        };
        let contracts = size.0.abs();
        if contracts == 0.0 {
            return Some((None, symbol.into(), leverage, margin_type));
        }
        let side = if wire.side.eq_ignore_ascii_case("short")
            || wire.side.eq_ignore_ascii_case("sell")
            || wire.sign.0 < 0.0
            || size.0 < 0.0
        {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let entry = wire.avg_entry_price.0;
        let mut liquidation = wire.liquidation_price.0;
        let wrong_side = liquidation <= 0.0
            || if side == PositionSide::Long {
                liquidation >= entry
            } else {
                liquidation <= entry
            };
        if margin_type == MarginType::Isolated && wrong_side && entry > 0.0 {
            let allocated = wire
                .allocated_margin
                .as_ref()
                .map(|n| n.0)
                .filter(|n| *n > 0.0)
                .unwrap_or(entry * contracts / leverage);
            let maintenance = wire
                .maintenance_margin_fraction
                .as_ref()
                .map(|n| ratio(n.0))
                .or_else(|| meta.map(|m| ratio(m.maintenance_margin_fraction)))
                .unwrap_or(0.0);
            if (0.0..1.0).contains(&maintenance) {
                let value = if side == PositionSide::Long {
                    (entry * contracts - allocated) / (contracts * (1.0 - maintenance))
                } else {
                    (entry * contracts + allocated) / (contracts * (1.0 + maintenance))
                };
                if value.is_finite() && value > 0.0 {
                    liquidation = value;
                }
            }
        }
        let reported_pnl = wire.unrealized_pnl.map(|n| n.0);
        let unrealized_pnl = reported_pnl
            .or_else(|| {
                cx.store
                    .position(symbol)
                    .filter(|prior| prior.side == side)
                    .map(|prior| prior.unrealized_pnl)
            })
            .filter(|pnl| pnl.is_finite())
            .unwrap_or(0.0);
        if reported_pnl.is_none() {
            cx.notice(
                Level::Warn,
                code::VALUATION_UNAVAILABLE,
                "Lighter position PnL unavailable; retaining prior valuation",
            );
        }
        Some((
            Some(Position {
                id: symbol.into(),
                symbol: symbol.into(),
                side,
                entry_price: entry,
                notional: wire.position_value.map_or(contracts * entry, |n| n.0.abs()),
                leverage,
                margin_type: Some(margin_type),
                position_mode: Some(PositionMode::Oneway),
                unrealized_pnl,
                realized_pnl: wire.realized_pnl.map(|n| n.0),
                contracts,
                liquidation_price: liquidation,
                margin_size: None,
                last_update: Some(cx.now),
            }),
            symbol.into(),
            leverage,
            margin_type,
        ))
    }
    /// Stored positions are keyed by symbol, so a native id resolves through the
    /// catalogue, then the store's markets; an id neither knows stays as itself.
    fn position_key(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        market_id: &str,
        hint: Option<&str>,
    ) -> Option<String> {
        if !symbol.is_empty() {
            return Some(symbol.into());
        }
        let id = if market_id.is_empty() {
            hint.unwrap_or_default()
        } else {
            market_id
        };
        (!id.is_empty()).then(|| {
            self.symbol_for_id(id)
                .or_else(|| {
                    cx.store
                        .markets()
                        .iter()
                        .find(|m| m.id == id)
                        .map(|m| m.symbol.as_str())
                })
                .unwrap_or(id)
                .into()
        })
    }
    fn snapshot_row(
        &self,
        cx: &mut Cx<'_, Pending>,
        wire: AccountWire<'_>,
    ) -> Result<Snapshot, String> {
        if let Some(identity) = wire.identity()
            && index(&identity.0) != self.account.account_index
        {
            return Err("Lighter account snapshot belongs to another account".into());
        }
        if wire.positions.is_none()
            || wire.available_balance.is_none()
            || (wire.total_asset_value.is_none()
                && wire.collateral.is_none()
                && wire.cross_asset_value.is_none())
        {
            return Err("Incomplete Lighter account snapshot".into());
        }
        let mut snapshot = Snapshot {
            equity: wire
                .total_asset_value
                .or(wire.collateral)
                .or(wire.cross_asset_value)
                .map_or(0.0, |n| n.0),
            free: wire.available_balance.map_or(0.0, |n| n.0),
            ..Snapshot::default()
        };
        if let Some(raw) = wire.positions {
            let mut invalid_limits = false;
            each_row(raw, |row: &RawValue, hint| {
                let Ok(row) = serde_json::from_str::<PositionWire<'_>>(row.get()) else {
                    #[derive(Deserialize)]
                    struct Identity<'a> {
                        #[serde(default, borrow, alias = "marketId")]
                        market_id: Atom<'a>,
                        #[serde(default, borrow)]
                        symbol: Cow<'a, str>,
                        #[serde(default, borrow, alias = "position", alias = "qty")]
                        size: Option<&'a RawValue>,
                        #[serde(default, borrow, alias = "unrealizedPnl")]
                        unrealized_pnl: Option<&'a RawValue>,
                    }
                    let num = |raw: Option<&RawValue>| {
                        raw.and_then(|raw| serde_json::from_str::<Num>(raw.get()).ok())
                            .map(|n| n.0)
                    };
                    let (identity, size, pnl) =
                        match serde_json::from_str::<Identity<'_>>(row.get()) {
                            Ok(row) => (
                                self.position_key(cx, &row.symbol, &row.market_id.0, hint),
                                num(row.size),
                                num(row.unrealized_pnl),
                            ),
                            Err(_) => (None, None, None),
                        };
                    // A flat row of a known market is a closure whatever else failed.
                    if identity.is_some() && size == Some(0.0) {
                        return;
                    }
                    cx.notice(
                        Level::Warn,
                        code::ROW_SKIPPED,
                        match &identity {
                            Some(id) => format!("Lighter skipped malformed position {id}"),
                            None => "Lighter skipped malformed position without a market".into(),
                        },
                    );
                    snapshot.skipped.push((identity.clone(), pnl));
                    snapshot.coverage.reject(identity);
                    return;
                };
                let owned = row.size.as_ref().is_some_and(|n| n.0 != 0.0);
                let pnl = row.unrealized_pnl.as_ref().map(|n| n.0);
                let market_id = if row.market_id.0.is_empty() {
                    hint.unwrap_or_default()
                } else {
                    &row.market_id.0
                };
                if self.spot_market(market_id) || self.spot_market(&row.symbol) {
                    return;
                }
                let symbol = if !row.symbol.is_empty() {
                    Some(row.symbol.as_ref())
                } else {
                    self.symbol_for_id(market_id)
                };
                let identity = self.position_key(cx, &row.symbol, &row.market_id.0, hint);
                if symbol
                    .and_then(|symbol| cx.store.market(symbol))
                    .is_some_and(|market| {
                        safe_cex_engine::num::clamp_leverage(
                            1.0,
                            market.limits.leverage.min,
                            market.limits.leverage.max,
                        )
                        .is_err()
                    })
                {
                    invalid_limits = true;
                    return;
                }
                let known = margin(row.margin_mode.as_ref()).is_some()
                    && (row.leverage.as_ref().is_some_and(|n| n.0 > 0.0)
                        || row
                            .initial_margin_fraction
                            .as_ref()
                            .is_some_and(|n| n.ratio().is_some()));
                let explicit_size = row.size.is_some();
                if let Some((position, symbol, leverage, margin)) = self.position_row(cx, row, hint)
                {
                    if known {
                        snapshot.known_settings.insert(symbol.clone());
                    }
                    if !explicit_size {
                        snapshot.unconfirmed_positions.insert(symbol.clone());
                    }
                    snapshot.settings.push((symbol, leverage, margin));
                    if let Some(position) = position {
                        snapshot.positions.push(position);
                    }
                } else if owned {
                    cx.notice(
                        Level::Warn,
                        code::ROW_SKIPPED,
                        match &identity {
                            Some(id) => {
                                format!("Lighter skipped position {id} on an unknown market")
                            }
                            None => "Lighter skipped position without a market".into(),
                        },
                    );
                    snapshot.skipped.push((identity.clone(), pnl));
                    snapshot.coverage.reject(identity);
                }
            })?;
            if invalid_limits {
                return Err("Invalid Lighter market leverage limits".into());
            }
        }
        Ok(snapshot)
    }
    pub(crate) fn account_map_order(
        &self,
        cx: &Cx<'_, Pending>,
        raw: &RawValue,
        market_hint: Option<&str>,
    ) -> Result<Option<Order>, String> {
        self.map_order_row(cx, raw, market_hint, false)
            .map_err(|e| match e {
                OrderRowError::Unmapped(_) => {
                    "Lighter working order has unavailable market metadata".into()
                }
                OrderRowError::Invalid(e) => e,
            })
    }
    /// Maps one active-order snapshot row into `orders` or records it as skipped.
    fn snapshot_order(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        raw: &RawValue,
        market_hint: Option<&str>,
        orders: &mut Vec<Order>,
        coverage: &mut SnapshotCoverage<String>,
        census: &mut CensusRead,
    ) {
        match self.map_order_row(cx, raw, market_hint, true) {
            Ok(order) => {
                // A snapshot row proves a grouped leg by its own client order index.
                if let Some(order) = &order
                    && let Some(client_index) = client_order_index(raw)
                {
                    census
                        .observed
                        .push((client_index, order.id.clone(), order.symbol.clone()));
                }
                if let Some(order) = &order {
                    census.originals.push((order.clone(), raw.get().into()));
                }
                return orders.extend(order);
            }
            Err(OrderRowError::Unmapped(id)) => {
                cx.notice(
                    Level::Warn,
                    code::ROW_SKIPPED,
                    format!("Lighter skipped order {id} on an unknown market"),
                );
                return coverage.reject_in(Some(id), market_hint.unwrap_or(""));
            }
            Err(OrderRowError::Invalid(_)) => {}
        }
        #[derive(Deserialize)]
        struct Nested<'a> {
            #[serde(default, borrow)]
            order: Option<&'a RawValue>,
        }
        #[derive(Deserialize)]
        struct Identity<'a> {
            #[serde(default, borrow, alias = "orderId", alias = "OrderId")]
            order_id: Option<Atom<'a>>,
            #[serde(default, borrow, alias = "orderIndex", alias = "OrderIndex")]
            order_index: Option<Atom<'a>>,
        }
        let id = serde_json::from_str::<Nested<'_>>(raw.get())
            .ok()
            .and_then(|n| serde_json::from_str::<Identity<'_>>(n.order.unwrap_or(raw).get()).ok())
            .and_then(|row| {
                row.order_id
                    .or(row.order_index)
                    .and_then(|a| index(&a.0))
                    .map(|n| n.to_string())
            });
        cx.notice(
            Level::Warn,
            code::ROW_SKIPPED,
            match &id {
                Some(id) => format!("Lighter skipped unmappable order {id}"),
                None => "Lighter skipped unmappable order without an id".into(),
            },
        );
        coverage.reject_in(id, market_hint.unwrap_or(""));
    }
    fn map_order_row(
        &self,
        cx: &Cx<'_, Pending>,
        raw: &RawValue,
        market_hint: Option<&str>,
        known_kind: bool,
    ) -> Result<Option<Order>, OrderRowError> {
        #[derive(Deserialize)]
        struct Nested<'a> {
            #[serde(default, borrow)]
            order: Option<&'a RawValue>,
        }
        let nested: Nested<'_> = serde_json::from_str(raw.get()).map_err(OrderRowError::serde)?;
        let wire: OrderWire<'_> = serde_json::from_str(nested.order.unwrap_or(raw).get())
            .map_err(OrderRowError::serde)?;
        let Some(id) = wire
            .order_id
            .as_ref()
            .or(wire.order_index.as_ref())
            .and_then(|a| index(&a.0))
            .map(|n| n.to_string())
        else {
            return Err(OrderRowError::Invalid(
                "Lighter order has no valid native order id".into(),
            ));
        };
        let market_id = if wire.market_id.0.is_empty() {
            market_hint.unwrap_or_default()
        } else {
            &wire.market_id.0
        };
        let symbol = if !wire.symbol.is_empty() {
            Some(wire.symbol.as_ref())
        } else {
            self.symbol_for_id(market_id)
        };
        let Some(symbol) = symbol.filter(|symbol| cx.store.market(symbol).is_some()) else {
            let status = normalized(&wire.status);
            if matches!(status.as_str(), "FILLED" | "EXPIRED" | "REJECTED")
                || status.starts_with("CANCELED")
                || status.starts_with("CANCELLED")
                || self.spot_market(market_id)
                || self.spot_market(&wire.symbol)
            {
                return Ok(None);
            }
            return Err(OrderRowError::Unmapped(id));
        };
        let existing = cx
            .store
            .orders()
            .iter()
            .find(|o| o.id == id)
            .or_else(|| self.account.records.get(&id));
        let mut kind = match normalized(&wire.kind.0).as_str() {
            // TWAP, its slices and liquidations rest like limits, as the stream maps them.
            "0" | "6" | "7" | "8" | "LIMIT" | "TWAP" | "TWAP_SUB" | "LIQUIDATION" => {
                OrderType::Limit
            }
            "1" | "MARKET" => OrderType::Market,
            "2" | "3" | "STOP_LOSS" | "STOP_MARKET" | "STOP_LIMIT" | "STOP_LOSS_LIMIT" => {
                OrderType::StopLoss
            }
            "4" | "5" | "TAKE_PROFIT" | "TAKE_PROFIT_MARKET" | "TAKE_PROFIT_LIMIT" => {
                OrderType::TakeProfit
            }
            _ if known_kind => {
                return Err(OrderRowError::Invalid(
                    "Lighter order has an unknown type".into(),
                ));
            }
            _ => OrderType::Limit,
        };
        if kind == OrderType::Limit
            && (wire.kind.0.is_empty() || wire.trigger_price.is_none())
            && let Some(o) = existing
            && o.kind != OrderType::Limit
        {
            kind = o.kind;
        }
        let tif = if wire.post_only.as_ref().is_some_and(Bool::get) {
            Some(TimeInForce::PostOnly)
        } else {
            match normalized(&wire.time_in_force.0).as_str() {
                "0" | "IOC" | "IMMEDIATE_OR_CANCEL" => Some(TimeInForce::ImmediateOrCancel),
                "2" | "POST_ONLY" => Some(TimeInForce::PostOnly),
                "FOK" => Some(TimeInForce::FillOrKill),
                "1" | "GTC" | "GTT" | "GOOD_TILL_TIME" | "GOOD_TILL_CANCEL" => {
                    Some(TimeInForce::GoodTillCancel)
                }
                _ => existing
                    .and_then(|o| o.time_in_force)
                    .or(Some(TimeInForce::GoodTillCancel)),
            }
        };
        let side = match wire.side.to_ascii_uppercase().as_str() {
            "BUY" | "LONG" => OrderSide::Buy,
            "SELL" | "SHORT" => OrderSide::Sell,
            _ => {
                if let Some(ask) = wire.is_ask.as_ref() {
                    if ask.get() {
                        OrderSide::Sell
                    } else {
                        OrderSide::Buy
                    }
                } else if let Some(buy) = wire.is_buy.as_ref() {
                    if buy.get() {
                        OrderSide::Buy
                    } else {
                        OrderSide::Sell
                    }
                } else {
                    existing.map_or(OrderSide::Sell, |o| o.side)
                }
            }
        };
        let amount = wire.initial_base_amount.as_ref().map_or_else(
            || {
                wire.filled_base_amount.as_ref().map_or(0.0, |n| n.0)
                    + wire.remaining_base_amount.as_ref().map_or(0.0, |n| n.0)
            },
            |n| n.0,
        );
        let filled = wire.filled_base_amount.as_ref().map_or_else(
            || {
                wire.remaining_base_amount
                    .as_ref()
                    .map_or(0.0, |n| (amount - n.0).max(0.0))
            },
            |n| n.0,
        );
        let mut remaining = wire
            .remaining_base_amount
            .as_ref()
            .map_or_else(|| (amount - filled).max(0.0), |n| n.0);
        let protective = matches!(kind, OrderType::StopLoss | OrderType::TakeProfit);
        let mut price = if protective {
            wire.trigger_price
                .as_ref()
                .filter(|n| n.0 > 0.0)
                .or(wire.price.as_ref())
        } else {
            wire.price.as_ref()
        }
        .map_or(0.0, |n| n.0);
        if price <= 0.0 {
            price = existing.map_or(0.0, |o| o.price);
        }
        let native_status = normalized(&wire.status);
        let status = match native_status.as_str() {
            "FILLED" | "EXPIRED" => OrderStatus::Closed,
            "CANCELLED" | "CANCELED" | "REJECTED" => OrderStatus::Canceled,
            s if s.starts_with("CANCELED_") || s.starts_with("CANCELLED_") => OrderStatus::Canceled,
            _ => OrderStatus::Open,
        };
        let reject_reason = if native_status.contains("POST_ONLY") {
            Some(RejectReason::PostOnly)
        } else if native_status.contains("REDUCE_ONLY") {
            Some(RejectReason::ReduceOnly)
        } else if native_status.contains("NOT_ENOUGH_BALANCE")
            || native_status.contains("INSUFFICIENT_BALANCE")
        {
            Some(RejectReason::InsufficientBalance)
        } else {
            None
        };
        if status == OrderStatus::Open && remaining <= 0.0 && amount > 0.0 {
            remaining = amount;
        }
        if status == OrderStatus::Open && amount <= 0.0 && price > 0.0 {
            remaining = wire
                .remaining_quote_amount
                .as_ref()
                .map_or(remaining, |n| n.0 / price);
        }
        let reduce_only = wire
            .reduce_only
            .as_ref()
            .map(Bool::get)
            .or_else(|| existing.map(|o| o.reduce_only))
            .unwrap_or(false);
        let remaining_notional = wire
            .remaining_quote_amount
            .as_ref()
            .map(|n| n.0)
            .or_else(|| {
                wire.quote_amount
                    .as_ref()
                    .filter(|_| amount > 0.0)
                    .map(|n| remaining / amount * n.0)
            })
            .unwrap_or(remaining * price);
        Ok(Some(Order {
            id,
            parent_id: None,
            status,
            symbol: symbol.into(),
            kind,
            side,
            price,
            amount,
            filled,
            remaining,
            reduce_only,
            close: None,
            // What the placement answered with: it names this row to whoever holds that id.
            client_order_id: client_order_index(raw),
            time_in_force: tif,
            last_update: event_time(
                wire.updated_at.as_ref().or(wire.timestamp.as_ref()),
                wire.transaction_time.as_ref(),
            )
            .unwrap_or(cx.now),
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: Some(OrderExtra {
                conditional_order_price: matches!(
                    normalized(&wire.kind.0).as_str(),
                    "3" | "5" | "STOP_LIMIT" | "STOP_LOSS_LIMIT" | "TAKE_PROFIT_LIMIT"
                )
                .then(|| wire.price.as_ref().map_or(0.0, |p| p.0)),
                remaining_native: Some(remaining),
                remaining_notional: Some(remaining_notional),
                reject_reason,
                ..OrderExtra::default()
            }),
        }))
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::Limits { .. } => {}
            Request::Backfill {
                executions,
                oldest,
                count,
                ..
            } => {
                *count += 1;
                let Some(account) = self.account.account_index.map(|a| a.to_string()) else {
                    return;
                };
                if let Ok(trade) = serde_json::from_str::<TradeWire<'_>>(item)
                    && let Some(execution) = self.execution(cx, &trade, None, &account)
                {
                    *oldest = oldest.min(execution.timestamp);
                    executions.push(execution);
                }
            }
            Request::Lookup { index: found, .. } => {
                if found.is_none()
                    && let Ok(wire) = serde_json::from_str::<AccountWire<'_>>(item)
                {
                    *found = wire.identity().and_then(|n| index(&n.0));
                }
            }
            Request::Snapshot { account, valid, .. } => {
                if account.is_some() {
                    return;
                }
                match serde_json::from_str::<AccountWire<'_>>(item)
                    .map_err(|e| e.to_string())
                    .and_then(|wire| self.snapshot_row(cx, wire))
                {
                    Ok(s) => *account = Some(s),
                    Err(_) => *valid = false,
                }
            }
            Request::Orders {
                orders,
                coverage,
                census,
                valid,
                market,
                ..
            } => match serde_json::from_str::<&RawValue>(item) {
                Ok(raw) => {
                    self.snapshot_order(cx, raw, market.as_deref(), orders, coverage, census)
                }
                Err(_) => *valid = false,
            },
        }
    }
    /// Whether the read rejected a row, and whether a newer stream frame held any.
    fn commit_snapshot(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        revision: u64,
        snapshot: Snapshot,
    ) -> (bool, bool) {
        let rejected = snapshot.coverage.rejected() != 0;
        let held = self
            .account
            .position_revision
            .values()
            .any(|changed| *changed > revision);
        let mut positions = snapshot
            .positions
            .into_iter()
            .filter(|p| {
                self.account
                    .position_revision
                    .get(&p.symbol)
                    .copied()
                    .unwrap_or(0)
                    <= revision
            })
            .map(|mut p| {
                if self
                    .account
                    .settings_revision
                    .get(&p.symbol)
                    .copied()
                    .unwrap_or(0)
                    > revision
                    && let Some(setting) = cx.store.setting(&p.symbol)
                {
                    p.leverage = setting.leverage;
                    p.margin_type = setting.margin_type;
                }
                p
            })
            .collect::<Vec<_>>();
        let kept = cx
            .store
            .positions()
            .iter()
            .filter(|p| {
                (self
                    .account
                    .position_revision
                    .get(&p.symbol)
                    .copied()
                    .unwrap_or(0)
                    > revision
                    || !snapshot.coverage.allows_absence(&p.symbol))
                    && !positions.iter().any(|q| q.symbol == p.symbol)
            })
            .cloned()
            .collect::<Vec<_>>();
        positions.extend(kept);
        cx.store.set_positions(positions);
        let mut settings = snapshot
            .settings
            .into_iter()
            .map(|(symbol, leverage, margin)| (symbol, (leverage, margin)))
            .collect::<HashMap<_, _>>();
        for m in cx.store.markets().to_vec() {
            if self
                .account
                .settings_revision
                .get(&m.symbol)
                .copied()
                .unwrap_or(0)
                > revision
                || self
                    .account
                    .position_revision
                    .get(&m.symbol)
                    .copied()
                    .unwrap_or(0)
                    > revision
            {
                continue;
            }
            let provided = settings.remove(&m.symbol);
            if provided.is_some() && snapshot.known_settings.contains(&m.symbol) {
                let epoch = self
                    .account
                    .setting_epochs
                    .entry(m.symbol.clone())
                    .or_default();
                *epoch = epoch.wrapping_add(1);
            }
            let (leverage, margin) = provided.unwrap_or_else(|| {
                let old = cx.store.setting(&m.symbol);
                (
                    old.map_or_else(|| self.default_leverage(&m), |s| s.leverage),
                    old.and_then(|s| s.margin_type).unwrap_or(MarginType::Cross),
                )
            });
            cx.store.update_setting(&m.symbol, |s| {
                s.leverage = leverage;
                s.margin_type = Some(margin);
            });
        }
        // Equity, free and used are native aggregates. PnL sums the committed rows
        // (retained copies included) plus each skipped row with no copy; a skipped
        // row without a copy or a parsed PnL leaves the split unknown, so it holds.
        let skipped_pnl = snapshot.skipped.iter().try_fold(0.0, |sum, (key, pnl)| {
            if key
                .as_ref()
                .is_some_and(|key| cx.store.positions().iter().any(|p| &p.symbol == key))
            {
                Some(sum)
            } else {
                pnl.map(|pnl| sum + pnl)
            }
        });
        if let Some(skipped_pnl) = skipped_pnl {
            let upnl = cx
                .store
                .positions()
                .iter()
                .try_fold(skipped_pnl, |sum, position| {
                    let value = sum + position.unrealized_pnl;
                    value.is_finite().then_some(value)
                });
            let Some(upnl) = upnl else {
                cx.notice(
                    Level::Warn,
                    code::VALUATION_UNAVAILABLE,
                    "Lighter position PnL invalid; preserving previous balance",
                );
                return (rejected, held);
            };
            cx.store.set_balance(Balance {
                total: snapshot.equity - upnl,
                upnl,
                used: (snapshot.equity - snapshot.free).max(0.0),
                free: snapshot.free,
            });
        }
        if self.account.revision != revision {
            self.schedule_heal(cx);
        }
        (rejected, held)
    }
    fn commit_orders(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        revision: u64,
        orders: Vec<Order>,
        coverage: &SnapshotCoverage<String>,
        census: CensusRead,
        paged: bool,
    ) {
        let mut ids = HashSet::new();
        let mut merged = orders
            .into_iter()
            .filter(|o| {
                !self.account.suppressed.contains(&o.id)
                    && self.account.order_revision.get(&o.id).copied().unwrap_or(0) <= revision
            })
            .filter(|o| ids.insert(o.id.clone()))
            .collect::<Vec<_>>();
        merged.extend(
            cx.store
                .orders()
                .iter()
                .filter(|o| {
                    (self.account.order_revision.get(&o.id).copied().unwrap_or(0) > revision
                        || (!coverage.allows_absence(&o.id) && !ids.contains(&o.id)))
                        && !self.account.suppressed.contains(&o.id)
                })
                .cloned(),
        );
        // What this census may replace, decided before any per-order state moves
        // (docs/plans/RUST_FRAME_FED_READ_COVER.md §4.1).
        let scope = safe_cex_engine::order_read_cover::OrderReadScope::Account {
            key: ORDERS_SCOPE,
            account: &[ORDERS_SCOPE],
        };
        let refused = census.read.filter(|_| !census.lost).and_then(|earliest| {
            cx.orders_read_refusals_from(earliest, scope, |_| true, &merged)
                .map(|refused| (earliest, refused))
        });
        let Some((earliest, refused)) = refused else {
            // Refused whole, or unprovable: newer rows already stand, and another
            // census follows to prove what they cannot (absences).
            self.census_follow_up(cx, census.started);
            return;
        };
        if !refused.is_empty() {
            self.census_follow_up(cx, census.started);
        }
        for (client_index, id, symbol) in census.observed {
            if !refused.contains(&symbol) {
                self.note_client_order(&client_index, &id);
                self.trade_group_observed(cx, &client_index);
            }
        }
        // Any rejected row proves the account nothing.
        let mut cover_unproven = coverage.unproven().to_vec();
        if coverage.rejected() != 0 {
            cover_unproven.push(String::new());
        }
        if cx
            .commit_orders_read_from(earliest, scope, |_| true, merged, &cover_unproven)
            .is_none()
        {
            self.census_follow_up(cx, census.started);
            return;
        }
        for (order, raw) in census.originals {
            if !refused.contains(&order.symbol)
                && self
                    .account
                    .order_revision
                    .get(&order.id)
                    .copied()
                    .unwrap_or(0)
                    <= revision
                && cx.store.order(&order.id) == Some(&order)
            {
                self.observe_original(cx.store, &raw, &order);
            }
        }
        self.protection
            .originals
            .retain(|id, original| cx.store.order(id) == Some(&original.observed));
        self.protection
            .complex
            .retain(|id| cx.store.order(id).is_some());
        cx.store.set_orders_loaded();
        let active = cx
            .store
            .orders()
            .iter()
            .map(|o| o.id.as_str())
            .collect::<HashSet<_>>();
        self.account
            .records
            .retain(|id, _| active.contains(id.as_str()));
        // Each per-market page carries its own read token, so the last one cannot
        // vouch for the first: a paged census owed a successor may predate the write.
        if revision == self.account.revision && !(paged && self.account.orders_again) {
            let mut unproven = coverage.unproven().to_vec();
            unproven.extend(refused);
            cx.orders_reconciled_except(&unproven, u64::MAX);
        }
    }
    /// Owe a census after one the stream outran, `CENSUS_FOLLOW_UP_MS` after
    /// that one started at the earliest.
    fn census_follow_up(&mut self, cx: &mut Cx<'_, Pending>, started: f64) {
        if self.account.census_follow_up.is_some() {
            return;
        }
        let wait = (started + CENSUS_FOLLOW_UP_MS - cx.now).max(0.0);
        self.account.census_follow_up = Some(cx.set_timer(wait));
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let status = serde_json::from_str::<Status<'_>>(response.body).ok();
        if matches!(response.status, 405 | 429) {
            self.account.polls_held_until = cx.now + RATE_LIMITED_POLL_HOLD_MS;
        }
        let ok = (200..300).contains(&response.status)
            && (status.as_ref().is_some_and(Status::valid)
                || response.body.trim_start().starts_with('['));
        match request {
            Request::Limits { started_at } => self.limits_response(cx, started_at, response),

            Request::Backfill {
                run,
                mut executions,
                oldest,
                count,
            } => {
                if !ok {
                    let message = status.as_ref().map_or("", |s| s.message.as_ref());
                    let error = format!("HTTP {} {message}", response.status);
                    self.backfill_failed(cx, run, error);
                    return;
                }
                executions.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                for execution in executions {
                    self.observe_execution(cx, execution, run.since);
                }
                #[derive(Deserialize)]
                struct Page<'a> {
                    #[serde(default, borrow)]
                    next_cursor: Option<Cow<'a, str>>,
                }
                let next = serde_json::from_str::<Page<'_>>(response.body)
                    .ok()
                    .and_then(|p| p.next_cursor)
                    .filter(|c| !c.is_empty() && Some(c.as_ref()) != run.cursor.as_deref())
                    .map(Cow::into_owned);
                if count >= 100 && oldest >= run.since {
                    if let Some(next) = next.filter(|_| run.page < 20) {
                        let run = Backfill {
                            page: run.page + 1,
                            cursor: Some(next),
                            ..run
                        };
                        self.request_backfill(cx, run);
                        return;
                    }
                    BackgroundCoverage::Incomplete.report(cx,
                        "Lighter fill backfill stopped before the outage start; older fills may be missing",
                    );
                }
                if self.account.outage_epoch == run.epoch {
                    self.account.outage = None;
                }
            }
            Request::Lookup {
                candidates,
                index: found,
            } => {
                let found = if found.is_some() {
                    found
                } else {
                    #[derive(Deserialize)]
                    struct Legacy<'a> {
                        #[serde(default, borrow)]
                        data: Option<AccountWire<'a>>,
                    }
                    serde_json::from_str::<Legacy<'_>>(response.body)
                        .ok()
                        .and_then(|l| l.data)
                        .and_then(|a| a.identity().and_then(|n| index(&n.0)))
                };
                if ok && let Some(index) = found {
                    self.identity_ready(cx, index);
                } else {
                    // Only these say the account is absent; a 405 verification page says nothing.
                    self.account.lookup_failed |= !ok && !matches!(response.status, 400 | 404);
                    self.next_lookup(cx, candidates);
                }
            }
            Request::Snapshot {
                serial,
                revision,
                account,
                valid,
            } => {
                self.account.snapshot_inflight = false;
                let account = account.or_else(|| {
                    #[derive(Deserialize)]
                    struct Legacy<'a> {
                        #[serde(default, borrow)]
                        data: Option<AccountWire<'a>>,
                    }
                    serde_json::from_str::<Legacy<'_>>(response.body)
                        .ok()
                        .and_then(|l| l.data)
                        .and_then(|w| self.snapshot_row(cx, w).ok())
                });
                if ok
                    && valid
                    && let Some(account) = account
                {
                    let confirmed = self.trade_position_symbol(serial).is_none_or(|symbol| {
                        account.coverage.allows_absence(symbol)
                            && !account.unconfirmed_positions.contains(symbol)
                    });
                    let (rejected, held) = self.commit_snapshot(cx, revision, account);
                    if self.account.identity.is_none() && !cx.store.loaded().orders {
                        cx.store.set_orders(vec![]);
                    }
                    self.trade_account_ready(cx, serial, confirmed);
                    let read = serial as u32;
                    if rejected {
                        let error = VenueError::new("Lighter positions: a row could not be mapped");
                        self.account.fresh_positions.failed(cx, 0, read, &error);
                    } else if !held {
                        self.account.fresh_positions.committed(cx, 0, read, &[]);
                    }
                } else {
                    self.trade_account_ready(cx, serial, false);
                    cx.notice(
                        Level::Warn,
                        code::SNAPSHOT_DISCARDED,
                        "Lighter account snapshot failed; preserving account state",
                    );
                    let error = snapshot_read_error(&response, ok);
                    let read = serial as u32;
                    self.account.fresh_positions.failed(cx, 0, read, &error);
                }
                self.read_fresh_positions_again(cx, serial as u32);
            }
            Request::Orders {
                revision,
                market,
                orders,
                coverage,
                mut census,
                valid,
            } => {
                match cx.order_read_mark() {
                    Some(mark) => {
                        census.read =
                            Some(census.read.map_or(mark, |earliest| earliest.earliest(mark)));
                    }
                    None => census.lost = true,
                }
                if ok && valid {
                    if let Some(batch) = self.account.batch.as_mut() {
                        batch.orders.extend(orders);
                        if let Some(market) = batch.markets.pop_front() {
                            let revision = batch.revision;
                            // The batch's skipped rows ride along to its commit.
                            self.request_orders(cx, Some(market), revision, coverage, census);
                        } else {
                            let batch = self.account.batch.take().unwrap();
                            self.commit_orders(
                                cx,
                                batch.revision,
                                batch.orders,
                                &coverage,
                                census,
                                true,
                            );
                            self.orders_settled(cx);
                        }
                    } else {
                        self.commit_orders(cx, revision, orders, &coverage, census, false);
                        self.orders_settled(cx);
                    }
                } else if market.is_none()
                    && (status.as_ref().is_some_and(|s| {
                        s.message.to_ascii_lowercase().contains("market")
                            || s.message.to_ascii_lowercase().contains("invalid param")
                    }))
                {
                    let mut markets = cx
                        .store
                        .markets()
                        .iter()
                        .map(|m| m.id.clone())
                        .collect::<VecDeque<_>>();
                    if let Some(market) = markets.pop_front() {
                        self.account.batch = Some(OrderBatch {
                            revision,
                            markets,
                            orders: Vec::new(),
                        });
                        // The failed all-market read stays the census's earliest leg.
                        let census = CensusRead {
                            observed: Vec::new(),
                            ..census
                        };
                        self.request_orders(
                            cx,
                            Some(market),
                            revision,
                            SnapshotCoverage::default(),
                            census,
                        );
                    } else {
                        self.orders_settled(cx);
                    }
                } else {
                    self.account.batch = None;
                    self.orders_settled(cx);
                    cx.notice(
                        Level::Warn,
                        code::SNAPSHOT_DISCARDED,
                        "Lighter order snapshot failed; preserving all orders",
                    );
                }
            }
        }
    }
    pub(crate) fn account_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if let Request::Orders {
            valid,
            orders,
            coverage,
            census,
            market,
            ..
        } = &mut request
        {
            #[derive(Deserialize)]
            struct Envelope<'a> {
                #[serde(default, borrow)]
                orders: Option<&'a RawValue>,
            }
            *valid = false;
            if let Ok(envelope) = serde_json::from_str::<Envelope<'_>>(response.body)
                && let Some(raw) = envelope.orders
                && raw.get().starts_with('{')
            {
                let result = each_row(raw, |row: &RawValue, hint| {
                    self.snapshot_order(
                        cx,
                        row,
                        hint.or(market.as_deref()),
                        orders,
                        coverage,
                        census,
                    )
                });
                *valid = result.is_ok();
            }
        }
        self.account_response(cx, request, response);
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        match request {
            Request::Limits { started_at } => self.limits_failed(cx, started_at),
            Request::Backfill { run, .. } => self.backfill_failed(cx, run, error.into()),
            Request::Lookup { candidates, .. } => {
                self.account.lookup_failed = true;
                self.next_lookup(cx, candidates);
            }
            Request::Snapshot { serial, .. } => {
                self.account.snapshot_inflight = false;
                self.trade_account_ready(cx, serial, false);
                cx.notice(
                    Level::Warn,
                    code::REQUEST_FAILED,
                    format!("Lighter account snapshot failed: {error}"),
                );
                let fresh = self
                    .account
                    .read_error
                    .take()
                    .unwrap_or_else(|| VenueError::new(error));
                let read = serial as u32;
                self.account.fresh_positions.failed(cx, 0, read, &fresh);
                self.read_fresh_positions_again(cx, read);
            }
            Request::Orders { .. } => {
                self.account.batch = None;
                self.orders_settled(cx);
                cx.notice(
                    Level::Warn,
                    code::REQUEST_FAILED,
                    format!("Lighter order snapshot failed: {error}"),
                );
            }
        }
    }
    /// `readPositions`: wait on the next read. One already out may predate the
    /// close, so it is followed by a new read and never answers.
    pub(crate) fn read_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.account_index.is_none() {
            let error =
                VenueError::new("Lighter account is not loaded").kind(ErrorKind::InvalidInput);
            fresh_positions::refuse(cx, error);
            return;
        }
        if cx.now < self.account.polls_held_until {
            let wait = self.account.polls_held_until - cx.now;
            fresh_positions::refuse(cx, fresh_positions::rate_limited(FRESH_PAUSED, wait));
            return;
        }
        let need = self.account.snapshot_serial as u32 + 1;
        if self.account.fresh_positions.admit(cx, vec![(0, need)]) {
            self.refresh_account(cx);
        }
    }

    /// At once for a waiter the read predates, else once a newer stream frame
    /// that held the read can have been overtaken.
    fn read_fresh_positions_again(&mut self, cx: &mut Cx<'_, Pending>, read: u32) {
        if self.account.fresh_positions.predates(0, read) {
            self.retry_fresh_positions(cx);
        } else if self.account.fresh_positions.owed(0)
            && self.account.fresh_positions_timer.is_none()
        {
            self.account.fresh_positions_timer = Some(cx.set_timer(FRESH_POSITIONS_RETRY_MS));
        }
    }

    fn retry_fresh_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.now < self.account.polls_held_until {
            let wait = self.account.polls_held_until - cx.now;
            let error = fresh_positions::rate_limited(FRESH_PAUSED, wait);
            self.account.fresh_positions.failed(cx, 0, u32::MAX, &error);
            return;
        }
        self.refresh_account(cx);
    }

    pub(crate) fn account_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.account.connection.stream.socket != Some(socket) {
            return false;
        }
        let Ok(frame) = serde_json::from_str::<Frame<'_>>(text) else {
            cx.notice(
                Level::Warn,
                code::STREAM_MALFORMED,
                "Invalid Lighter private frame",
            );
            return true;
        };
        if frame.kind == "ping" {
            cx.send(
                socket,
                ControlFrame::heartbeat(format!(
                    r#"{{"type":"pong","timestamp":{}}}"#,
                    cx.protocol_now()
                )),
            );
            return true;
        }
        let account = self.account.account_index.unwrap().to_string();
        let channel = frame.channel.replace(':', "/");
        if channel != "account_all"
            && channel != "account_all_orders"
            && channel != format!("account_all/{account}")
            && channel != format!("account_all_orders/{account}")
        {
            return true;
        }
        let nested = frame
            .data
            .and_then(|data| serde_json::from_str::<Payload<'_>>(data.get()).ok());
        let direct = Payload {
            orders: frame.orders,
            positions: frame.positions,
            trades: frame.trades,
            fills: frame.fills,
            account: frame.account,
            balances: frame.balances,
        };
        let payload = nested.as_ref().unwrap_or(&direct);
        let meaningful = payload.orders.is_some()
            || payload.positions.is_some()
            || payload.fills.is_some()
            || payload.trades.is_some()
            || payload.account.is_some()
            || payload.balances.is_some();
        if meaningful || frame.kind.starts_with("subscribed") {
            self.account.baselined = true;
        }
        if meaningful {
            self.account.connection.last_payload = Some(cx.now);
            self.account.connection.recovery_started = None;
            self.account.connection.read_only_failures = 0;
            self.account.revision += 1;
            self.schedule_heal(cx);
        }
        if let Some(raw) = payload.orders {
            let result = each_row(raw, |row: &RawValue, hint| {
                if let Ok(Some(mut order)) = self.account_map_order(cx, row, hint) {
                    if let Some(time) =
                        event_time(frame.timestamp.as_ref(), frame.transaction_time.as_ref())
                    {
                        order.last_update = time;
                    }
                    if self
                        .account
                        .order_source
                        .get(&order.id)
                        .is_some_and(|t| *t > order.last_update)
                    {
                        return;
                    }
                    self.account
                        .order_source
                        .insert(order.id.clone(), order.last_update);
                    let client_index = client_order_index(row);
                    self.trade_order_observed(
                        cx,
                        &order.id,
                        client_index.as_deref(),
                        order.filled,
                        order.status,
                    );
                    if self.account.suppressed.contains(&order.id) {
                        return;
                    }
                    if let Some(old) = cx.store.orders().iter().find(|o| o.id == order.id) {
                        order.filled = order.filled.max(old.filled);
                        if matches!(old.status, OrderStatus::Closed | OrderStatus::Canceled)
                            && order.status == OrderStatus::Open
                        {
                            return;
                        }
                    }
                    self.account
                        .order_revision
                        .insert(order.id.clone(), self.account.revision);
                    if order.status == OrderStatus::Canceled
                        && cx.now - order.last_update <= 5_000.0
                        && cx.now - self.account.connection.connected_at > 5_000.0
                        && let Some(error) = rejection_error(row, &order)
                    {
                        cx.notice_error(Level::Error, code::ORDER_REJECTED, &error);
                    }
                    if matches!(order.status, OrderStatus::Canceled | OrderStatus::Closed) {
                        self.protection.originals.remove(&order.id);
                        self.protection.complex.remove(&order.id);
                        self.account.suppressed.insert(order.id.clone());
                        self.account.records.remove(&order.id);
                        if order.status == OrderStatus::Canceled {
                            cx.store.remove_orders(&[&order.id]);
                        } else {
                            cx.store.upsert_order(order);
                        }
                    } else {
                        self.observe_original(cx.store, row.get(), &order);
                        cx.store.upsert_order(order);
                    }
                }
            });
            if let Err(e) = result {
                cx.notice(Level::Warn, code::STREAM_MALFORMED, e);
            }
        }
        if let Some(raw) = payload.positions {
            let result = each_row(raw, |wire: PositionWire<'_>, hint| {
                let time = event_time(
                    wire.updated_at.as_ref().or(wire.timestamp.as_ref()),
                    wire.transaction_time.as_ref(),
                )
                .or_else(|| event_time(frame.timestamp.as_ref(), frame.transaction_time.as_ref()))
                .unwrap_or(cx.now);
                let explicit_zero = wire.size.as_ref().is_some_and(|n| n.0 == 0.0);
                if let Some((position, symbol, leverage, margin)) =
                    self.position_row(cx, wire, hint)
                {
                    if self
                        .account
                        .position_source
                        .get(&symbol)
                        .is_some_and(|(old, closed)| {
                            *old > time || (*old == time && *closed && position.is_some())
                        })
                    {
                        return;
                    }
                    self.account
                        .position_source
                        .insert(symbol.clone(), (time, explicit_zero));
                    self.account
                        .position_revision
                        .insert(symbol.clone(), self.account.revision);
                    self.account
                        .settings_revision
                        .insert(symbol.clone(), self.account.revision);
                    cx.store.update_setting(&symbol, |s| {
                        s.leverage = leverage;
                        s.margin_type = Some(margin);
                    });
                    if let Some(position) = position {
                        cx.store.upsert_position(position);
                    } else {
                        cx.store.update_positions(|p| {
                            if p.symbol == symbol {
                                p.contracts = 0.0;
                            }
                        });
                    }
                }
            });
            if let Err(e) = result {
                cx.notice(Level::Warn, code::STREAM_MALFORMED, e);
            }
        }
        let live_after = if frame.kind.starts_with("subscribed") {
            self.account.live_after.unwrap_or(f64::INFINITY)
        } else {
            f64::NEG_INFINITY
        };
        for raw in [payload.fills, payload.trades].into_iter().flatten() {
            let result = each_row(raw, |trade: TradeWire<'_>, hint| {
                if let Some(execution) = self.execution(cx, &trade, hint, &account) {
                    self.observe_execution(cx, execution, live_after);
                }
            });
            if let Err(e) = result {
                cx.notice(Level::Warn, code::STREAM_MALFORMED, e);
            }
        }
        true
    }
    fn execution(
        &self,
        cx: &Cx<'_, Pending>,
        trade: &TradeWire<'_>,
        hint: Option<&str>,
        account: &str,
    ) -> Option<Execution> {
        let execution_id = [&trade.trade_id_str, &trade.trade_id, &trade.fill_id]
            .into_iter()
            .map(|id| id.0.as_ref())
            .find(|id| !id.is_empty())?;
        let market = if trade.market_id.0.is_empty() {
            hint.unwrap_or_default()
        } else {
            &trade.market_id.0
        };
        let symbol = if !trade.symbol.is_empty() {
            trade.symbol.as_ref()
        } else {
            self.symbol_for_id(market)?
        };
        let side = if trade.ask_account_id.0 == account {
            OrderSide::Sell
        } else if trade.bid_account_id.0 == account {
            OrderSide::Buy
        } else {
            if !trade.ask_account_id.0.is_empty() || !trade.bid_account_id.0.is_empty() {
                return None;
            }
            match trade.side.to_ascii_uppercase().as_str() {
                "BUY" => OrderSide::Buy,
                "SELL" => OrderSide::Sell,
                _ => {
                    if trade.is_ask.as_ref()?.get() {
                        OrderSide::Sell
                    } else {
                        OrderSide::Buy
                    }
                }
            }
        };
        if trade.price.0 <= 0.0 || trade.size.0 <= 0.0 {
            return None;
        }
        let (id, id_str) = if side == OrderSide::Sell {
            (&trade.ask_id, &trade.ask_id_str)
        } else {
            (&trade.bid_id, &trade.bid_id_str)
        };
        let order = [&trade.order_id, id_str, id]
            .into_iter()
            .map(|id| id.0.as_ref())
            .find(|id| !id.is_empty())?;
        // A venue clock ahead of the host is still a real execution; keep its time.
        let timestamp = venue_millis(trade.timestamp.0);
        if timestamp <= 0.0 {
            return None;
        }
        let market = cx.store.market(symbol).map_or(market, |m| m.id.as_str());
        Some(Execution {
            id: format!("{market}:{execution_id}"),
            timestamp,
            fill: Fill {
                id: None,
                order_id: order.into(),
                symbol: symbol.into(),
                side,
                price: trade.price.0,
                amount: trade.size.0,
            },
        })
    }
    /// Records an execution once; it is emitted unless it predates `live_after`.
    fn observe_execution(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        execution: Execution,
        live_after: f64,
    ) {
        if !self.account.seen_fills.insert(execution.id.clone()) {
            return;
        }
        self.account.fill_queue.push_back(execution.id);
        while self.account.fill_queue.len() > 8192 {
            if let Some(id) = self.account.fill_queue.pop_front() {
                self.account.seen_fills.remove(&id);
            }
        }
        self.account.newest_fill = self.account.newest_fill.max(execution.timestamp);
        // Subscription history is baseline; delayed update executions remain live.
        if execution.timestamp < live_after {
            return;
        }
        self.trade_execution_observed(&execution.fill.order_id, execution.timestamp);
        cx.fill(execution.fill);
    }
    /// Account payloads stopped at `quiet_since`; executions after it may be
    /// missing until a backfill completes. Only after a baseline, so pre-session
    /// history is never replayed as new.
    fn arm_outage(&mut self, quiet_since: f64) {
        if !self.account.baselined {
            return;
        }
        let earliest = self
            .account
            .outage
            .map_or(quiet_since, |o| o.min(quiet_since));
        self.account.outage = Some(earliest);
        self.account.outage_epoch += 1;
    }
    /// Executions at or after this time are emitted from history, once.
    fn cutoff(&self, outage: f64) -> f64 {
        // Up to 10 s before the outage absorbs clock skew; seen_fills dedupes.
        let floor = (outage - 10_000.0).max(self.account.session_start.unwrap_or(f64::MIN));
        self.account.newest_fill.clamp(floor, outage.max(floor))
    }
    fn backfill_failed(&mut self, cx: &mut Cx<'_, Pending>, mut run: Backfill, error: String) {
        if run.retried {
            BackgroundCoverage::Incomplete.report(cx,
                format!("Lighter fill backfill failed; fills during the outage may be missing until the next reconnect: {error}"),
            );
            return;
        }
        cx.notice(
            Level::Info,
            code::REQUEST_FAILED,
            format!("Lighter fill backfill failed, retrying: {error}"),
        );
        run.retried = true;
        if let Some((timer, _)) = self.account.backfill_retry.take() {
            cx.clear_timer(timer);
        }
        self.account.backfill_retry = Some((cx.set_timer(2_000.0), run));
    }
    fn request_backfill(&mut self, cx: &mut Cx<'_, Pending>, run: Backfill) {
        let account = self.account.account_index.unwrap();
        let mut url = format!(
            "{}/api/v1/trades?limit=100&account_index={account}&sort_by=timestamp&sort_dir=desc&aggregate=false",
            self.host()
        );
        if let Some(cursor) = &run.cursor {
            url.push_str(&format!("&cursor={}", safe_cex_engine::url::encode(cursor)));
        }
        self.account_request(
            cx,
            url,
            Some(json::ArrayAt::Field("trades")),
            Request::Backfill {
                run,
                executions: Vec::new(),
                oldest: f64::INFINITY,
                count: 0,
            },
            Some(sign::AuthPlacement::Header),
        );
    }
}

/// The reply's own error for a positions read that cannot answer.
fn snapshot_read_error(response: &Response<'_>, ok: bool) -> VenueError {
    if ok {
        return VenueError::new("Lighter positions: malformed account snapshot");
    }
    let error = VenueError::new(format!(
        "Lighter positions HTTP {}: {}",
        response.status, response.body
    ));
    if matches!(response.status, 405 | 429) {
        error.kind(ErrorKind::RateLimited)
    } else {
        error
    }
}

impl Request {
    pub(crate) fn startup_sections(&self) -> safe_cex_engine::startup::Sections {
        use safe_cex_engine::startup::Sections as S;
        match self {
            Self::Limits { .. } => S::ACCOUNT,
            Self::Lookup { .. } => S::ACCOUNT,
            Self::Snapshot { .. } => S::BALANCE.union(S::POSITIONS),
            Self::Orders { .. } => S::ORDERS,
            _ => S::NONE,
        }
    }
}
