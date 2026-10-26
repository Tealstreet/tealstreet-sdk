//! Account snapshots fence newer writes; position WS IDs do not replace REST financial state.
use crate::sign::{self, Params};
use crate::wire::*;
use crate::{BitunixFutures, Feed, Pending, REST};
use safe_cex_engine::Level;
use safe_cex_engine::coverage::{RowAdmission, row_skipped};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::execution::{Arithmetic, Cursor, ExecutionRounding};
use safe_cex_engine::history::BackgroundCoverage;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{Cx, Fill, Method, Response, TimerId};
use std::collections::{HashMap, HashSet};

pub const ACCOUNT: &str = "/api/v1/futures/account";
pub const POSITIONS: &str = "/api/v1/futures/position/get_pending_positions";
pub const SYMBOL_SETTINGS: &str = "/api/v1/futures/account/get_leverage_margin_mode";
pub const ORDERS: &str = "/api/v1/futures/trade/get_pending_orders";
pub const TPSL: &str = "/api/v1/futures/tpsl/get_pending_orders";
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    Balance,
    Positions,
    Orders,
    Tpsl,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SymbolSettings {
    leverage: f64,
    margin_mode: String,
}
fn decode_symbol_settings(response: &Response<'_>) -> Option<SymbolSettings> {
    if !crate::recovery::success(response) {
        return None;
    }
    data(response.body)
        .ok()
        .and_then(|body| serde_json::from_str::<SymbolSettings>(body).ok())
        .filter(|settings| settings.leverage.is_finite() && settings.leverage > 0.0)
}
pub struct PositionAdmission {
    id: String,
    symbol: String,
    route: String,
    mode: PositionMode,
}
pub struct Read {
    pub axis: Axis,
    pub generation: u64,
    pub balance_write_revision: u64,
    pub position_write_revision: u64,
    pub order_token: Option<safe_cex_engine::OrderSnapshotToken>,
    pub serial: u64,
    pub after: Option<u32>,
    pub positions: Vec<Position>,
    pub position_rejected: RowAdmission<String>,
    pub position_admitted: Vec<PositionAdmission>,
    pub identities: Vec<(String, String)>,
    pub orders: Vec<Order>,
    pub order_rejected: RowAdmission<String>,
    pub order_accepted: HashSet<String>,
    pub order_newer: RowAdmission<String>,
    pub native: Vec<(String, NativeOrder)>,
    pub invalid: bool,
    pub baselines: Vec<(String, (f64, f64))>,
}
impl Read {
    pub(crate) fn orders_current(&self, store: &safe_cex_engine::Store) -> bool {
        self.order_token
            .as_ref()
            .is_some_and(|token| store.orders_snapshot_current(token))
    }

    pub(crate) fn partial_orders(&self) -> bool {
        self.order_rejected.any() || self.order_newer.overflowed()
    }
    fn reject_order(&mut self, cx: &mut Cx<'_, Pending>, item: &str) {
        self.order_rejected.skip(
            cx,
            order_identity(item),
            Level::Warn,
            "Bitunix malformed order row; keeping its cached native family",
        );
    }
}
fn order_identity(item: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Identity<'a> {
        #[serde(
            borrow,
            default,
            deserialize_with = "text",
            alias = "id",
            rename = "orderId"
        )]
        raw: Str<'a>,
    }
    serde_json::from_str::<Identity<'_>>(item)
        .ok()
        .filter(|row| !row.raw.is_empty())
        .map(|row| row.raw.into_owned())
}
impl Read {
    fn reject_position(&mut self, cx: &mut Cx<'_, Pending>, item: &str) {
        #[derive(serde::Deserialize)]
        struct Identity<'a> {
            #[serde(
                borrow,
                default,
                alias = "instId",
                alias = "contract",
                alias = "instrument"
            )]
            symbol: Str<'a>,
        }
        let key = serde_json::from_str::<Identity<'_>>(item)
            .ok()
            .filter(|v| !v.symbol.is_empty())
            .map(|v| v.symbol.into_owned());
        self.position_rejected.skip(
            cx,
            key,
            Level::Warn,
            "Bitunix malformed position row; keeping its cached scope",
        );
    }
}
#[derive(Clone, Default)]
pub struct NativeOrder {
    pub raw_id: String,
    pub position_id: String,
    pub full_position: bool,
    pub attached: bool,
    pub tp: Option<f64>,
    pub sl: Option<f64>,
    pub tp_stop: Option<StopTriggerType>,
    pub sl_stop: Option<StopTriggerType>,
    pub tp_execution: String,
    pub sl_execution: String,
    pub tp_limit: f64,
    pub sl_limit: f64,
}
pub struct OrderReadProgress {
    serial: u64,
    issued: f64,
    timer: TimerId,
    dirty: bool,
    newer: RowAdmission<String>,
    rejected: RowAdmission<String>,
}
#[derive(Default)]
pub struct AccountState {
    pub(crate) recovery: crate::recovery::State,
    /// Symbols whose leverage/margin mode were read on first candle interest.
    pub symbol_settings: std::collections::HashSet<String>,
    /// Serial of the last balance read that was answered (reply or failure).
    pub balance_answered: u64,
    /// When any issuer last dispatched a balance read.
    pub balance_polled_at: f64,
    pub generations: [u64; 3],
    pub balance_write_revision: u64,
    pub position_write_revision: u64,
    /// Current ordinary or command-owned read; background refresh cannot supersede it.
    pub position_read: Option<(u64, f64)>,
    pub position_dirty: bool,
    pub serials: [u64; 4],
    pub initial_orders: [Option<Read>; 2],
    pub order_reads: [Option<OrderReadProgress>; 2],
    pub terminal: HashMap<String, f64>,
    pub timers: [Option<TimerId>; 3],
    pub tick: Option<TimerId>,
    pub count: u32,
    pub position_ids: HashMap<String, String>,
    pub ws_position_ids: HashMap<String, String>,
    pub native: HashMap<String, NativeOrder>,
    pub tifs: HashMap<String, TimeInForce>,
    pub reduces: HashMap<String, bool>,
    pub filled: HashMap<String, (f64, f64)>,
    pub pending_executions: HashMap<String, f64>,
    pub login_retry: Option<(TimerId, safe_cex_engine::SocketId)>,
}
/// The TS Bitunix tick (exchange metadata): balance every tick, the full
/// account every 32 ticks.
const TICK_MS: f64 = 500.0;
/// An unanswered balance read is abandoned after this long.
const BALANCE_STALL_MS: f64 = 10_000.0;
const POSITION_STALL_MS: f64 = 10_000.0;
const ORDER_STALL_MS: f64 = 10_000.0;

impl Axis {
    fn order_slot(self) -> Option<usize> {
        match self {
            Self::Orders => Some(0),
            Self::Tpsl => Some(1),
            _ => None,
        }
    }
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Balance => 0,
            Self::Positions => 1,
            Self::Orders | Self::Tpsl => 2,
        }
    }
}
pub fn order_side(s: &str) -> OrderSide {
    if s.eq_ignore_ascii_case("BUY") {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    }
}
pub fn order_type(s: &str) -> OrderType {
    match s.to_ascii_uppercase().as_str() {
        "MARKET" => OrderType::Market,
        "STOP_MARKET" => OrderType::StopLoss,
        "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
        _ => OrderType::Limit,
    }
}
pub fn tif(s: &str) -> TimeInForce {
    match s.to_ascii_uppercase().as_str() {
        "IOC" => TimeInForce::ImmediateOrCancel,
        "FOK" => TimeInForce::FillOrKill,
        "POST_ONLY" => TimeInForce::PostOnly,
        _ => TimeInForce::GoodTillCancel,
    }
}
pub fn trigger(s: &str) -> StopTriggerType {
    if s.to_ascii_uppercase().starts_with("LAST") {
        StopTriggerType::LastPrice
    } else {
        StopTriggerType::MarkPrice
    }
}
fn position_key(symbol: &str, side: PositionSide) -> String {
    format!(
        "{symbol}:{}",
        if side == PositionSide::Long {
            "long"
        } else {
            "short"
        }
    )
}

