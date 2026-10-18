//! Stateful account reads/configuration from AsterDEXExchange. Configured REST
//! instances are supplied by the host; API construction and exchange startup
//! remain separate rows. Futures must be polled by the caller.
use crate::{
    exchange_helpers as helpers,
    types::{
        AsterDEXAPIVersion, AsterDEXMarketResponse, AsterDEXOrderResponse, AsterDEXTickerResponse,
    },
};
use safe_cex_exchange::{exchange::BaseExchange, runtime::ExchangeError};
use safe_cex_num::{js_number_string, js_string, js_truthy, parse_float};
use safe_cex_store::StoreUpdate;
use safe_cex_transport::{RestAdapter, RestBody, RestRequest};
use safe_cex_types::{MarginType, Market, Order, Position, PositionSide, Ticker};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{cell::Cell, collections::BTreeMap, rc::Rc};

pub struct AccountFetch {
    pub owner: Rc<BaseExchange>,
    primary: Rc<dyn RestAdapter>,
    public: Rc<dyn RestAdapter>,
    version: AsterDEXAPIVersion,
    pub balance_state_version: Cell<f64>,
    pub last_balance_reconciliation_at: Cell<f64>,
}
fn decode<T: DeserializeOwned>(value: Value) -> Result<T, ExchangeError> {
    serde_json::from_value(value).map_err(|error| {
        let mut error = ExchangeError::new(error.to_string());
        error.name = "RepresentationError".into();
        error
    })
}
fn mapper_error((name, message): (&str, String)) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = name.into();
    error
}
impl AccountFetch {
    pub fn new(
        owner: Rc<BaseExchange>,
        primary: Rc<dyn RestAdapter>,
        public: Rc<dyn RestAdapter>,
        version: AsterDEXAPIVersion,
    ) -> Self {
        Self {
            owner,
            primary,
            public,
            version,
            balance_state_version: Cell::new(0.0),
            last_balance_reconciliation_at: Cell::new(0.0),
        }
    }
    fn path(&self, endpoint: &str) -> String {
        helpers::get_fapi_path(endpoint, &self.version, None)
    }
    async fn request(
        &self,
        adapter: &dyn RestAdapter,
        method: &str,
        url: String,
        body: Option<Value>,
    ) -> Result<Value, ExchangeError> {
        let response = adapter
            .execute(RestRequest {
                method: method.into(),
                url,
                body: body
                    .map(safe_cex_types::retained_json::RetainedJsonInput::from_value)
                    .transpose()
                    .map_err(|error| ExchangeError::new(error.to_string()))?
                    .map(RestBody::Untransformed),
                ..Default::default()
            })
            .await
            .map_err(|failure| {
                // Preserve the available shared REST failure metadata, rather than
                // inventing a JS Error object identity at the native boundary.
                let method = failure.request.method.clone();
                let url = failure.request.url.clone();
                let mut error: ExchangeError = failure.into();
                error.is_axios_error = true;
                error.method = Some(method);
                error.url = Some(url);
                error
            })?;
        response
            .body
            .transform_axios_response()
            .map_err(|error| ExchangeError::new(error.to_string()))?
            .json_value()
            .map_err(|error| ExchangeError::new(error.to_string()))
    }
    fn finish<T>(
        &self,
        result: Result<T, ExchangeError>,
        prefix: &str,
    ) -> Result<T, ExchangeError> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                self.owner
                    .log(&format!("{prefix}: {}", error.message), "error")?;
                Err(error)
            }
        }
    }
    pub(crate) async fn fetch_balance(&self) -> Result<safe_cex_types::Balance, ExchangeError> {
        let result = async {
            let data = self
                .request(
                    self.primary.as_ref(),
                    "GET",
                    helpers::get_account_endpoint(&self.version),
                    None,
                )
                .await?;
            if data.is_null() {
                return Err(mapper_error((
                    "TypeError",
                    "Cannot read properties of null (reading 'totalWalletBalance')".into(),
                )));
            }
            helpers::balance_from_account_response(&data)
                .map_err(|e| mapper_error((e.name, e.message.into())))
        }
        .await;
        self.finish(result, "Failed to fetch balance")
    }
    // ts: AsterDEXExchange.reconcileBalanceAfterPrivateUpdate
    pub async fn reconcile_balance_after_private_update(
        &self,
        version: f64,
    ) -> Result<bool, ExchangeError> {
        let result: Result<bool, ExchangeError> = async {
            let balance = self.fetch_balance().await?;
            if self.owner.is_disposed() || version != self.balance_state_version.get() {
                return Ok(false);
            }
            let mut loaded = self.owner.store.loaded().clone();
            loaded.balance = true;
            self.owner.store.update(
                StoreUpdate {
                    balance: Some(balance),
                    loaded: Some(loaded),
                    ..Default::default()
                },
                false,
            )?;
            self.last_balance_reconciliation_at
                .set(self.owner.runtime.now_ms());
            Ok(true)
        }
        .await;
        match result {
            Ok(applied) => Ok(applied),
            Err(error) => {
                self.owner.log(
                    &format!("Balance reconciliation failed: {}", error.message),
                    "warning",
                )?;
                Ok(true)
            }
        }
    }
    // ts: AsterDEXExchange#syncPositionMode
    pub async fn sync_position_mode(&self) -> Result<(), ExchangeError> {
        let data = self
            .request(
                self.primary.as_ref(),
                "GET",
                self.path("positionSide/dual"),
                None,
            )
            .await?;
        let hedged = helpers::parse_position_mode_response(Some(&data)).map_err(mapper_error)?;
        self.owner.store.set_setting("isHedged", hedged)?;
        self.owner.log(
            &format!(
                "Position mode loaded: {}",
                if hedged { "Hedge" } else { "One-Way" }
            ),
            "info",
        )
    }
    // ts: AsterDEXExchange.fetchPositions
    pub async fn fetch_positions(
        &self,
        symbols: Option<&[String]>,
    ) -> Result<Vec<Position>, ExchangeError> {
        let result = async {
            let data = self
                .request(
                    self.primary.as_ref(),
                    "GET",
                    self.path("positionRisk"),
                    None,
                )
                .await?;
            // The source consumes raw rows, not the full declared wire record.
            let positions = data.as_array().ok_or_else(|| {
                mapper_error(("TypeError", "positions.forEach is not a function".into()))
            })?;
            let mode = helpers::get_account_position_mode(self.owner.store.options().is_hedged);
            let mut leverage_by_symbol = BTreeMap::new();
            for pos in positions {
                if pos.is_null() {
                    return Err(mapper_error((
                        "TypeError",
                        "Cannot read properties of null (reading 'leverage')".into(),
                    )));
                }
                let raw = pos.get("leverage");
                let text = if js_truthy(raw) {
                    js_string(raw).map_err(|e| mapper_error((e.name, e.to_string())))?
                } else {
                    "0".into()
                };
                let leverage = parse_float(&text).abs();
                if leverage.is_finite() && leverage > 0.0 {
                    if let Some(symbol) = pos.get("symbol").and_then(Value::as_str) {
                        leverage_by_symbol.insert(symbol.to_owned(), leverage);
                    }
                }
            }
            let selected = |symbol: &String| symbols.is_none_or(|symbols| symbols.contains(symbol));
            let mut result = Vec::new();
            for pos in positions {
                if symbols.is_some_and(|symbols| {
                    !symbols
                        .iter()
                        .any(|symbol| pos.get("symbol") == Some(&Value::String(symbol.clone())))
                }) {
                    continue;
                }
                let amount = js_string(pos.get("positionAmt"))
                    .map_err(|e| mapper_error((e.name, e.to_string())))?;
                if parse_float(&amount) == 0.0 {
                    continue;
                }
                let position = helpers::transform_position(pos, &self.owner.store.positions())
                    .map_err(mapper_error)?;
                // uniqBy keeps the first real position, before all placeholders.
                if !result.iter().any(|old: &Position| old.id == position.id) {
                    result.push(position);
                }
            }
            for market in self
                .owner
                .store
                .markets()
                .iter()
                .filter(|m| m.active && m.is_spot != Some(true) && selected(&m.symbol))
            {
                for side in [PositionSide::Long, PositionSide::Short] {
                    let id = safe_cex_types::model_utils::create_position_id(
                        &market.symbol,
                        Some(side),
                        Some(mode),
                    );
                    if result.iter().any(|pos| pos.id == id) {
                        continue;
                    }
                    result.push(Position {
                        id,
                        symbol: market.symbol.clone(),
                        side: side.into(),
                        entry_price: 0.0,
                        notional: 0.0,
                        leverage: leverage_by_symbol
                            .get(&market.symbol)
                            .copied()
                            .unwrap_or(0.0),
                        unrealized_pnl: 0.0,
                        contracts: 0.0,
                        liquidation_price: 0.0,
                        margin_type: (Some(MarginType::Cross)).into(),
                        position_mode: (Some(mode)).into(),
                        info: (Some(json!({}))).into(),
                        realized_pnl: (None).into(),
                        margin_size: (None).into(),
                        last_update: (None).into(),
                        extra: (None).into(),
                        option: (None).into(),
                    });
                }
            }
            Ok(result)
        }
        .await;
        self.finish(result, "Failed to fetch positions")
    }
    // ts: AsterDEXExchange.fetchOrders
    pub async fn fetch_orders(
        &self,
        symbols: Option<&[String]>,
    ) -> Result<Vec<Order>, ExchangeError> {
        let result = async {
            let data = self
                .request(self.primary.as_ref(), "GET", self.path("openOrders"), None)
                .await?;
            let items = data
                .as_array()
                .ok_or_else(|| ExchangeError::new("Open orders response is not an array"))?;
            let mut orders = Vec::new();
            for item in items {
                if item.is_null() {
                    return Err(mapper_error((
                        "TypeError",
                        format!(
                            "Cannot read properties of null (reading '{}')",
                            if symbols.is_some() {
                                "symbol"
                            } else {
                                "stopPrice"
                            }
                        ),
                    )));
                }
                if symbols.is_some_and(|symbols| {
                    !item
                        .get("symbol")
                        .and_then(Value::as_str)
                        .is_some_and(|s| symbols.iter().any(|v| v == s))
                }) {
                    continue;
                }
                let order: AsterDEXOrderResponse = decode(item.clone())?;
                orders.push(helpers::transform_order(&order).map_err(mapper_error)?);
            }
            Ok(orders)
        }
        .await;
        self.finish(result, "Failed to fetch orders")
    }
    // ts: AsterDEXExchange.fetchMarkets
    pub async fn fetch_markets(&self) -> Result<Vec<Market>, ExchangeError> {
        let result = async {
            let data = self
                .request(self.public.as_ref(), "GET", self.path("exchangeInfo"), None)
                .await?;
            if data.is_null() {
                return Err(mapper_error((
                    "TypeError",
                    "Cannot read properties of null (reading 'symbols')".into(),
                )));
            }
            let items = data.get("symbols").and_then(Value::as_array);
            if let Some(items) = items.filter(|items| !items.is_empty()) {
                let mut sample = &items[0];
                for item in items {
                    if item.is_null() {
                        return Err(mapper_error((
                            "TypeError",
                            "Cannot read properties of null (reading 'symbol')".into(),
                        )));
                    }
                    if item.get("symbol").and_then(Value::as_str) == Some("BTCUSDT") {
                        sample = item;
                        break;
                    }
                }
                let text = |key: &str| {
                    safe_cex_num::js_string(sample.get(key))
                        .map_err(|e| mapper_error((e.name, e.message.into())))
                };
                self.owner.log(
                    &format!(
                        "Raw market data ({}): pricePrecision={}, quantityPrecision={}",
                        text("symbol")?,
                        text("pricePrecision")?,
                        text("quantityPrecision")?
                    ),
                    "info",
                )?;
            }
            // Brackets are fetched even for an empty/missing symbols list.
            let brackets_response = self
                .request(
                    self.primary.as_ref(),
                    "GET",
                    self.path("leverageBracket"),
                    None,
                )
                .await;
            let (entries, message, severity) = helpers::parse_leverage_brackets_response(
                brackets_response.as_ref().map_err(|e| e.message.as_str()),
            );
            self.owner.log(&message, severity.as_str())?;
            let mut brackets = BTreeMap::new();
            for (key, value) in entries {
                // A non-string Map key cannot match the source market.symbol string.
                let Some(key) = key.as_ref().and_then(Value::as_str) else {
                    continue;
                };
                let value = value
                    .as_ref()
                    .map(safe_cex_exchange::js_runtime::value::from_json)
                    .unwrap_or(safe_cex_exchange::js_runtime::value::Js::Undefined);
                brackets.insert(key.to_owned(), value);
            }
            let items =
                items.ok_or_else(|| ExchangeError::new("Exchange info symbols is not an array"))?;
            let mut markets = Vec::new();
            for item in items {
                if item.is_null() {
                    return Err(mapper_error((
                        "TypeError",
                        "Cannot read properties of null (reading 'status')".into(),
                    )));
                }
                if item.get("status").and_then(Value::as_str) != Some("TRADING") {
                    continue;
                }
                let market: AsterDEXMarketResponse = decode(item.clone())?;
                markets.push(helpers::transform_market(&market, &brackets).map_err(mapper_error)?);
            }
            Ok(markets)
        }
        .await;
        self.finish(result, "Failed to fetch markets")
    }
    // ts: AsterDEXExchange.fetchTickers
    pub async fn fetch_tickers(&self) -> Result<Vec<Ticker>, ExchangeError> {
        let result = async {
            let data = self
                .request(self.public.as_ref(), "GET", self.path("ticker/24hr"), None)
                .await?;
            let tickers: Vec<AsterDEXTickerResponse> = decode(data)?;
            let symbols = tickers
                .iter()
                .map(|ticker| ticker.symbol.clone())
                .collect::<Vec<_>>();
            let mut log_error = None;
            let map = helpers::fetch_open_interest_map(
                self.public.as_ref(),
                &self.path("openInterest"),
                Some(&symbols),
                false,
                |message| {
                    if let Err(error) = self.owner.log(&message, "warning") {
                        log_error = Some(error);
                    }
                },
            )
            .await;
            if let Some(error) = log_error {
                return Err(error);
            }
            let mut open_interest = BTreeMap::new();
            for (key, value) in map {
                if let Some(value) = value.as_f64() {
                    open_interest.insert(key, value);
                }
            }
            tickers
                .iter()
                .map(|ticker| {
                    helpers::transform_ticker(ticker, Some(&open_interest))
                        .map_err(|error| ExchangeError::new(error.to_string()))
                })
                .collect()
        }
        .await;
        self.finish(result, "Failed to fetch tickers")
    }
    // ts: AsterDEXExchange.setLeverage
    pub async fn set_leverage(
        &self,
        symbol: &str,
        leverage: f64,
        _side: Option<PositionSide>,
    ) -> Result<(), ExchangeError> {
        let result = async {
            let (min, max) = {
                let markets = self.owner.store.markets_by_symbol();
                let market = markets.get(symbol).ok_or_else(|| {
                    ExchangeError::new(format!("Market {symbol} not found, cannot set leverage"))
                })?;
                let min = market.limits.leverage.min;
                (
                    if min == 0.0 || min.is_nan() { 1.0 } else { min },
                    market.limits.leverage.max,
                )
            };
            // JS Math.round ties towards +infinity, including negative halves.
            let normalized = if leverage == 0.0 {
                leverage
            } else if (-0.5..0.0).contains(&leverage) {
                -0.0
            } else if leverage.is_finite() {
                let floor = leverage.floor();
                if leverage - floor >= 0.5 {
                    floor + 1.0
                } else {
                    floor
                }
            } else {
                f64::NAN
            };
            if !normalized.is_finite() {
                return Err(ExchangeError::new(format!(
                    "Invalid leverage value for {symbol}: {}",
                    js_number_string(leverage)
                )));
            }
            let clamped = safe_cex_num::math_min(safe_cex_num::math_max(normalized, min), max);
            // serde_json::Value has no non-finite number: refuse rather than
            // replacing an untransformed native request value with JSON null.
            if !clamped.is_finite() {
                return Err(mapper_error((
                    "RepresentationError",
                    "Non-finite clamped leverage cannot be held by RestBody::Untransformed".into(),
                )));
            }
            self.request(
                self.primary.as_ref(),
                "POST",
                self.path("leverage"),
                Some(json!({"symbol":symbol,"leverage":clamped})),
            )
            .await?;
            self.owner.log(
                &format!(
                    "Leverage set to {}x for {symbol}",
                    js_number_string(clamped)
                ),
                "info",
            )
        }
        .await;
        self.finish(result, "Failed to set leverage")
    }
    // ts: AsterDEXExchange.changePositionMode
    pub async fn change_position_mode(
        &self,
        hedged: bool,
        _symbol: Option<&str>,
    ) -> Result<(), ExchangeError> {
        let result = async {
            self.request(
                self.primary.as_ref(),
                "POST",
                self.path("positionSide/dual"),
                Some(json!({"dualSidePosition": if hedged {"true"} else {"false"}})),
            )
            .await?;
            self.owner.store.set_setting("isHedged", hedged)?;
            self.owner.log(
                &format!(
                    "Position mode changed to {}",
                    if hedged { "Hedge" } else { "One-Way" }
                ),
                "info",
            )?;
            let refresh = async {
                let positions = self.fetch_positions(None).await?;
                self.owner.store.update(
                    StoreUpdate {
                        positions: Some(positions),
                        ..Default::default()
                    },
                    false,
                )?;
                Ok::<_, ExchangeError>(())
            }
            .await;
            if let Err(error) = refresh {
                self.owner.log(
                    &format!(
                        "Failed to refresh positions after position mode change: {}",
                        error.message
                    ),
                    "warning",
                )?;
            }
            Ok(())
        }
        .await;
        self.finish(result, "Failed to change position mode")
    }
}
