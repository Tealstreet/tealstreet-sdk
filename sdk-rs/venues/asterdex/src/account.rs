//! Account reconciliation is split by state domain; failed or stale reads never clear it.
use crate::sign::Query;
use crate::{AsterDex, Pending};
use safe_cex_engine::Level;
use safe_cex_engine::backoff::Backoff;
use safe_cex_engine::coverage::{RowReports, SnapshotBuilder};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::execution::ObservedProgress;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::num::parse_decimal;
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{
    Cx, ErrorKind, Fill, Method, Response, SocketChannel, SocketId, TimerId, VenueError, num,
};
use safe_cex_venue_binance::wire::{self, ListenKey, OpenOrder, OrderUpdateEvent};
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Default)]
pub struct AccountState {
    /// listenKey upkeep session. Aster returns the active key on create and
    /// extends it on keepalive, so an extra write is harmless (repeat-safe).
    listen_key_session: Option<safe_cex_engine::request_context::Session>,
    pub socket: Option<SocketId>,
    open: bool,
    supervisor: Supervisor,
    key_epoch: u64,
    key_pending: bool,
    keepalive: Option<TimerId>,
    tick: Option<TimerId>,
    positions_timer: Option<TimerId>,
    orders_timer: Option<TimerId>,
    balance_timer: Option<TimerId>,
    last_positions: f64,
    last_orders: f64,
    last_balance: f64,
    positions_in_flight: bool,
    orders_in_flight: bool,
    balance_in_flight: bool,
    position_version: u64,
    mode_version: u64,
    balance_version: u64,
    pub order_revision: u64,
    last_event: f64,
    executions: HashSet<(String, u64)>,
    execution_order: VecDeque<(String, u64)>,
    execution_fills: HashMap<String, f64>,
    execution_fill_order: VecDeque<String>,
    orders_seeded: bool,
    pub terminal: HashMap<String, (f64, Order)>,
    canceled_ids: HashMap<String, f64>,
    native_order_times: HashMap<String, f64>,
    balances: HashMap<String, (f64, f64)>,
    known_assets: HashSet<String>,
    currencies: HashMap<String, Balance>,
    mode: ModeLoad,
    mode_timer: Option<TimerId>,
    mode_failures: u32,
    order_position_sides: HashMap<String, PositionSide>,
    /// Position ids, symbols or empty (unidentified) whose skipped row was already logged.
    skip_logged: RowReports<String>,
}

/// Hedge routing depends on the account mode; placements wait for its first read.
#[derive(Default, PartialEq)]
enum ModeLoad {
    #[default]
    Unrequested,
    Loading,
    Known,
}

impl AccountState {
    fn commit_execution_progress(&mut self, id: &str, progress: ObservedProgress) {
        if !self.execution_fills.contains_key(id) {
            self.execution_fill_order.push_back(id.to_owned());
        }
        self.execution_fills
            .insert(id.to_owned(), progress.quantity());
        while self.execution_fill_order.len() > 10_000 {
            if let Some(old) = self.execution_fill_order.pop_front() {
                self.execution_fills.remove(&old);
            }
        }
    }
}