impl BitunixFutures {
    pub(crate) fn private_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.slots.has_primary() {
            cx.store.set_balance(Balance::default());
            cx.store.set_positions(vec![]);
            cx.store.set_orders(vec![]);
            return;
        }
        // Settings reset to defaults below; symbol reads must run again.
        self.account.symbol_settings.clear();
        let settings: Vec<_> = cx
            .store
            .markets()
            .iter()
            .enumerate()
            .map(|(i, _)| {
                (
                    i as u32,
                    MarketSetting {
                        leverage: 10.0,
                        margin_type: Some(MarginType::Cross),
                        ..MarketSetting::default()
                    },
                )
            })
            .collect();
        cx.store.set_settings(settings);
        self.connect(cx, Feed::Private);
        self.read_axis(cx, Axis::Balance);
        self.read_axis(cx, Axis::Positions);
        self.read_axis(cx, Axis::Orders);
        self.account.tick = Some(cx.set_timer(TICK_MS));
    }
    pub(crate) fn signed_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        params: Params,
        body: Option<String>,
        pending: Pending,
    ) {
        // By role, not method: a GET inside a trading job signs Primary.
        let role = match &pending {
            Pending::Private(read) => CredentialRole::Read {
                scope: Some(match read.axis {
                    Axis::Balance => safe_cex_engine::AccountReadScope::Balance,
                    Axis::Positions => safe_cex_engine::AccountReadScope::Positions,
                    Axis::Orders | Axis::Tpsl => safe_cex_engine::AccountReadScope::Orders,
                }),
            },
            Pending::SymbolSettings { .. } => CredentialRole::Read {
                scope: Some(safe_cex_engine::AccountReadScope::Settings),
            },
            Pending::Trade(_) => CredentialRole::Mutation,
            _ => CredentialRole::Read { scope: None },
        };
        let Ok((selection, identity)) = self.slots.select(role) else {
            self.fail_private(cx, pending, "Bitunix credentials required");
            return;
        };
        let identity = identity.clone();
        let testnet = self.options.testnet;
        let headers = if testnet {
            sign::dispatch::TESTNET_HEADERS
        } else {
            sign::dispatch::HEADERS
        }
        .iter()
        .map(|(name, value)| (*name, (*value).to_owned()))
        .collect();
        let query = params.query();
        let url = format!(
            "{REST}{path}{}{}",
            if query.is_empty() { "" } else { "?" },
            query
        );
        let recipe = sign::dispatch::Http::new(
            identity,
            method,
            url.clone(),
            params.signing(),
            body.clone(),
            testnet,
        );
        let id = if matches!(&pending,Pending::Private(r) if r.axis!=Axis::Balance)
            || matches!(&pending, Pending::History(_))
        {
            cx.request_streamed(method, url, headers, crate::recovery::LIST, pending)
        } else {
            cx.request(method, url, headers, body, pending)
        };
        selection.sign(cx, id, Box::new(recipe));
    }
    pub(crate) fn read_axis(&mut self, cx: &mut Cx<'_, Pending>, axis: Axis) {
        self.read_axis_for(cx, axis, None);
    }
    pub(crate) fn read_axis_for(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        axis: Axis,
        after: Option<u32>,
    ) {
        if !self.slots.has_primary() {
            return;
        }
        if self
            .account
            .recovery
            .blocked(cx, &crate::recovery::Key::Axis(axis))
        {
            if let Some(command) = after {
                self.prepared(cx, command, false);
            }
            return;
        }
        let order_token = if axis.order_slot().is_some() {
            let Some(token) = cx.store.begin_orders_snapshot() else {
                self.account.initial_orders = [None, None];
                for progress in &mut self.account.order_reads {
                    if let Some(progress) = progress.take() {
                        cx.clear_timer(progress.timer);
                    }
                }
                if let Some(timer) = self.account.timers[2].take() {
                    cx.clear_timer(timer);
                }
                cx.notice(
                    Level::Error,
                    code::SNAPSHOT_DISCARDED,
                    "Bitunix order snapshot admission exhausted",
                );
                if let Some(command) = after {
                    self.prepared(cx, command, false);
                }
                return;
            };
            Some(token)
        } else {
            None
        };
        if axis == Axis::Balance
            && after.is_none()
            && self.account.balance_answered != self.account.serials[0]
            && cx.now < self.account.balance_polled_at + BALANCE_STALL_MS
        {
            return;
        }
        if axis == Axis::Positions {
            if after.is_none()
                && let Some((_, issued)) = self.account.position_read
                && cx.now < issued + POSITION_STALL_MS
            {
                self.account.position_dirty = true;
                if self.account.timers[1].is_none() {
                    self.account.timers[1] =
                        Some(cx.set_timer(issued + POSITION_STALL_MS - cx.now));
                }
                return;
            }
            if let Some(timer) = self.account.timers[1].take() {
                cx.clear_timer(timer);
            }
        }
        if let Some(slot) = axis.order_slot() {
            if after.is_none()
                && let Some(progress) = self.account.order_reads[slot].as_mut()
                && cx.now < progress.issued + ORDER_STALL_MS
            {
                progress.dirty = true;
                return;
            }
            if let Some(progress) = self.account.order_reads[slot].take() {
                cx.clear_timer(progress.timer);
            }
        }
        let path = match axis {
            Axis::Balance => ACCOUNT,
            Axis::Positions => POSITIONS,
            Axis::Orders => ORDERS,
            Axis::Tpsl => TPSL,
        };
        let params = if axis == Axis::Balance {
            Params::default().str("marginCoin", "USDT")
        } else {
            Params::default()
        };
        let slot = match axis {
            Axis::Balance => 0,
            Axis::Positions => 1,
            Axis::Orders => 2,
            Axis::Tpsl => 3,
        };
        self.account.serials[slot] += 1;
        if axis == Axis::Balance {
            self.account.balance_polled_at = cx.now;
        }
        let read = Read {
            after,
            serial: self.account.serials[slot],
            axis,
            generation: self.account.generations[axis.index()],
            balance_write_revision: self.account.balance_write_revision,
            position_write_revision: self.account.position_write_revision,
            order_token,
            positions: vec![],
            position_rejected: Default::default(),
            position_admitted: vec![],
            identities: vec![],
            orders: vec![],
            order_rejected: Default::default(),
            order_accepted: HashSet::new(),
            order_newer: Default::default(),
            native: vec![],
            invalid: false,
            baselines: vec![],
        };
        if let Some(slot) = axis.order_slot() {
            self.account.order_reads[slot] = Some(OrderReadProgress {
                serial: read.serial,
                issued: cx.now,
                timer: cx.set_timer(ORDER_STALL_MS),
                dirty: false,
                newer: Default::default(),
                rejected: Default::default(),
            });
        }
        if axis == Axis::Positions {
            self.account.position_read = Some((read.serial, cx.now));
            self.account.position_dirty = false;
            self.account.timers[1] = Some(cx.set_timer(POSITION_STALL_MS));
        }
        self.signed_request(cx, Method::Get, path, params, None, Pending::Private(read));
    }
    /// Like TS listenOHLCV: read a symbol's leverage and margin mode once,
    /// so a flat symbol reports its real settings instead of defaults.
    pub(crate) fn read_symbol_settings(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if !self.slots.has_primary()
            || cx.store.market(symbol).is_none()
            || self
                .account
                .recovery
                .blocked(cx, &crate::recovery::Key::Settings(symbol.into()))
            || !self.account.symbol_settings.insert(symbol.into())
        {
            return;
        }
        self.account.recovery.retain_settings(cx);
        let serial = self.account.recovery.next_settings(symbol);
        let params = Params::default()
            .str("marginCoin", "USDT")
            .str("symbol", symbol);
        let pending = Pending::SymbolSettings {
            symbol: symbol.into(),
            write_revision: self.account.position_write_revision,
            serial,
        };
        self.signed_request(cx, Method::Get, SYMBOL_SETTINGS, params, None, pending);
    }
    pub(crate) fn symbol_settings_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        write_revision: u64,
        serial: u64,
        response: Response<'_>,
    ) {
        // A newer leverage/margin acknowledgement supersedes this read.
        if !self.account.recovery.settings_current(symbol, serial) {
            return;
        }
        if write_revision != self.account.position_write_revision {
            self.account.symbol_settings.remove(symbol);
            self.account.recovery.defer(
                cx,
                crate::recovery::Key::Settings(symbol.into()),
                cx.now + 1000.0,
            );
            return;
        }
        let Some(settings) = decode_symbol_settings(&response) else {
            self.settings_failed(cx, symbol, write_revision, serial, Some(&response));
            return;
        };
        let margin = if settings.margin_mode.eq_ignore_ascii_case("ISOLATION") {
            MarginType::Isolated
        } else {
            MarginType::Cross
        };
        self.account
            .recovery
            .succeeded(cx, &crate::recovery::Key::Settings(symbol.into()));
        cx.store.update_setting(symbol, |s| {
            s.leverage = settings.leverage;
            s.margin_type = Some(margin);
        });
        cx.store.update_positions(|p| {
            if p.symbol == symbol {
                p.leverage = settings.leverage;
                p.margin_type = Some(margin);
            }
        });
    }
    pub(crate) fn fail_private(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Candles { context } => {
                let _ = cx.fail_context(&context, crate::error::malformed(error, false));
            }
            Pending::Trade(request) => {
                let kind = if !self.slots.has_primary() {
                    safe_cex_engine::ErrorKind::AuthInvalid
                } else {
                    safe_cex_engine::ErrorKind::Internal
                };
                self.trade_error(cx, request, crate::error::not_sent(error, kind));
            }
            Pending::History(read) => self.history_sign_failed(cx, read, error),
            Pending::Private(read) => self.account_failed(cx, read, None),
            Pending::SymbolSettings {
                symbol,
                write_revision,
                serial,
            } => self.settings_failed(cx, &symbol, write_revision, serial, None),
            _ => cx.retried_failure("bitunix.account:2", error),
        }
    }
    pub(crate) fn login(&self, cx: &mut Cx<'_, Pending>, socket: safe_cex_engine::SocketId) {
        if let Ok((_, identity)) = self.slots.select(CredentialRole::SessionAuth) {
            cx.send_signed(
                socket,
                String::new(),
                Box::new(sign::dispatch::WsLogin::new(identity.clone())),
            );
        }
    }
    pub(crate) fn position_answered(&mut self, cx: &mut Cx<'_, Pending>, read: &Read) {
        if read.axis != Axis::Positions
            || !self
                .account
                .position_read
                .is_some_and(|(serial, _)| serial == read.serial)
        {
            return;
        }
        self.account.position_read = None;
        if let Some(timer) = self.account.timers[1].take() {
            cx.clear_timer(timer);
        }
        let dirty = std::mem::take(&mut self.account.position_dirty);
        if dirty || read.generation != self.account.generations[1] {
            self.debounce(cx, Axis::Positions);
        }
    }
    /// Decorate before strict prerequisite validation and before releasing the token.
    pub(crate) fn order_overlap(&mut self, read: &mut Read) {
        if let Some(slot) = read.axis.order_slot()
            && let Some(progress) = self.account.order_reads[slot].as_mut()
            && progress.serial == read.serial
        {
            read.order_newer.extend(std::mem::take(&mut progress.newer));
            read.order_rejected
                .extend(std::mem::take(&mut progress.rejected));
        }
    }
    fn observe_order(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        tpsl: bool,
        raw: Option<String>,
        newer: bool,
    ) {
        let slot = usize::from(tpsl);
        let mut overflow = false;
        if let Some(progress) = self.account.order_reads[slot].as_mut() {
            progress.dirty = true;
            let coverage = if newer {
                &mut progress.newer
            } else {
                &mut progress.rejected
            };
            let previous = coverage.overflowed();
            coverage.reject(raw.clone());
            overflow |= !previous && coverage.overflowed();
        }
        if let Some(read) = self.account.initial_orders[slot].as_mut() {
            let coverage = if newer {
                &mut read.order_newer
            } else {
                &mut read.order_rejected
            };
            let previous = coverage.overflowed();
            coverage.reject(raw);
            overflow |= !previous && coverage.overflowed();
        }
        if overflow {
            cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED, "Bitunix order overlap coverage exceeded its bound; keeping cached endpoint families");
        }
    }
    pub(crate) fn order_answered(&mut self, cx: &mut Cx<'_, Pending>, read: &Read) {
        let Some(slot) = read.axis.order_slot() else {
            return;
        };
        if !self.account.order_reads[slot]
            .as_ref()
            .is_some_and(|p| p.serial == read.serial)
        {
            return;
        }
        let progress = self.account.order_reads[slot].take().unwrap();
        cx.clear_timer(progress.timer);
        if progress.dirty || read.generation != self.account.generations[2] {
            self.debounce(cx, Axis::Orders);
        }
    }
    pub(crate) fn debounce(&mut self, cx: &mut Cx<'_, Pending>, axis: Axis) {
        let i = axis.index();
        if self.account.timers[i].is_some() {
            return;
        }
        if let Some(t) = self.account.timers[i].take() {
            cx.clear_timer(t);
        }
        self.account.timers[i] = Some(cx.set_timer(1000.0));
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if let Some(keys) = self.account.recovery.timer(cx, timer) {
            for key in keys {
                match key {
                    crate::recovery::Key::Axis(axis) => self.read_axis(cx, axis),
                    crate::recovery::Key::Settings(symbol) => {
                        self.read_symbol_settings(cx, &symbol)
                    }
                }
            }
            return true;
        }
        if let Some(slot) = self
            .account
            .order_reads
            .iter()
            .position(|p| p.as_ref().is_some_and(|p| p.timer == timer))
        {
            let progress = self.account.order_reads[slot].as_mut().unwrap();
            let remaining = progress.issued + ORDER_STALL_MS - cx.now;
            if remaining > 0.0 {
                progress.timer = cx.set_timer(remaining);
            } else {
                self.read_axis(cx, if slot == 0 { Axis::Orders } else { Axis::Tpsl });
            }
            return true;
        }
        if let Some((owned_timer, socket)) = self.account.login_retry
            && owned_timer == timer
        {
            self.account.login_retry = None;
            if self
                .sockets
                .iter()
                .any(|s| s.active && s.id == socket && matches!(s.feed, Feed::Private) && s.opened)
            {
                self.login(cx, socket);
            }
            return true;
        }
        if self.account.tick == Some(timer) {
            self.account
                .terminal
                .retain(|_, at| cx.now - *at < 300_000.0);
            self.account.filled.retain(|id, _| {
                self.account.terminal.contains_key(id) || cx.store.order(id).is_some()
            });
            // Deferred metadata follows the same bounded lifetime as execution watermarks.
            self.account.pending_executions.retain(|id, _| {
                self.account.terminal.contains_key(id) || cx.store.order(id).is_some()
            });
            // A slow reply must not be superseded every tick (stale serials are
            // dropped), so poll again only once the last balance read finished.
            // A read lost without any answer cannot stall polling forever.
            if self.account.balance_answered == self.account.serials[0]
                || cx.now - self.account.balance_polled_at >= BALANCE_STALL_MS
            {
                self.read_axis(cx, Axis::Balance);
            }
            self.account.count += 1;
            if self.account.count.is_multiple_of(32) {
                for a in [Axis::Positions, Axis::Orders, Axis::Tpsl] {
                    self.read_axis(cx, a);
                }
            }
            self.account.tick = Some(cx.set_timer(TICK_MS));
            return true;
        }
        for axis in [Axis::Balance, Axis::Positions, Axis::Orders] {
            let i = axis.index();
            if self.account.timers[i] == Some(timer) {
                self.account.timers[i] = None;
                self.read_axis(cx, axis);
                if axis == Axis::Orders {
                    self.read_axis(cx, Axis::Tpsl);
                }
                return true;
            }
        }
        false
    }
    pub(crate) fn account_item(&mut self, cx: &mut Cx<'_, Pending>, read: &mut Read, item: &str) {
        let result = match read.axis {
            Axis::Positions => serde_json::from_str::<PositionRow<'_>>(item).map(|p| {
                if p.symbol.is_empty() {
                    read.reject_position(cx, item);
                    return;
                }
                let qty = p.quantity();
                let side = if p.side.eq_ignore_ascii_case("BUY")
                    || p.side.eq_ignore_ascii_case("LONG")
                    || (p.side.eq_ignore_ascii_case("BOTH") && qty >= 0.0)
                {
                    PositionSide::Long
                } else {
                    PositionSide::Short
                };
                let mode = if p.position_mode.eq_ignore_ascii_case("ONE_WAY") {
                    PositionMode::Oneway
                } else {
                    PositionMode::Hedged
                };
                let id = if mode == PositionMode::Oneway {
                    p.symbol.to_string()
                } else {
                    position_key(&p.symbol, side)
                };
                let setting = cx.store.setting(&p.symbol);
                let margin = match p.margin_mode.to_ascii_uppercase().as_str() {
                    "ISOLATION" | "ISOLATED" => MarginType::Isolated,
                    "CROSS" => MarginType::Cross,
                    _ => setting
                        .and_then(|s| s.margin_type)
                        .unwrap_or(MarginType::Cross),
                };
                let key = position_key(&p.symbol, side);
                read.position_admitted.push(PositionAdmission {
                    id: id.clone(),
                    symbol: p.symbol.to_string(),
                    route: key.clone(),
                    mode,
                });
                if !p.position_id.is_empty() {
                    read.identities.push((key, p.position_id.to_string()));
                }
                if qty != 0.0 {
                    read.positions.push(Position {
                        id,
                        symbol: p.symbol.to_string(),
                        side,
                        entry_price: p.entry(),
                        notional: p.notional(),
                        leverage: if p.leverage.0 != 0.0 {
                            p.leverage.0
                        } else {
                            setting.map_or(10.0, |s| s.leverage)
                        },
                        margin_type: Some(margin),
                        position_mode: Some(mode),
                        unrealized_pnl: p.unrealized_pnl.0,
                        // Reported net of fees AND funding. Never subtract either again.
                        realized_pnl: Some(p.realized_pnl.0),
                        contracts: qty.abs(),
                        liquidation_price: p.liq_price.0,
                        margin_size: None,
                        last_update: None,
                    });
                }
                let _ = (p.fee, p.funding, &p.event);
            }),
            Axis::Orders | Axis::Tpsl => serde_json::from_str::<OrderRow<'_>>(item).map(|o| {
                if o.order_id.is_empty() || o.symbol.is_empty() {
                    read.reject_order(cx, item);
                    return;
                }
                read.order_accepted.insert(o.order_id.to_string());
                if self.account.terminal.contains_key(o.order_id.as_ref()) {
                    return;
                }
                if read.axis == Axis::Orders {
                    read.baselines.push((
                        o.order_id.to_string(),
                        (
                            o.trade_qty.0,
                            o.trade_qty.0
                                * if o.average_price.0 > 0.0 {
                                    o.average_price.0
                                } else {
                                    o.price.0
                                },
                        ),
                    ));
                }
                let (orders, native) = self.map_orders(cx, &o, read.axis == Axis::Tpsl, false);
                read.orders.extend(orders);
                read.native.extend(native);
            }),
            Axis::Balance => return,
        };
        if result.is_err() {
            if read.axis == Axis::Positions {
                read.reject_position(cx, item);
            } else if matches!(read.axis, Axis::Orders | Axis::Tpsl) {
                read.reject_order(cx, item);
            } else {
                read.invalid = true;
            }
        }
    }
    pub(crate) fn read_current(&self, store: &safe_cex_engine::Store, read: &Read) -> bool {
        let slot = match read.axis {
            Axis::Balance => 0,
            Axis::Positions => 1,
            Axis::Orders => 2,
            Axis::Tpsl => 3,
        };
        if read.serial != self.account.serials[slot] {
            return false;
        }
        if read.axis == Axis::Balance && read.after.is_none() {
            read.balance_write_revision == self.account.balance_write_revision
        } else if read.axis == Axis::Positions && read.after.is_none() {
            read.position_write_revision == self.account.position_write_revision
        } else if read.axis.order_slot().is_some() {
            read.orders_current(store)
                && (read.after.is_none()
                    || read.generation == self.account.generations[read.axis.index()])
        } else {
            read.generation == self.account.generations[read.axis.index()]
        }
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        response: Option<&Response<'_>>,
    ) {
        self.position_answered(cx, &read);
        self.order_answered(cx, &read);
        if read.axis == Axis::Balance {
            self.account.balance_answered = self.account.balance_answered.max(read.serial);
        }
        if self.read_current(cx.store, &read) {
            self.account.recovery.failed(
                cx,
                crate::recovery::Key::Axis(read.axis),
                response.map_or(0.0, crate::recovery::pause),
            );
            crate::recovery::notice(cx, response);
        }
        if let Some(command) = read.after {
            self.prepared(cx, command, false);
        }
    }
    pub(crate) fn settings_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        write_revision: u64,
        serial: u64,
        response: Option<&Response<'_>>,
    ) {
        if !self.account.recovery.settings_current(symbol, serial) {
            return;
        }
        self.account.symbol_settings.remove(symbol);
        let key = crate::recovery::Key::Settings(symbol.into());
        if write_revision != self.account.position_write_revision {
            self.account.recovery.defer(cx, key, cx.now + 1000.0);
            return;
        }
        self.account
            .recovery
            .failed(cx, key, response.map_or(0.0, crate::recovery::pause));
        crate::recovery::notice(cx, response);
    }
    pub(crate) fn account_auth(
        &self,
        pending: &Pending,
        response: &Response<'_>,
        store: &safe_cex_engine::Store,
    ) -> Option<safe_cex_engine::AuthOutcome> {
        use safe_cex_engine::AuthOutcome::{Accepted, Rejected};
        let rejected = crate::recovery::refused(response);
        let success = crate::recovery::success(response);
        match pending {
            Pending::Private(read) if self.read_current(store, read) => {
                let outcome = if rejected {
                    Some(Rejected)
                } else if success {
                    if read.axis == Axis::Balance {
                        data(response.body)
                            .ok()
                            .and_then(|body| extract_account(body, 0))
                            .map(|_| Accepted)
                    } else {
                        safe_cex_engine::AuthOutcome::accepted_list(response, crate::recovery::LIST)
                    }
                } else {
                    None
                };
                match read.axis {
                    Axis::Orders => self.account.recovery.orders_auth.report(0, outcome),
                    Axis::Tpsl => self.account.recovery.orders_auth.report(1, outcome),
                    _ => outcome,
                }
            }
            Pending::SymbolSettings {
                symbol,
                write_revision,
                serial,
            } if *write_revision == self.account.position_write_revision
                && self.account.recovery.settings_current(symbol, *serial) =>
            {
                let outcome = if rejected {
                    Some(Rejected)
                } else {
                    decode_symbol_settings(response).map(|_| Accepted)
                };
                self.account.recovery.settings_auth(symbol, outcome)
            }
            _ => None,
        }
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut read: Read,
        response: Response<'_>,
    ) {
        self.position_answered(cx, &read);
        self.order_answered(cx, &read);
        if read.axis == Axis::Balance {
            self.account.balance_answered = self.account.balance_answered.max(read.serial);
        }
        if !self.read_current(cx.store, &read) {
            self.debounce(cx, read.axis);
            return;
        }
        let body = match data(response.body) {
            Ok(b)
                if crate::recovery::success(&response)
                    && !read.invalid
                    && (read.axis != Axis::Balance || extract_account(b, 0).is_some()) =>
            {
                b
            }
            _ => {
                self.account_failed(cx, read, Some(&response));
                return;
            }
        };
        self.account
            .recovery
            .succeeded(cx, &crate::recovery::Key::Axis(read.axis));
        if !cx.store.loaded().orders && read.partial_orders() {
            cx.notice(
                Level::Warn,
                code::SNAPSHOT_DISCARDED,
                "Bitunix partial initial orders cannot establish readiness",
            );
            self.debounce(cx, Axis::Orders);
            return;
        }
        if !cx.store.loaded().orders && matches!(read.axis, Axis::Orders | Axis::Tpsl) {
            let slot = usize::from(read.axis == Axis::Tpsl);
            self.account.initial_orders[slot] = Some(read);
            if self.account.initial_orders.iter().all(Option::is_some) {
                for slot in 0..2 {
                    if self.account.initial_orders[slot].as_ref().is_some_and(|r| {
                        !r.orders_current(cx.store)
                            || r.partial_orders()
                            || (r.after.is_some() && r.generation != self.account.generations[2])
                            || r.serial
                                != self.account.serials[if r.axis == Axis::Orders { 2 } else { 3 }]
                    }) {
                        self.account.initial_orders[slot] = None;
                        self.debounce(cx, Axis::Orders);
                        return;
                    }
                }
                let mut regular = self.account.initial_orders[0].take().unwrap();
                let mut tpsl = self.account.initial_orders[1].take().unwrap();
                let tokens = [
                    regular.order_token.take().unwrap(),
                    tpsl.order_token.take().unwrap(),
                ];
                if !cx.store.commit_orders_snapshot(tokens, |current| {
                    let regular_rows = self.apply_orders_snapshot(current, regular);
                    self.apply_orders_snapshot(&regular_rows, tpsl)
                }) {
                    self.debounce(cx, Axis::Orders);
                }
            }
            return;
        }
        match read.axis {
            Axis::Balance => {
                let result =
                    extract_account(body, 0).ok_or_else(|| "missing Bitunix account".to_string());
                match result {
                    Ok(a) => {
                        let previous = cx.store.hedged();
                        let hedged = !a.position_mode.eq_ignore_ascii_case("ONE_WAY");
                        cx.store.set_hedged(hedged);
                        cx.store.set_balance(Balance {
                            free: a.available.unwrap_or_default().0,
                            used: a.margin.unwrap_or_default().0,
                            total: a.available.unwrap_or_default().0
                                + a.margin.unwrap_or_default().0
                                + a.frozen.unwrap_or_default().0,
                            upnl: if a.cross_unrealized_pnl.is_some()
                                || a.isolation_unrealized_pnl.is_some()
                            {
                                a.cross_unrealized_pnl.unwrap_or_default().0
                                    + a.isolation_unrealized_pnl.unwrap_or_default().0
                            } else {
                                a.unrealized_pnl.unwrap_or_default().0
                            },
                        });
                        if previous != hedged {
                            self.debounce(cx, Axis::Positions);
                            self.debounce(cx, Axis::Orders);
                        }
                    }
                    Err(e) => cx.notice(Level::Warn, code::STREAM_MALFORMED, e.to_string()),
                }
            }
            Axis::Positions => {
                if read.position_rejected.any() && !cx.store.loaded().positions {
                    cx.notice(
                        Level::Warn,
                        code::SNAPSHOT_DISCARDED,
                        "Bitunix partial initial positions cannot establish readiness",
                    );
                    self.debounce(cx, Axis::Positions);
                    return;
                }
                read.position_admitted
                    .retain(|p| !read.position_rejected.conflicts(&p.symbol));
                read.positions
                    .retain(|p| !read.position_rejected.conflicts(&p.symbol));
                let admitted_ids: HashSet<_> = read
                    .position_admitted
                    .iter()
                    .map(|p| p.id.as_str())
                    .collect();
                let admitted_routes: HashSet<_> = read
                    .position_admitted
                    .iter()
                    .map(|p| p.route.as_str())
                    .collect();
                let mut held: Vec<_> = cx
                    .store
                    .positions()
                    .iter()
                    .filter(|p| {
                        read.position_rejected.holds(&p.symbol)
                            && !admitted_ids.contains(p.id.as_str())
                    })
                    .cloned()
                    .collect();
                if read.position_rejected.any() {
                    let mut modes = read
                        .position_admitted
                        .iter()
                        .map(|p| p.mode)
                        .chain(held.iter().filter_map(|p| p.position_mode));
                    if let Some(first) = modes.next()
                        && modes.any(|mode| mode != first)
                    {
                        cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED, "Bitunix partial positions disagree on position mode; keeping last snapshot");
                        self.debounce(cx, Axis::Positions);
                        return;
                    }
                }
                let mut identities: HashMap<_, _> = read
                    .identities
                    .into_iter()
                    .filter(|(key, _)| admitted_routes.contains(key.as_str()))
                    .collect();
                for (key, native) in &self.account.position_ids {
                    let symbol = key
                        .strip_suffix(":long")
                        .or_else(|| key.strip_suffix(":short"));
                    if symbol.is_some_and(|symbol| read.position_rejected.holds(symbol))
                        && !admitted_routes.contains(key.as_str())
                    {
                        identities.insert(key.clone(), native.clone());
                    }
                }
                for p in &read.positions {
                    cx.store.update_setting(&p.symbol, |s| {
                        s.leverage = p.leverage;
                        s.margin_type = p.margin_type;
                    });
                }
                self.account.position_ids = identities;
                held.extend(read.positions);
                cx.store.set_positions(held);
                self.read_axis(cx, Axis::Tpsl);
            }
            Axis::Orders | Axis::Tpsl => {
                self.commit_orders(cx, read);
            }
        }
    }
    fn commit_orders(&mut self, cx: &mut Cx<'_, Pending>, mut read: Read) {
        let Some(token) = read.order_token.take() else {
            return;
        };
        if !cx
            .store
            .commit_orders_snapshot([token], |current| self.apply_orders_snapshot(current, read))
        {
            self.debounce(cx, Axis::Orders);
        }
    }
    fn apply_orders_snapshot(&mut self, current: &[Order], mut read: Read) -> Vec<Order> {
        let tpsl = read.axis == Axis::Tpsl;
        read.order_accepted
            .retain(|raw| !read.order_rejected.conflicts(raw) && !read.order_newer.holds(raw));
        let incoming_raw: HashMap<_, _> = read
            .native
            .iter()
            .map(|(id, meta)| (id.as_str(), meta.raw_id.as_str()))
            .collect();
        read.orders.retain(|row| {
            let raw = incoming_raw
                .get(row.id.as_str())
                .copied()
                .unwrap_or(&row.id);
            !read.order_rejected.conflicts(raw) && !read.order_newer.holds(raw)
        });
        read.baselines
            .retain(|(raw, _)| read.order_accepted.contains(raw));
        let optimistic: std::collections::HashSet<_> = current
            .iter()
            .filter(|o| matches!(o.status, OrderStatus::Creating | OrderStatus::Canceling))
            .map(|o| o.id.clone())
            .collect();
        read.orders.retain(|o| !optimistic.contains(&o.id));
        read.native.retain(|(id, meta)| {
            !optimistic.contains(id)
                && !read.order_rejected.conflicts(&meta.raw_id)
                && !read.order_newer.holds(&meta.raw_id)
        });
        let mut orders: Vec<_> = current
            .iter()
            .filter(|o| {
                optimistic.contains(&o.id)
                    || self.account.native.get(&o.id).is_some_and(|n| {
                        !n.attached
                            && (o.kind == OrderType::StopLoss || o.kind == OrderType::TakeProfit)
                    }) != tpsl
                    || {
                        let raw = self
                            .account
                            .native
                            .get(&o.id)
                            .map_or(o.id.as_str(), |meta| meta.raw_id.as_str());
                        read.order_newer.holds(raw)
                            || read.order_rejected.holds(raw) && !read.order_accepted.contains(raw)
                    }
            })
            .cloned()
            .collect();
        let retained: std::collections::HashSet<_> = orders.iter().map(|o| o.id.clone()).collect();
        self.account.native.retain(|id, _| retained.contains(id));
        for (id, baseline) in read.baselines {
            // A snapshot must not consume an observed execution awaiting price or side.
            if self.account.pending_executions.contains_key(&id) {
                continue;
            }
            let previous = self.account.filled.entry(id).or_insert(baseline);
            if baseline.0 > previous.0 {
                *previous = baseline;
            }
        }
        orders.extend(read.orders);
        self.account.native.extend(read.native);
        orders
    }
    pub(crate) fn map_orders(
        &self,
        cx: &Cx<'_, Pending>,
        o: &OrderRow<'_>,
        tpsl: bool,
        ws: bool,
    ) -> (Vec<Order>, Vec<(String, NativeOrder)>) {
        let raw = o.order_id.to_string();
        let reduce = o
            .reduce_only
            .as_ref()
            .and_then(Supported::boolean)
            .unwrap_or_else(|| {
                self.account
                    .reduces
                    .get(o.client_id.as_ref())
                    .copied()
                    .unwrap_or(false)
            });
        let side = if tpsl {
            cx.store
                .positions()
                .iter()
                .find(|p| {
                    self.account
                        .position_ids
                        .get(&position_key(&p.symbol, p.side))
                        .is_some_and(|id| id == o.position_id.as_ref())
                })
                .map_or_else(
                    || order_side(&o.side),
                    |p| {
                        if p.side == PositionSide::Long {
                            OrderSide::Sell
                        } else {
                            OrderSide::Buy
                        }
                    },
                )
        } else {
            order_side(&o.side)
        };
        let kind = order_type(if o.order_type.is_empty() {
            &o.kind
        } else {
            &o.order_type
        });
        let amount = o.qty.0;
        let filled = if ws {
            o.deal_amount.unwrap_or_default().0
        } else {
            o.trade_qty.0
        };
        let native = NativeOrder {
            raw_id: raw.clone(),
            position_id: o.position_id.to_string(),
            full_position: o.kind.eq_ignore_ascii_case("POSITION_TPSL"),
            attached: false,
            tp: o.tp_price.map(|n| n.0),
            sl: o.sl_price.map(|n| n.0),
            tp_stop: Some(trigger(&o.tp_stop_type)),
            sl_stop: Some(trigger(&o.sl_stop_type)),
            tp_execution: o.tp_order_type.to_ascii_uppercase(),
            sl_execution: o.sl_order_type.to_ascii_uppercase(),
            tp_limit: o.tp_order_price.0,
            sl_limit: o.sl_order_price.0,
        };
        let base = Order {
            id: raw.clone(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: o.symbol.to_string(),
            kind,
            side,
            price: o.price.0,
            amount,
            filled,
            remaining: amount - filled,
            reduce_only: reduce,
            close: None,
            client_order_id: if o.client_id.is_empty() {
                None
            } else {
                Some(o.client_id.to_string())
            },
            time_in_force: Some(
                self.account
                    .tifs
                    .get(o.client_id.as_ref())
                    .copied()
                    .unwrap_or_else(|| tif(&o.effect)),
            ),
            last_update: if time_ms(&o.mtime) != 0.0 {
                time_ms(&o.mtime)
            } else {
                cx.now
            },
            // Only protection legs trigger; a plain order has no trigger basis.
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        };
        let mut orders = vec![];
        let mut natives = vec![];
        if !tpsl {
            orders.push(base.clone());
            natives.push((raw.clone(), native.clone()));
        }
        for (price, qty, kind, leg, stop) in [
            (
                native.tp,
                o.tp_qty.0,
                OrderType::TakeProfit,
                "tp",
                native.tp_stop,
            ),
            (
                native.sl,
                o.sl_qty.0,
                OrderType::StopLoss,
                "sl",
                native.sl_stop,
            ),
        ] {
            let Some(price) = price else {
                continue;
            };
            let both = native.tp.is_some() && native.sl.is_some();
            let id = if !tpsl {
                format!("{raw}__{leg}")
            } else if both {
                format!(
                    "{raw}__{}__{leg}",
                    if o.position_id.is_empty() {
                        "position"
                    } else {
                        &o.position_id
                    }
                )
            } else {
                raw.clone()
            };
            let mut row = base.clone();
            row.id = id.clone();
            row.kind = kind;
            row.price = price;
            row.amount = if tpsl { qty } else { amount };
            row.filled = 0.0;
            row.remaining = row.amount;
            row.side = if tpsl { side } else { side.opposite() };
            row.reduce_only = if tpsl { !ws && reduce || ws } else { true };
            row.trigger_type = stop;
            // A caller ID names exactly one order: the parent, not its virtual legs.
            if !tpsl {
                row.client_order_id = None;
            }
            row.time_in_force = if tpsl {
                None
            } else {
                Some(TimeInForce::GoodTillCancel)
            };
            let mut n = native.clone();
            n.attached = !tpsl;
            orders.push(row);
            natives.push((id, n));
        }
        (orders, natives)
    }
    pub(crate) fn private_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: safe_cex_engine::SocketId,
        m: &Message<'_>,
    ) {
        let operation = if m.op.is_empty() {
            m.ch.as_ref()
        } else {
            m.op.as_ref()
        };
        if operation == "login" {
            #[derive(serde::Deserialize)]
            struct Login {
                result: bool,
            }
            let result = m
                .data
                .and_then(|raw| serde_json::from_str::<Login>(raw.get()).ok())
                .map(|login| login.result);
            if result == Some(true) {
                if let Some((timer, owner)) = self.account.login_retry
                    && owner == socket
                {
                    self.account.login_retry = None;
                    cx.clear_timer(timer);
                }
                return;
            }
            if result == Some(false) {
                cx.notice(
                    Level::Error,
                    code::AUTH_REJECTED,
                    "Bitunix private ws login failed, double check your key and secret",
                );
            } else {
                cx.notice(
                    Level::Warn,
                    code::STREAM_MALFORMED,
                    "Malformed Bitunix private login response",
                );
            }
            if self
                .account
                .login_retry
                .is_some_and(|(_, owner)| owner == socket)
            {
                return;
            }
            if let Some((timer, _)) = self.account.login_retry.take() {
                cx.clear_timer(timer);
            }
            self.account.login_retry = Some((cx.set_timer(2000.0), socket));
            return;
        }
        let Some(raw) = m.data else {
            if matches!(operation, "order" | "tpsl") {
                self.observe_order(cx, m.ch == "tpsl", None, false);
                self.account.generations[2] += 1;
                self.debounce(cx, Axis::Orders);
                row_skipped(
                    cx,
                    Level::Warn,
                    "Bitunix order event missing data; keeping cached families",
                );
            }
            return;
        };
        let text = raw.get();
        match operation {
            "balance" => {
                self.account.generations[0] += 1;
                self.debounce(cx, Axis::Balance);
            }
            "position" => {
                if let Ok(p) = serde_json::from_str::<PositionRow<'_>>(text) {
                    let side = if p.side.eq_ignore_ascii_case("BUY")
                        || p.side.eq_ignore_ascii_case("LONG")
                    {
                        PositionSide::Long
                    } else {
                        PositionSide::Short
                    };
                    let key = position_key(&p.symbol, side);
                    if p.event == "CLOSE" {
                        self.account.ws_position_ids.remove(&key);
                    } else if !p.position_id.is_empty() {
                        self.account
                            .ws_position_ids
                            .insert(key, p.position_id.to_string());
                    }
                }
                self.account.generations[1] += 1;
                self.debounce(cx, Axis::Positions);
            }
            "order" | "tpsl" => {
                let tpsl = m.ch == "tpsl";
                if let Ok(o) = serde_json::from_str::<OrderRow<'_>>(text) {
                    let raw = o.order_id.as_ref();
                    if raw.is_empty() || o.symbol.is_empty() {
                        self.observe_order(cx, tpsl, order_identity(text), false);
                        self.account.generations[2] += 1;
                        self.debounce(cx, Axis::Orders);
                        row_skipped(cx, Level::Warn, "Bitunix order event missing identity");
                        return;
                    }
                    let emitted = self.account.filled.get(raw).map_or(0.0, |v| v.0);
                    let (filled, updated, closed) = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|row| {
                            row.id == raw
                                || self
                                    .account
                                    .native
                                    .get(&row.id)
                                    .is_some_and(|meta| meta.raw_id == raw)
                        })
                        .fold(
                            (emitted, 0.0_f64, false),
                            |(filled, updated, closed), row| {
                                (
                                    filled.max(row.filled),
                                    updated.max(row.last_update),
                                    closed || row.status == OrderStatus::Closed,
                                )
                            },
                        );
                    let timestamp = time_ms(&o.mtime);
                    let stale_timestamp = timestamp > 0.0 && timestamp < updated;
                    let cumulative = o.deal_amount.map(|value| value.0);
                    // Visible cumulative quantity can advance before execution metadata arrives.
                    // Recovery advances the emission watermark, even if quantity is unchanged.
                    let advancing_execution = !tpsl
                        && matches!(o.event.as_ref(), "CLOSE" | "UPDATE")
                        && cumulative.is_some_and(|value| value > emitted);
                    // Fence stale events before touching raw protection metadata too.
                    // A late terminal execution can still advance a canceled order.
                    if (!tpsl
                        && cumulative
                            .or_else(|| (o.event == "CREATE").then_some(0.0))
                            .is_some_and(|value| value < filled))
                        || (stale_timestamp && !advancing_execution)
                        || (closed && o.event == "CLOSE" && o.order_status != "FILLED")
                    {
                        self.debounce(cx, Axis::Orders);
                        return;
                    }
                    if self.account.terminal.contains_key(raw)
                        && o.event != "CLOSE"
                        && !(o.event == "UPDATE"
                            && !tpsl
                            && cumulative.is_some_and(|value| value > emitted))
                    {
                        self.debounce(cx, Axis::Orders);
                        return;
                    }
                    // Capture the reconciled parent before CREATE can synthesize defaults.
                    let execution_parent = cx
                        .store
                        .order(raw)
                        .filter(|row| row.symbol == o.symbol.as_ref())
                        .map(|row| (row.side, row.price));
                    if !stale_timestamp {
                        for n in self.account.native.values_mut().filter(|n| n.raw_id == raw) {
                            if let Some(p) = o.tp_price {
                                n.tp = Some(p.0);
                            }
                            if let Some(p) = o.sl_price {
                                n.sl = Some(p.0);
                            }
                            if !o.tp_stop_type.is_empty() {
                                n.tp_stop = Some(trigger(&o.tp_stop_type));
                            }
                            if !o.sl_stop_type.is_empty() {
                                n.sl_stop = Some(trigger(&o.sl_stop_type));
                            }
                            if !o.tp_order_type.is_empty() {
                                n.tp_execution = o.tp_order_type.to_ascii_uppercase();
                            }
                            if !o.sl_order_type.is_empty() {
                                n.sl_execution = o.sl_order_type.to_ascii_uppercase();
                            }
                            if o.tp_order_price.0 != 0.0 {
                                n.tp_limit = o.tp_order_price.0;
                            }
                            if o.sl_order_price.0 != 0.0 {
                                n.sl_limit = o.sl_order_price.0;
                            }
                        }
                    }
                    if o.event == "CREATE"
                        && !self.account.terminal.contains_key(raw)
                        // A sparse CREATE must not manufacture a parent side for later fills.
                        && (tpsl
                            || o.side.eq_ignore_ascii_case("BUY")
                            || o.side.eq_ignore_ascii_case("SELL"))
                    {
                        let (rows, native) = self.map_orders(cx, &o, tpsl, true);
                        if !o.client_id.is_empty() && !o.effect.is_empty() {
                            self.account
                                .tifs
                                .insert(o.client_id.to_string(), tif(&o.effect));
                        }
                        for row in rows {
                            cx.store.upsert_order(row);
                        }
                        self.account.native.extend(native);
                    } else if o.event != "CREATE"
                        || (!tpsl && execution_parent.is_some() && o.side.is_empty())
                    {
                        if o.event == "CLOSE" {
                            self.account.terminal.insert(raw.to_owned(), cx.now);
                        }
                        let ids: Vec<_> = cx
                            .store
                            .orders()
                            .iter()
                            .filter(|r| {
                                r.id == raw
                                    || tpsl
                                        && self
                                            .account
                                            .native
                                            .get(&r.id)
                                            .is_some_and(|n| n.raw_id == raw)
                            })
                            .map(|r| r.id.clone())
                            .collect();
                        for id in ids {
                            cx.store.update_order(&id, |r| {
                                let timestamp = time_ms(&o.mtime);
                                if timestamp > r.last_update {
                                    r.last_update = timestamp;
                                }
                                if o.event == "CLOSE" {
                                    if o.order_status == "FILLED" {
                                        r.status = OrderStatus::Closed;
                                    } else if r.status != OrderStatus::Closed {
                                        r.status = OrderStatus::Canceled;
                                    }
                                    if !tpsl {
                                        r.remaining = 0.0;
                                    }
                                }
                                if tpsl {
                                    let qty = if r.kind == OrderType::TakeProfit {
                                        o.tp_qty.0
                                    } else {
                                        o.sl_qty.0
                                    };
                                    if qty > 0.0 {
                                        r.amount = qty;
                                        r.remaining = qty;
                                    }
                                    let stop = if r.kind == OrderType::TakeProfit {
                                        &o.tp_stop_type
                                    } else {
                                        &o.sl_stop_type
                                    };
                                    if !stop.is_empty() {
                                        r.trigger_type = Some(trigger(stop));
                                    }
                                    if let Some(p) = if r.kind == OrderType::TakeProfit {
                                        o.tp_price
                                    } else {
                                        o.sl_price
                                    } {
                                        r.price = p.0;
                                    }
                                } else {
                                    if !stale_timestamp && o.price.0 != 0.0 {
                                        r.price = o.price.0;
                                    }
                                    if !stale_timestamp && o.qty.0 != 0.0 {
                                        r.amount = o.qty.0;
                                    }
                                    if let Some(cumulative) = cumulative {
                                        r.filled = r.filled.max(cumulative);
                                    }
                                    if o.event != "CLOSE"
                                        && !matches!(
                                            r.status,
                                            OrderStatus::Closed | OrderStatus::Canceled
                                        )
                                    {
                                        r.remaining = r.amount - r.filled;
                                    }
                                }
                            });
                        }
                    }
                    if !tpsl {
                        let previous = self.account.filled.get(raw).copied().unwrap_or((0.0, 0.0));
                        let cumulative = cumulative.unwrap_or(previous.0);
                        if cumulative > previous.0 {
                            let side = if o.side.eq_ignore_ascii_case("BUY") {
                                Some(OrderSide::Buy)
                            } else if o.side.eq_ignore_ascii_case("SELL") {
                                Some(OrderSide::Sell)
                            } else if o.side.is_empty() {
                                execution_parent.map(|(side, _)| side)
                            } else {
                                None
                            };
                            let price = if o.price.0 == 0.0 {
                                execution_parent.map(|(_, price)| price)
                            } else {
                                Some(o.price.0)
                            }
                            .filter(|price| price.is_finite() && *price > 0.0);
                            let prepared = Cursor::seed(previous.0, Some(previous.1))
                                .and_then(|cursor| {
                                    cursor.prepare_average(
                                        cumulative,
                                        (o.average_price.0 > 0.0).then_some(o.average_price.0),
                                        price,
                                        Arithmetic::Binary,
                                    )
                                })
                                .and_then(|prepared| {
                                    prepared
                                        .map(|prepared| {
                                            prepared.rounded(ExecutionRounding::Significant15)
                                        })
                                        .transpose()
                                })
                                .ok()
                                .flatten()
                                .and_then(|prepared| {
                                    prepared.next().value().map(|cost| (prepared, cost))
                                });
                            if let Some((side, (prepared, cost))) = side.zip(prepared) {
                                cx.fill(Fill {
                                    id: None,
                                    order_id: o.order_id.to_string(),
                                    symbol: o.symbol.to_string(),
                                    side,
                                    price: prepared.price(),
                                    amount: prepared.amount(),
                                });
                                self.account
                                    .filled
                                    .insert(raw.to_owned(), (prepared.next().quantity(), cost));
                                if self
                                    .account
                                    .pending_executions
                                    .get(raw)
                                    .is_some_and(|pending| cumulative >= *pending)
                                {
                                    self.account.pending_executions.remove(raw);
                                }
                            } else {
                                // Leave the execution watermark available for a complete event.
                                self.account
                                    .pending_executions
                                    .entry(raw.to_owned())
                                    .and_modify(|pending| *pending = pending.max(cumulative))
                                    .or_insert(cumulative);
                                BackgroundCoverage::Incomplete
                                    .report(cx, "Bitunix fill metadata is not reconciled");
                            }
                        }
                    }
                    // A sparse unknown UPDATE may only carry an execution awaiting
                    // REST metadata. Keep that repair eligible. Existing family
                    // mutations and CLOSE tombstones supersede older REST instead.
                    let observed = o.event == "CREATE"
                        || timestamp > 0.0
                        || (!tpsl && (o.price.0 != 0.0 || o.qty.0 != 0.0 || cumulative.is_some()))
                        || (tpsl && (o.tp_qty.0 > 0.0 || o.sl_qty.0 > 0.0))
                        || o.tp_price.is_some()
                        || o.sl_price.is_some()
                        || !o.tp_stop_type.is_empty()
                        || !o.sl_stop_type.is_empty()
                        || !o.tp_order_type.is_empty()
                        || !o.sl_order_type.is_empty()
                        || o.tp_order_price.0 != 0.0
                        || o.sl_order_price.0 != 0.0;
                    if o.event == "CLOSE"
                        || (observed
                            && (cx.store.order(raw).is_some()
                                || self.account.native.values().any(|meta| meta.raw_id == raw)))
                    {
                        self.observe_order(cx, tpsl, Some(raw.to_owned()), true);
                    }
                } else {
                    self.observe_order(cx, tpsl, order_identity(text), false);
                    row_skipped(
                        cx,
                        Level::Warn,
                        "Bitunix malformed order event; keeping its cached native family",
                    );
                }
                self.account.generations[2] += 1;
                self.debounce(cx, Axis::Orders);
            }
            _ => {}
        }
    }
}

