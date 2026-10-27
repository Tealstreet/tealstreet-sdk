//! Private challenge state, atomic reconciliation and execution-only fills.
use crate::{Kraken, Pending, wire};
use safe_cex_engine::Level;
use safe_cex_engine::backoff::Backoff;
use safe_cex_engine::coverage::{RowRepeat, RowReports, SnapshotBuilder};
use safe_cex_engine::log::code;
use safe_cex_engine::{
    Cx, Fill, Method, Response, SocketId, TimerId,
    json::{self, ArrayAt},
    model::*,
};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
};

const FEEDS: [&str; 3] = ["fills", "open_orders", "open_positions"];
/// Recent execution deduplication and terminal fences survive reconnects without
/// retaining every historical identity for the lifetime of the engine.
#[derive(Default)]
struct RecentIds<const CAPACITY: usize> {
    members: HashSet<String>,
    arrival: VecDeque<String>,
}
impl<const CAPACITY: usize> RecentIds<CAPACITY> {
    fn insert(&mut self, id: String) -> bool {
        if !self.members.insert(id.clone()) {
            return false;
        }
        self.arrival.push_back(id);
        while self.arrival.len() > CAPACITY {
            if let Some(expired) = self.arrival.pop_front() {
                self.members.remove(&expired);
            }
        }
        true
    }
    fn contains(&self, id: &str) -> bool {
        self.members.contains(id)
    }
    #[cfg(test)]
    fn len(&self) -> usize {
        self.members.len()
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Orders,
    Positions,
    Settings,
    Balance,
}
pub struct Request {
    scope: Scope,
    read_epoch: u64,
    revision: u64,
    settings_revision: u64,
    orders: SnapshotBuilder<String, Order>,
    positions: SnapshotBuilder<String, Position>,
    settings: Vec<(String, Option<f64>)>,
    /// False on a structural failure or held catalogue exposure: nothing commits.
    valid: bool,
}
impl Request {
    fn new(scope: Scope, revision: u64, settings_revision: u64, read_epoch: u64) -> Self {
        Self {
            scope,
            read_epoch,
            revision,
            settings_revision,
            orders: SnapshotBuilder::default(),
            positions: SnapshotBuilder::default(),
            settings: vec![],
            valid: true,
        }
    }
}
struct LocalOrderUpdate {
    at: f64,
    amount: f64,
    amount_changed: bool,
    native_time: Option<f64>,
}
#[derive(Default)]
pub struct Account {
    pub records: HashMap<String, Order>,
    local_order_updates: HashMap<String, LocalOrderUpdate>,
    settings: HashMap<String, (f64, MarginType)>,
    settings_revision: u64,
    settings_write_revision: u64,
    orders_revision: u64,
    positions_revision: u64,
    terminal: RecentIds<16384>,
    inflight: HashSet<u8>,
    orders_reconcile: bool,
    started: bool,
    pub(super) feed: Option<safe_cex_engine::socket_lifecycle::FeedId>,
    socket: Option<SocketId>,
    open: bool,
    challenge: Option<String>,
    signed_challenge: Option<String>,
    waiting_challenge: bool,
    acks: HashSet<String>,
    payload_complete: bool,
    poll: Option<TimerId>,
    heal: Option<TimerId>,
    seen_fills: RecentIds<8192>,
    payloads: HashMap<String, f64>,
    snapshot_orders_revision: u64,
    snapshot_settings_revision: u64,
    snapshot_positions_seen: bool,
    read_failures: u32,
    unlisted_logged: RowReports<String>,
    /// `order:`/`position:` + identity (empty when unreadable) of REST skips already logged.
    skip_logged: RowReports<String>,
}
impl Account {
    /// Logs a skipped row once per key, allowing one Error upgrade if its kept copy is lost.
    fn log_skip(&mut self, cx: &mut Cx<'_, Pending>, key: String, level: Level, message: String) {
        self.skip_logged.report(cx, key, level, message);
    }
    /// Re-arms the skip logs of `prefix`: all after a clean read, else those `ids` mapped.
    fn rearm_skips(&mut self, prefix: &str, clean: bool, ids: Vec<String>) {
        self.skip_logged
            .rearm_namespace(prefix, clean, ids.iter().map(String::as_str));
    }
    pub(crate) fn new() -> Self {
        Self {
            skip_logged: RowReports::new(RowRepeat::ErrorUpgrade),
            unlisted_logged: RowReports::unbounded(RowRepeat::Once),
            ..Self::default()
        }
    }
}
/// What a REST list row that failed to decode still says about its identity.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RowIdentity<'a> {
    #[serde(borrow, alias = "order_id", alias = "uid")]
    order_id: Cow<'a, str>,
    #[serde(borrow, alias = "instrument")]
    symbol: Cow<'a, str>,
}
/// Log labels of accepted rows whose id no rejection protects: they recovered.
/// `key` returns a row's (store id, log label).
fn recovered<T>(
    builder: &SnapshotBuilder<String, T>,
    key: impl Fn(&T) -> (&String, &String),
) -> Vec<String> {
    builder
        .rows()
        .iter()
        .map(key)
        .filter(|(id, _)| builder.allows_absence(id))
        .map(|(_, label)| label.clone())
        .collect()
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderWire<'a> {
    #[serde(default, borrow, alias = "order_id", alias = "uid")]
    order_id: Cow<'a, str>,
    #[serde(default, borrow, alias = "instrument")]
    symbol: Cow<'a, str>,
    #[serde(default, borrow, alias = "type")]
    order_type: Cow<'a, str>,
    #[serde(default, borrow)]
    side: Cow<'a, str>,
    #[serde(default)]
    direction: Option<Direction<'a>>,
    #[serde(
        default,
        alias = "limit_price",
        alias = "price",
        deserialize_with = "wire::number"
    )]
    limit_price: f64,
    #[serde(default, alias = "stop_price", deserialize_with = "wire::number")]
    stop_price: f64,
    #[serde(default, deserialize_with = "wire::optional_number")]
    unfilled_size: Option<f64>,
    #[serde(default, alias = "filled", deserialize_with = "wire::number")]
    filled_size: f64,
    #[serde(
        default,
        alias = "quantity",
        alias = "size",
        deserialize_with = "wire::number"
    )]
    qty: f64,
    #[serde(default, alias = "reduce_only")]
    reduce_only: bool,
    #[serde(default, borrow, alias = "cli_ord_id")]
    cli_ord_id: Cow<'a, str>,
    #[serde(default, borrow, alias = "triggerSignal")]
    trigger_signal: Cow<'a, str>,
    #[serde(
        default,
        borrow,
        alias = "last_update_time",
        alias = "lastUpdateTimestamp"
    )]
    last_update_time: Option<Stamp<'a>>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Direction<'a> {
    Number(u8),
    Text(#[serde(borrow)] Cow<'a, str>),
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Stamp<'a> {
    Number(f64),
    Text(#[serde(borrow)] Cow<'a, str>),
}
impl Stamp<'_> {
    fn time(&self) -> Option<f64> {
        match self {
            Self::Number(n) => n.is_finite().then_some(*n),
            Self::Text(s) => crate::history::timestamp(s),
        }
    }
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionWire<'a> {
    #[serde(default, borrow, alias = "instrument")]
    symbol: Cow<'a, str>,
    #[serde(default, borrow)]
    side: Cow<'a, str>,
    #[serde(default, deserialize_with = "wire::optional_number")]
    size: Option<f64>,
    #[serde(default, deserialize_with = "wire::optional_number")]
    balance: Option<f64>,
    #[serde(default, alias = "entry_price", deserialize_with = "wire::number")]
    price: f64,
    #[serde(default, alias = "mark_price", deserialize_with = "wire::number")]
    mark_price: f64,
    #[serde(default, alias = "pnl", deserialize_with = "wire::number")]
    unrealized_pnl: f64,
    #[serde(
        default,
        alias = "liquidation_threshold",
        deserialize_with = "wire::number"
    )]
    liquidation_price: f64,
    #[serde(default, deserialize_with = "wire::optional_number")]
    max_fixed_leverage: Option<f64>,
}
#[derive(Deserialize)]
struct Preference<'a> {
    #[serde(borrow)]
    symbol: Cow<'a, str>,
    #[serde(rename = "maxLeverage", deserialize_with = "wire::optional_number")]
    max_leverage: Option<f64>,
}
#[derive(Default, Deserialize)]
struct Ws<'a> {
    #[serde(default, borrow)]
    event: Cow<'a, str>,
    #[serde(default, borrow)]
    feed: Cow<'a, str>,
    #[serde(default, borrow)]
    message: Cow<'a, str>,
    #[serde(default, borrow)]
    reason: Cow<'a, str>,
    #[serde(default)]
    is_cancel: bool,
    #[serde(default, borrow)]
    order_id: Cow<'a, str>,
    #[serde(default, borrow)]
    order: Option<&'a RawValue>,
    #[serde(default, borrow)]
    orders: Option<&'a RawValue>,
    #[serde(default, borrow)]
    positions: Option<&'a RawValue>,
    #[serde(default, borrow)]
    fills: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct Execution<'a> {
    #[serde(
        borrow,
        alias = "fill_id",
        alias = "execution_id",
        alias = "executionId"
    )]
    fill_id: Cow<'a, str>,
    #[serde(borrow, alias = "orderId")]
    order_id: Cow<'a, str>,
    #[serde(borrow, alias = "symbol")]
    instrument: Cow<'a, str>,
    #[serde(deserialize_with = "wire::number")]
    qty: f64,
    #[serde(deserialize_with = "wire::number")]
    price: f64,
    buy: bool,
    #[serde(default, deserialize_with = "wire::optional_number")]
    remaining_order_qty: Option<f64>,
    #[serde(default, deserialize_with = "wire::optional_number")]
    time: Option<f64>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountWire<'a> {
    #[serde(default, borrow, rename = "type")]
    kind: Cow<'a, str>,
    #[serde(default)]
    balances: BTreeMap<String, Number>,
    #[serde(default)]
    currencies: BTreeMap<String, Currency>,
    #[serde(default, deserialize_with = "wire::optional_number")]
    portfolio_value: Option<f64>,
    #[serde(default, deserialize_with = "wire::number")]
    balance_value: f64,
    #[serde(default, deserialize_with = "wire::number")]
    available_margin: f64,
    #[serde(default, deserialize_with = "wire::number")]
    initial_margin: f64,
    #[serde(default, deserialize_with = "wire::number")]
    maintenance_margin: f64,
    #[serde(default, deserialize_with = "wire::number")]
    pnl: f64,
    #[serde(default)]
    auxiliary: Option<Aux<'a>>,
}
#[derive(Deserialize)]
struct Number(#[serde(deserialize_with = "wire::number")] f64);
#[derive(Default, Deserialize)]
struct Currency {
    #[serde(default, deserialize_with = "wire::number")]
    quantity: f64,
    #[serde(default, deserialize_with = "wire::number")]
    value: f64,
}
#[derive(Default, Deserialize)]
struct Aux<'a> {
    #[serde(default, borrow)]
    currency: Cow<'a, str>,
    #[serde(default, deserialize_with = "wire::number")]
    pv: f64,
    #[serde(default, deserialize_with = "wire::number")]
    af: f64,
    #[serde(default, deserialize_with = "wire::number")]
    pnl: f64,
}
#[derive(Deserialize)]
struct Accounts<'a> {
    #[serde(borrow)]
    accounts: BTreeMap<Cow<'a, str>, AccountWire<'a>>,
}