pub enum Request {
    Info {
        root: safe_cex_engine::request_context::Context,
    },
    ListenKey {
        epoch: u64,
    },
    Keepalive {
        epoch: u64,
    },
    Mode {
        version: u64,
    },
    Balance {
        version: u64,
    },
    Positions {
        version: u64,
        rows: SnapshotBuilder<String, Position>,
        settings: Vec<(String, MarketSetting)>,
    },
    Orders {
        version: u64,
        rows: SnapshotBuilder<String, Order>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrderRow<'a> {
    #[serde(flatten, borrow)]
    pub row: OpenOrder<'a>,
    #[serde(default = "new_status", borrow)]
    pub status: Cow<'a, str>,
    #[serde(default)]
    pub time: f64,
    #[serde(default, borrow)]
    pub position_side: Option<Cow<'a, str>>,
}
fn new_status<'a>() -> Cow<'a, str> {
    Cow::Borrowed("NEW")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestPosition<'a> {
    #[serde(flatten, borrow)]
    row: wire::PositionRisk<'a>,
    #[serde(default, borrow)]
    margin_type: Cow<'a, str>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountResponse<'a> {
    #[serde(default, borrow)]
    assets: Vec<RestAsset<'a>>,
    #[serde(borrow)]
    total_wallet_balance: Cow<'a, str>,
    #[serde(borrow)]
    available_balance: Cow<'a, str>,
    #[serde(borrow)]
    total_initial_margin: Cow<'a, str>,
    #[serde(borrow)]
    total_open_order_initial_margin: Cow<'a, str>,
    #[serde(borrow)]
    total_unrealized_profit: Cow<'a, str>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestAsset<'a> {
    #[serde(borrow)]
    asset: Cow<'a, str>,
    #[serde(default, borrow)]
    wallet_balance: Option<Cow<'a, str>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Mode {
    dual_side_position: BoolValue,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum BoolValue {
    Bool(bool),
    Text(String),
}
impl BoolValue {
    fn value(self) -> Option<bool> {
        match self {
            Self::Bool(x) => Some(x),
            Self::Text(x) if x == "true" => Some(true),
            Self::Text(x) if x == "false" => Some(false),
            _ => None,
        }
    }
}

pub fn order_type(text: &str) -> Option<OrderType> {
    Some(match text {
        "LIMIT" => OrderType::Limit,
        "MARKET" => OrderType::Market,
        "STOP" | "STOP_MARKET" => OrderType::StopLoss,
        "TAKE_PROFIT" | "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
        "TRAILING_STOP" | "TRAILING_STOP_MARKET" => OrderType::TrailingStopLoss,
        _ => return None,
    })
}
pub fn order_side(text: &str) -> OrderSide {
    if text == "SELL" {
        OrderSide::Sell
    } else {
        OrderSide::Buy
    }
}
pub fn time_in_force(text: &str) -> Option<TimeInForce> {
    Some(match text {
        "GTC" => TimeInForce::GoodTillCancel,
        "IOC" => TimeInForce::ImmediateOrCancel,
        "FOK" => TimeInForce::FillOrKill,
        "GTX" => TimeInForce::PostOnly,
        _ => return None,
    })
}
pub fn position_id(symbol: &str, side: PositionSide, mode: PositionMode) -> String {
    if mode == PositionMode::Oneway {
        symbol.into()
    } else {
        format!(
            "{symbol}:{}",
            if side == PositionSide::Long {
                "long"
            } else {
                "short"
            }
        )
    }
}
fn side(ps: &str, amount: f64) -> PositionSide {
    if ps == "SHORT" || (ps != "LONG" && amount < 0.0) {
        PositionSide::Short
    } else {
        PositionSide::Long
    }
}
/// Rejects a malformed position row: its leg when the side is readable, else every leg.
/// Every caller's row is owned or of unreadable size, so one with no stored copy is an error.
fn reject_position(
    cx: &mut Cx<'_, Pending>,
    logged: &mut RowReports<String>,
    rows: &mut SnapshotBuilder<String, Position>,
    symbol: &str,
    position_side: Option<&str>,
) {
    let ids = match hedge_side(position_side) {
        Some(side) => vec![position_id(symbol, side, PositionMode::Hedged)],
        None => [PositionSide::Long, PositionSide::Short]
            .map(|side| position_id(symbol, side, PositionMode::Hedged))
            .into_iter()
            .chain([symbol.to_owned()])
            .collect(),
    };
    let label = if ids.len() == 1 { &ids[0] } else { symbol };
    let stored = ids.iter().any(|id| cx.store.position(id).is_some());
    log_skip(
        cx,
        logged,
        label,
        if stored { Level::Warn } else { Level::Error },
        format!("Aster skipped malformed position row {label}"),
    );
    for id in ids {
        rows.reject(Some(id));
    }
}
/// Logs a skipped position once per `key` until it maps again; polls would otherwise repeat it.
fn log_skip(
    cx: &mut Cx<'_, Pending>,
    logged: &mut RowReports<String>,
    key: &str,
    level: Level,
    message: impl Into<String>,
) {
    logged.report(cx, key.to_owned(), level, message);
}
fn hedge_side(text: Option<&str>) -> Option<PositionSide> {
    match text {
        Some("LONG") => Some(PositionSide::Long),
        Some("SHORT") => Some(PositionSide::Short),
        _ => None,
    }
}
fn status(text: &str) -> Option<OrderStatus> {
    Some(match text {
        "NEW" | "PARTIALLY_FILLED" => OrderStatus::Open,
        "FILLED" => OrderStatus::Closed,
        "CANCELED" | "EXPIRED" | "REJECTED" | "EXPIRED_IN_MATCH" => OrderStatus::Canceled,
        _ => return None,
    })
}
fn trigger(kind: OrderType, text: Option<&str>) -> Option<StopTriggerType> {
    if !matches!(kind, OrderType::StopLoss | OrderType::TakeProfit) {
        return None;
    }
    match text {
        Some("MARK_PRICE") => Some(StopTriggerType::MarkPrice),
        Some("CONTRACT_PRICE") => Some(StopTriggerType::LastPrice),
        _ => None,
    }
}
pub(crate) fn order_from_wire(row: &OrderRow<'_>) -> Option<Order> {
    let o = &row.row;
    let kind = order_type(&o.kind)?;
    let amount = parse_decimal(&o.orig_qty)?;
    let filled = parse_decimal(&o.executed_qty)?;
    let stop = o
        .stop_price
        .as_deref()
        .and_then(parse_decimal)
        .filter(|x| *x > 0.0);
    Some(Order {
        id: o.order_id.to_string(),
        parent_id: None,
        status: status(&row.status)?,
        symbol: o.symbol.to_string(),
        kind,
        side: order_side(&o.side),
        price: stop.unwrap_or(parse_decimal(&o.price)?),
        amount,
        filled,
        remaining: (amount - filled).max(0.0),
        reduce_only: o.close_position || o.reduce_only,
        close: None,
        client_order_id: o
            .client_order_id
            .as_deref()
            .filter(|x| !x.is_empty())
            .map(str::to_owned),
        time_in_force: o.time_in_force.as_deref().and_then(time_in_force),
        last_update: row.time,
        trigger_type: trigger(kind, o.working_type.as_deref()),
        take_profit: None,
        stop_loss: None,
        extra: None,
    })
}
pub(crate) fn venue_error(response: &Response<'_>) -> Option<(i64, String)> {
    if let Ok(error) = serde_json::from_str::<wire::ErrorBody>(response.body)
        && error.code < 0
    {
        let msg = if error.code == -5025
            || error
                .msg
                .to_lowercase()
                .contains("cannot found builder config")
        {
            "Aster builder attribution is not synced for this API key. Open your Aster API key settings, sync or enable the Tealstreet builder authorization, then reconnect the account and try again.".into()
        } else {
            error.msg.into_owned()
        };
        return Some((error.code, msg));
    }
    if (200..300).contains(&response.status) {
        None
    } else {
        Some((0, format!("HTTP {}", response.status)))
    }
}

impl AsterDex {
    pub(crate) fn fetch_account_info(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(root) = crate::request::command_read_root(cx) else {
            return;
        };
        let path = if self.version == crate::sign::Version::V1 {
            "/fapi/v4/account".into()
        } else {
            self.path("account")
        };
        self.signed(
            cx,
            CredentialRole::Read { scope: None },
            Method::Get,
            &path,
            Query::new(),
            Pending::Account(Request::Info { root }),
        );
    }
    pub(crate) fn start_account(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.supervisor = Supervisor::new(Config {
            idle_timeout_ms: 20_000.0,
            ping_interval_ms: Some(5_000.0),
            ..Config::default()
        });
        self.refresh_balance(cx);
        self.refresh_positions(cx);
        self.request_refresh_orders(cx);
        self.account.mode = ModeLoad::Loading;
        self.request_mode(cx);
        self.request_listen_key(cx);
        self.account.tick = Some(cx.set_poll_timer(3_000.0));
    }
    fn request_mode(&mut self, cx: &mut Cx<'_, Pending>) {
        let path = self.path("positionSide/dual");
        self.signed(
            cx,
            CredentialRole::Read { scope: None },
            Method::Get,
            &path,
            Query::new(),
            Pending::Account(Request::Mode {
                version: self.account.mode_version,
            }),
        );
    }
    /// Placements are refused until the startup position-mode read lands.
    pub(crate) fn position_mode_pending(&self) -> bool {
        self.account.mode == ModeLoad::Loading
    }
    pub(crate) fn order_position_side(&self, id: &str) -> Option<PositionSide> {
        self.account.order_position_sides.get(id).copied()
    }
    pub(crate) fn record_order_position_side(&mut self, id: &str, side: PositionSide) {
        self.account
            .order_position_sides
            .insert(id.to_owned(), side);
    }
    fn request_listen_key(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.key_pending || self.account.socket.is_some() {
            return;
        }
        self.account.key_pending = true;
        self.account.supervisor.connecting(cx);
        let path = self.path("listenKey");
        self.signed(
            cx,
            CredentialRole::SessionAuth,
            Method::Post,
            &path,
            Query::new(),
            Pending::Account(Request::ListenKey {
                epoch: self.account.key_epoch,
            }),
        );
    }
    /// A completed settings write invalidates older position/settings snapshots.
    pub(crate) fn invalidate_position_settings(&mut self) {
        self.account.position_version = self.account.position_version.wrapping_add(1);
    }

    pub(crate) fn on_mode_changed(&mut self, cx: &mut Cx<'_, Pending>, hedged: bool) {
        cx.store.set_hedged(hedged);
        self.mode_known(cx);
        self.account.mode_version = self.account.mode_version.wrapping_add(1);
        self.account.position_version += 1;
        self.refresh_positions(cx);
    }
    fn refresh_balance(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.balance_in_flight {
            return;
        }
        self.account.balance_in_flight = true;
        let path = if self.version == crate::sign::Version::V1 {
            "/fapi/v4/account".into()
        } else {
            self.path("account")
        };
        self.signed(
            cx,
            CredentialRole::Read { scope: None },
            Method::Get,
            &path,
            Query::new(),
            Pending::Account(Request::Balance {
                version: self.account.balance_version,
            }),
        );
    }
    fn refresh_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.positions_in_flight {
            return;
        }
        self.account.positions_in_flight = true;
        let path = self.path("positionRisk");
        self.signed_streamed(
            cx,
            &path,
            Query::new(),
            Pending::Account(Request::Positions {
                version: self.account.position_version,
                rows: SnapshotBuilder::default(),
                settings: Vec::new(),
            }),
        );
    }
    pub(crate) fn request_refresh_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.orders_in_flight {
            return;
        }
        self.account.orders_in_flight = true;
        let path = self.path("openOrders");
        self.signed_streamed(
            cx,
            &path,
            Query::new(),
            Pending::Account(Request::Orders {
                version: self.account.order_revision,
                rows: SnapshotBuilder::default(),
            }),
        );
    }
    pub(crate) fn mark_order_canceled(&mut self, cx: &mut Cx<'_, Pending>, id: &str) {
        self.account.order_revision += 1;
        self.account.canceled_ids.insert(id.into(), cx.now);
        if cx
            .store
            .order(id)
            .is_some_and(|o| o.status == OrderStatus::Closed)
        {
            return;
        }
        if let Some(mut order) = cx.store.order(id).cloned() {
            order.status = OrderStatus::Canceled;
            order.last_update = cx.now;
            self.account
                .terminal
                .insert(id.into(), (cx.now, order.clone()));
            cx.store.upsert_order(order);
        }
    }
    pub(crate) fn on_account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::Orders { rows, .. } => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Identity {
                    order_id: wire::Id,
                }
                let row = serde_json::from_str::<OrderRow>(item).ok();
                match row.as_ref().and_then(|o| Some((o, order_from_wire(o)?))) {
                    Some((row, order)) => {
                        if let Some(side) = hedge_side(row.position_side.as_deref()) {
                            self.account
                                .order_position_sides
                                .insert(order.id.clone(), side);
                        }
                        rows.push(order);
                    }
                    None => match serde_json::from_str::<Identity>(item) {
                        Ok(row) => {
                            let id = row.order_id.to_string();
                            cx.notice(
                                Level::Warn,
                                code::ROW_SKIPPED,
                                format!("Aster skipped malformed open order row {id}"),
                            );
                            rows.reject(Some(id));
                        }
                        Err(_) => {
                            cx.notice(
                                Level::Warn,
                                code::ROW_SKIPPED,
                                "Aster skipped malformed open order row without an id",
                            );
                            rows.reject(None);
                        }
                    },
                }
            }
            Request::Positions { rows, settings, .. } => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Identity<'a> {
                    #[serde(borrow)]
                    symbol: Cow<'a, str>,
                    #[serde(default, borrow)]
                    position_side: Option<Cow<'a, str>>,
                }
                let Ok(p) = serde_json::from_str::<RestPosition>(item) else {
                    match serde_json::from_str::<Identity>(item) {
                        Ok(row) if !row.symbol.is_empty() => reject_position(
                            cx,
                            &mut self.account.skip_logged,
                            rows,
                            &row.symbol,
                            row.position_side.as_deref(),
                        ),
                        _ => {
                            let level = if cx.store.positions().is_empty() {
                                Level::Error
                            } else {
                                Level::Warn
                            };
                            log_skip(
                                cx,
                                &mut self.account.skip_logged,
                                "",
                                level,
                                "Aster skipped malformed position row without a symbol",
                            );
                            rows.reject(None);
                        }
                    }
                    return;
                };
                let r = p.row;
                let Some(amount) = parse_decimal(&r.position_amt) else {
                    reject_position(
                        cx,
                        &mut self.account.skip_logged,
                        rows,
                        &r.symbol,
                        Some(&r.position_side),
                    );
                    return;
                };
                let leverage = r
                    .leverage
                    .filter(|x| x.is_finite() && *x > 0.0)
                    .unwrap_or(0.0);
                let margin = if p.margin_type.eq_ignore_ascii_case("isolated") {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                };
                settings.push((
                    r.symbol.to_string(),
                    MarketSetting {
                        leverage,
                        margin_type: Some(margin),
                        ..Default::default()
                    },
                ));
                if amount == 0.0 {
                    return;
                }
                let values = [
                    &*r.entry_price,
                    &*r.un_realized_profit,
                    &*r.notional,
                    &*r.liquidation_price,
                ]
                .map(parse_decimal);
                let [Some(entry), Some(upnl), Some(notional), Some(liquidation)] = values else {
                    reject_position(
                        cx,
                        &mut self.account.skip_logged,
                        rows,
                        &r.symbol,
                        Some(&r.position_side),
                    );
                    return;
                };
                let mode = if r.position_side == "BOTH" || r.position_side.is_empty() {
                    PositionMode::Oneway
                } else {
                    PositionMode::Hedged
                };
                let side = side(&r.position_side, amount);
                let id = position_id(&r.symbol, side, mode);
                self.account.skip_logged.forget(&id);
                rows.push(Position {
                    id: id.clone(),
                    symbol: r.symbol.to_string(),
                    side,
                    entry_price: entry,
                    notional,
                    leverage: leverage.max(1.0),
                    margin_type: Some(margin),
                    position_mode: Some(mode),
                    unrealized_pnl: upnl,
                    realized_pnl: Some(
                        cx.store
                            .position(&id)
                            .and_then(|p| p.realized_pnl)
                            .unwrap_or(0.0),
                    ),
                    contracts: amount.abs(),
                    liquidation_price: liquidation,
                    margin_size: None,
                    last_update: None,
                });
            }

            _ => {}
        }
    }
    pub(crate) fn on_account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        let error = match &response {
            Ok(r) => venue_error(r).map(|(_, m)| m),
            Err(e) => Some((*e).into()),
        };
        let body = response.as_ref().map_or("", |r| r.body);
        let valid = error.is_none();
        match request {
            Request::Info { root } => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Info {
                    account_alias: Option<String>,
                }
                #[derive(serde::Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Output {
                    user_id: String,
                }
                // Without an alias: V3's public signer address, else the trusted
                // primary stableId; never the API key (WEEX precedent).
                let fallback = match self.version {
                    crate::sign::Version::V3 => self.options.signer_address.clone(),
                    crate::sign::Version::V1 => None,
                }
                .or_else(|| {
                    self.options
                        .credentials
                        .as_ref()
                        .and_then(|id| id.stable_id.clone())
                });
                let result = if let Some(error) = &error {
                    Err(VenueError::new(error.clone()))
                } else {
                    serde_json::from_str::<Info>(body)
                        .map_err(|e| VenueError::new(e.to_string()))
                        .and_then(|info| {
                            let user_id = info
                                .account_alias
                                .filter(|x| !x.is_empty())
                                .or(fallback)
                                .ok_or_else(|| {
                                    VenueError::new(
                                        "Aster account identity requires an account alias or a trusted primary stableId",
                                    )
                                    .kind(ErrorKind::Unsupported)
                                })?;
                            serde_json::to_string(&Output { user_id })
                                .map_err(|e| VenueError::new(e.to_string()))
                        })
                };
                let _ = match result {
                    Ok(data) => cx.done_context(&root, data),
                    Err(error) => cx.fail_context(&root, error),
                };
            }
            Request::ListenKey { epoch } => {
                if epoch != self.account.key_epoch {
                    return;
                }
                self.account.key_pending = false;
                if valid
                    && let Ok(key) = serde_json::from_str::<ListenKey>(body)
                    && !key.listen_key.is_empty()
                {
                    let base = if self.options.testnet {
                        "wss://fstream5.asterdex-testnet.com"
                    } else {
                        "wss://fstream.asterdex.com"
                    };
                    if let Some(old) = self.account.socket.take() {
                        cx.close(old);
                    }
                    self.account.socket = Some(
                        cx.connect(
                            safe_cex_engine::SocketSpec::new(format!(
                                "{base}/ws/{}",
                                key.listen_key
                            ))
                            .channel(SocketChannel::Private),
                        ),
                    );
                    if let Some(t) = self.account.keepalive.take() {
                        cx.clear_timer(t);
                    }
                    self.account.keepalive = Some(cx.set_timer(30.0 * 60_000.0));
                } else {
                    self.close_private(cx);
                }
            }
            Request::Keepalive { epoch } => {
                if epoch != self.account.key_epoch {
                    return;
                }
                if valid {
                    self.account.keepalive = Some(cx.set_timer(30.0 * 60_000.0));
                } else {
                    self.close_private(cx);
                }
            }
            Request::Mode { version } => {
                if version != self.account.mode_version {
                    return;
                }
                if valid
                    && let Ok(mode) = serde_json::from_str::<Mode>(body)
                    && let Some(value) = mode.dual_side_position.value()
                {
                    cx.store.set_hedged(value);
                    self.mode_known(cx);
                } else {
                    let delay = cx.jittered(
                        Backoff::new(1_000.0, 5, f64::INFINITY).delay(self.account.mode_failures),
                    );
                    self.account.mode_failures = self.account.mode_failures.saturating_add(1);
                    let reason = error.as_deref().unwrap_or("unrecognized dualSidePosition");
                    cx.retried_failure("asterdex.account:778",
                        format!(
                            "Aster position mode read failed ({reason}); order placement is held until it loads, retrying in {:.1}s",
                            delay / 1_000.0
                        ));
                    if self.account.mode_timer.is_none() {
                        self.account.mode_timer = Some(cx.set_timer(delay));
                    }
                }
                return;
            }
            Request::Balance { version } => {
                self.account.balance_in_flight = false;
                self.account.last_balance = cx.now;
                if valid
                    && version == self.account.balance_version
                    && let Ok(r) = serde_json::from_str::<AccountResponse>(body)
                {
                    let values = [
                        &*r.total_wallet_balance,
                        &*r.available_balance,
                        &*r.total_initial_margin,
                        &*r.total_open_order_initial_margin,
                        &*r.total_unrealized_profit,
                    ]
                    .map(parse_decimal);
                    if let [
                        Some(total),
                        Some(free),
                        Some(initial),
                        Some(open),
                        Some(upnl),
                    ] = values
                    {
                        self.account.known_assets.extend(
                            r.assets
                                .iter()
                                .filter(|a| {
                                    a.wallet_balance
                                        .as_deref()
                                        .and_then(parse_decimal)
                                        .is_some_and(|x| x != 0.0)
                                })
                                .map(|a| a.asset.to_string()),
                        );
                        cx.store.set_balance(Balance {
                            total,
                            free,
                            used: initial + open,
                            upnl,
                        });
                    }
                }
                if version != self.account.balance_version && self.account.balance_timer.is_none() {
                    self.account.balance_timer = Some(cx.set_timer(1_000.0));
                }
            }
            Request::Positions {
                version,
                rows,
                settings,
            } => {
                self.account.positions_in_flight = false;
                self.account.last_positions = cx.now;
                if valid && version == self.account.position_version {
                    if rows.rejected() == 0 {
                        self.account.skip_logged.clear();
                    }
                    // A skipped row cannot prove its leg flat; keep what the store knows.
                    let rows = rows.commit(cx.store.positions().iter().cloned(), |p| p.id.clone());
                    cx.store.set_positions(rows);
                    for (symbol, setting) in settings {
                        cx.store.update_setting(&symbol, |s| *s = setting);
                    }
                } else if version != self.account.position_version
                    && self.account.positions_timer.is_none()
                {
                    self.account.positions_timer = Some(cx.set_timer(1_000.0));
                }
            }
            Request::Orders { version, rows } => {
                self.account.orders_in_flight = false;
                self.account.last_orders = cx.now;
                if valid {
                    if version != self.account.order_revision {
                        // A snapshot predating a stream event cannot overwrite any order domain.
                        if self.account.orders_timer.is_none() {
                            self.account.orders_timer = Some(cx.set_timer(1_000.0));
                        }
                    } else {
                        let mut rows =
                            rows.commit(cx.store.orders().iter().cloned(), |o| o.id.clone());
                        self.account
                            .terminal
                            .retain(|_, (at, _)| cx.now - *at < 60_000.0);
                        self.account
                            .canceled_ids
                            .retain(|_, at| cx.now - *at < 60_000.0);
                        rows.retain(|order| {
                            order.status != OrderStatus::Open
                                || !self.account.canceled_ids.contains_key(&order.id)
                        });
                        for (_, terminal) in self.account.terminal.values() {
                            rows.retain(|o| o.id != terminal.id);
                            rows.push(terminal.clone());
                        }
                        self.account
                            .native_order_times
                            .retain(|id, _| rows.iter().any(|o| &o.id == id));
                        self.account
                            .order_position_sides
                            .retain(|id, _| rows.iter().any(|o| &o.id == id));
                        if !self.account.orders_seeded {
                            for order in &rows {
                                let previous = self
                                    .account
                                    .execution_fills
                                    .get(&order.id)
                                    .copied()
                                    .unwrap_or(0.0);
                                if let Ok(progress) =
                                    ObservedProgress::new(previous, Some(order.filled))
                                {
                                    self.account.commit_execution_progress(&order.id, progress);
                                }
                            }
                            self.account.orders_seeded = true;
                        }
                        cx.store.set_orders(rows);
                        cx.orders_reconciled(None);
                    }
                }
            }
        }
        if let Some(error) = error {
            cx.notice(
                Level::Warn,
                code::REQUEST_FAILED,
                format!("Aster account read: {error}"),
            );
        }
    }
    fn mode_known(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.mode = ModeLoad::Known;
        self.account.mode_failures = 0;
        if let Some(timer) = self.account.mode_timer.take() {
            cx.clear_timer(timer);
        }
    }
    pub(crate) fn on_account_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.account.open = true;
        self.account.supervisor.opened(cx);
        cx.send(socket, r#"{"id":42,"method":"LIST_SUBSCRIPTIONS"}"#);
        self.request_refresh_orders(cx);
        true
    }
    fn close_private(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(socket) = self.account.socket.take() {
            cx.close(socket);
        }
        self.account.open = false;
        self.retire_listen_key_session(cx);
        self.account.key_epoch = self.account.key_epoch.wrapping_add(1);
        self.account.key_pending = false;
        if let Some(timer) = self.account.keepalive.take() {
            cx.clear_timer(timer);
        }
        self.account.supervisor.retry(cx);
    }
    pub(crate) fn listen_key_current(&self, pending: &Pending) -> bool {
        matches!(
            pending,
            Pending::Account(Request::ListenKey { epoch } | Request::Keepalive { epoch })
                if *epoch == self.account.key_epoch
        )
    }
    /// Each create opens a new generation; keepalives reuse the live one.
    pub(crate) fn listen_key_session(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        create: bool,
        credential: &safe_cex_engine::request_context::Credential,
    ) -> Option<safe_cex_engine::request_context::Session> {
        let live = self
            .account
            .listen_key_session
            .as_ref()
            .is_some_and(|session| cx.session_live(session));
        if create || !live {
            self.retire_listen_key_session(cx);
            self.account.listen_key_session = cx
                .open_session(safe_cex_engine::request_context::SessionPolicy {
                    credential: Some(credential.clone()),
                    repeat_safe: true,
                    // Signed: routed by its credential, never a session egress.
                    private_egress: None,
                })
                .ok();
        }
        self.account.listen_key_session.clone()
    }
    fn retire_listen_key_session(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(session) = self.account.listen_key_session.take() {
            let _ = cx.retire_session(&session);
        }
    }
    pub(crate) fn on_account_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.close_private(cx);
        true
    }
    pub(crate) fn on_account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        match self.account.supervisor.on_timer(cx, timer) {
            Tick::Reconnect => {
                self.request_listen_key(cx);
                return true;
            }
            Tick::TimedOut(_) => {
                self.close_private(cx);
                return true;
            }
            Tick::Ping => {
                if let Some(socket) = self.account.socket {
                    cx.send(socket, r#"{"id":42,"method":"LIST_SUBSCRIPTIONS"}"#);
                }
                return true;
            }
            Tick::Handled => return true,
            Tick::Unhandled => {}
        }
        if self.account.keepalive == Some(timer) {
            self.account.keepalive = None;
            let path = self.path("listenKey");
            self.signed(
                cx,
                CredentialRole::SessionAuth,
                Method::Put,
                &path,
                Query::new(),
                Pending::Account(Request::Keepalive {
                    epoch: self.account.key_epoch,
                }),
            );
        } else if self.account.tick == Some(timer) {
            self.account.tick = None;
            if cx.now - self.account.last_balance >= 25_000.0 {
                self.refresh_balance(cx);
            }
            if cx.now - self.account.last_positions >= 25_000.0 {
                self.refresh_positions(cx);
            }
            if cx.now - self.account.last_orders >= 75_000.0 {
                self.request_refresh_orders(cx);
            }
            self.account.tick = Some(cx.set_poll_timer(3_000.0));
        } else if self.account.positions_timer == Some(timer) {
            self.account.positions_timer = None;
            self.refresh_positions(cx);
        } else if self.account.orders_timer == Some(timer) {
            self.account.orders_timer = None;
            self.request_refresh_orders(cx);
        } else if self.account.balance_timer == Some(timer) {
            self.account.balance_timer = None;
            self.refresh_balance(cx);
        } else if self.account.mode_timer == Some(timer) {
            self.account.mode_timer = None;
            if self.account.mode == ModeLoad::Loading {
                self.request_mode(cx);
            }
        } else {
            return false;
        }
        true
    }
    pub(crate) fn on_account_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.account.supervisor.received(cx.now);
        #[derive(Deserialize)]
        struct Head<'a> {
            #[serde(borrow, default)]
            e: Cow<'a, str>,
        }
        match serde_json::from_str::<Head>(text)
            .ok()
            .as_ref()
            .map(|h| &*h.e)
        {
            Some("listenKeyExpired") => self.close_private(cx),
            Some("ORDER_TRADE_UPDATE" | "executionReport") => self.on_order_event(cx, text),
            Some("outboundAccountPosition") => self.on_legacy_balance(cx, text),
            Some("ACCOUNT_UPDATE") => self.on_account_event(cx, text),
            _ => {}
        }
        true
    }
    fn on_order_event(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let o = if let Ok(event) = serde_json::from_str::<OrderUpdateEvent>(text) {
            event.o
        } else {
            let Ok(event) = serde_json::from_str::<LegacyExecution>(text) else {
                return;
            };
            wire::OrderUpdate {
                i: event.i,
                s: event.s,
                side: event.side,
                ot: None,
                o: Some(event.kind),
                status: event.status,
                x: None,
                t: None,
                p: event.p,
                sp: event.sp,
                ap: None,
                q: event.q,
                z: event.z,
                l: event.l,
                last_price: event.last_price,
                reduce_only: false,
                close_position: false,
                wt: None,
                f: event.f,
                c: event.c,
                r: None,
            }
        };
        let id = o.i.to_string();
        #[derive(Deserialize)]
        struct TradeHead<'a> {
            #[serde(borrow)]
            o: TradeIdentity<'a>,
        }
        #[derive(Deserialize)]
        struct TradeIdentity<'a> {
            #[serde(default)]
            t: Option<u64>,
            #[serde(default, borrow)]
            ps: Option<Cow<'a, str>>,
        }
        let head = serde_json::from_str::<TradeHead>(text).ok();
        let position_side = head.as_ref().and_then(|h| hedge_side(h.o.ps.as_deref()));
        let execution = head
            .and_then(|h| h.o.t)
            .or_else(|| serde_json::from_str::<TradeIdentity>(text).ok()?.t);
        let fill = matches!(o.status.as_ref(), "PARTIALLY_FILLED" | "FILLED")
            .then(|| {
                let qty =
                    o.l.as_deref()
                        .and_then(parse_decimal)
                        .filter(|x| *x > 0.0)?;
                let price = o
                    .last_price
                    .as_deref()
                    .and_then(parse_decimal)
                    .filter(|x| *x > 0.0)
                    .or_else(|| o.ap.as_deref().and_then(parse_decimal).filter(|x| *x > 0.0))?;
                Some((price, qty))
            })
            .flatten();
        // Execution identity is independent of acceptance of its order-state update.
        if let (Some(execution), Some((price, qty))) = (execution, fill) {
            let key = (id.clone(), execution);
            if self.account.executions.insert(key.clone()) {
                self.account.execution_order.push_back(key);
                while self.account.execution_order.len() > 10_000 {
                    if let Some(old) = self.account.execution_order.pop_front() {
                        self.account.executions.remove(&old);
                    }
                }
                cx.fill(Fill {
                    id: None,
                    order_id: id.clone(),
                    symbol: o.s.to_string(),
                    side: order_side(&o.side),
                    price,
                    amount: qty,
                });
            }
        }
        // REST progress describes order state, not executions already delivered to the caller.
        if let (Some((price, qty)), Some(filled)) = (fill, parse_decimal(&o.z)) {
            let previous = self
                .account
                .execution_fills
                .get(&id)
                .copied()
                .unwrap_or(0.0);
            if let Ok(progress) = ObservedProgress::new(previous, Some(filled)) {
                let delivery = if execution.is_none() {
                    progress.prepare_capped(Some(qty), price)
                } else {
                    Ok(None)
                };
                if let Ok(delivery) = delivery {
                    self.account.commit_execution_progress(&id, progress);
                    if let Some(prepared) = delivery {
                        let prepared = prepared.delivery();
                        cx.fill(Fill {
                            id: None,
                            order_id: id.clone(),
                            symbol: o.s.to_string(),
                            side: order_side(&o.side),
                            price: prepared.price(),
                            amount: prepared.amount(),
                        });
                    }
                }
            }
        }
        // Liquidation/ADL rows have no order model; their executions are delivered above.
        let Some(kind) = o.ot.as_deref().or(o.o.as_deref()).and_then(order_type) else {
            return;
        };
        let Some(status) = status(&o.status) else {
            return;
        };
        let [Some(amount), Some(filled), Some(price)] = [&*o.q, &*o.z, &*o.p].map(parse_decimal)
        else {
            return;
        };
        if status == OrderStatus::Open
            && (self.account.terminal.contains_key(&id)
                || self.account.canceled_ids.contains_key(&id))
        {
            return;
        }
        #[derive(Deserialize)]
        struct EventTime {
            #[serde(rename = "E", default)]
            time: Option<f64>,
        }
        let time = serde_json::from_str::<EventTime>(text)
            .ok()
            .and_then(|e| e.time)
            .filter(|t| t.is_finite() && *t > 0.0)
            .unwrap_or(cx.now);
        if self
            .account
            .native_order_times
            .get(&id)
            .is_some_and(|old| *old > time)
        {
            return;
        }
        let previous_filled = cx.store.order(&id).map_or(0.0, |prev| prev.filled);
        if filled < previous_filled
            || cx.store.order(&id).is_some_and(|prev| {
                prev.status == OrderStatus::Closed && status != OrderStatus::Closed
            })
        {
            return;
        }
        self.account.native_order_times.insert(id.clone(), time);
        if let Some(side) = position_side {
            self.account.order_position_sides.insert(id.clone(), side);
        }
        self.account.order_revision += 1;
        let stop = o.sp.as_deref().and_then(parse_decimal).filter(|x| *x > 0.0);
        let order = Order {
            id: id.clone(),
            parent_id: None,
            status,
            symbol: o.s.to_string(),
            kind,
            side: order_side(&o.side),
            price: stop.unwrap_or(price),
            amount,
            filled,
            remaining: (amount - filled).max(0.0),
            reduce_only: o.reduce_only || o.close_position,
            close: None,
            client_order_id: o.c.as_deref().filter(|x| !x.is_empty()).map(str::to_owned),
            time_in_force: o.f.as_deref().and_then(time_in_force),
            last_update: time,
            trigger_type: trigger(kind, o.wt.as_deref()),
            take_profit: None,
            stop_loss: None,
            extra: None,
        };
        if !self.account.terminal.contains_key(&id) || status != OrderStatus::Open {
            cx.store.upsert_order(order.clone());
        }
        if status != OrderStatus::Open {
            self.account.terminal.insert(id.clone(), (cx.now, order));
        }
        if let Some(t) = self.account.orders_timer.take() {
            cx.clear_timer(t);
        }
        self.account.orders_timer = Some(cx.set_timer(10_000.0));
    }
    fn on_account_event(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Ok(event) = serde_json::from_str::<AccountEvent>(text) else {
            return;
        };
        if !event.time.is_finite()
            || event.time.fract() != 0.0
            || event.time <= self.account.last_event
            || event.time <= 0.0
            || event.time > 9_007_199_254_740_991.0
        {
            return;
        }
        if event.a.positions.iter().any(|p| {
            p.s.is_empty()
                || [&*p.pa, &*p.up, &*p.cr]
                    .into_iter()
                    .any(|x| parse_decimal(x).is_none())
                || match p.ep.as_deref() {
                    Some(entry) => parse_decimal(entry).is_none(),
                    None => parse_decimal(&p.pa) != Some(0.0),
                }
        }) || event.a.balances.iter().any(|b| {
            b.a.is_empty()
                || parse_decimal(&b.wb).is_none()
                || parse_decimal(&b.cw).is_none()
                || parse_decimal(&b.wb)
                    .zip(parse_decimal(&b.cw))
                    .is_some_and(|(total, free)| total < free)
        }) {
            return;
        }
        self.account.last_event = event.time;
        self.account.balance_version += 1;
        let mut delta = 0.0;
        let mut asset_deltas = HashMap::<String, f64>::new();
        let mut positions = cx.store.positions().to_vec();
        if !event.a.positions.is_empty() {
            self.account.position_version += 1;
        }
        for p in event.a.positions {
            let amount = num::parse(&p.pa);
            let side = side(&p.ps, amount);
            let mode = if p.ps == "BOTH" {
                PositionMode::Oneway
            } else {
                PositionMode::Hedged
            };
            let id = position_id(&p.s, side, mode);
            let previous = positions.iter().find(|old| old.id == id).cloned();
            let change =
                num::parse(&p.up) - previous.as_ref().map_or(0.0, |old| old.unrealized_pnl);
            let asset = self
                .account
                .currencies
                .keys()
                .find(|a| p.s.ends_with(a.as_str()))
                .cloned()
                .or_else(|| p.s.ends_with("USDT").then(|| "USDT".to_string()));
            if let Some(asset) = asset {
                *asset_deltas.entry(asset).or_default() += change;
            }
            if p.s.ends_with("USDT") {
                delta += change;
            }
            positions.retain(|old| {
                if mode == PositionMode::Oneway {
                    old.symbol != p.s
                } else {
                    old.id != id
                }
            });
            if amount != 0.0 {
                let setting = cx.store.setting(&p.s).unwrap_or_default();
                let entry = p.ep.as_deref().map(num::parse).unwrap_or(0.0);
                let mut next = previous.unwrap_or(Position {
                    id,
                    symbol: p.s.to_string(),
                    side,
                    entry_price: entry,
                    notional: 0.0,
                    leverage: setting.leverage,
                    margin_type: setting.margin_type,
                    position_mode: Some(mode),
                    unrealized_pnl: 0.0,
                    realized_pnl: Some(0.0),
                    contracts: 0.0,
                    liquidation_price: 0.0,
                    margin_size: None,
                    last_update: None,
                });
                next.side = side;
                next.position_mode = Some(mode);
                next.contracts = amount.abs();
                next.entry_price = entry;
                next.notional = amount.abs() * entry;
                next.unrealized_pnl = num::parse(&p.up);
                next.realized_pnl = Some(num::parse(&p.cr));
                positions.push(next);
            }
        }
        cx.store.set_positions(positions);
        let multi = event.a.balances.iter().any(|b| b.a != "USDT")
            || self.account.balances.keys().any(|a| a != "USDT")
            || self.account.known_assets.iter().any(|a| a != "USDT");
        let mut root = cx.store.balance().clone();
        for b in event.a.balances {
            let total = num::parse(&b.wb);
            let free = num::parse(&b.cw);
            self.account.balances.insert(b.a.to_string(), (total, free));
            let bucket = self.account.currencies.entry(b.a.to_string()).or_default();
            bucket.total = total;
            bucket.free = free;
            bucket.used = total - free;
            bucket.upnl += asset_deltas.get(b.a.as_ref()).copied().unwrap_or(0.0);
            if b.a == "USDT" && !multi {
                root.total = total;
                root.free = free;
                root.used = total - free;
            }
        }
        if !multi {
            root.upnl += delta;
            cx.store.set_balance(root);
        }
        self.commit_currencies(cx);
        if multi && self.account.balance_timer.is_none() {
            self.account.balance_timer = Some(cx.set_timer(1_000.0));
        }
        if self.account.positions_timer.is_none() {
            self.account.positions_timer = Some(cx.set_timer(1_000.0));
        }
    }
    fn commit_currencies(&self, cx: &mut Cx<'_, Pending>) {
        let mut currencies = self
            .account
            .currencies
            .iter()
            .map(|(currency, balance)| CurrencyBalance {
                currency: currency.clone(),
                balance: balance.clone(),
            })
            .collect::<Vec<_>>();
        currencies.sort_by(|a, b| a.currency.cmp(&b.currency));
        cx.store.set_currency_balances(currencies);
    }
    fn on_legacy_balance(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Ok(event) = serde_json::from_str::<LegacyBalances>(text) else {
            return;
        };
        if event.balances.iter().any(|b| {
            b.a.is_empty()
                || parse_decimal(&b.f).is_none()
                || parse_decimal(&b.l).is_none()
                || parse_decimal(&b.l).is_some_and(|used| used < 0.0)
        }) {
            return;
        }
        self.account.balance_version += 1;
        for b in event.balances {
            let free = num::parse(&b.f);
            let used = num::parse(&b.l);
            let total = free + used;
            self.account.balances.insert(b.a.to_string(), (total, free));
            let bucket = self.account.currencies.entry(b.a.to_string()).or_default();
            bucket.total = total;
            bucket.free = free;
            bucket.used = used;
        }
        self.commit_currencies(cx);
        let multi = self
            .account
            .balances
            .keys()
            .chain(self.account.known_assets.iter())
            .any(|a| a != "USDT");
        if !multi && let Some((total, free)) = self.account.balances.get("USDT") {
            let mut balance = cx.store.balance().clone();
            balance.total = *total;
            balance.free = *free;
            balance.used = total - free;
            cx.store.set_balance(balance);
        }
        if multi && self.account.balance_timer.is_none() {
            self.account.balance_timer = Some(cx.set_timer(1000.0));
        }
    }
}
#[derive(Deserialize)]
struct AccountEvent<'a> {
    #[serde(rename = "E")]
    time: f64,
    #[serde(borrow)]
    a: AccountDelta<'a>,
}
#[derive(Deserialize)]
struct AccountDelta<'a> {
    #[serde(rename = "B", borrow, default)]
    balances: Vec<AssetDelta<'a>>,
    #[serde(rename = "P", borrow, default)]
    positions: Vec<PositionDelta<'a>>,
}
#[derive(Deserialize)]
struct AssetDelta<'a> {
    #[serde(borrow)]
    a: Cow<'a, str>,
    #[serde(borrow)]
    wb: Cow<'a, str>,
    #[serde(borrow)]
    cw: Cow<'a, str>,
}
#[derive(Deserialize)]
struct PositionDelta<'a> {
    #[serde(borrow)]
    s: Cow<'a, str>,
    #[serde(borrow)]
    pa: Cow<'a, str>,
    #[serde(borrow, default, deserialize_with = "present_entry")]
    ep: Option<Cow<'a, str>>,
    #[serde(borrow)]
    up: Cow<'a, str>,
    #[serde(borrow)]
    ps: Cow<'a, str>,
    #[serde(borrow, default = "zero")]
    cr: Cow<'a, str>,
}
// Missing is permitted for a proven flat close; present null or a non-string
// remains malformed, matching the prior strict wire contract.
fn present_entry<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Cow<'de, str>>, D::Error> {
    Cow::<'de, str>::deserialize(d).map(Some)
}
fn zero<'a>() -> Cow<'a, str> {
    Cow::Borrowed("0")
}