#[derive(serde::Deserialize)]
struct AccountWrapper<'a> {
    #[serde(borrow, default)]
    data: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    result: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    payload: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    list: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    rows: Option<&'a serde_json::value::RawValue>,
}
fn extract_account(text: &str, depth: u8) -> Option<Account<'_>> {
    if depth > 4 {
        return None;
    }
    if text.trim_start().starts_with('[') {
        let rows: Vec<&serde_json::value::RawValue> = serde_json::from_str(text).ok()?;
        let mut first = None;
        for row in rows {
            if let Some(a) = extract_account(row.get(), depth + 1) {
                if a.margin_coin.eq_ignore_ascii_case("USDT") {
                    return Some(a);
                }
                if first.is_none() {
                    first = Some(a);
                }
            }
        }
        return first;
    }
    if let Ok(a) = serde_json::from_str::<Account<'_>>(text)
        && (!a.margin_coin.is_empty()
            || !a.position_mode.is_empty()
            || a.available.is_some()
            || a.margin.is_some()
            || a.frozen.is_some()
            || a.cross_unrealized_pnl.is_some()
            || a.isolation_unrealized_pnl.is_some())
    {
        return Some(a);
    }
    let w: AccountWrapper<'_> = serde_json::from_str(text).ok()?;
    [w.data, w.result, w.payload, w.list, w.rows]
        .into_iter()
        .flatten()
        .find_map(|v| extract_account(v.get(), depth + 1))
}
