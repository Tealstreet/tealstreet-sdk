//! Venue method bodies over the shared exchange, Store and explicit host capabilities.
use crate::{
    data::{self, get, number, property, string, truthy},
    executions, protocol,
};
use safe_cex_exchange::{
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::{JsJsonValue as J, math_round, subtract};
use safe_cex_types::{
    Balance, ExchangeOptions, Execution, MarginType, Market, MarketLimits, MarketLimitsAmount,
    MarketLimitsLeverage, MarketPrecision, ModelField, Order, OrderSide, OrderStopTriggerType,
    OrderTimeInForce, OrderType, Position, PositionMode, PositionSide,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

#[derive(Debug, Clone)]
pub struct ApexRequest {
    pub method: &'static str,
    pub path: &'static str,
    pub params: J,
    pub data: J,
    pub content_type: Option<&'static str>,
    pub timeout_ms: Option<f64>,
    pub unlimited: bool,
}
pub trait ApexHost {
    fn services(&self) -> Result<Rc<dyn crate::lifecycle::ApexServices>, ExchangeError>;
    fn create_api(
        &self,
        options: &ExchangeOptions,
        exchange: Rc<BaseExchange>,
        max_rps: Option<u8>,
    ) -> Result<(), ExchangeError>;
    fn create_websocket(
        &self,
        parent: std::rc::Weak<ApexOmniExchange>,
        private: bool,
    ) -> Result<(), ExchangeError>;
    fn dispose_websocket(&self, private: bool) -> Result<(), ExchangeError>;
    fn request(&self, request: ApexRequest) -> ExchangeFuture<'_, J>;
    fn date_parse(&self, text: &str) -> Result<f64, ExchangeError>;
    fn console(&self, level: &str, message: &str, detail: Option<&J>) -> Result<(), ExchangeError>;
}

pub struct ApexOmniExchange {
    pub base: Rc<BaseExchange>,
    pub host: Rc<dyn ApexHost>,
    pub unified_margin_status: Cell<f64>,
    pub network_id: Cell<f64>,
    pub last_websocket_positions_update: Cell<Option<f64>>,
    pub recently_cancelled_orders: RefCell<HashMap<String, f64>>,
    pub recent_ws_executions: RefCell<Vec<Execution>>,
}

pub(crate) fn failure(error: String) -> ExchangeError {
    ExchangeError::new(error)
}
pub(crate) fn float(value: &J) -> Result<f64, ExchangeError> {
    executions::parse(value).map_err(|message| {
        let mut error = failure(message);
        error.name = "TypeError".into();
        error
    })
}
pub(crate) fn numeric(value: &J) -> Result<f64, ExchangeError> {
    number(value).map_err(|message| {
        let mut error = failure(message);
        error.name = "TypeError".into();
        error
    })
}
pub(crate) fn field_string(value: &J) -> Result<String, ExchangeError> {
    if let J::String(value) = value {
        Ok(value.clone())
    } else {
        Err(failure(
            "shared typed record refuses a non-string field".into(),
        ))
    }
}
pub(crate) fn text(value: &J) -> Result<String, ExchangeError> {
    string(value).map_err(|message| {
        let mut error = failure(message);
        error.name = "TypeError".into();
        error
    })
}
fn or_zero(value: &J) -> J {
    if truthy(value) {
        value.clone()
    } else {
        J::Number(0.0)
    }
}
fn nullish_zero(value: &J) -> J {
    if matches!(value, J::Undefined | J::Null) {
        J::Number(0.0)
    } else {
        value.clone()
    }
}
pub(crate) fn fields(value: &J) -> Result<serde_json::Value, ExchangeError> {
    fn retained(
        value: &J,
    ) -> Result<safe_cex_types::retained_json::RetainedJsonInput, ExchangeError> {
        use safe_cex_types::retained_json::RetainedJsonInput as R;
        Ok(match value {
            J::Null => R::Null,
            J::Bool(v) => R::Bool(*v),
            J::Number(v) => R::Float(*v),
            J::String(v) => R::Text(v.encode_utf16().collect()),
            J::Array(v) => R::Array(v.iter().map(retained).collect::<Result<_, _>>()?),
            J::Object(v) => R::Object(
                v.iter()
                    .filter(|(_, v)| !matches!(v, J::Undefined))
                    .map(|(k, v)| Ok((k.encode_utf16().collect(), retained(v)?)))
                    .collect::<Result<_, ExchangeError>>()?,
            ),
            _ => {
                return Err(failure(
                    "shared retained JSON refuses this prepared value".into(),
                ));
            }
        })
    }
    retained(value)?
        .into_value()
        .map_err(|e| failure(e.to_string()))
}

impl ApexOmniExchange {
    // ts: ApexOmniExchange.constructor
    pub fn new(mut base: BaseExchange, host: Rc<dyn ApexHost>) -> Result<Rc<Self>, ExchangeError> {
        base.settings.client_order_id_rule = Some(
            safe_cex_exchange::client_order_id::pattern_client_order_id_rule(
                "ApeX Omni".into(),
                r"/^\S+$/".into(),
                Rc::new(|id| !id.is_empty() && safe_cex_num::js_split_whitespace(id).len() == 1),
                Default::default(),
            ),
        );
        let venue = Rc::new(Self {
            base: Rc::new(base),
            host,
            unified_margin_status: Cell::new(1.0),
            network_id: Cell::new(1.0),
            last_websocket_positions_update: Cell::new(None),
            recently_cancelled_orders: RefCell::new(HashMap::new()),
            recent_ws_executions: RefCell::new(Vec::new()),
        });
        venue
            .host
            .create_api(&venue.base.options(), venue.base.clone(), Some(5))?;
        venue
            .host
            .create_api(&venue.base.options(), venue.base.clone(), None)?;
        venue.host.create_websocket(Rc::downgrade(&venue), false)?;
        venue.host.create_websocket(Rc::downgrade(&venue), true)?;
        Ok(venue)
    }
    // ts: ApexOmniExchange.dispose
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        self.base.dispose()?;
        self.host.dispose_websocket(false)?;
        self.host.dispose_websocket(true)
    }
    // ts: ApexOmniExchange.get accountType
    pub fn account_type(&self) -> &'static str {
        if self.unified_margin_status.get() == 1.0 {
            "CONTRACT"
        } else {
            "UNIFIED"
        }
    }

    pub(crate) fn request(
        &self,
        method: &'static str,
        path: &'static str,
        params: J,
        data: J,
        unlimited: bool,
    ) -> ExchangeFuture<'_, J> {
        self.host.request(ApexRequest {
            method,
            path,
            params,
            data,
            content_type: None,
            timeout_ms: None,
            unlimited,
        })
    }
    // ts: ApexOmniExchange.getAccount
    pub async fn get_account(&self) -> Result<J, ExchangeError> {
        let response = self
            .request("get", "/v3/account", J::Undefined, J::Undefined, false)
            .await?;
        Ok(data::object([
            (
                "userId",
                property(property(&response, "data")?, "id")?.clone(),
            ),
            ("affiliateId", J::Null),
        ]))
    }
    // ts: ApexOmniExchange.fetchRawBalance
    pub async fn fetch_raw_balance(&self) -> Result<Balance, ExchangeError> {
        let response = self
            .request(
                "get",
                "/v3/account-balance",
                J::Undefined,
                J::Undefined,
                false,
            )
            .await?;
        let balance = get(&response, "data");
        if !truthy(balance) {
            self.base.log(
                "Balance response carried no account payload; keeping the last known balance",
                "warning",
            )?;
            return Ok(self.base.store.balance().clone());
        }
        let mut upnl = 0.0;
        for position in self.base.store.positions().iter() {
            if let Some(ticker) = self.base.store.tickers_by_symbol().get(&position.symbol) {
                upnl += (if position.side == PositionSide::Long {
                    ticker.last - position.entry_price
                } else {
                    position.entry_price - ticker.last
                }) * position.contracts;
            }
        }
        Ok(Balance {
            total: subtract(float(get(balance, "totalEquityValue"))?, upnl),
            upnl,
            used: float(get(balance, "maintenanceMargin"))?,
            free: numeric(get(balance, "totalEquityValue"))?
                - numeric(get(balance, "initialMargin"))?,
            info: Some(fields(balance)?),
            currencies: None,
            coins: None,
        })
    }
    // ts: ApexOmniExchange.fetchBalance
    pub async fn fetch_balance(&self) -> Result<Balance, ExchangeError> {
        let balance = self.fetch_raw_balance().await?;
        Ok(if self.base.is_disposed() {
            self.base.store.balance().clone()
        } else {
            balance
        })
    }
    // ts: ApexOmniExchange.mapPosition
    pub fn map_position(&self, row: &J) -> Result<Option<Position>, ExchangeError> {
        self.map_position_with_log(row, |message| self.base.log(message, "warning"))
    }
    pub(crate) fn map_position_with_log(
        &self,
        row: &J,
        log: impl FnOnce(&str) -> Result<(), ExchangeError>,
    ) -> Result<Option<Position>, ExchangeError> {
        let entry = if truthy(data::v(row, "avgPrice")?) {
            data::v(row, "avgPrice")?.clone()
        } else {
            or_zero(data::v(row, "entryPrice")?)
        };
        let entry_price = float(&entry)?;
        let contracts = float(&nullish_zero(get(row, "size")))?;
        let side = if matches!(get(row,"side"),J::String(s) if s == "SHORT") {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let unrealized_pnl = float(&or_zero(data::v(row, "unrealisedPnl")?))?;
        let symbol = text(get(row, "symbol"))?;
        let Some(market) = self.base.store.markets_by_id().get(&symbol).cloned() else {
            log(&format!("Market not found for position symbol: {symbol}"))?;
            return Ok(None);
        };
        let id = safe_cex_types::model_utils::create_position_id(
            &symbol,
            Some(side),
            Some(PositionMode::Hedged),
        );
        let custom = numeric(get(row, "customImr"))?;
        let leverage = if custom != 0.0 && !custom.is_nan() {
            1.0 / custom
        } else {
            let custom_initial = numeric(get(row, "customInitialMarginRate"))?;
            if custom_initial != 0.0 && !custom_initial.is_nan() {
                1.0 / custom_initial
            } else {
                let previous = self
                    .base
                    .store
                    .positions()
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.leverage);
                if let Some(previous) = previous.filter(|v| *v != 0.0 && !v.is_nan()) {
                    previous
                } else {
                    let default = market
                        .info
                        .as_ref()
                        .and_then(|v| v.get("defaultInitialMarginRate"))
                        .cloned()
                        .map(J::from)
                        .unwrap_or(J::Undefined);
                    let default = 1.0 / numeric(&default)?;
                    if default != 0.0 && !default.is_nan() {
                        default
                    } else {
                        0.0
                    }
                }
            }
        };
        Ok(Some(Position {
            id,
            symbol: market.symbol,
            side: side.into(),
            entry_price,
            notional: contracts * entry_price,
            leverage: math_round(leverage),
            unrealized_pnl,
            realized_pnl: (Some(float(&or_zero(data::v(row, "curRealisedPnl")?))?)).into(),
            contracts,
            liquidation_price: float(&or_zero(data::v(row, "liqPrice")?))?,
            position_mode: (Some(PositionMode::Hedged)).into(),
            margin_type: (Some(MarginType::Cross)).into(),
            info: (Some(fields(row)?)).into(),
            last_update: (Some(safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms()))
                .into(),
            margin_size: (None).into(),
            extra: (None).into(),
            option: (None).into(),
        }))
    }
    // ts: ApexOmniExchange.mapOrder
    pub fn map_order(&self, row: &J) -> Result<Vec<Order>, ExchangeError> {
        use safe_cex_num::{TableHit, js_table_lookup};
        // Table keys are evaluated at the source call sites, but shared typed
        // output admission waits until the source's coercions and clocks finish.
        let type_key = text(data::v(row, "type")?)?;
        let order_type = js_table_lookup(&protocol::ORDER_TYPE, &type_key);
        let time_in_force = if truthy(property(row, "postOnly")?) {
            Some(OrderTimeInForce::PostOnly)
        } else {
            let key = text(property(row, "timeInForce")?)?;
            match js_table_lookup(&protocol::ORDER_TIME_IN_FORCE, &key) {
                TableHit::Mapped(value) => Some(*value),
                TableHit::Inherited(_) | TableHit::Missing => None,
            }
        };
        let symbol = text(property(row, "symbol")?)?;
        let Some(market) = self.base.store.markets_by_id().get(&symbol).cloned() else {
            self.host.console(
                "warn",
                "[APEX OMNI] Market not found for order:",
                Some(property(row, "symbol")?),
            )?;
            return Ok(Vec::new());
        };
        let raw_id = property(row, "id")?;
        let status_key = text(data::v(row, "status")?)?;
        let status = js_table_lookup(&protocol::ORDER_STATUS, &status_key);
        let raw_side = property(row, "side")?;
        let side_key = match raw_side {
            J::String(value) => value.to_uppercase(),
            J::Undefined | J::Null => "undefined".into(),
            _ => {
                let mut error = failure("o.side?.toUpperCase is not a function".into());
                error.name = "TypeError".into();
                return Err(error);
            }
        };
        let side = js_table_lookup(&protocol::ORDER_SIDE, &side_key);
        let trigger = data::v(row, "triggerPrice")?;
        let raw_price = if truthy(trigger) && numeric(property(row, "triggerPrice")?)? > 0.0 {
            trigger
        } else {
            data::v(row, "price")?
        };
        let price = float(raw_price)?;
        let amount = float(&nullish_zero(property(row, "size")?))?;
        let filled = float(&nullish_zero(data::v(row, "cumSuccessFillSize")?))?;
        let raw_reduce_only = data::v(row, "reduceOnly")?;
        let remaining = subtract(
            numeric(&nullish_zero(property(row, "size")?))?,
            numeric(&nullish_zero(data::v(row, "cumSuccessFillSize")?))?,
        );
        let last_update = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
        let trigger_type = |part: &J| -> Result<Option<OrderStopTriggerType>, ExchangeError> {
            let key = text(data::v(part, "triggerPriceType")?)?;
            Ok(match js_table_lookup(&protocol::ORDER_TRIGGER_TYPE, &key) {
                TableHit::Mapped(value) => Some(*value),
                TableHit::Inherited(_) | TableHit::Missing => None,
            })
        };
        let mut tp_trigger_type = None;
        let mut sl_trigger_type = None;
        if matches!(order_type, TableHit::Mapped(OrderType::TakeProfit)) {
            tp_trigger_type = trigger_type(row)?;
        }
        if matches!(order_type, TableHit::Mapped(OrderType::StopLoss)) {
            sl_trigger_type = trigger_type(row)?;
        }
        let sl_part = data::v(row, "openSlParams")?;
        let sl_part = if truthy(sl_part) {
            sl_part
        } else {
            data::v(row, "openSlParam")?
        };
        let tp_part = data::v(row, "openTpParams")?;
        let tp_part = if truthy(tp_part) {
            tp_part
        } else {
            data::v(row, "openTpParam")?
        };
        let sl_price = float(data::v(sl_part, "triggerPrice")?)?;
        let tp_price = float(data::v(tp_part, "triggerPrice")?)?;
        let mut brackets = Vec::new();
        for (part, price, kind, suffix) in [
            (sl_part, sl_price, OrderType::StopLoss, "__stop_loss"),
            (tp_part, tp_price, OrderType::TakeProfit, "__take_profit"),
        ] {
            if price > 0.0 {
                let id = format!("{}{suffix}", text(raw_id)?);
                let updated = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
                brackets.push((id, price, kind, updated, trigger_type(part)?));
            }
        }
        let order_type = match order_type {
            TableHit::Mapped(value) => ModelField::Value(*value),
            TableHit::Missing => ModelField::Undefined,
            TableHit::Inherited(_) => {
                return Err(failure(
                    "shared typed Order refuses unknown venue order type".into(),
                ));
            }
        };
        let status = match status {
            TableHit::Mapped(value) => ModelField::Value(*value),
            TableHit::Missing => ModelField::Undefined,
            TableHit::Inherited(_) => {
                return Err(failure(
                    "shared typed Order refuses unknown venue order status".into(),
                ));
            }
        };
        let side = match side {
            TableHit::Mapped(value) => ModelField::Value(*value),
            TableHit::Missing => ModelField::Undefined,
            TableHit::Inherited(_) => {
                return Err(failure(
                    "shared typed Order refuses unknown venue order side".into(),
                ));
            }
        };
        let reduce_only = match raw_reduce_only {
            J::Bool(value) => *value,
            value if !truthy(value) => false,
            value => {
                self.host.console(
                    "warn",
                    &format!("[APEX OMNI] Refused order row: Order.reduceOnly must be boolean, got {value:?}"),
                    None,
                )?;
                return Ok(Vec::new());
            }
        };
        let order = Order {
            client_order_id: (None).into(),
            id: field_string(raw_id)?,
            status,
            symbol: market.symbol,
            r#type: order_type,
            side,
            price: price.into(),
            amount,
            filled,
            remaining,
            reduce_only: reduce_only.into(),
            info: (Some(fields(row)?)).into(),
            time_in_force: time_in_force.into(),
            last_update,
            tp_trigger_type: tp_trigger_type.into(),
            sl_trigger_type: sl_trigger_type.into(),
            parent_id: (None).into(),
            close: (None).into(),
            trigger_type: (None).into(),
            take_profit: (None).into(),
            stop_loss: (None).into(),
            extra: (None).into(),
        };
        let mut orders = vec![order.clone()];
        for (id, price, kind, updated, trigger) in brackets {
            let mut bracket = order.clone();
            bracket.id = id;
            bracket.r#type = (kind).into();
            bracket.side = if order.side == OrderSide::Buy {
                safe_cex_types::ModelField::Value(OrderSide::Sell)
            } else {
                safe_cex_types::ModelField::Value(OrderSide::Buy)
            };
            bracket.price = (price).into();
            bracket.filled = 0.0;
            bracket.remaining = order.amount;
            bracket.last_update = updated;
            if kind == OrderType::StopLoss {
                bracket.sl_trigger_type = (trigger).into();
            } else {
                bracket.tp_trigger_type = (trigger).into();
            }
            orders.push(bracket);
        }
        Ok(orders)
    }
    // ts: ApexOmniExchange.fetchOrders
    pub async fn fetch_orders(&self) -> Result<Vec<Order>, ExchangeError> {
        let response = self
            .request("get", "/v3/open-orders", J::Undefined, J::Undefined, false)
            .await?;
        if self.base.is_disposed() {
            return Ok(Vec::new());
        }
        let rows = property(&response, "data")?;
        let now = self.base.runtime.now_ms();
        self.recently_cancelled_orders
            .borrow_mut()
            .retain(|_, timestamp| {
                let age = now - *timestamp;
                age.is_nan() || age <= 10_000.0
            });
        let empty = J::Array(Vec::new());
        let rows = if truthy(rows) { rows } else { &empty };
        let mut filtered_orders = Vec::new();
        for row in data::array(rows).map_err(|_| {
            let mut error = failure("orders.filter is not a function".into());
            error.name = "TypeError".into();
            error
        })? {
            let id = property(row, "id")?;
            // Source Map.has compares keys without string coercion.
            let cancelled = if let J::String(id) = id {
                self.recently_cancelled_orders.borrow().contains_key(id)
            } else {
                false
            };
            if !cancelled {
                filtered_orders.push(row);
            }
        }
        let mut orders = Vec::new();
        for row in filtered_orders {
            orders.extend(self.map_order(row)?);
        }
        Ok(orders)
    }
    // ts: ApexOmniExchange#normalizeApexSymbolKey
    pub fn normalize_apex_symbol_key(&self, value: &J) -> Result<String, ExchangeError> {
        executions::normalize_apex_symbol_key(value).map_err(failure)
    }
    // ts: ApexOmniExchange#normalizeExecutionTimestampToSeconds
    pub fn normalize_execution_timestamp_to_seconds(
        &self,
        value: &J,
    ) -> Result<Option<f64>, ExchangeError> {
        executions::normalize_execution_timestamp_to_seconds(value, |text| {
            self.host.date_parse(text)
        })
    }
    // ts: ApexOmniExchange.recordWsExecution
    pub fn record_ws_execution(&self, row: &J) -> Result<(), ExchangeError> {
        let raw = executions::row_symbol(row);
        if !truthy(raw) {
            return Ok(());
        }
        let row_symbol = text(raw)?;
        let key = self.normalize_apex_symbol_key(raw)?;
        let market = self
            .base
            .store
            .markets_by_id()
            .get(&row_symbol)
            .cloned()
            .or_else(|| {
                self.base
                    .store
                    .markets_by_symbol()
                    .get(&row_symbol)
                    .cloned()
            })
            .or_else(|| {
                self.base
                    .store
                    .markets()
                    .iter()
                    .find(|m| {
                        self.normalize_apex_symbol_key(&J::String(m.id.clone()))
                            .is_ok_and(|s| s == key)
                            || self
                                .normalize_apex_symbol_key(&J::String(m.symbol.clone()))
                                .is_ok_and(|s| s == key)
                    })
                    .cloned()
            });
        let Some(execution) = executions::map_ws_execution(
            row,
            market.as_ref().map_or(&row_symbol, |m| &m.symbol),
            market.as_ref().map(|m| m.quote.as_str()),
            |s| self.host.date_parse(s),
        )?
        else {
            return Ok(());
        };
        let mut executions = self.recent_ws_executions.borrow_mut();
        executions.retain(|e| {
            e.order_id != execution.order_id
                || e.timestamp != execution.timestamp
                || e.price != execution.price
                || e.qty != execution.qty
                || e.side != execution.side
        });
        executions.insert(0, execution);
        executions.sort_by(|a, b| {
            b.timestamp
                .partial_cmp(&a.timestamp)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        executions.truncate(500);
        Ok(())
    }
    // ts: ApexOmniExchange#getWsExecutionFallback
    pub fn get_ws_execution_fallback(
        &self,
        symbol: &str,
        order_id: Option<&str>,
        from: Option<f64>,
        to: Option<f64>,
        limit: f64,
    ) -> Result<Vec<Execution>, ExchangeError> {
        let key = self.normalize_apex_symbol_key(&J::String(symbol.to_owned()))?;
        let mut result = Vec::new();
        for e in self.recent_ws_executions.borrow().iter() {
            if self.normalize_apex_symbol_key(&J::String(e.symbol.clone()))? != key {
                continue;
            }
            if let (Some(wanted), Some(actual)) =
                (order_id.filter(|s| !s.is_empty()), e.order_id.as_deref())
                && !actual.is_empty()
                && actual != wanted
            {
                continue;
            }
            if from.is_some_and(|f| f != 0.0 && !f.is_nan() && e.timestamp * 1000.0 < f)
                || to.is_some_and(|t| t != 0.0 && !t.is_nan() && e.timestamp * 1000.0 > t)
            {
                continue;
            }
            result.push(e.clone());
        }
        result.sort_by(|a, b| {
            b.timestamp
                .partial_cmp(&a.timestamp)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let keep = if limit.is_nan() {
            0
        } else if limit < 0.0 {
            (result.len() as f64 + limit.trunc()).max(0.0) as usize
        } else {
            limit.min(result.len() as f64) as usize
        };
        result.truncate(keep);
        Ok(result)
    }
}

impl ApexOmniExchange {
    // ts: ApexOmniExchange.fetchPositions
    pub async fn fetch_positions(&self) -> Result<Vec<Position>, ExchangeError> {
        let response = self
            .request("get", "/v3/account", J::Undefined, J::Undefined, false)
            .await?;
        if self.base.is_disposed() {
            return Ok(Vec::new());
        }
        let account = property(&response, "data")?;
        let account_positions = property(account, "positions")?;
        let mut positions = Vec::new();
        if !matches!(account_positions, J::Undefined | J::Null) {
            for row in data::array(account_positions).map_err(|_| {
                let mut error = failure("data.data.positions?.map is not a function".into());
                error.name = "TypeError".into();
                error
            })? {
                if let Some(position) = self.map_position(row)? {
                    positions.push(position);
                }
            }
        }
        let old = self
            .base
            .store
            .zk_link_config()
            .as_deref()
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        if !safe_cex_num::js_truthy(old.get("l2Key")) {
            let mut config = old.as_object().cloned().unwrap_or_default();
            for (key, source) in [
                ("l2Key", "l2key"),
                ("ethereumAddress", "ethereumAddress"),
                ("id", "id"),
                ("spotAccount", "spotAccount"),
                ("spotWallets", "spotWallets"),
                ("contractAccount", "contractAccount"),
                ("contractWallets", "contractWallets"),
            ] {
                let value = get(account, source);
                if !matches!(value, J::Undefined) {
                    config.insert(key.into(), fields(value)?);
                } else {
                    config.remove(key);
                }
            }
            self.base.store.update_zk_link_config(Some(config.into()))?;
        }
        for market in self.base.store.markets().iter() {
            let margin = market
                .info
                .as_ref()
                .and_then(|v| v.get("initialMarginRate"))
                .cloned()
                .map(J::from)
                .unwrap_or(J::Undefined);
            let leverage = 1.0 / numeric(&margin)?;
            let leverage = if leverage == 0.0 || leverage.is_nan() {
                10.0
            } else {
                leverage
            };
            for side in [PositionSide::Long, PositionSide::Short] {
                positions.push(Position {
                    id: safe_cex_types::model_utils::create_position_id(
                        &market.symbol,
                        Some(side),
                        Some(PositionMode::Hedged),
                    ),
                    symbol: market.symbol.clone(),
                    side: side.into(),
                    entry_price: 0.0,
                    notional: 0.0,
                    unrealized_pnl: 0.0,
                    contracts: 0.0,
                    leverage,
                    liquidation_price: 0.0,
                    position_mode: (Some(PositionMode::Hedged)).into(),
                    margin_type: (Some(MarginType::Cross)).into(),
                    realized_pnl: (None).into(),
                    last_update: (None).into(),
                    info: (None).into(),
                    margin_size: (None).into(),
                    extra: (None).into(),
                    option: (None).into(),
                });
            }
        }
        let mut seen = std::collections::HashSet::new();
        positions.retain(|p| seen.insert(p.id.clone()));
        Ok(positions)
    }
    // ts: ApexOmniExchange.fetchMarketsHelper
    pub async fn fetch_markets_helper(&self) -> Result<Vec<Market>, ExchangeError> {
        let response = self
            .request("get", "/v3/symbols", J::Undefined, J::Undefined, false)
            .await?;
        let config = property(property(&response, "data")?, "contractConfig")?;
        let mut old = self
            .base
            .store
            .zk_link_config()
            .as_deref()
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default();
        if matches!(config, J::Undefined) {
            // The owned Store snapshot omits this source undefined property.
            old.remove("contractConfig");
        } else {
            old.insert("contractConfig".into(), fields(config)?);
        }
        self.base.store.update_zk_link_config(Some(old.into()))?;
        let contracts = property(config, "perpetualContract")?;
        property(contracts, "filter")?;
        let rows = data::array(contracts).map_err(|_| {
            let mut error =
                failure("contractConfig.perpetualContract.filter is not a function".into());
            error.name = "TypeError".into();
            error
        })?;
        let mut filtered = Vec::new();
        for row in rows {
            if truthy(property(row, "enableTrade")?) {
                filtered.push(row);
            }
        }
        // Complete the source map before admitting any typed output fields.
        let mut mapped = Vec::new();
        for row in filtered {
            let amount = float(get(row, "stepSize"))?;
            let price = float(get(row, "tickSize"))?;
            let min_amount = float(get(row, "minOrderSize"))?;
            let max_amount = float(get(row, "maxOrderSize"))?;
            let min_leverage = float(get(row, "displayMinLeverage"))?;
            let max_leverage = float(get(row, "displayMaxLeverage"))?;
            mapped.push((
                row,
                MarketPrecision { amount, price },
                MarketLimits {
                    amount: MarketLimitsAmount {
                        min: min_amount,
                        max: max_amount,
                    },
                    leverage: MarketLimitsLeverage {
                        min: min_leverage,
                        max: max_leverage,
                    },
                    price: None,
                },
            ));
        }
        let mut markets = Vec::new();
        for (index, (row, precision, limits)) in mapped.into_iter().enumerate() {
            let projected: Result<Market, ExchangeError> = (|| {
                let typed_string = |field, raw_field| {
                    let value = get(row, raw_field);
                    field_string(value)
                        .map_err(|cause| failure(format!("{field}={value:?}: {}", cause.message)))
                };
                let active = if let J::Bool(value) = get(row, "enableTrade") {
                    *value
                } else {
                    return Err(failure(format!(
                        "active={:?}: expected boolean",
                        get(row, "enableTrade")
                    )));
                };
                Ok(Market {
                    id: typed_string("id", "symbol")?,
                    symbol: typed_string("symbol", "crossSymbolName")?,
                    base: typed_string("base", "baseTokenId")?,
                    quote: typed_string("quote", "settleAssetId")?,
                    active,
                    precision,
                    limits,
                    is_expiring: Some(false),
                    is_perpetual: None,
                    info: Some(
                        fields(row)
                            .map_err(|cause| failure(format!("info={row:?}: {}", cause.message)))?,
                    ),
                    pretty_symbol: None,
                    contract_size: None,
                    is_inverse: None,
                    is_spot: None,
                    is_read_only: None,
                    taker_fee: None,
                    maker_fee: None,
                    option: None,
                })
            })();
            match projected {
                Ok(market) => markets.push(market),
                Err(cause) => self.base.log(
                    &format!(
                        "Refused ApexOmni market row {index} id={:?}: {}",
                        get(row, "symbol"),
                        cause.message
                    ),
                    "warning",
                )?,
            }
        }
        Ok(markets)
    }
    // ts: ApexOmniExchange.fetchMarkets
    pub async fn fetch_markets(&self) -> Result<Vec<Market>, ExchangeError> {
        self.fetch_markets_helper().await
    }

    // ts: ApexOmniExchange.cancelOrdersHelper
    pub async fn cancel_orders_helper(
        &self,
        orders: &[Order],
        _category: &str,
    ) -> Result<(), ExchangeError> {
        let mut bases = std::collections::HashSet::new();
        let mut cancel = Vec::new();
        let mut remove = Vec::new();
        for order in orders {
            if order.id.contains("__stop_loss") || order.id.contains("__take_profit") {
                let base = order.id.split("__").next().unwrap().to_owned();
                if bases.insert(base.clone()) {
                    cancel.push(base);
                }
                remove.push(order.id.clone());
            } else {
                cancel.push(order.id.clone());
                remove.extend([
                    order.id.clone(),
                    format!("{}__stop_loss", order.id),
                    format!("{}__take_profit", order.id),
                ]);
            }
        }
        let now = self.base.runtime.now_ms();
        for id in &cancel {
            self.recently_cancelled_orders
                .borrow_mut()
                .insert(id.clone(), now);
        }
        self.base
            .store
            .remove_orders(&remove.iter().map(String::as_str).collect::<Vec<_>>())?;
        for (index, lot) in cancel.chunks(10).enumerate() {
            if index > 0 {
                self.base.runtime.sleep(1000).await?;
            }
            for id in lot {
                self.host
                    .request(ApexRequest {
                        method: "post",
                        path: "/v3/delete-order",
                        params: J::Undefined,
                        data: data::object([("id", J::String(id.clone()))]),
                        content_type: Some("application/x-www-form-urlencoded; charset=UTF-8"),
                        timeout_ms: None,
                        unlimited: true,
                    })
                    .await?;
            }
        }
        Ok(())
    }
    // ts: ApexOmniExchange.cancelOrders
    pub async fn cancel_orders(&self, orders: &[Order]) -> Result<(), ExchangeError> {
        self.cancel_orders_helper(orders, "0").await
    }
}
