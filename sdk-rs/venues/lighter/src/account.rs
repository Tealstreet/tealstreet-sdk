//! Account snapshots are committed only after every scoped read succeeds.
use crate::{Lighter, Pending, sign};
use safe_cex_engine::de::number;
use safe_cex_engine::model::*;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, Response, SocketId, TimerId, json};
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
pub enum Request {
    Limits {
        started_at: f64,
    },
    ChangeTier {
        command: safe_cex_engine::CommandId,
    },
    Lookup {
        candidates: VecDeque<Lookup>,
        index: Option<u64>,
    },
    Snapshot {
        revision: u64,
        account: Option<Snapshot>,
        valid: bool,
    },
    Orders {
        revision: u64,
        market: Option<String>,
        orders: Vec<Order>,
        valid: bool,
    },
}
#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    last_received: f64,
    connected_at: f64,
    last_payload: Option<f64>,
    last_recovery: Option<f64>,
    recovery_started: Option<f64>,
    watchdog: Option<TimerId>,
    reconnect: Option<TimerId>,
    attempts: u32,
}
struct OrderBatch {
    revision: u64,
    markets: VecDeque<String>,
    orders: Vec<Order>,
}
#[derive(Default)]
pub struct Account {
    pub account_index: Option<u64>,
    pub api_key_index: Option<u8>,
    pub signer: Option<sign::Signer>,
    setting_epochs: HashMap<String, u64>,
    pub records: HashMap<String, Order>,
    connection: Connection,
    started: bool,
    revision: u64,
    position_revision: HashMap<String, u64>,
    position_source: HashMap<String, (f64, bool)>,
    order_source: HashMap<String, f64>,
    order_revision: HashMap<String, u64>,
    settings_revision: HashMap<String, u64>,
    suppressed: HashSet<String>,
    pub snapshot_inflight: bool,
    orders_inflight: bool,
    batch: Option<OrderBatch>,
    sync: Option<TimerId>,
    heal: Option<TimerId>,
    seen_fills: HashSet<String>,
    fill_queue: VecDeque<String>,
}
#[derive(Default)]
pub struct Snapshot {
    positions: Vec<Position>,
    settings: Vec<(String, f64, MarginType)>,
    known_settings: HashSet<String>,
    equity: f64,
    free: f64,
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
    #[serde(default, borrow, alias = "account_index", alias = "accountIndex")]
    index: Option<Atom<'a>>,
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
    #[serde(default, alias = "unrealizedPnl")]
    unrealized_pnl: Num,
    #[serde(default, alias = "realizedPnl")]
    realized_pnl: Option<Num>,
    #[serde(default, alias = "initialLeverage", alias = "initial_leverage")]
    leverage: Option<Num>,
    #[serde(default, alias = "initialMarginFraction")]
    initial_margin_fraction: Option<Num>,
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
    #[serde(default, alias = "updatedAt", alias = "timestamp")]
    updated_at: Option<Num>,
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
        alias = "MarketIndex"
    )]
    market_id: Atom<'a>,
    #[serde(default, borrow, alias = "marketSymbol")]
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
    #[serde(default, alias = "updatedAt", alias = "timestamp")]
    updated_at: Option<Num>,
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
// Account positions use ratio, percent, or integer fraction ticks. Public
// market risk metadata keeps its separate fixed-tick contract in `ratio`.
fn position_margin_ratio(value: f64) -> Option<f64> {
    if !value.is_finite() || value <= 0.0 {
        None
    } else if value > 100.0 {
        Some(value / 10000.0)
    } else if value > 1.0 {
        Some(value / 100.0)
    } else {
        Some(value)
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
fn event_time(updated: Option<&Num>, transaction: Option<&Num>) -> Option<f64> {
    transaction
        .filter(|n| n.0 > 0.0)
        .map(|n| {
            if n.0 >= 100_000_000_000_000.0 {
                n.0 / 1000.0
            } else {
                millis(n.0)
            }
        })
        .or_else(|| updated.filter(|n| n.0 > 0.0).map(|n| millis(n.0)))
}

impl Lighter {
    pub(crate) fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.started {
            return;
        }
        self.account.started = true;
        cx.store.set_hedged(false);
        for m in cx.store.markets().to_vec() {
            let mode = self.metadata.get(&m.symbol).map_or(0, |v| v.margin_mode);
            cx.store.update_setting(&m.symbol, |s| {
                s.leverage = m.limits.leverage.max;
                s.margin_type = Some(if mode == 1 {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                });
            });
        }
        if self.options.public_data_only
            || (self.options.secret.as_deref().is_none_or(str::is_empty)
                && !self.options.remote_signer)
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
        self.account.api_key_index = self
            .options
            .api_key_index
            .as_deref()
            .and_then(|s| s.parse::<u8>().ok());
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
        for source in [Some(app), self.options.key.as_deref()]
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
        if let Some(key) = self.options.api_key_index.as_deref().and_then(index) {
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
        self.next_lookup(cx, candidates);
    }
    fn next_lookup(&mut self, cx: &mut Cx<'_, Pending>, mut candidates: VecDeque<Lookup>) {
        if let Some(candidate) = candidates.pop_front() {
            let path = if candidate.address {
                format!("accountsByL1Address?l1_address={}", candidate.value)
            } else {
                format!("account?by={}&value={}", candidate.by, candidate.value)
            };
            cx.request_streamed(
                Method::Get,
                format!("{}/api/v1/{path}", self.host()),
                vec![],
                json::ArrayAt::Envelope(&[
                    &["accounts"],
                    &["sub_accounts"],
                    &["data", "accounts"],
                    &["data", "sub_accounts"],
                ]),
                Pending::Account(Request::Lookup {
                    candidates,
                    index: None,
                }),
            );
        } else {
            cx.log(
                LogLevel::Error,
                "Failed to resolve Lighter account index. Provide account index in Application ID.",
            );
        }
    }
    fn identity_ready(&mut self, cx: &mut Cx<'_, Pending>, account: u64) {
        self.account.account_index = Some(account);
        let signer = if self.options.secret.as_deref().is_none_or(str::is_empty) {
            Err("Lighter private key is missing".into())
        } else if self.account.api_key_index.is_none() {
            Err("Invalid API key index. Expected an integer from 0 to 255.".into())
        } else if let Some(provider) = self.options.signer_provider.as_deref()
            && !provider.is_empty()
            && provider != "tealstreet-sdk"
        {
            Err(format!(
                "Unsupported Lighter signer provider \"{provider}\". Legacy third-party signer providers are removed; use tealstreet-sdk."
            ))
        } else {
            sign::Signer::new(
                self.options.secret.as_deref().unwrap_or_default(),
                self.chain_id(),
                account,
                self.account.api_key_index.unwrap_or_default(),
            )
        };
        match signer {
            Ok(signer) => {
                self.account.signer = Some(signer);
                self.connect_private(cx);
            }
            Err(error) => cx.log(
                LogLevel::Warning,
                format!("Lighter signer unavailable at startup: {error}"),
            ),
        }
        self.refresh_account(cx);
        self.refresh_orders(cx);
        self.account.sync = Some(cx.set_timer(2_000.0));
    }

    pub(crate) fn account_auth(&self, now: f64) -> Result<String, String> {
        self.account
            .signer
            .as_ref()
            .ok_or("Lighter signer unavailable")?
            .auth_token((now / 1000.0).floor() as u64 + 60)
    }
    pub(crate) fn refresh_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.snapshot_inflight {
            return;
        }
        let Some(account) = self.account.account_index else {
            return;
        };
        self.account.snapshot_inflight = true;
        cx.request_streamed(
            Method::Get,
            format!("{}/api/v1/account?by=index&value={account}", self.host()),
            vec![],
            json::ArrayAt::Field("accounts"),
            Pending::Account(Request::Snapshot {
                revision: self.account.revision,
                account: None,
                valid: true,
            }),
        );
    }
    pub(crate) fn refresh_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.orders_inflight || self.account.signer.is_none() {
            return;
        }
        if self.account.account_index.is_none() {
            return;
        }
        self.account.orders_inflight = true;
        self.request_orders(cx, None, self.account.revision);
    }
    fn request_orders(&mut self, cx: &mut Cx<'_, Pending>, market: Option<String>, revision: u64) {
        let auth = match self.account_auth(cx.now) {
            Ok(a) => a,
            Err(e) => {
                self.account.orders_inflight = false;
                cx.log(LogLevel::Warning, e);
                return;
            }
        };
        let id = market.as_deref().unwrap_or("255");
        let account = self.account.account_index.unwrap();
        cx.request_streamed(
            Method::Get,
            format!(
                "{}/api/v1/accountActiveOrders?account_index={account}&market_id={id}&auth={auth}",
                self.host()
            ),
            vec![],
            json::ArrayAt::Envelope(&[
                &["orders"],
                &["data", "orders"],
                &["result", "orders"],
                &["data"],
                &["result"],
                &[],
            ]),
            Pending::Account(Request::Orders {
                revision,
                market,
                orders: Vec::new(),
                valid: true,
            }),
        );
    }
    pub(crate) fn account_order_terminal(&self, id: &str) -> bool {
        self.account.suppressed.contains(id)
    }
    pub(crate) fn record_cancel(&mut self, id: &str, _now: f64) {
        self.account.revision += 1;
        self.account
            .order_revision
            .insert(id.into(), self.account.revision);
        self.account.suppressed.insert(id.into());
        self.account.records.remove(id);
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
    fn connect_private(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.connection.open = false;
        self.account.connection.last_received = cx.now;
        self.account.connection.socket =
            Some(cx.connect_channel(self.ws_url(), safe_cex_engine::SocketChannel::Private));
        if let Some(timer) = self.account.connection.watchdog.take() {
            cx.clear_timer(timer);
        }
        self.account.connection.watchdog = Some(cx.set_timer(10_000.0));
    }
    fn subscribe_private(&self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let account = self.account.account_index.unwrap();
        cx.send(
            socket,
            format!(r#"{{"type":"subscribe","channel":"account_all/{account}"}}"#),
        );
        match self.account_auth(cx.now){Ok(auth)=>cx.send(socket,format!(r#"{{"type":"subscribe","channel":"account_all_orders/{account}","auth":"{auth}"}}"#)),Err(e)=>cx.log(LogLevel::Warning,e)}
    }
    pub(crate) fn account_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.connection.socket != Some(socket) {
            return false;
        }
        self.account.connection.open = true;
        if let Some(timer) = self.account.connection.watchdog.take() {
            cx.clear_timer(timer);
        }
        self.account.connection.last_received = cx.now;
        self.account.connection.attempts = 0;
        self.account.connection.connected_at = cx.now;
        self.account.connection.last_payload = None;
        self.account.connection.last_recovery = None;
        self.account.connection.recovery_started = None;
        self.subscribe_private(cx, socket);
        cx.send(
            socket,
            format!(r#"{{"type":"ping","timestamp":{}}}"#, cx.now),
        );
        self.account.connection.watchdog = Some(cx.set_timer(10_000.0));
        self.refresh_account(cx);
        self.refresh_orders(cx);
        true
    }
    pub(crate) fn account_close(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        code: u16,
    ) -> bool {
        if self.account.connection.socket != Some(socket) {
            return false;
        }
        self.account.connection.open = false;
        self.account.connection.socket = None;
        if let Some(t) = self.account.connection.watchdog.take() {
            cx.clear_timer(t);
        }
        self.account.connection.attempts = self.account.connection.attempts.saturating_add(1);
        let delay = if code == 1013 {
            30_000.0
        } else {
            (1000.0 * 2f64.powi(self.account.connection.attempts.min(5) as i32 - 1)).min(30_000.0)
        };
        self.account.connection.reconnect = Some(cx.set_timer(delay));
        true
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.connection.reconnect == Some(timer) {
            self.account.connection.reconnect = None;
            self.connect_private(cx);
            return true;
        }
        if self.account.connection.watchdog == Some(timer) {
            self.account.connection.watchdog = None;
            if let Some(socket) = self.account.connection.socket {
                if cx.now - self.account.connection.last_received >= 180_000.0 {
                    cx.close(socket);
                    self.account_close(cx, socket, 1006);
                } else {
                    if self.account.connection.open {
                        let connection = &self.account.connection;
                        if connection
                            .recovery_started
                            .is_some_and(|at| cx.now - at >= 20_000.0)
                        {
                            cx.close(socket);
                            self.account_close(cx, socket, 1006);
                            return true;
                        }
                        let payload_age =
                            cx.now - connection.last_payload.unwrap_or(connection.connected_at);
                        let stale_after = if connection.last_payload.is_some() {
                            120_000.0
                        } else {
                            180_000.0
                        };
                        if payload_age >= stale_after
                            && connection.recovery_started.is_none()
                            && connection
                                .last_recovery
                                .is_none_or(|at| cx.now - at >= 180_000.0)
                        {
                            self.account.connection.last_recovery = Some(cx.now);
                            self.account.connection.recovery_started = Some(cx.now);
                            self.subscribe_private(cx, socket);
                        }
                        cx.send(
                            socket,
                            format!(r#"{{"type":"ping","timestamp":{}}}"#, cx.now),
                        );
                    }
                    self.account.connection.watchdog = Some(cx.set_timer(10_000.0));
                }
            }
            return true;
        }
        if self.account.sync == Some(timer) {
            self.account.sync = Some(cx.set_timer(2_000.0));
            self.refresh_account(cx);
            self.refresh_orders(cx);
            self.refresh_limits(cx);
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
        cx: &Cx<'_, Pending>,
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
                    .and_then(|n| position_margin_ratio(n.0))
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
            .unwrap_or(market.limits.leverage.max);
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
        Some((
            Some(Position {
                id: symbol.into(),
                symbol: symbol.into(),
                side,
                entry_price: entry,
                notional: contracts * entry,
                leverage,
                margin_type: Some(margin_type),
                position_mode: Some(PositionMode::Oneway),
                unrealized_pnl: wire.unrealized_pnl.0,
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
    fn snapshot_row(
        &self,
        cx: &Cx<'_, Pending>,
        wire: AccountWire<'_>,
    ) -> Result<Snapshot, String> {
        if let Some(identity) = wire.index.as_ref()
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
            each_row(raw, |row: PositionWire<'_>, hint| {
                let symbol = if !row.symbol.is_empty() {
                    Some(row.symbol.as_ref())
                } else {
                    self.symbol_for_id(if row.market_id.0.is_empty() {
                        hint.unwrap_or_default()
                    } else {
                        &row.market_id.0
                    })
                };
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
                            .is_some_and(|n| n.0 > 0.0));
                if let Some((position, symbol, leverage, margin)) = self.position_row(cx, row, hint)
                {
                    if known {
                        snapshot.known_settings.insert(symbol.clone());
                    }
                    snapshot.settings.push((symbol, leverage, margin));
                    if let Some(position) = position {
                        snapshot.positions.push(position);
                    }
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
        #[derive(Deserialize)]
        struct Nested<'a> {
            #[serde(default, borrow)]
            order: Option<&'a RawValue>,
        }
        let nested: Nested<'_> = serde_json::from_str(raw.get()).map_err(|e| e.to_string())?;
        let wire: OrderWire<'_> =
            serde_json::from_str(nested.order.unwrap_or(raw).get()).map_err(|e| e.to_string())?;
        let Some(id) = wire
            .order_id
            .as_ref()
            .or(wire.order_index.as_ref())
            .and_then(|a| index(&a.0))
            .map(|n| n.to_string())
        else {
            return Err("Lighter order has no valid native order id".into());
        };
        let market_id = if wire.market_id.0.is_empty() {
            market_hint.unwrap_or_default()
        } else {
            &wire.market_id.0
        };
        let symbol = if !wire.symbol.is_empty() {
            wire.symbol.as_ref()
        } else {
            match self.symbol_for_id(market_id) {
                Some(s) => s,
                None => return Ok(None),
            }
        };
        if cx.store.market(symbol).is_none() {
            return Ok(None);
        }
        let existing = cx
            .store
            .orders()
            .iter()
            .find(|o| o.id == id)
            .or_else(|| self.account.records.get(&id));
        let mut kind = match normalized(&wire.kind.0).as_str() {
            "1" | "MARKET" => OrderType::Market,
            "2" | "3" | "STOP_LOSS" | "STOP_MARKET" | "STOP_LIMIT" | "STOP_LOSS_LIMIT" => {
                OrderType::StopLoss
            }
            "4" | "5" | "TAKE_PROFIT" | "TAKE_PROFIT_MARKET" | "TAKE_PROFIT_LIMIT" => {
                OrderType::TakeProfit
            }
            _ => OrderType::Limit,
        };
        if kind == OrderType::Limit
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
        if kind == OrderType::Limit
            && reduce_only
            && price > 0.0
            && let Some(t) = cx.store.ticker(symbol)
        {
            let reference = [t.mark, t.last, t.index].into_iter().find(|v| *v > 0.0);
            if reference.is_some_and(|reference| {
                if side == OrderSide::Sell {
                    price <= reference
                } else {
                    price >= reference
                }
            }) {
                kind = OrderType::StopLoss;
            }
        }
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
            client_order_id: None,
            time_in_force: tif,
            last_update: event_time(wire.updated_at.as_ref(), wire.transaction_time.as_ref())
                .unwrap_or(cx.now),
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: Some(OrderExtra {
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
            Request::Limits { .. } | Request::ChangeTier { .. } => {}
            Request::Lookup { index: found, .. } => {
                if found.is_none()
                    && let Ok(wire) = serde_json::from_str::<AccountWire<'_>>(item)
                {
                    *found = wire.index.and_then(|n| index(&n.0));
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
                valid,
                market,
                ..
            } => {
                match serde_json::from_str::<&RawValue>(item)
                    .map_err(|e| e.to_string())
                    .and_then(|raw| self.account_map_order(cx, raw, market.as_deref()))
                {
                    Ok(Some(order)) => orders.push(order),
                    Ok(None) => {}
                    Err(_) => *valid = false,
                }
            }
        }
    }
    fn commit_snapshot(&mut self, cx: &mut Cx<'_, Pending>, revision: u64, snapshot: Snapshot) {
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
        positions.extend(
            cx.store
                .positions()
                .iter()
                .filter(|p| {
                    self.account
                        .position_revision
                        .get(&p.symbol)
                        .copied()
                        .unwrap_or(0)
                        > revision
                })
                .cloned(),
        );
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
                    old.map_or(m.limits.leverage.max, |s| s.leverage),
                    old.and_then(|s| s.margin_type).unwrap_or(MarginType::Cross),
                )
            });
            cx.store.update_setting(&m.symbol, |s| {
                s.leverage = leverage;
                s.margin_type = Some(margin);
            });
        }
        {
            let upnl = cx
                .store
                .positions()
                .iter()
                .map(|p| {
                    cx.store.ticker(&p.symbol).map_or(0.0, |t| {
                        if p.side == PositionSide::Long {
                            (t.last - p.entry_price) * p.contracts
                        } else {
                            (p.entry_price - t.last) * p.contracts
                        }
                    })
                })
                .sum::<f64>();
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
    }
    fn commit_orders(&mut self, cx: &mut Cx<'_, Pending>, revision: u64, orders: Vec<Order>) {
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
                    self.account.order_revision.get(&o.id).copied().unwrap_or(0) > revision
                        && !self.account.suppressed.contains(&o.id)
                })
                .cloned(),
        );
        let active = merged.iter().map(|o| o.id.as_str()).collect::<HashSet<_>>();
        self.account
            .records
            .retain(|id, _| active.contains(id.as_str()));
        cx.store.set_orders(merged);
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let status = serde_json::from_str::<Status<'_>>(response.body).ok();
        let ok = (200..300).contains(&response.status)
            && (status.as_ref().is_some_and(Status::valid)
                || response.body.trim_start().starts_with('['));
        match request {
            Request::Limits { started_at } => self.limits_response(cx, started_at, response),
            Request::ChangeTier { command } => self.tier_changed(cx, command, response),
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
                        .and_then(|a| a.index)
                        .and_then(|a| index(&a.0))
                };
                if ok && let Some(index) = found {
                    self.identity_ready(cx, index);
                } else {
                    self.next_lookup(cx, candidates);
                }
            }
            Request::Snapshot {
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
                    self.commit_snapshot(cx, revision, account);
                    if self.account.signer.is_none() && !cx.store.loaded().orders {
                        cx.store.set_orders(vec![]);
                    }
                    self.trade_kick(cx);
                } else {
                    cx.log(
                        LogLevel::Warning,
                        "Lighter account snapshot failed; preserving account state",
                    );
                }
            }
            Request::Orders {
                revision,
                market,
                orders,
                valid,
            } => {
                if ok && valid {
                    if let Some(batch) = self.account.batch.as_mut() {
                        batch.orders.extend(orders);
                        if let Some(market) = batch.markets.pop_front() {
                            let revision = batch.revision;
                            self.request_orders(cx, Some(market), revision);
                        } else {
                            let batch = self.account.batch.take().unwrap();
                            self.account.orders_inflight = false;
                            self.commit_orders(cx, batch.revision, batch.orders);
                        }
                    } else {
                        self.account.orders_inflight = false;
                        self.commit_orders(cx, revision, orders);
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
                        self.request_orders(cx, Some(market), revision);
                    } else {
                        self.account.orders_inflight = false;
                    }
                } else {
                    self.account.orders_inflight = false;
                    self.account.batch = None;
                    cx.log(
                        LogLevel::Warning,
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
                let mut accepted = true;
                let result = each_row(raw, |row: &RawValue, hint| {
                    match self.account_map_order(cx, row, hint.or(market.as_deref())) {
                        Ok(Some(order)) => orders.push(order),
                        Ok(None) => {}
                        Err(_) => accepted = false,
                    }
                });
                *valid = accepted && result.is_ok();
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
            Request::ChangeTier { command } => cx.done(
                command,
                Err(format!(
                    "Lighter account tier change outcome is unknown: {error}"
                )),
            ),
            Request::Lookup { candidates, .. } => self.next_lookup(cx, candidates),
            Request::Snapshot { .. } => {
                self.account.snapshot_inflight = false;
                cx.log(
                    LogLevel::Warning,
                    format!("Lighter account snapshot failed: {error}"),
                );
            }
            Request::Orders { .. } => {
                self.account.orders_inflight = false;
                self.account.batch = None;
                cx.log(
                    LogLevel::Warning,
                    format!("Lighter order snapshot failed: {error}"),
                );
            }
        }
    }
    pub(crate) fn account_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.account.connection.socket != Some(socket) {
            return false;
        }
        self.account.connection.last_received = cx.now;
        let Ok(frame) = serde_json::from_str::<Frame<'_>>(text) else {
            cx.log(LogLevel::Warning, "Invalid Lighter private frame");
            return true;
        };
        if frame.kind == "ping" {
            cx.send(
                socket,
                format!(r#"{{"type":"pong","timestamp":{}}}"#, cx.now),
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
        if meaningful {
            self.account.connection.last_payload = Some(cx.now);
            self.account.connection.recovery_started = None;
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
                    self.trade_order_observed(&order.id, order.filled, order.status);
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
                    if matches!(order.status, OrderStatus::Canceled | OrderStatus::Closed) {
                        self.account.suppressed.insert(order.id.clone());
                        self.account.records.remove(&order.id);
                        if order.status == OrderStatus::Canceled {
                            cx.store.remove_orders(&[&order.id]);
                        } else {
                            cx.store.upsert_order(order);
                        }
                    } else {
                        cx.store.upsert_order(order);
                    }
                }
            });
            if let Err(e) = result {
                cx.log(LogLevel::Warning, e);
            }
        }
        if let Some(raw) = payload.positions {
            let result = each_row(raw, |wire: PositionWire<'_>, hint| {
                let time = event_time(wire.updated_at.as_ref(), wire.transaction_time.as_ref())
                    .or_else(|| {
                        event_time(frame.timestamp.as_ref(), frame.transaction_time.as_ref())
                    })
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
                cx.log(LogLevel::Warning, e);
            }
        }
        for raw in [payload.fills, payload.trades].into_iter().flatten() {
            let result = each_row(raw, |trade: TradeWire<'_>, hint| {
                let execution_id = if !trade.trade_id_str.0.is_empty() {
                    trade.trade_id_str.0.as_ref()
                } else if !trade.trade_id.0.is_empty() {
                    trade.trade_id.0.as_ref()
                } else {
                    trade.fill_id.0.as_ref()
                };
                if execution_id.is_empty() {
                    return;
                }
                let market = if trade.market_id.0.is_empty() {
                    hint.unwrap_or_default()
                } else {
                    &trade.market_id.0
                };
                let timestamp = millis(trade.timestamp.0);
                let symbol = if !trade.symbol.is_empty() {
                    trade.symbol.as_ref()
                } else {
                    match self.symbol_for_id(market) {
                        Some(s) => s,
                        None => return,
                    }
                };
                let side = if trade.ask_account_id.0 == account {
                    OrderSide::Sell
                } else if trade.bid_account_id.0 == account {
                    OrderSide::Buy
                } else {
                    if !trade.ask_account_id.0.is_empty() || !trade.bid_account_id.0.is_empty() {
                        return;
                    }
                    match trade.side.to_ascii_uppercase().as_str() {
                        "BUY" => OrderSide::Buy,
                        "SELL" => OrderSide::Sell,
                        _ => match trade.is_ask.as_ref() {
                            Some(ask) => {
                                if ask.get() {
                                    OrderSide::Sell
                                } else {
                                    OrderSide::Buy
                                }
                            }
                            None => return,
                        },
                    }
                };
                if trade.price.0 <= 0.0 || trade.size.0 <= 0.0 {
                    return;
                }
                let order = if !trade.order_id.0.is_empty() {
                    trade.order_id.0.as_ref()
                } else if side == OrderSide::Sell {
                    trade.ask_id.0.as_ref()
                } else {
                    trade.bid_id.0.as_ref()
                };
                if order.is_empty() || timestamp <= 0.0 || timestamp > cx.now + 1000.0 {
                    return;
                }
                let symbol = symbol.to_owned();
                let market = cx.store.market(&symbol).map_or(market, |m| m.id.as_str());
                let id = format!("{market}:{execution_id}");
                if !self.account.seen_fills.insert(id.clone()) {
                    return;
                }
                self.account.fill_queue.push_back(id);
                while self.account.fill_queue.len() > 8192 {
                    if let Some(id) = self.account.fill_queue.pop_front() {
                        self.account.seen_fills.remove(&id);
                    }
                }
                // Subscription history is baseline; delayed update executions remain live.
                if frame.kind.starts_with("subscribed") {
                    return;
                }
                self.trade_execution_observed(order, timestamp);
                cx.fill(Fill {
                    order_id: order.into(),
                    symbol,
                    side,
                    price: trade.price.0,
                    amount: trade.size.0,
                });
            });
            if let Err(e) = result {
                cx.log(LogLevel::Warning, e);
            }
        }
        true
    }
}