#[derive(Deserialize)]
struct LegacyExecution<'a> {
    i: u64,
    #[serde(borrow)]
    s: Cow<'a, str>,
    #[serde(rename = "S", borrow)]
    side: Cow<'a, str>,
    #[serde(rename = "o", borrow)]
    kind: Cow<'a, str>,
    #[serde(rename = "X", borrow)]
    status: Cow<'a, str>,
    #[serde(borrow)]
    p: Cow<'a, str>,
    #[serde(rename = "P", borrow, default)]
    sp: Option<Cow<'a, str>>,
    #[serde(borrow)]
    q: Cow<'a, str>,
    #[serde(borrow)]
    z: Cow<'a, str>,
    #[serde(borrow, default)]
    l: Option<Cow<'a, str>>,
    #[serde(rename = "L", borrow, default)]
    last_price: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    f: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    c: Option<Cow<'a, str>>,
}
#[derive(Deserialize)]
struct LegacyBalances<'a> {
    #[serde(rename = "B", borrow)]
    balances: Vec<LegacyAsset<'a>>,
}
#[derive(Deserialize)]
struct LegacyAsset<'a> {
    #[serde(borrow)]
    a: Cow<'a, str>,
    #[serde(borrow)]
    f: Cow<'a, str>,
    #[serde(borrow)]
    l: Cow<'a, str>,
}