struct SignedParts {
    url: String,
    headers: Vec<(&'static str, String)>,
    body: Option<String>,
}
impl Kraken {
    pub(crate) fn signed_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        form: String,
        pending: Pending,
    ) -> Result<(), String> {
        let SignedParts { url, headers, body } = self.signed_parts(cx, method, path, form)?;
        cx.request(method, url, headers, body, pending);
        Ok(())
    }
    pub(crate) fn signed_streamed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        form: String,
        at: ArrayAt,
        pending: Pending,
    ) -> Result<(), String> {
        let SignedParts { url, headers, body } = self.signed_parts(cx, method, path, form)?;
        let id = cx.request_streamed_body(method, url, headers, body, at, pending);
        if path.starts_with("/api/history/") {
            cx.route_request(id, safe_cex_engine::RequestRoute::Primary);
        }
        Ok(())
    }
    fn signed_parts(
        &mut self,
        cx: &Cx<'_, Pending>,
        method: Method,
        path: &str,
        form: String,
    ) -> Result<SignedParts, String> {
        let read = method == Method::Get
            && !path.starts_with("/api/history/")
            && self.read_signer.is_some();
        // Nonces belong to the API key even when its two configured secrets differ.
        let same_key = self
            .options
            .read_credentials
            .as_ref()
            .is_some_and(|r| Some(r.key.as_str()) == self.options.key.as_deref());
        let nonce = if read && !same_key {
            self.read_signer.as_mut()
        } else {
            self.signer.as_mut()
        }
        .ok_or("Kraken signing credentials unavailable")?
        .nonce(cx.now)?;
        let signer = if read {
            self.read_signer.as_ref()
        } else {
            self.signer.as_ref()
        }
        .ok_or("Kraken signing credentials unavailable")?;
        let auth = signer.signature(&form, &nonce, path)?;
        let headers = vec![
            (
                "APIKey",
                if read {
                    self.options.read_credentials.as_ref().unwrap().key.clone()
                } else {
                    self.options.key.clone().unwrap_or_default()
                },
            ),
            ("Authent", auth),
            ("Nonce", nonce),
            ("Content-Type", "application/x-www-form-urlencoded".into()),
        ];
        let url = if method == Method::Get && !form.is_empty() {
            format!("{}{path}?{form}", self.host())
        } else {
            format!("{}{path}", self.host())
        };
        Ok(SignedParts {
            url,
            headers,
            body: if method == Method::Get {
                None
            } else {
                Some(form)
            },
        })
    }
    pub(crate) fn account_settings_revision(&self) -> u64 {
        self.account.settings_revision
    }
    pub(crate) fn account_setting(&self, symbol: &str) -> Option<(f64, MarginType)> {
        self.account.settings.get(symbol).copied()
    }
    pub(crate) fn account_settings_accepted(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        leverage: f64,
        margin: MarginType,
    ) {
        self.account.settings_revision += 1;
        self.account.settings_write_revision += 1;
        self.account_settings_store(cx, symbol, leverage, margin);
    }
    fn account_settings_store(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        leverage: f64,
        margin: MarginType,
    ) {
        if self.account.settings.get(symbol) != Some(&(leverage, margin)) {
            self.account.settings_revision += 1;
        }
        self.account
            .settings
            .insert(symbol.into(), (leverage, margin));
        cx.store.update_setting(symbol, |s| {
            s.leverage = leverage;
            s.margin_type = Some(margin);
            s.buy_leverage = Some(leverage);
            s.sell_leverage = Some(leverage);
        });
        cx.store.update_positions(|p| {
            if p.symbol == symbol {
                p.leverage = leverage;
                p.margin_type = Some(margin);
            }
        });
    }
    pub(crate) fn account_order_accepted(&mut self, cx: &mut Cx<'_, Pending>, order: &Order) {
        let prior = self.account.records.get(&order.id);
        let local_update = prior.is_none_or(|old| old != order);
        let native_time = prior.and_then(|old| {
            self.account_local_order_clock(old)
                .map_or(Some(old.last_update), |local| local.native_time)
        });
        let amount_changed = prior.is_some_and(|old| {
            old.amount != order.amount
                || self
                    .account_local_order_clock(old)
                    .is_some_and(|local| local.amount_changed)
        });
        self.account_apply_order(cx, order.clone(), false);
        if local_update
            && self.account.records.get(&order.id).is_some_and(|row| {
                row.last_update == order.last_update
                    && row.price == order.price
                    && row.amount == order.amount
            })
        {
            self.account.local_order_updates.insert(
                order.id.clone(),
                LocalOrderUpdate {
                    at: order.last_update,
                    amount: order.amount,
                    amount_changed,
                    native_time,
                },
            );
        }
    }
    // A local ACK arrival is not a native source timestamp. A delayed native
    // cumulative fill still advances progress, while the accepted price, total
    // quantity and newer source fields remain intact.
    fn account_merge_order(&self, mut order: Order) -> Order {
        if let Some(old) = self.account.records.get(&order.id) {
            let filled = order.filled.max(old.filled);
            if order.last_update < old.last_update || order.amount < old.filled {
                order = old.clone();
            }
            order.filled = filled.min(order.amount);
            order.remaining = (order.amount - order.filled).max(0.0);
            if order.trigger_type.is_none() {
                order.trigger_type = old.trigger_type;
            }
        }
        order
    }
    fn account_local_order_clock(&self, order: &Order) -> Option<&LocalOrderUpdate> {
        self.account
            .local_order_updates
            .get(&order.id)
            .filter(|local| local.at == order.last_update && local.amount == order.amount)
    }
    fn account_native_order_clock(&mut self, id: &str, time: f64) {
        if let Some(local) = self.account.local_order_updates.get_mut(id) {
            local.native_time = Some(local.native_time.map_or(time, |old| old.max(time)));
        }
    }
    pub(crate) fn account_order_removed(&mut self, cx: &mut Cx<'_, Pending>, id: &str) {
        self.account.orders_revision += 1;
        self.account.records.remove(id);
        self.account.local_order_updates.remove(id);
        self.account.terminal.insert(id.into());
        cx.store.remove_orders(&[id]);
    }
    fn account_apply_order(&mut self, cx: &mut Cx<'_, Pending>, order: Order, live: bool) {
        if self.account.terminal.contains(&order.id) {
            return;
        }
        if live {
            self.account_native_order_clock(&order.id, order.last_update);
        }
        let order = self.account_merge_order(order);
        if live {
            self.trade_order_observed(&order.id, order.filled, order.status);
        }
        self.account.orders_revision += 1;
        self.account.records.insert(order.id.clone(), order.clone());
        cx.store.upsert_order(order);
    }
    /// Rows on a market the catalogue filtered or never listed are skipped; only a
    /// market skipped as malformed (`skipped_markets`) holds the snapshot.
    fn account_unlisted(&mut self, cx: &mut Cx<'_, Pending>, symbols: Vec<String>) {
        for symbol in symbols {
            let message = format!("Kraken {symbol} is not a listed market; its rows are skipped");
            self.account
                .unlisted_logged
                .report(cx, symbol, Level::Warn, message);
        }
    }
    pub(crate) fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.started || self.signer.is_none() || self.options.public_data_only {
            return;
        }
        self.account.started = true;
        cx.store.set_hedged(false);
        let defaults: Vec<_> = cx
            .store
            .markets()
            .iter()
            .map(|m| (m.symbol.clone(), m.limits.leverage.max))
            .collect();
        for (symbol, leverage) in defaults {
            cx.store.update_setting(&symbol, |s| {
                s.leverage = leverage;
                s.margin_type = Some(MarginType::Cross);
                s.buy_leverage = Some(leverage);
                s.sell_leverage = Some(leverage);
            });
        }
        self.account_connect(cx);
        self.account_refresh(cx);
        self.account.poll = Some(cx.set_poll_timer(5000.0));
    }
    pub(super) fn account_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Err(error) = self.managed_connect(cx, true) {
            self.socket_error(cx, true, error);
        }
    }
    pub(crate) fn reconcile_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.reconcile_reask()
            && (self.account.orders_reconcile
                || self.account.inflight.contains(&(Scope::Orders as u8)))
        {
            return;
        }
        self.account.orders_reconcile = true;
        self.account_refresh(cx);
    }
    pub(crate) fn account_refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.signer.is_none() {
            return;
        }
        for (scope, path, array) in [
            (
                Scope::Orders,
                "/derivatives/api/v3/openorders",
                Some("openOrders"),
            ),
            (
                Scope::Positions,
                "/derivatives/api/v3/openpositions",
                Some("openPositions"),
            ),
            (
                Scope::Settings,
                "/derivatives/api/v3/leveragepreferences",
                Some("leveragePreferences"),
            ),
            (Scope::Balance, "/derivatives/api/v3/accounts", None),
        ] {
            let key = scope as u8;
            if !self.account.inflight.insert(key) {
                continue;
            }
            if scope == Scope::Orders {
                self.account.orders_reconcile = false;
            }
            let revision = match scope {
                Scope::Orders => self.account.orders_revision,
                Scope::Positions => self.account.positions_revision,
                Scope::Settings => self.account.settings_revision,
                Scope::Balance => 0,
            };
            let request = Pending::Account(Request::new(
                scope,
                revision,
                self.account.settings_revision,
                cx.order_read_epoch(),
            ));
            let result = if let Some(array) = array {
                self.signed_streamed(
                    cx,
                    Method::Get,
                    path,
                    String::new(),
                    ArrayAt::Field(array),
                    request,
                )
            } else {
                self.signed_request(cx, Method::Get, path, String::new(), request)
            };
            if let Err(error) = result {
                self.account.inflight.remove(&key);
                cx.retried_failure("kraken.account:634", error);
            }
        }
    }
    fn account_schedule_heal(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.heal.is_none() {
            let nominal = Backoff::new(1_000.0, 5, 30_000.0).delay(self.account.read_failures);
            // A heal after a websocket event keeps its fixed debounce; failed reads spread.
            let delay = if self.account.read_failures > 0 {
                cx.jittered(nominal)
            } else {
                nominal
            };
            self.account.heal = Some(cx.set_timer(delay));
        }
    }
    pub(super) fn account_control(
        &self,
        authenticated: bool,
        text: &str,
    ) -> safe_cex_engine::socket_lifecycle::Control {
        use safe_cex_engine::socket_lifecycle::{AuthEvidence, Control};
        let Ok(msg) = serde_json::from_str::<Ws<'_>>(text) else {
            return Control::Other;
        };
        if msg.event == "pong" {
            return Control::Pong;
        }
        if msg.event == "error"
            || msg.event == "alert"
            || msg.event == "subscribed_failed"
            || msg.event == "unsubscribed_failed"
            || (msg.event == "info" && msg.message.to_ascii_lowercase().contains("authentication"))
        {
            return Control::Auth(AuthEvidence::Rejected(safe_cex_engine::VenueError::new(
                "Kraken private subscription rejected",
            )));
        }
        if !authenticated {
            if msg.event == "challenge" && self.account.waiting_challenge && !msg.message.is_empty()
            {
                return Control::Auth(AuthEvidence::Progress);
            }
            if msg.event == "subscribed"
                && FEEDS.contains(&msg.feed.as_ref())
                && self.account.signed_challenge.is_some()
                && !self.account.acks.contains(msg.feed.as_ref())
            {
                return Control::Auth(if self.account.acks.len() == FEEDS.len() - 1 {
                    AuthEvidence::Accepted
                } else {
                    AuthEvidence::Progress
                });
            }
        }
        Control::Traffic
    }
    pub(super) fn install_account_socket(&mut self, socket: SocketId) {
        self.account.socket = Some(socket);
        self.account.open = false;
    }
    pub(crate) fn account_ws_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.account.open = true;
        self.account.snapshot_orders_revision = self.account.orders_revision;
        self.account.snapshot_settings_revision = self.account.settings_write_revision;
        self.account.snapshot_positions_seen = false;
        self.account.waiting_challenge = true;
        self.account.challenge = None;
        self.account.signed_challenge = None;
        self.account.acks.clear();
        self.account.payloads.clear();
        self.account.payload_complete = false;
        cx.send(
            socket,
            serde_json::to_string(
                &serde_json::json!({"event":"challenge","api_key":self.options.key}),
            )
            .unwrap(),
        );
        true
    }
    pub(crate) fn account_ws_close(&mut self, _cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.account.orders_revision += 1;
        self.account.positions_revision += 1;
        self.account.socket = None;
        self.account.open = false;
        self.account.challenge = None;
        self.account.signed_challenge = None;
        self.account.acks.clear();
        self.account.payloads.clear();
        self.account.payload_complete = false;
        self.account.waiting_challenge = false;
        true
    }
    fn account_disconnect(&mut self, cx: &mut Cx<'_, Pending>, message: &str) {
        cx.notice(Level::Error, code::STREAM_UNAVAILABLE, message);
        if let Some(socket) = self.account.socket {
            if let Some(feed) = self.account.feed {
                cx.retire_socket(
                    feed,
                    safe_cex_engine::socket_lifecycle::RetireReason::Closed,
                );
            }
            let _ = socket;
        }
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.poll == Some(timer) {
            self.account.poll = None;
            self.account_refresh(cx);
            self.account.poll = Some(cx.set_poll_timer(5000.0));
            return true;
        }
        if self.account.heal == Some(timer) {
            self.account.heal = None;
            self.account_refresh(cx);
            return true;
        }
        false
    }
    pub(crate) fn account_ws_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        if !self.account.open {
            return true;
        }
        let Ok(msg) = serde_json::from_str::<Ws<'_>>(text) else {
            return true;
        };
        if msg.event == "challenge" {
            if !self.account.waiting_challenge || msg.message.is_empty() {
                return true;
            }
            self.account.waiting_challenge = false;
            self.account.challenge = Some(msg.message.to_string());
            let Some(signer) = self.signer.as_ref() else {
                self.account_disconnect(cx, "Kraken signing credentials unavailable");
                return true;
            };
            let signature = signer.challenge(&msg.message);
            self.account.signed_challenge = Some(signature.clone());
            for feed in FEEDS {
                cx.send(socket, serde_json::json!({"event":"subscribe","feed":feed,"api_key":self.options.key,"original_challenge":msg.message,"signed_challenge":signature}).to_string());
            }
            return true;
        }
        if msg.event == "error"
            || msg.event == "alert"
            || msg.event == "subscribed_failed"
            || msg.event == "unsubscribed_failed"
            || (msg.event == "info" && msg.message.to_ascii_lowercase().contains("authentication"))
        {
            self.account_disconnect(cx, "Kraken private subscription rejected");
            return true;
        }
        if msg.event == "subscribed"
            && FEEDS.contains(&msg.feed.as_ref())
            && self.account.signed_challenge.is_some()
        {
            self.account.acks.insert(msg.feed.to_string());
            return true;
        }
        if self.account.signed_challenge.is_none() {
            return true;
        }
        match msg.feed.as_ref() {
            "fills" | "fills_snapshot" => {
                if let Some(raw) = msg.fills {
                    let mut executions = Vec::new();
                    let decoded = json::each::<Execution<'_>>(raw.get(), |e| executions.push(e));
                    if decoded.is_ok() {
                        self.account.payloads.insert("fills".into(), cx.now);
                        for e in executions {
                            if e.fill_id.is_empty()
                                || e.order_id.is_empty()
                                || e.instrument.is_empty()
                                || e.qty <= 0.0
                                || e.price <= 0.0
                            {
                                continue;
                            }
                            if !self.account.seen_fills.insert(e.fill_id.to_string()) {
                                continue;
                            }
                            if msg.feed == "fills_snapshot"
                                || cx.store.market(&e.instrument).is_none()
                            {
                                continue;
                            }
                            self.trade_execution_observed(&e.order_id);
                            if let (Some(remaining), Some(time), Some(mut order)) = (
                                e.remaining_order_qty,
                                e.time,
                                self.account.records.get(e.order_id.as_ref()).cloned(),
                            ) && self.account_local_order_clock(&order).map_or(
                                time >= order.last_update,
                                |local| {
                                    !local.amount_changed
                                        && local.native_time.is_none_or(|floor| time >= floor)
                                },
                            ) && remaining >= 0.0
                                && remaining <= order.amount
                            {
                                order.filled = order.filled.max(order.amount - remaining);
                                order.remaining = (order.amount - order.filled).max(0.0);
                                order.last_update = order.last_update.max(time);
                                self.account_native_order_clock(&order.id, time);
                                if order.remaining == 0.0 {
                                    self.account_order_removed(cx, &order.id);
                                } else {
                                    self.account_apply_order(cx, order, false);
                                }
                            }
                            cx.fill(Fill {
                                id: None,
                                order_id: e.order_id.to_string(),
                                symbol: e.instrument.to_string(),
                                side: if e.buy {
                                    OrderSide::Buy
                                } else {
                                    OrderSide::Sell
                                },
                                price: e.price,
                                amount: e.qty,
                            });
                        }
                        self.account_schedule_heal(cx);
                    }
                }
            }
            "open_orders" | "open_orders_snapshot" => {
                if msg.is_cancel && !msg.order_id.is_empty() {
                    self.account_ws_remove(cx, &msg.order_id, 0.0, &msg.reason);
                    self.account.payloads.insert("open_orders".into(), cx.now);
                    self.account_schedule_heal(cx);
                } else if let Some(raw) = msg.orders {
                    let mut request = Request::new(Scope::Orders, 0, 0, 0);
                    let result = json::each::<&RawValue>(raw.get(), |item| {
                        self.account_order_item(cx, &mut request, item.get())
                    });
                    if result.is_ok() && request.valid {
                        self.account.payloads.insert("open_orders".into(), cx.now);
                        let builder = request.orders;
                        if msg.feed == "open_orders_snapshot"
                            && self.account.orders_revision != self.account.snapshot_orders_revision
                        {
                            for order in builder.rows().to_vec() {
                                self.account_apply_order(cx, order, true);
                            }
                        } else {
                            self.account.orders_revision += 1;
                            let mapped = recovered(&builder, |o| (&o.id, &o.id));
                            self.account
                                .rearm_skips("order:", builder.rejected() == 0, mapped);
                            let live = builder.rows().len();
                            let orders =
                                builder.commit(cx.store.orders().iter().cloned(), |o| o.id.clone());
                            self.account_commit_orders(cx, orders, live);
                        }
                    } else {
                        cx.notice(
                            Level::Warn,
                            code::SNAPSHOT_DISCARDED,
                            "Kraken order snapshot held",
                        );
                        self.account_schedule_heal(cx);
                    }
                } else if let Some(raw) = msg.order
                    && let Ok(row) = serde_json::from_str::<OrderWire<'_>>(raw.get())
                    && let Some(order) = self.map_order(cx, row)
                {
                    if msg.is_cancel
                        && self.account.records.get(&order.id).is_some_and(|old| {
                            old.last_update > order.last_update
                                && self.account_local_order_clock(old).is_none()
                        })
                    {
                        return true;
                    }
                    self.account.payloads.insert("open_orders".into(), cx.now);
                    if msg.is_cancel {
                        self.account_ws_remove(cx, &order.id, order.filled, &msg.reason);
                    } else {
                        self.account_apply_order(cx, order, true);
                    }
                    self.account_schedule_heal(cx);
                }
            }
            "open_positions" | "open_positions_snapshot" => {
                if let Some(raw) = msg.positions {
                    let mut request = Request::new(Scope::Positions, 0, 0, 0);
                    let result = json::each::<&RawValue>(raw.get(), |item| {
                        self.account_position_item(cx, &mut request, item.get(), true)
                    });
                    if result.is_ok() && request.valid {
                        self.account.positions_revision += 1;
                        self.account
                            .payloads
                            .insert("open_positions".into(), cx.now);
                        let builder = request.positions;
                        let mapped = recovered(&builder, |p| (&p.id, &p.symbol));
                        self.account
                            .rearm_skips("position:", builder.rejected() == 0, mapped);
                        let mut positions =
                            builder.commit(cx.store.positions().iter().cloned(), |p| p.id.clone());
                        if !self.account.snapshot_positions_seen
                            && self.account.snapshot_settings_revision
                                != self.account.settings_write_revision
                        {
                            for p in &mut positions {
                                if let Some((l, m)) = self.account_setting(&p.symbol) {
                                    p.leverage = l;
                                    p.margin_type = Some(m);
                                }
                            }
                        }
                        self.account.snapshot_positions_seen = true;
                        self.account_commit_positions(cx, positions);
                    } else {
                        cx.notice(
                            Level::Warn,
                            code::SNAPSHOT_DISCARDED,
                            "Kraken position snapshot held",
                        );
                        self.account_schedule_heal(cx);
                    }
                }
            }
            _ => {}
        }
        if !self.account.payload_complete
            && FEEDS
                .iter()
                .all(|feed| self.account.payloads.contains_key(*feed))
        {
            match cx.complete_socket_initial_payload() {
                Ok(()) => self.account.payload_complete = true,
                Err(error) => self.socket_error(cx, true, error),
            }
        }
        true
    }
    fn account_ws_remove(&mut self, cx: &mut Cx<'_, Pending>, id: &str, filled: f64, reason: &str) {
        if reason == "stop_order_triggered" {
            self.trade_execution_observed(id);
            self.account.orders_revision += 1;
            self.account.records.remove(id);
            self.account.local_order_updates.remove(id);
            cx.store.remove_orders(&[id]);
        } else {
            self.trade_order_observed(
                id,
                filled,
                if reason == "full_fill" {
                    OrderStatus::Closed
                } else {
                    OrderStatus::Canceled
                },
            );
            self.account_order_removed(cx, id);
        }
    }
    fn map_order(&self, cx: &Cx<'_, Pending>, row: OrderWire<'_>) -> Option<Order> {
        if row.order_id.is_empty() || cx.store.market(&row.symbol).is_none() {
            return None;
        }
        let typ = row.order_type.to_ascii_lowercase();
        let kind = match typ.as_str() {
            "stp" | "stop" => OrderType::StopLoss,
            "take_profit" => OrderType::TakeProfit,
            "trailing_stop" => OrderType::TrailingStopLoss,
            "lmt" | "limit" | "post" | "ioc" | "fok" => OrderType::Limit,
            "mkt" | "market" => OrderType::Market,
            _ => return None,
        };
        let buy = match row.direction {
            Some(Direction::Number(0)) => true,
            Some(Direction::Number(1)) => false,
            Some(Direction::Text(t)) if t.eq_ignore_ascii_case("buy") => true,
            Some(Direction::Text(t)) if t.eq_ignore_ascii_case("sell") => false,
            None if row.side.eq_ignore_ascii_case("buy") => true,
            None if row.side.eq_ignore_ascii_case("sell") => false,
            _ => return None,
        };
        let remaining = row
            .unfilled_size
            .unwrap_or((row.qty - row.filled_size).max(0.0));
        let amount = if row.unfilled_size.is_some() {
            remaining + row.filled_size
        } else {
            row.qty
        };
        if amount <= 0.0 || remaining < 0.0 || row.filled_size < 0.0 || row.filled_size > amount {
            return None;
        }
        let trigger_type = match row.trigger_signal.as_ref() {
            "mark" => Some(StopTriggerType::MarkPrice),
            "last" => Some(StopTriggerType::LastPrice),
            "spot" | "index" => Some(StopTriggerType::IndexPrice),
            _ => None,
        };
        Some(Order {
            id: row.order_id.into_owned(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: row.symbol.into_owned(),
            kind,
            side: if buy { OrderSide::Buy } else { OrderSide::Sell },
            price: if matches!(
                kind,
                OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
            ) {
                row.stop_price
            } else {
                row.limit_price
            },
            amount,
            filled: row.filled_size,
            remaining,
            reduce_only: row.reduce_only,
            close: None,
            client_order_id: (!row.cli_ord_id.is_empty()).then(|| row.cli_ord_id.into_owned()),
            time_in_force: Some(match typ.as_str() {
                "post" => TimeInForce::PostOnly,
                "ioc" => TimeInForce::ImmediateOrCancel,
                "fok" => TimeInForce::FillOrKill,
                _ => TimeInForce::GoodTillCancel,
            }),
            last_update: row
                .last_update_time
                .as_ref()
                .and_then(Stamp::time)
                .unwrap_or(cx.now),
            trigger_type,
            take_profit: None,
            stop_loss: None,
            extra: None,
        })
    }
    fn map_position(
        &self,
        cx: &Cx<'_, Pending>,
        row: PositionWire<'_>,
        live: bool,
    ) -> Option<Position> {
        let market = cx.store.market(&row.symbol)?;
        let quantity = row.balance.or(row.size)?;
        if row.balance.is_none() && quantity < 0.0
            || row.max_fixed_leverage.is_some_and(|l| l <= 0.0)
        {
            return None;
        }
        if row.balance.is_none() && !matches!(row.side.as_ref(), "long" | "short") {
            return None;
        }
        if quantity != 0.0 && row.price <= 0.0 {
            return None;
        }
        let side = if row.balance.is_some() {
            if quantity >= 0.0 {
                PositionSide::Long
            } else {
                PositionSide::Short
            }
        } else if row.side == "short" {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let setting = self.account_setting(&row.symbol);
        let (leverage, margin) = if let Some(leverage) = row.max_fixed_leverage {
            (leverage, MarginType::Isolated)
        } else if live && let Some(setting) = setting {
            // Stream rows can omit maxFixedLeverage; keep the REST-proven mode as TS does.
            setting
        } else {
            (
                setting.map_or(market.limits.leverage.max, |s| s.0),
                MarginType::Cross,
            )
        };
        let mark = if row.mark_price > 0.0 {
            row.mark_price
        } else {
            cx.store
                .tickers()
                .iter()
                .find(|t| t.symbol == row.symbol)
                .map_or(0.0, |t| t.mark)
        };
        let old = cx
            .store
            .positions()
            .iter()
            .find(|p| p.symbol == row.symbol && p.side == side);
        Some(Position {
            id: format!(
                "{}:{}",
                row.symbol,
                if side == PositionSide::Long {
                    "long"
                } else {
                    "short"
                }
            ),
            symbol: row.symbol.into_owned(),
            side,
            entry_price: row.price,
            notional: if !live {
                old.filter(|p| p.contracts == quantity.abs())
                    .map_or(quantity.abs() * mark, |p| p.notional)
            } else {
                quantity.abs() * mark
            },
            leverage,
            margin_type: Some(margin),
            position_mode: Some(PositionMode::Oneway),
            unrealized_pnl: row.unrealized_pnl,
            realized_pnl: None,
            contracts: quantity.abs(),
            liquidation_price: if live {
                row.liquidation_price
            } else {
                old.map_or(0.0, |p| p.liquidation_price)
            },
            margin_size: None,
            last_update: Some(cx.now),
        })
    }
    /// The first `live` rows are native observations; the rest are kept stored copies.
    fn account_commit_orders(&mut self, cx: &mut Cx<'_, Pending>, orders: Vec<Order>, live: usize) {
        let mut mapped = vec![];
        for (n, order) in orders.into_iter().enumerate() {
            if self.account.terminal.contains(&order.id) {
                continue;
            }
            self.account_native_order_clock(&order.id, order.last_update);
            let order = self.account_merge_order(order);
            if n < live {
                self.trade_order_observed(&order.id, order.filled, order.status);
            }
            mapped.push(order);
        }
        self.account.records = mapped.iter().map(|o| (o.id.clone(), o.clone())).collect();
        self.account
            .local_order_updates
            .retain(|id, _| self.account.records.contains_key(id));
        cx.store.set_orders(mapped);
    }
    fn account_commit_positions(&mut self, cx: &mut Cx<'_, Pending>, mut positions: Vec<Position>) {
        positions.retain(|p| p.contracts > 0.0);
        for p in &positions {
            self.account_settings_store(
                cx,
                &p.symbol,
                p.leverage,
                p.margin_type.unwrap_or(MarginType::Cross),
            );
        }
        cx.store.set_positions(positions);
    }
    /// A row without an id protects every stored order; otherwise only its own.
    fn account_reject_order(&mut self, cx: &mut Cx<'_, Pending>, request: &mut Request, id: &str) {
        let kept = cx
            .store
            .orders()
            .iter()
            .any(|o| (id.is_empty() || o.id == id) && !self.account.terminal.contains(&o.id));
        let level = if kept { Level::Warn } else { Level::Error };
        if id.is_empty() {
            let message = "Kraken skipped order row without an id".to_owned();
            self.account.log_skip(cx, "order:".into(), level, message);
            request.orders.reject(None);
        } else {
            let message = format!("Kraken skipped unmappable order {id}");
            self.account
                .log_skip(cx, format!("order:{id}"), level, message);
            request.orders.reject(Some(id.to_owned()));
        }
    }
    /// Both legs of `symbol` keep their stored copies; an empty symbol protects all.
    /// A row with no stored copy is reported as an error: it commits as flat.
    fn account_reject_position(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        symbol: &str,
    ) {
        let stored = cx
            .store
            .positions()
            .iter()
            .any(|p| symbol.is_empty() || p.symbol == symbol);
        let level = if stored { Level::Warn } else { Level::Error };
        if symbol.is_empty() {
            let message = "Kraken skipped position row without a symbol".to_owned();
            self.account
                .log_skip(cx, "position:".into(), level, message);
            request.positions.reject(None);
        } else {
            let message = format!("Kraken skipped unmappable {symbol} position row");
            self.account
                .log_skip(cx, format!("position:{symbol}"), level, message);
            for side in ["long", "short"] {
                request.positions.reject(Some(format!("{symbol}:{side}")));
            }
        }
    }
    /// Maps one order row into `request`; a malformed-catalogue market holds it.
    fn account_order_item(&mut self, cx: &mut Cx<'_, Pending>, request: &mut Request, item: &str) {
        match serde_json::from_str::<OrderWire<'_>>(item) {
            Ok(row) => {
                if row.symbol.is_empty() {
                    self.account_reject_order(cx, request, &row.order_id);
                } else if cx.store.market(&row.symbol).is_some() {
                    let id = row.order_id.to_string();
                    if let Some(order) = self.map_order(cx, row) {
                        request.orders.push(order);
                    } else {
                        self.account_reject_order(cx, request, &id);
                    }
                } else if self.skipped_markets.contains(row.symbol.as_ref()) {
                    request.valid = false;
                } else {
                    self.account_unlisted(cx, vec![row.symbol.into_owned()]);
                }
            }
            Err(_) => {
                let row = serde_json::from_str::<RowIdentity<'_>>(item).unwrap_or_default();
                if self.skipped_markets.contains(row.symbol.as_ref())
                    && cx.store.market(&row.symbol).is_none()
                {
                    request.valid = false;
                } else {
                    self.account_reject_order(cx, request, &row.order_id);
                }
            }
        }
    }
    /// Maps one position row into `request`; `live` marks a stream row.
    fn account_position_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
        live: bool,
    ) {
        match serde_json::from_str::<PositionWire<'_>>(item) {
            Ok(row) => {
                if row.symbol.is_empty() {
                    self.account_reject_position(cx, request, "");
                } else if cx.store.market(&row.symbol).is_some() {
                    let symbol = row.symbol.to_string();
                    if let Some(p) = self.map_position(cx, row, live) {
                        request.positions.push(p);
                    } else {
                        self.account_reject_position(cx, request, &symbol);
                    }
                } else if row.balance.or(row.size) == Some(0.0) {
                } else if self.skipped_markets.contains(row.symbol.as_ref()) {
                    request.valid = false;
                } else {
                    self.account_unlisted(cx, vec![row.symbol.into_owned()]);
                }
            }
            Err(_) => {
                let row = serde_json::from_str::<RowIdentity<'_>>(item).unwrap_or_default();
                if row.symbol.is_empty() || cx.store.market(&row.symbol).is_some() {
                    self.account_reject_position(cx, request, &row.symbol);
                } else if self.skipped_markets.contains(row.symbol.as_ref()) {
                    request.valid = false;
                } else {
                    self.account_unlisted(cx, vec![row.symbol.into_owned()]);
                }
            }
        }
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request.scope {
            Scope::Orders => self.account_order_item(cx, request, item),
            Scope::Positions => self.account_position_item(cx, request, item, false),
            Scope::Settings => match serde_json::from_str::<Preference<'_>>(item) {
                Ok(p) => {
                    if p.max_leverage.is_none_or(|leverage| leverage > 0.0) {
                        request
                            .settings
                            .push((p.symbol.into_owned(), p.max_leverage));
                    } else {
                        request.valid = false;
                    }
                }
                Err(_) => request.valid = false,
            },
            Scope::Balance => request.valid = false,
        }
    }
    pub(crate) fn account_http(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        self.account.inflight.remove(&(request.scope as u8));
        if request.scope == Scope::Orders && self.account.orders_reconcile {
            self.account_refresh(cx);
        }
        let field = match request.scope {
            Scope::Orders => Some("openOrders"),
            Scope::Positions => Some("openPositions"),
            Scope::Settings => Some("leveragePreferences"),
            Scope::Balance => None,
        };
        // Direct host responses also traverse the same staged item mapper.
        if let Some(field) = field {
            let parsed = json::each_in_field::<&RawValue>(response.body, field, |item| {
                self.account_item(cx, &mut request, item.get())
            });
            if !matches!(parsed, Ok(true)) {
                request.valid = false;
            }
        }
        if !wire::success(response) || !request.valid {
            self.account.read_failures = self.account.read_failures.saturating_add(1);
            cx.retried_failure(
                "kraken.account:1266",
                "Kraken account reconciliation failed",
            );
            self.account_schedule_heal(cx);
            return;
        }
        self.account.read_failures = 0;
        match request.scope {
            Scope::Orders => {
                if request.revision == self.account.orders_revision {
                    let orders = request.orders;
                    let clean = orders.rejected() == 0;
                    let mapped = recovered(&orders, |o| (&o.id, &o.id));
                    self.account
                        .rearm_skips("order:", orders.rejected() == 0, mapped);
                    // Rejected rows keep their stored copies.
                    let orders = orders.commit(cx.store.orders().iter().cloned(), |o| o.id.clone());
                    self.account_commit_orders(cx, orders, 0);
                    if clean {
                        cx.orders_reconciled_since(None, request.read_epoch);
                    }
                }
            }
            Scope::Positions => {
                if request.revision == self.account.positions_revision {
                    let builder = request.positions;
                    let mapped = recovered(&builder, |p| (&p.id, &p.symbol));
                    self.account
                        .rearm_skips("position:", builder.rejected() == 0, mapped);
                    // Rejected rows keep both stored legs.
                    let mut positions =
                        builder.commit(cx.store.positions().iter().cloned(), |p| p.id.clone());
                    if request.settings_revision != self.account.settings_revision {
                        for p in &mut positions {
                            if let Some((l, m)) = self.account_setting(&p.symbol) {
                                p.leverage = l;
                                p.margin_type = Some(m);
                            }
                        }
                    }
                    self.account_commit_positions(cx, positions);
                }
            }
            Scope::Settings => {
                if request.revision == self.account.settings_revision {
                    let settings: HashMap<_, _> = request.settings.into_iter().collect();
                    let defaults: Vec<_> = cx
                        .store
                        .markets()
                        .iter()
                        .map(|m| (m.symbol.clone(), m.limits.leverage.max))
                        .collect();
                    for (symbol, default) in defaults {
                        let preference = settings.get(&symbol).copied().flatten();
                        self.account_settings_store(
                            cx,
                            &symbol,
                            preference.unwrap_or(default),
                            if preference.is_some() {
                                MarginType::Isolated
                            } else {
                                MarginType::Cross
                            },
                        );
                    }
                }
            }
            Scope::Balance => match serde_json::from_str::<Accounts<'_>>(response.body) {
                Ok(accounts) => {
                    let (balance, coins) = balance(accounts);
                    cx.store.set_balance(balance);
                    cx.store.set_coin_balances(coins);
                }
                Err(_) => {
                    cx.notice(
                        Level::Error,
                        code::SNAPSHOT_DISCARDED,
                        "Kraken malformed account balances",
                    );
                    self.account_schedule_heal(cx);
                }
            },
        }
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        _error: &str,
    ) {
        self.account.inflight.remove(&(request.scope as u8));
        if request.scope == Scope::Orders && self.account.orders_reconcile {
            self.account_refresh(cx);
        }
        self.account.read_failures = self.account.read_failures.saturating_add(1);
        cx.retried_failure(
            "kraken.account:1343",
            "Kraken account reconciliation transport failure",
        );
        self.account_schedule_heal(cx);
    }
}
fn balance(accounts: Accounts<'_>) -> (Balance, Vec<CoinBalance>) {
    let mut out = Balance::default();
    let mut coins: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for (name, account) in &accounts.accounts {
        if name == "cash" || name == "cashAccount" {
            for (currency, value) in &account.balances {
                let currency = currency.to_ascii_uppercase();
                coins.insert(currency.clone(), (value.0, value.0));
                if ["USD", "USDT", "USDC"].contains(&currency.as_str()) {
                    out.total += value.0;
                    out.free += value.0;
                }
            }
        }
    }
    if let Some(account) = accounts
        .accounts
        .get("flex")
        .or_else(|| accounts.accounts.get("multiCollateralMarginAccount"))
        && account.kind == "multiCollateralMarginAccount"
    {
        let total = account
            .portfolio_value
            .filter(|n| *n > 0.0)
            .unwrap_or(account.balance_value);
        if total > 0.0 {
            out = Balance {
                total,
                free: account.available_margin.max(0.0),
                used: account.initial_margin + account.maintenance_margin,
                upnl: account.pnl,
            };
        }
        for (currency, value) in &account.currencies {
            coins.insert(currency.to_ascii_uppercase(), (value.quantity, value.value));
        }
    }
    for (name, account) in &accounts.accounts {
        if name.starts_with("fi_")
            && let Some(aux) = &account.auxiliary
        {
            let currency = if aux.currency.is_empty() {
                "USD".to_string()
            } else {
                aux.currency.to_ascii_uppercase()
            };
            let e = coins.entry(currency.clone()).or_default();
            e.0 += aux.pv;
            e.1 += aux.pv;
            if currency == "USD" {
                out.total += aux.pv;
                out.free += aux.af;
                out.upnl += aux.pnl;
            }
        }
    }
    if out.total == 0.0 {
        for account in accounts.accounts.values() {
            if let Some(aux) = &account.auxiliary
                && aux.pv > 0.0
            {
                out = Balance {
                    total: aux.pv,
                    free: aux.af,
                    used: aux.pv - aux.af,
                    upnl: aux.pnl,
                };
                break;
            }
        }
    }
    (
        out,
        coins
            .into_iter()
            .map(|(base, (amount, notional))| CoinBalance {
                base,
                amount,
                notional,
            })
            .collect(),
    )
}
#[cfg(test)]
mod cache_tests {
    use super::*;
    #[test]
    fn execution_identity_memory_is_bounded_without_forgetting_recent_duplicates() {
        let mut account = Account::new();
        for n in 0..20000 {
            assert!(account.seen_fills.insert(format!("fill-{n}")));
        }
        assert!(
            account.seen_fills.len() <= 8192,
            "execution identity cache grew without bound"
        );
        assert!(account.seen_fills.contains("fill-19999"));
        assert!(!account.seen_fills.insert("fill-19999".into()));
        assert!(!account.seen_fills.contains("fill-0"));
    }
    #[test]
    fn terminal_order_memory_is_bounded_without_forgetting_recent_fences() {
        let mut account = Account::new();
        for n in 0..20000 {
            assert!(account.terminal.insert(format!("order-{n}")));
        }
        assert!(
            account.terminal.len() <= 16384,
            "terminal order cache grew without bound"
        );
        assert!(account.terminal.contains("order-19999"));
        assert!(!account.terminal.insert("order-19999".into()));
        assert!(!account.terminal.contains("order-0"));
    }
}
