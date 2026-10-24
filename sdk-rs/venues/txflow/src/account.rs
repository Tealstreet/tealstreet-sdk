//! Unsigned main-wallet reads. Reads commit only after complete, successful,
//! generation/revision-matched responses. Order events never invent executions.
use crate::{Pending, TxFlow};
use safe_cex_engine::Level;
use safe_cex_engine::coverage::{RowReports, SnapshotBuilder};
use safe_cex_engine::history::BackgroundCoverage;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{Cx, Fill, Response, TimerId, json};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;
use std::collections::{HashMap, HashSet, VecDeque};

const PAGE: usize = 500;
const FILL_CAP: usize = 100_000;

#[derive(Default)]
pub struct AccountState {
    started: bool,
    generation: u64,
    state_revision: u64,
    orders_revision: u64,
    settings_revision: u64,
    state_time: f64,
    order_times: HashMap<String, f64>,
    order_memo: VecDeque<String>,
    memo_ids: HashSet<String>,
    terminal: HashSet<String>,
    terminal_queue: VecDeque<String>,
    inflight: HashSet<String>,
    refresh_timer: Option<TimerId>,
    setting_timer: Option<TimerId>,
    setting_queue: VecDeque<String>,
    last_settings: f64,
    startup_at: f64,
    seen_fills: HashSet<u64>,
    fills_exhausted: bool,
    settings: HashMap<String, MarketSetting>,
    pub(crate) order_ids: HashMap<String, String>,
    /// Leg ids, `asset N` or empty (unidentified) whose skipped position was already logged.
    skip_logged: RowReports<String>,
}
enum Skipped {
    Unidentified,
    Unlisted { asset: u32, owned: bool },
    Legs { keys: Vec<String>, owned: bool },
}
fn skip_level(harmless: bool) -> Level {
    if harmless { Level::Warn } else { Level::Error }
}
/// Logs a skipped position once per `key` until it maps again; polls would otherwise repeat it.
fn log_skip(
    cx: &mut Cx<'_, Pending>,
    logged: &mut RowReports<String>,
    key: String,
    level: Level,
    message: impl Into<String>,
) {
    logged.report(cx, key, level, message);
}
#[derive(Clone, Copy)]
pub struct Fence {
    generation: u64,
    revision: u64,
}
pub enum Request {
    State {
        fence: Fence,
        // A newer asset setting owns account mode even if exposure can reconcile.
        mode_revision: u64,
        positions: SnapshotBuilder<String, Position>,
        /// An owned row sits on an asset a catalogue still loading may list.
        hold: bool,
    },
    Orders {
        fence: Fence,
        offset: usize,
        orders: SnapshotBuilder<String, Order>,
        ids: HashSet<String>,
        page_rows: usize,
        valid: bool,
    },
    Fills {
        generation: u64,
        offset: usize,
        fills: Vec<Execution>,
        ids: HashSet<u64>,
        valid: bool,
    },
    Setting {
        fence: Fence,
        coin: String,
    },
}
#[derive(Deserialize)]
pub(crate) struct Envelope<T> {
    pub success: bool,
    pub code: i64,
    pub data: Option<T>,
}
impl<T> Envelope<T> {
    pub(crate) fn data(self) -> Result<T, &'static str> {
        if !self.success || self.code != 0 {
            return Err("TxFlow rejected account read");
        }
        self.data.ok_or("TxFlow account read omitted data")
    }
}
use safe_cex_engine::de::Number as N;
fn optional_number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<N>, D::Error> {
    Ok(safe_cex_engine::de::optional_number_or_blank(d)?.map(N))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    account_value: N,
    total_margin_used: N,
    total_raw_usd: N,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct State {
    margin_summary: Summary,
    #[serde(default)]
    time: Option<N>,
    #[serde(default)]
    user: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetPosition {
    position: NativePosition,
    r#type: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativePosition {
    asset: String,
    szi: N,
    leverage: PositionLeverage,
    position_value: N,
    unrealized_pnl: N,
    margin_used: N,
    entry_px: Option<N>,
    liquidation_px: Option<N>,
    update_time: Option<N>,
}
/// Clearinghouse rows report the effective leverage of this leg, never both
/// configured hedge rails. Only asset-data responses establish market settings.
#[derive(Deserialize)]
struct PositionLeverage {
    r#type: String,
    value: N,
}
fn margin_type(value: &str) -> Result<MarginType, &'static str> {
    match value.to_ascii_lowercase().as_str() {
        "cross" => Ok(MarginType::Cross),
        "isolated" => Ok(MarginType::Isolated),
        _ => Err("Unknown TxFlow margin mode"),
    }
}
#[derive(Deserialize)]
struct Leverage {
    r#type: String,
    #[serde(default)]
    value: Option<N>,
    #[serde(default)]
    long_value: Option<N>,
    #[serde(default)]
    short_value: Option<N>,
    #[serde(default)]
    mode: Option<String>,
}
impl Leverage {
    fn setting(&self) -> Result<MarketSetting, &'static str> {
        if self
            .mode
            .as_deref()
            .is_some_and(|m| m != "oneWay" && m != "hedge")
        {
            return Err("Unknown TxFlow position mode");
        }
        let margin = margin_type(&self.r#type)?;
        if self.mode.as_deref() == Some("hedge") {
            let a = self.long_value.ok_or("Missing TxFlow long leverage")?.0;
            let b = self.short_value.ok_or("Missing TxFlow short leverage")?.0;
            if a <= 0.0 || b <= 0.0 {
                return Err("Invalid TxFlow leverage");
            }
            Ok(MarketSetting {
                leverage: a.max(b),
                margin_type: Some(margin),
                buy_leverage: Some(a),
                sell_leverage: Some(b),
            })
        } else {
            let value = self.value.ok_or("Missing TxFlow leverage")?.0;
            if value <= 0.0 {
                return Err("Invalid TxFlow leverage");
            }
            Ok(MarketSetting {
                leverage: value,
                margin_type: Some(margin),
                buy_leverage: None,
                sell_leverage: None,
            })
        }
    }
}
#[derive(Deserialize)]
struct AssetData {
    coin: String,
    user: Option<String>,
    leverage: Leverage,
}
#[derive(Deserialize)]
struct AllAssets<'a> {
    user: String,
    #[serde(borrow)]
    coins: &'a RawValue,
}
#[derive(Default)]
struct Children(bool);
impl<'de> Deserialize<'de> for Children {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self(
            Option::<Vec<serde::de::IgnoredAny>>::deserialize(d)?.is_none_or(|c| c.is_empty()),
        ))
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeOrder {
    asset: u32,
    oid: u64,
    side: String,
    order_type: String,
    limit_px: N,
    orig_sz: N,
    sz: N,
    timestamp: N,
    #[serde(default)]
    is_trigger: bool,
    #[serde(default)]
    reduce_only: Option<bool>,
    #[serde(default, deserialize_with = "optional_number")]
    trigger_px: Option<N>,
    #[serde(default)]
    trigger_type: Option<String>,
    #[serde(default)]
    cloid: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    tif: Option<String>,
    #[serde(default)]
    children: Children,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Update {
    order: NativeOrder,
    status: String,
    status_timestamp: N,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Execution {
    tid: u64,
    oid: u64,
    instrument_id: u32,
    px: N,
    sz: N,
    side: String,
    time: N,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FillPage {
    has_more: bool,
}
#[derive(Deserialize)]
struct Frame<'a> {
    channel: &'a str,
    #[serde(borrow)]
    data: &'a RawValue,
}

impl TxFlow {
    pub(crate) fn user(&self) -> Option<&str> {
        self.options
            .vault
            .as_deref()
            .or(self.options.application_id.as_deref())
            .filter(|u| !u.is_empty())
    }
    fn account_subject(&self, user: &str) -> bool {
        self.user()
            .is_some_and(|subject| subject.eq_ignore_ascii_case(user))
    }
    fn account_body(
        &self,
        kind: &str,
        offset: Option<usize>,
        coin: Option<&str>,
        fills: bool,
    ) -> String {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Scope<'a> {
            One(&'a str),
            Many(&'a [&'a str]),
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Read<'a> {
            r#type: &'a str,
            user: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            offset: Option<usize>,
            #[serde(skip_serializing_if = "Option::is_none")]
            limit: Option<usize>,
            #[serde(skip_serializing_if = "Option::is_none")]
            coin: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            account_type: Option<Scope<'a>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            start_time_ms: Option<u64>,
        }
        serde_json::to_string(&Read {
            r#type: kind,
            user: self.user().unwrap_or_default(),
            offset,
            limit: offset.map(|_| PAGE),
            coin,
            account_type: if fills {
                Some(Scope::Many(&["perp"][..]))
            } else if kind == "clearinghouseState" {
                Some(Scope::One("perp"))
            } else {
                None
            },
            start_time_ms: fills.then_some(self.account.startup_at as u64),
        })
        .unwrap()
    }
    pub(crate) fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.started || self.options.public_data_only || self.user().is_none() {
            return;
        }
        self.account.started = true;
        self.account.startup_at = cx.now.floor().max(0.0);
        cx.store.set_hedged(false);
        self.account_refresh(cx);
        self.account.refresh_timer = Some(cx.set_poll_timer(5000.0));
    }
    pub(crate) fn account_refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        self.refresh_account(cx);
    }
    pub(crate) fn refresh_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.account.started {
            return;
        }
        if self.account.inflight.insert("state".into()) {
            let request = Request::State {
                fence: Fence {
                    generation: self.account.generation,
                    revision: self.account.state_revision,
                },
                mode_revision: self.account.settings_revision,
                positions: SnapshotBuilder::default(),
                hold: false,
            };
            self.info(
                cx,
                self.account_body("clearinghouseState", None, None, false),
                ArrayAt::Path(&["data", "assetPositions"]),
                Pending::Account(request),
            );
        }
        self.refresh_orders(cx);
        if !self.account.fills_exhausted && self.account.inflight.insert("fills".into()) {
            let request = Request::Fills {
                generation: self.account.generation,
                offset: 0,
                fills: Vec::new(),
                ids: HashSet::new(),
                valid: true,
            };
            self.info(
                cx,
                self.account_body("userFills", Some(0), None, true),
                ArrayAt::Path(&["data", "data"]),
                Pending::Account(request),
            );
        }
        if self.account.setting_queue.is_empty()
            && (self.account.last_settings == 0.0
                || cx.now - self.account.last_settings >= 60_000.0)
        {
            self.account.last_settings = cx.now;
            self.account.setting_queue = cx.store.markets().iter().map(|m| m.id.clone()).collect();
            self.next_setting(cx);
        }
        if self.account.inflight.contains("state")
            && self.account.inflight.contains("orders")
            && (self.account.fills_exhausted || self.account.inflight.contains("fills"))
        {
            self.node.refresh_due = false;
        }
    }
    pub(crate) fn refresh_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.account.started || !self.account.inflight.insert("orders".into()) {
            return;
        }
        let request = Request::Orders {
            fence: Fence {
                generation: self.account.generation,
                revision: self.account.orders_revision,
            },
            offset: 0,
            orders: SnapshotBuilder::default(),
            ids: HashSet::new(),
            page_rows: 0,
            valid: true,
        };
        self.info(
            cx,
            self.account_body("frontendOpenOrders", Some(0), None, false),
            ArrayAt::Field("data"),
            Pending::Account(request),
        );
    }
    fn next_setting(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.setting_timer.is_some() || self.account.inflight.contains("setting") {
            return;
        }
        let Some(coin) = self.account.setting_queue.pop_front() else {
            return;
        };
        self.account.inflight.insert("setting".into());
        let body = self.account_body("activeAssetData", None, Some(&coin), false);
        cx.request(
            safe_cex_engine::Method::Post,
            format!("{}/info", self.rest()),
            vec![
                ("Content-Type", "application/json".into()),
                ("x-api-version", crate::API_VERSION.into()),
            ],
            Some(body),
            Pending::Account(Request::Setting {
                fence: Fence {
                    generation: self.account.generation,
                    revision: self.account.settings_revision,
                },
                coin,
            }),
        );
    }
    pub(crate) fn acknowledge_write(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.state_revision = self.account.state_revision.saturating_add(1);
        self.account.orders_revision = self.account.orders_revision.saturating_add(1);
        self.account.settings_revision = self.account.settings_revision.saturating_add(1);
        self.refresh_account(cx);
    }
    pub(crate) fn confirmed_setting(
        &self,
        _cx: &Cx<'_, Pending>,
        symbol: &str,
    ) -> Option<&MarketSetting> {
        self.account.settings.get(symbol)
    }
    pub(crate) fn confirm_setting(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        setting: &MarketSetting,
    ) {
        self.account.settings_revision = self.account.settings_revision.saturating_add(1);
        self.account.settings.insert(symbol.into(), *setting);
        cx.store.update_setting(symbol, |s| *s = *setting);
    }
    pub(crate) fn account_disconnected(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.generation = self.account.generation.saturating_add(1);
        self.account.inflight.clear();
        self.account.setting_queue.clear();
        self.account.last_settings = 0.0;
        if let Some(t) = self.account.setting_timer.take() {
            cx.clear_timer(t);
        }
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account.refresh_timer == Some(timer) {
            self.account.refresh_timer = Some(cx.set_poll_timer(5000.0));
            self.refresh_account(cx);
        }
        if self.account.setting_timer == Some(timer) {
            self.account.setting_timer = None;
            self.next_setting(cx);
        }
    }
    fn position_row(&self, row: AssetPosition) -> Result<Option<Position>, &'static str> {
        let p = row.position;
        let asset = p
            .asset
            .parse::<u32>()
            .map_err(|_| "Invalid TxFlow position asset")?;
        let symbol = self
            .symbol(asset)
            .ok_or("Unknown TxFlow position asset")?
            .to_owned();
        let margin = margin_type(&p.leverage.r#type)?;
        let leverage = p.leverage.value.0;
        if leverage <= 0.0 {
            return Err("Invalid TxFlow position leverage");
        }
        let side = if p.szi.0 >= 0.0 {
            PositionSide::Long
        } else {
            PositionSide::Short
        };
        let mode = match row.r#type.to_ascii_lowercase().as_str() {
            "oneway" => PositionMode::Oneway,
            "hedge" => PositionMode::Hedged,
            _ => return Err("Unknown TxFlow position mode"),
        };
        let position = (p.szi.0 != 0.0).then(|| Position {
            id: format!(
                "{symbol}:{}",
                if side == PositionSide::Long {
                    "long"
                } else {
                    "short"
                }
            ),
            symbol: symbol.clone(),
            side,
            entry_price: p.entry_px.map_or(0.0, |n| n.0),
            notional: p.position_value.0.abs(),
            leverage,
            margin_type: Some(margin),
            position_mode: Some(mode),
            unrealized_pnl: p.unrealized_pnl.0,
            realized_pnl: None,
            contracts: p.szi.0.abs(),
            liquidation_price: p.liquidation_px.map_or(0.0, |n| n.0),
            margin_size: Some(p.margin_used.0),
            last_update: p.update_time.map(|n| n.0),
        });
        Ok(position)
    }
    fn order_row(&self, row: NativeOrder, status: &str, at: f64) -> Result<Order, &'static str> {
        let symbol = self
            .symbol(row.asset)
            .ok_or("Unknown TxFlow order asset")?
            .to_owned();
        let side = match row.side.as_str() {
            "B" => OrderSide::Buy,
            "A" => OrderSide::Sell,
            _ => return Err("Unknown TxFlow order side"),
        };
        let status = match status {
            "Open" | "PartialFilled" => OrderStatus::Open,
            "Pending" => OrderStatus::Creating,
            "Filled" => OrderStatus::Closed,
            "PartialCanceled" | "Canceled" | "Rejected" => OrderStatus::Canceled,
            _ => return Err("Unknown TxFlow order status"),
        };
        if row.orig_sz.0 < 0.0 || row.sz.0 < 0.0 || row.sz.0 > row.orig_sz.0 {
            return Err("Invalid TxFlow order size");
        }
        let kind = if row.is_trigger {
            match row
                .trigger_type
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "tp" | "takeprofit" => OrderType::TakeProfit,
                "sl" | "stoploss" => OrderType::StopLoss,
                _ => OrderType::Conditional,
            }
        } else {
            match row.order_type.as_str() {
                "Limit" => OrderType::Limit,
                "Market" => OrderType::Market,
                _ => return Err("Unknown TxFlow order type"),
            }
        };
        let tif = match row.tif.as_deref() {
            None | Some("") => None,
            Some("GTC" | "Gtc") => Some(TimeInForce::GoodTillCancel),
            Some("IOC" | "Ioc") => Some(TimeInForce::ImmediateOrCancel),
            Some("FOK" | "Fok") => Some(TimeInForce::FillOrKill),
            Some("ALO" | "Alo" | "PostOnly") => Some(TimeInForce::PostOnly),
            _ => return Err("Unknown TxFlow order tif"),
        };
        Ok(Order {
            id: row.oid.to_string(),
            parent_id: None,
            status,
            symbol,
            kind,
            side,
            price: if row.is_trigger {
                row.trigger_px.map_or(row.limit_px.0, |n| n.0)
            } else {
                row.limit_px.0
            },
            amount: row.orig_sz.0,
            filled: row.orig_sz.0 - row.sz.0,
            remaining: row.sz.0,
            reduce_only: row.reduce_only.unwrap_or(false),
            close: None,
            client_order_id: row.cloid,
            time_in_force: tif,
            last_update: at.max(row.timestamp.0),
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: Some(OrderExtra {
                take_profit_known: row.children.0,
                stop_loss_known: row.children.0,
                ..OrderExtra::default()
            }),
        })
    }
    /// What a malformed position row may cover: its leg when the size is readable,
    /// else both legs; an unlisted asset matches no stored row. `owned` is false only
    /// for a readable zero size.
    fn skipped_position(&self, item: &str) -> Skipped {
        #[derive(Deserialize)]
        struct Identity<'a> {
            #[serde(borrow)]
            position: Leg<'a>,
        }
        #[derive(Deserialize)]
        struct Leg<'a> {
            asset: std::borrow::Cow<'a, str>,
            #[serde(default, borrow)]
            szi: Option<&'a RawValue>,
        }
        let Ok(row) = serde_json::from_str::<Identity<'_>>(item) else {
            return Skipped::Unidentified;
        };
        let Ok(asset) = row.position.asset.parse::<u32>() else {
            return Skipped::Unidentified;
        };
        let size = row
            .position
            .szi
            .and_then(|raw| serde_json::from_str::<N>(raw.get()).ok())
            .map(|n| n.0)
            .filter(|n| n.is_finite());
        let owned = size != Some(0.0);
        let Some(symbol) = self.symbol(asset) else {
            return Skipped::Unlisted { asset, owned };
        };
        let keys = match size.filter(|n| *n != 0.0) {
            Some(size) if size > 0.0 => vec![format!("{symbol}:long")],
            Some(_) => vec![format!("{symbol}:short")],
            None => vec![format!("{symbol}:long"), format!("{symbol}:short")],
        };
        Skipped::Legs { keys, owned }
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::State {
                positions, hold, ..
            } => match serde_json::from_str::<AssetPosition>(item)
                .map_err(|_| "Malformed TxFlow position")
                .and_then(|p| self.position_row(p))
            {
                Ok(p) => {
                    if let Some(p) = p {
                        self.account.skip_logged.forget(&p.id);
                        positions.push(p);
                    }
                }
                // A skipped row with nothing stored to keep commits as flat: an error.
                Err(_) => match self.skipped_position(item) {
                    Skipped::Unlisted { owned: true, .. } if !self.discovery_settled() => {
                        cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED,
                            "TxFlow owned position asset unavailable; preserving previous account state",
                        );
                        *hold = true;
                    }
                    Skipped::Unlisted { asset, owned } => {
                        log_skip(
                            cx,
                            &mut self.account.skip_logged,
                            format!("asset {asset}"),
                            skip_level(!owned),
                            format!("TxFlow skipped position on unlisted asset {asset}"),
                        );
                        positions.reject(Some(format!("asset {asset}")));
                    }
                    Skipped::Legs { keys, owned } => {
                        let stored = keys.iter().any(|key| cx.store.position(key).is_some());
                        log_skip(
                            cx,
                            &mut self.account.skip_logged,
                            keys.join(", "),
                            skip_level(stored || !owned),
                            format!("TxFlow skipped malformed position {}", keys.join(", ")),
                        );
                        for key in keys {
                            positions.reject(Some(key));
                        }
                    }
                    Skipped::Unidentified => {
                        let level = skip_level(!cx.store.positions().is_empty());
                        log_skip(
                            cx,
                            &mut self.account.skip_logged,
                            String::new(),
                            level,
                            "TxFlow skipped malformed position without an asset",
                        );
                        positions.reject(None);
                    }
                },
            },
            Request::Orders {
                orders,
                ids,
                page_rows,
                valid,
                ..
            } => {
                *page_rows += 1;
                match serde_json::from_str::<NativeOrder>(item)
                    .map_err(|_| "Malformed TxFlow order")
                    .and_then(|r| {
                        let s = r.state.clone().ok_or("Missing TxFlow order state")?;
                        let t = r.timestamp.0;
                        self.order_row(r, &s, t)
                    }) {
                    Ok(o) => {
                        if !ids.insert(o.id.clone()) {
                            *valid = false;
                        }
                        orders.push(o);
                    }
                    Err(_) => {
                        #[derive(Deserialize)]
                        struct Identity {
                            oid: u64,
                        }
                        match serde_json::from_str::<Identity>(item) {
                            Ok(Identity { oid }) => {
                                if !ids.insert(oid.to_string()) {
                                    *valid = false;
                                }
                                cx.notice(
                                    Level::Warn,
                                    code::ROW_SKIPPED,
                                    format!("TxFlow skipped malformed order {oid}"),
                                );
                                orders.reject(Some(oid.to_string()));
                            }
                            Err(_) => {
                                cx.notice(
                                    Level::Warn,
                                    code::ROW_SKIPPED,
                                    "TxFlow skipped malformed order without an id",
                                );
                                orders.reject(None);
                            }
                        }
                    }
                }
            }
            Request::Fills {
                fills, ids, valid, ..
            } => match serde_json::from_str::<Execution>(item) {
                Ok(f)
                    if f.px.0 > 0.0
                        && f.sz.0 > 0.0
                        && f.time.0 >= 0.0
                        && ["A", "B"].contains(&f.side.as_str()) =>
                {
                    if ids.insert(f.tid) {
                        fills.push(f);
                    }
                }
                _ => *valid = false,
            },
            Request::Setting { .. } => {}
        }
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            self.account_failed(cx, request, "TxFlow account HTTP failure");
            return;
        }
        match request {
            Request::State {
                fence,
                mode_revision,
                positions,
                hold,
            } => {
                if fence.generation != self.account.generation {
                    return;
                }
                self.account.inflight.remove("state");
                let Ok(state) = serde_json::from_str::<Envelope<State>>(response.body)
                    .map_err(|_| "Malformed TxFlow account state")
                    .and_then(Envelope::data)
                else {
                    cx.notice(
                        Level::Warn,
                        code::SNAPSHOT_DISCARDED,
                        "Malformed or failed TxFlow account state",
                    );
                    return;
                };
                if hold
                    || fence.revision != self.account.state_revision
                    || state.time.is_some_and(|t| t.0 < self.account.state_time)
                    || state
                        .user
                        .as_deref()
                        .is_some_and(|u| !self.account_subject(u))
                {
                    return;
                }
                self.apply_state(
                    cx,
                    state,
                    positions,
                    mode_revision == self.account.settings_revision,
                );
            }
            Request::Orders {
                fence,
                offset,
                orders,
                ids,
                page_rows,
                valid,
            } => {
                if fence.generation != self.account.generation {
                    return;
                }
                if !valid
                    || serde_json::from_str::<Envelope<serde::de::IgnoredAny>>(response.body)
                        .ok()
                        .and_then(|e| e.data().ok())
                        .is_none()
                {
                    self.account.inflight.remove("orders");
                    cx.notice(
                        Level::Warn,
                        code::SNAPSHOT_DISCARDED,
                        "Malformed or failed TxFlow open orders",
                    );
                    return;
                }
                if fence.revision != self.account.orders_revision {
                    self.account.inflight.remove("orders");
                    return;
                }
                if page_rows == PAGE {
                    if offset >= 200_000 {
                        self.account.inflight.remove("orders");
                        cx.notice(
                            Level::Error,
                            code::SNAPSHOT_DISCARDED,
                            "TxFlow open-order coverage exhausted",
                        );
                        return;
                    }
                    let next = offset + PAGE;
                    self.info(
                        cx,
                        self.account_body("frontendOpenOrders", Some(next), None, false),
                        ArrayAt::Field("data"),
                        Pending::Account(Request::Orders {
                            fence,
                            offset: next,
                            orders,
                            ids,
                            page_rows: 0,
                            valid,
                        }),
                    );
                    return;
                }
                self.account.inflight.remove("orders");
                for order in orders.rows() {
                    self.remember_order(order);
                }
                let orders = orders.commit(cx.store.orders().iter().cloned(), |o| o.id.clone());
                cx.store.set_orders(
                    orders
                        .into_iter()
                        .filter(|o| !self.account.terminal.contains(&o.id))
                        .collect(),
                );
                self.trim_order_memo(cx);
                self.trading_reconcile(cx);
            }
            Request::Fills {
                generation,
                offset,
                fills,
                ids,
                valid,
            } => {
                if generation != self.account.generation {
                    return;
                }
                let page = serde_json::from_str::<Envelope<FillPage>>(response.body)
                    .map_err(|_| "Malformed TxFlow fill page")
                    .and_then(Envelope::data);
                let Ok(page) = page else {
                    self.account.inflight.remove("fills");
                    cx.notice(
                        Level::Warn,
                        code::STREAM_MALFORMED,
                        "Malformed or failed TxFlow execution reconciliation",
                    );
                    return;
                };
                if !valid {
                    self.account.inflight.remove("fills");
                    cx.notice(
                        Level::Warn,
                        code::STREAM_MALFORMED,
                        "Malformed TxFlow execution",
                    );
                    return;
                }
                if page.has_more {
                    if offset >= 200_000 || fills.len() > FILL_CAP {
                        self.account.inflight.remove("fills");
                        self.account.fills_exhausted = true;
                        BackgroundCoverage::Exhausted.report(
                            cx,
                            "TxFlow execution coverage exhausted; restart account reconciliation",
                        );
                        return;
                    }
                    let next = offset + PAGE;
                    self.info(
                        cx,
                        self.account_body("userFills", Some(next), None, true),
                        ArrayAt::Path(&["data", "data"]),
                        Pending::Account(Request::Fills {
                            generation,
                            offset: next,
                            fills,
                            ids,
                            valid,
                        }),
                    );
                    return;
                }
                self.account.inflight.remove("fills");
                if self.account.seen_fills.len()
                    + fills
                        .iter()
                        .filter(|f| !self.account.seen_fills.contains(&f.tid))
                        .count()
                    > FILL_CAP
                {
                    self.account.fills_exhausted = true;
                    BackgroundCoverage::Exhausted.report(cx,"TxFlow execution deduplication capacity exhausted; restart account reconciliation");
                    return;
                }
                for f in fills.into_iter().rev() {
                    if !self.account.seen_fills.insert(f.tid) || f.time.0 < self.account.startup_at
                    {
                        continue;
                    }
                    if let Some(symbol) = self.symbol(f.instrument_id) {
                        cx.fill(Fill {
                            id: None,
                            order_id: f.oid.to_string(),
                            symbol: symbol.into(),
                            side: if f.side == "B" {
                                OrderSide::Buy
                            } else {
                                OrderSide::Sell
                            },
                            price: f.px.0,
                            amount: f.sz.0,
                        });
                    }
                }
            }
            Request::Setting { fence, coin } => {
                if fence.generation != self.account.generation {
                    return;
                }
                self.account.inflight.remove("setting");
                self.account.setting_timer = Some(cx.set_timer(100.0));
                if fence.revision != self.account.settings_revision {
                    return;
                }
                match serde_json::from_str::<Envelope<AssetData>>(response.body)
                    .map_err(|_| "Malformed TxFlow setting")
                    .and_then(Envelope::data)
                    .and_then(|data| self.asset_setting(data, Some(&coin)))
                {
                    Ok((symbol, setting, hedged)) => {
                        self.account.settings_revision =
                            self.account.settings_revision.saturating_add(1);
                        if let Some(hedged) = hedged {
                            cx.store.set_hedged(hedged);
                        }
                        self.account.settings.insert(symbol.clone(), setting);
                        cx.store.update_setting(&symbol, |s| *s = setting);
                    }
                    Err(e) => cx.notice(Level::Warn, code::STREAM_MALFORMED, e),
                }
            }
        }
    }
    fn apply_state(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        state: State,
        positions: SnapshotBuilder<String, Position>,
        apply_mode: bool,
    ) {
        if let Some(time) = state.time {
            self.account.state_time = time.0;
        }
        let equity = state.margin_summary.account_value.0;
        let used = state.margin_summary.total_margin_used.0;
        let upnl = equity - state.margin_summary.total_raw_usd.0;
        cx.store.set_balance(Balance {
            used,
            free: equity - used,
            total: equity,
            upnl,
        });
        // Only an accepted row proves the account mode; retained copies may predate it.
        if apply_mode && let Some(p) = positions.rows().first() {
            cx.store
                .set_hedged(p.position_mode == Some(PositionMode::Hedged));
        }
        if positions.rejected() == 0 {
            self.account.skip_logged.clear();
        }
        let positions = positions.commit(cx.store.positions().iter().cloned(), |p| p.id.clone());
        cx.store.set_positions(positions);
    }
    fn asset_setting(
        &self,
        data: AssetData,
        expected: Option<&str>,
    ) -> Result<(String, MarketSetting, Option<bool>), &'static str> {
        if expected.is_some_and(|e| e != data.coin)
            || data
                .user
                .as_deref()
                .is_some_and(|u| !self.account_subject(u))
        {
            return Err("Mismatched TxFlow account setting");
        }
        let id = data
            .coin
            .parse::<u32>()
            .map_err(|_| "Invalid TxFlow setting asset")?;
        Ok((
            self.symbol(id)
                .ok_or("Unknown TxFlow setting asset")?
                .to_owned(),
            data.leverage.setting()?,
            data.leverage.mode.as_deref().map(|m| m == "hedge"),
        ))
    }
    fn remember_order(&mut self, order: &Order) {
        if self.account.memo_ids.insert(order.id.clone()) {
            self.account.order_memo.push_back(order.id.clone());
        }
        if let Some(cloid) = &order.client_order_id {
            self.account
                .order_ids
                .insert(cloid.clone(), order.id.clone());
        }
    }
    fn trim_order_memo(&mut self, cx: &Cx<'_, Pending>) {
        let limit = 16_384 + cx.store.orders().len();
        while self.account.order_memo.len() > limit {
            let Some(id) = self.account.order_memo.pop_front() else {
                break;
            };
            if cx.store.order(&id).is_some() {
                self.account.order_memo.push_back(id);
                continue;
            }
            self.account.memo_ids.remove(&id);
            self.account.order_times.remove(&id);
            self.account.order_ids.retain(|_, oid| *oid != id);
        }
    }
    pub(crate) fn account_node_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        if !self.account.started {
            return;
        }
        let Ok(frame) = serde_json::from_str::<Frame<'_>>(text) else {
            return;
        };
        match frame.channel {
            "clearinghouseState" => {
                let Ok(state) = serde_json::from_str::<State>(frame.data.get()) else {
                    return;
                };
                if state
                    .user
                    .as_deref()
                    .is_some_and(|u| !self.account_subject(u))
                    || state.time.is_some_and(|n| n.0 < self.account.state_time)
                {
                    return;
                }
                let mut positions = SnapshotBuilder::default();
                let mut valid = true;
                let decoded =
                    json::each_in_field::<AssetPosition>(frame.data.get(), "assetPositions", |p| {
                        match self.position_row(p) {
                            Ok(p) => {
                                if let Some(p) = p {
                                    positions.push(p);
                                }
                            }
                            Err(_) => valid = false,
                        }
                    });
                if !valid || !matches!(decoded, Ok(true)) {
                    return;
                }
                self.account.state_revision = self.account.state_revision.saturating_add(1);
                self.account.settings_revision = self.account.settings_revision.saturating_add(1);
                self.apply_state(cx, state, positions, true);
            }
            "orderUpdates" => {
                let mut orders = Vec::new();
                let mut valid = true;
                if json::each::<Update>(frame.data.get(), |update| {
                    match self.order_row(update.order, &update.status, update.status_timestamp.0) {
                        Ok(o) => orders.push(o),
                        Err(_) => valid = false,
                    }
                })
                .is_err()
                    || !valid
                {
                    return;
                }
                self.account.orders_revision = self.account.orders_revision.saturating_add(1);
                for o in orders {
                    if self
                        .account
                        .order_times
                        .get(&o.id)
                        .is_some_and(|t| *t > o.last_update)
                        || self.account.terminal.contains(&o.id)
                    {
                        continue;
                    }
                    self.account.order_times.insert(o.id.clone(), o.last_update);
                    self.remember_order(&o);
                    if matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled) {
                        self.account.terminal.insert(o.id.clone());
                        self.account.terminal_queue.push_back(o.id.clone());
                        cx.store.remove_orders(&[&o.id]);
                    } else {
                        cx.store.upsert_order(o);
                    }
                }
                while self.account.terminal_queue.len() > 16_384 {
                    if let Some(id) = self.account.terminal_queue.pop_front() {
                        self.account.terminal.remove(&id);
                    }
                }
                self.trim_order_memo(cx);
                self.trading_reconcile(cx);
                // The event lacks execution price/size, so wake REST reconciliation only.
                self.refresh_account(cx);
            }
            "activeAssetData" => {
                if let Ok(data) = serde_json::from_str::<AssetData>(frame.data.get())
                    && let Ok((symbol, setting, hedged)) = self.asset_setting(data, None)
                {
                    self.account.settings_revision =
                        self.account.settings_revision.saturating_add(1);
                    if let Some(hedged) = hedged {
                        cx.store.set_hedged(hedged);
                    }
                    self.account.settings.insert(symbol.clone(), setting);
                    cx.store.update_setting(&symbol, |s| *s = setting);
                }
            }
            "userAssetData" => {
                let Ok(data) = serde_json::from_str::<AllAssets<'_>>(frame.data.get()) else {
                    return;
                };
                if !self.account_subject(&data.user) {
                    return;
                }
                let mut settings = Vec::new();
                let mut valid = true;
                if json::each::<AssetData>(data.coins.get(), |a| {
                    match self.asset_setting(a, None) {
                        Ok(s) => settings.push(s),
                        Err(_) => valid = false,
                    }
                })
                .is_err()
                    || !valid
                {
                    return;
                }
                self.account.settings_revision = self.account.settings_revision.saturating_add(1);
                for (symbol, setting, hedged) in settings {
                    if let Some(hedged) = hedged {
                        cx.store.set_hedged(hedged);
                    }
                    self.account.settings.insert(symbol.clone(), setting);
                    cx.store.update_setting(&symbol, |s| *s = setting);
                }
            }
            _ => {}
        }
    }
    pub(crate) fn account_api_frame(&mut self, _cx: &mut Cx<'_, Pending>, _text: &str) { /* userFills on this socket is historical, never a live fill source. */
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        let (generation, key) = match request {
            Request::State { fence, .. } => (fence.generation, "state"),
            Request::Orders { fence, .. } => (fence.generation, "orders"),
            Request::Fills { generation, .. } => (generation, "fills"),
            Request::Setting { fence, .. } => (fence.generation, "setting"),
        };
        if generation != self.account.generation {
            return;
        }
        self.account.inflight.remove(key);
        if key == "setting" {
            self.account.setting_timer = Some(cx.set_timer(100.0));
        }
        cx.notice(Level::Warn, code::REQUEST_FAILED, error);
    }
}

#[cfg(test)]
#[path = "../tests/unit/account_coverage.rs"]
mod account_coverage_tests;
