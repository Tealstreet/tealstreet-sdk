//! Independent settlement-account reconciliation and authenticated AOP stream.
pub use crate::account_wire::Context;
use crate::account_wire::{AccountRow, OrderRow, PositionRow, integer, order_side};
use crate::{Pending, PhemexPerpetuals};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::*;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, Response, SocketId, TimerId};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::{HashMap, HashSet, VecDeque};

const CURRENCIES: [&str; 2] = ["USDT", "USDC"];
const RETAINED_EVENTS: usize = 20_000;
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
    positions: Vec<Position>,
    contexts: HashMap<String, Context>,
    versions: HashMap<String, u64>,
    invalid: bool,
}
#[derive(Default)]
pub struct State {
    started: bool,
    cycle: u64,
    pending: u8,
    failed: bool,
    failures: u32,
    deadline: Option<TimerId>,
    refresh: Option<TimerId>,
    write_revision: u64,
    order_revision: u64,
    slices: [Slice; 2],
    contexts: HashMap<String, Context>,
    order_sides: HashMap<String, String>,
    socket: Option<SocketId>,
    watchdog: Option<TimerId>,
    reconnect: Option<TimerId>,
    connected_at: f64,
    authenticated: bool,
    snapshot: bool,
    snapshot_revision: u64,
    sequence: Option<u64>,
    last_pong: f64,
    attempts: u32,
    next_id: u64,
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
}
impl State {
    pub fn apply_settings(
        &mut self,
        symbol: &str,
        leverage: Option<f64>,
        margin: Option<MarginType>,
        mode: Option<PositionMode>,
    ) {
        if let Some(context) = self.contexts.get_mut(symbol) {
            if let Some(leverage) = leverage {
                context.long_leverage = leverage;
                context.short_leverage = leverage;
            }
            if let Some(margin) = margin {
                context.margin_type = margin;
            }
            if let Some(mode) = mode {
                context.mode = mode;
            }
        }
    }
    pub fn terminal(&mut self, id: &str, now: f64) {
        self.remember_terminal(id.to_string(), (now * 1e6) as u64);
        self.order_revision += 1;
    }
    pub fn context(&self, symbol: &str) -> Option<Context> {
        self.contexts.get(symbol).copied()
    }
    pub fn mode(&self, symbol: &str) -> Option<PositionMode> {
        self.context(symbol).map(|c| c.mode)
    }
    pub fn orders_ready(&self) -> bool {
        self.authenticated && self.snapshot && self.socket.is_some() && !self.uncertainty_overflow
    }
    pub fn order_revision(&self) -> u64 {
        self.order_revision
    }
    pub fn has_unreconciled_fills(&self) -> bool {
        self.uncertainty_overflow || !self.uncertain_fills.is_empty()
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
    error: Option<&'a RawValue>,
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
    pub(crate) fn account_request_current(&self, request: &Request) -> bool {
        request.cycle == self.account.cycle
    }

    pub(crate) fn start_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.started {
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
        if self.account.pending != 0 {
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
                positions: vec![],
                contexts: HashMap::new(),
                versions: HashMap::new(),
                invalid: false,
            };
            self.request(
                cx,
                Method::Get,
                "/g-accounts/accountPositions",
                &format!("currency={name}"),
                None,
                Pending::Account(request),
                Some(ArrayAt::Path(&["data", "positions"])),
            );
        }
    }
    fn account_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.socket =
            Some(cx.connect_channel(self.ws(), safe_cex_engine::SocketChannel::Private));
        self.account.authenticated = false;
        self.account.snapshot = false;
        self.account.snapshot_revision = self.account.order_revision;
        self.account.sequence = None;
        self.account.connected_at = cx.now;
        self.account.last_pong = cx.now;
        self.account.watchdog = Some(cx.set_timer(5_000.));
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
            2_000. * 2f64.powi(self.account.failures.min(4) as i32 - 1)
        } else {
            2_000.
        };
        self.account.refresh = Some(cx.set_timer(wait.max(self.paused_until - cx.now)));
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
        let Ok(row) = serde_json::from_str::<PositionRow<'_>>(item) else {
            request.invalid = true;
            return;
        };
        let Some(market) = cx.store.market(&row.symbol.to_ascii_uppercase()) else {
            return;
        };
        if market.quote != CURRENCIES[request.currency]
            || (!row.currency.is_empty() && row.currency != CURRENCIES[request.currency])
        {
            request.invalid = true;
            return;
        }
        let prior = request
            .contexts
            .get(row.symbol.as_ref())
            .copied()
            .or_else(|| self.account.context(&row.symbol.to_ascii_uppercase()));
        let Some((context, position)) = row.map(cx.store, prior) else {
            request.invalid = true;
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
            || request.invalid
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
            let mut positions: Vec<_> = cx
                .store
                .positions()
                .iter()
                .filter(|p| {
                    cx.store
                        .market(&p.symbol)
                        .is_none_or(|m| m.quote != CURRENCIES[currency])
                })
                .cloned()
                .collect();
            // A successful full slice also versions omissions. Keep exact native versions
            // for present rows: fresh authoritative REST may contain a long-lived position.
            let present: HashSet<_> = request.positions.iter().map(|p| p.id.clone()).collect();
            let mut known: HashSet<_> = self.account.position_versions.keys().cloned().collect();
            known.extend(cx.store.positions().iter().map(|p| p.id.clone()));
            known.extend(request.versions.keys().cloned());
            for key in known {
                let symbol = key.split(':').next().unwrap_or(&key);
                if !cx
                    .store
                    .market(symbol)
                    .is_some_and(|m| m.quote == CURRENCIES[currency])
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
            positions.extend(request.positions);
            cx.store.set_positions(positions);
            for (symbol, context) in request.contexts {
                self.account.install_context(cx, &symbol, context);
            }
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
        cx.log(
            LogLevel::Warning,
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
    }
    pub(crate) fn accept_write_order(&mut self, cx: &mut Cx<'_, Pending>, row: &OrderRow<'_>) {
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
        if let Some(orders) = row.orders(cx.now) {
            for order in orders {
                cx.store.upsert_order(order);
            }
        }
        self.account.order_revision += 1;
    }
    pub(crate) fn account_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        if let Some(credentials) = &self.credentials {
            cx.send(socket, credentials.websocket_auth(cx.now, 999));
            self.account.next_id = 1200;
            self.account_ping(cx);
        }
        true
    }
    fn account_ping(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(socket) = self.account.socket {
            self.account.next_id += 1;
            cx.send(
                socket,
                format!(
                    r#"{{"id":{},"method":"server.ping","params":[]}}"#,
                    self.account.next_id
                ),
            );
        }
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
        let Ok(message) = serde_json::from_str::<Message<'_>>(text) else {
            cx.log(LogLevel::Warning, "Phemex malformed private frame");
            return true;
        };
        if message.id == Some(999) {
            #[derive(Deserialize)]
            struct Auth<'a> {
                #[serde(borrow)]
                status: &'a str,
            }
            let success = message.error.is_none()
                && message
                    .result
                    .and_then(|r| serde_json::from_str::<Auth<'_>>(r.get()).ok())
                    .is_some_and(|r| r.status == "success");
            if success {
                self.account.authenticated = true;
                self.account.attempts = 0;
                self.account.next_id += 1;
                cx.send(
                    socket,
                    format!(
                        r#"{{"id":{},"method":"aop_p.subscribe","params":[]}}"#,
                        self.account.next_id
                    ),
                );
            } else {
                cx.log(
                    LogLevel::Error,
                    "Phemex private websocket authentication rejected",
                );
                cx.close(socket);
                self.account_close(cx, socket, 1008);
            }
            return true;
        }
        if message.method == "server.ping" || message.result.is_some_and(|r| r.get() == r#""pong""#)
        {
            self.account.last_pong = cx.now;
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
            cx.log(
                LogLevel::Warning,
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
                cx.log(
                    LogLevel::Warning,
                    format!("Phemex invalid private orders: {error}"),
                );
                return true;
            }
        };
        let positions = match positions {
            Ok(rows) => rows,
            Err(error) => {
                cx.log(
                    LogLevel::Warning,
                    format!("Phemex invalid private positions: {error}"),
                );
                return true;
            }
        };
        let accounts = match accounts {
            Ok(rows) => rows,
            Err(error) => {
                cx.log(
                    LogLevel::Warning,
                    format!("Phemex invalid private accounts: {error}"),
                );
                return true;
            }
        };
        if orders.as_ref().is_some_and(|rows| {
            rows.iter().any(|row| {
                cx.store.market(&row.symbol.to_ascii_uppercase()).is_some()
                    && row.orders(cx.now).is_none()
            })
        }) {
            cx.log(
                LogLevel::Warning,
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
                    continue;
                };
                let Some(currency) = CURRENCIES.iter().position(|c| *c == market.quote) else {
                    continue;
                };
                let prior = staged_contexts
                    .get(&symbol)
                    .copied()
                    .or_else(|| self.account.context(&symbol));
                let Some((context, position)) = row.map(cx.store, prior) else {
                    cx.log(
                        LogLevel::Warning,
                        "Phemex invalid position mapping; retaining private frame",
                    );
                    return true;
                };
                if !row.currency.is_empty() && row.currency != CURRENCIES[currency] {
                    cx.log(
                        LogLevel::Warning,
                        "Phemex mismatched position settlement; retaining private frame",
                    );
                    return true;
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
            cx.log(
                LogLevel::Warning,
                "Phemex invalid account mapping; retaining private frame",
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
                    .install_context(cx, &row.symbol.to_ascii_uppercase(), context);
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
                cx.log(LogLevel::Warning,"Phemex execution reconciliation capacity exceeded; reconnecting for an authoritative snapshot");
                if let Some(socket) = self.account.socket {
                    cx.close(socket);
                    self.account_close(cx, socket, 1006);
                }
            }
        }
        cx.fill(Fill {
            order_id: row.order_id.to_string(),
            symbol: row.symbol.to_string(),
            side,
            price: row.exec_price_rp,
            amount: row.exec_qty,
        });
    }
    pub(crate) fn account_close(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        _code: u16,
    ) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.account.socket = None;
        self.account.authenticated = false;
        if let Some(timer) = self.account.watchdog.take() {
            cx.clear_timer(timer);
        }
        self.account.attempts = self.account.attempts.saturating_add(1);
        if self.account.reconnect.is_none() {
            self.account.reconnect =
                Some(cx.set_timer(1_000. * 2f64.powi(self.account.attempts.min(5) as i32 - 1)));
        }
        true
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.refresh == Some(timer) {
            self.account.refresh = None;
            self.account_cycle(cx);
            return true;
        }
        if self.account.deadline == Some(timer) {
            self.account.deadline = None;
            self.account.cycle += 1;
            self.account.pending = 0;
            self.account.failures = self.account.failures.saturating_add(1);
            cx.log(
                LogLevel::Warning,
                "Phemex private reconciliation exceeded 20-second deadline; retaining state",
            );
            self.account.refresh = Some(cx.set_timer(2_000f64.max(self.paused_until - cx.now)));
            return true;
        }
        if self.account.reconnect == Some(timer) {
            self.account.reconnect = None;
            self.account_connect(cx);
            return true;
        }
        if self.account.watchdog == Some(timer) {
            self.account.watchdog = None;
            if cx.now - self.account.last_pong >= 12_500.
                || !self.account.snapshot && cx.now - self.account.connected_at >= 20_000.
            {
                if let Some(socket) = self.account.socket {
                    cx.close(socket);
                    self.account_close(cx, socket, 1006);
                }
            } else {
                self.account_ping(cx);
                self.account.watchdog = Some(cx.set_timer(5_000.));
            }
            return true;
        }
        false
    }
}
impl State {
    fn install_context(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, context: Context) {
        self.contexts.insert(symbol.to_string(), context);
        let leverage = if context.long_leverage > 0. {
            context.long_leverage
        } else {
            context.short_leverage
        };
        if leverage > 0. {
            cx.store.update_setting(symbol, |s| {
                s.leverage = leverage;
                s.margin_type = Some(context.margin_type)
            });
        }
        cx.store.set_hedged(
            self.contexts
                .values()
                .any(|c| c.mode == PositionMode::Hedged),
        );
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
