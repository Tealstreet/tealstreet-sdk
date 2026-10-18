//! BingX reads over the configured HTTP boundary and the shared exchange Store.
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput,
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::numeric_ingress::{
    JsNumberInput, JsNumberOutput, js_deserialize_passthrough, js_value_as_f64,
};
use safe_cex_num::{JsPrimitive, js_string, js_truthy, parse_float};
use safe_cex_store::StoreUpdate;
use safe_cex_transport::RestRequest;
use safe_cex_types::{
    Balance, FetchedLeverage, MarginType, Market, MarketLimits, MarketLimitsAmount,
    MarketLimitsLeverage, MarketPrecision, Order, OrderStatus, Position, PositionMode,
    PositionSide, Ticker,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

/// Configured HTTP capability; signing and double parsing remain upstream.
pub trait ReadTransport {
    fn send(&self, request: RestRequest) -> ExchangeFuture<'_, JsNumberInput>;
    /// The source stamps the request config; a client whose retry policy approves
    /// signature rejections must share this one stamp with the response resend.
    fn send_with_signature_retry(
        &self,
        request: RestRequest,
        retry: &safe_cex_transport::signature_rejection::SignatureRetry,
    ) -> ExchangeFuture<'_, JsNumberInput> {
        let _ = retry;
        self.send(request)
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct RawTickers {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dailys: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prices: Option<Value>,
}
#[derive(Debug, Serialize)]
pub struct Validation {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<Balance>,
}
#[derive(Serialize)]
#[serde(transparent)]
struct Response {
    value: Value,
}
impl JsNumberOutput for Response {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {} passthrough {value});
    }
}
pub struct BingxDataReads {
    pub base: Rc<BaseExchange>,
    transport: Rc<dyn ReadTransport>,
    account_position_mode: Cell<Option<PositionMode>>,
}
fn type_error(message: impl Into<String>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}
fn field<'a>(value: Option<&'a Value>, key: &str) -> Result<Option<&'a Value>, ExchangeError> {
    match value {
        None => Err(type_error(format!(
            "Cannot read properties of undefined (reading '{key}')"
        ))),
        Some(Value::Null) => Err(type_error(format!(
            "Cannot read properties of null (reading '{key}')"
        ))),
        Some(value) => Ok(value.get(key)),
    }
}
fn text(value: Option<&Value>) -> Result<String, ExchangeError> {
    js_string(value).map_err(|error| type_error(error.message))
}
fn float(value: Option<&Value>) -> Result<f64, ExchangeError> {
    Ok(parse_float(&text(value)?))
}
fn alias<'a>(value: &'a Value, key: &str) -> Result<Option<&'a Value>, ExchangeError> {
    use safe_cex_num::get_key::GetKeyUnknownValue;
    match safe_cex_num::get_key::v_unknown(Some(value), key)
        .map_err(|error| type_error(error.to_string()))?
    {
        GetKeyUnknownValue::Json(value) => Ok(Some(value)),
        GetKeyUnknownValue::Undefined => Ok(None),
        _ => Err(type_error("typed read field refuses inherited callable")),
    }
}
fn strict_string(value: Option<&Value>, field: &str) -> Result<String, ExchangeError> {
    value
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| type_error(format!("typed {field} requires a string")))
}
fn strict_number(value: Option<&Value>, field: &str) -> Result<f64, ExchangeError> {
    value
        .and_then(js_value_as_f64)
        .ok_or_else(|| type_error(format!("typed {field} requires a number")))
}
fn error_message(error: &ExchangeError) -> Result<String, ExchangeError> {
    let msg = error
        .response
        .as_ref()
        .and_then(|r| r.get("data"))
        .and_then(|d| d.get("msg"));
    if js_truthy(msg) {
        text(msg)
    } else {
        Ok(error.message.clone())
    }
}
impl BingxDataReads {
    pub fn new(base: Rc<BaseExchange>, transport: Rc<dyn ReadTransport>) -> Self {
        Self {
            base,
            transport,
            account_position_mode: Cell::new(None),
        }
    }
    async fn configured_request(&self, request: RestRequest) -> Result<Value, ExchangeError> {
        let response = self.transport.send(request).await;
        let mut yielded = false;
        std::future::poll_fn(|cx| {
            if yielded {
                std::task::Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
        })
        .await;
        response?
            .decode_with(|decoder| {
                js_deserialize_passthrough(decoder).map(|value| Response { value })
            })
            .map(|response| response.value)
            .map_err(|error| ExchangeError::new(error.to_string()))
    }
    async fn request(&self, endpoint: &str, params: Option<Value>) -> Result<Value, ExchangeError> {
        self.configured_request(RestRequest {
            method: "GET".into(),
            url: endpoint.into(),
            params,
            ..Default::default()
        })
        .await
    }
    async fn post(&self, endpoint: &str, body: Value) -> Result<Value, ExchangeError> {
        self.configured_request(RestRequest {
            method: "POST".into(),
            url: endpoint.into(),
            body: Some(safe_cex_transport::RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(body)
                    .map_err(|error| ExchangeError::new(error.to_string()))?,
            )),
            ..Default::default()
        })
        .await
    }
    fn check_code(response: &Value) -> Result<(), ExchangeError> {
        if field(Some(response), "code")?.and_then(js_value_as_f64) == Some(0.0) {
            Ok(())
        } else {
            Err(ExchangeError::new(text(response.get("msg"))?))
        }
    }
    fn report(&self, message: String, label: Option<&str>) -> Result<(), ExchangeError> {
        self.base
            .emit_and_log_error(ExchangeErrorInput::Message(message), label, None)
    }
    async fn primary_balance(&self) -> Result<Value, ExchangeError> {
        crate::account_reads::fetch_primary_balance_data(|request| self.transport.send(request))
            .await
    }
    pub async fn validate_account(&self) -> Result<Validation, ExchangeError> {
        let result = async {
            let data = self.primary_balance().await?;
            let total = float(data.get("balance"))?;
            let balance = if total.is_finite() {
                let or_zero = |key| -> Result<f64, ExchangeError> {
                    let value = float(data.get(key))?;
                    Ok(if value == 0.0 || value.is_nan() {
                        0.0
                    } else {
                        value
                    })
                };
                Some(Balance {
                    total,
                    free: or_zero("availableMargin")?,
                    used: or_zero("usedMargin")?,
                    upnl: or_zero("unrealizedProfit")?,
                    info: None,
                    currencies: None,
                    coins: None,
                })
            } else {
                None
            };
            Ok::<_, ExchangeError>(Validation {
                error: String::new(),
                balance,
            })
        }
        .await;
        match result {
            Ok(validation) => Ok(validation),
            Err(error) => {
                let message = error_message(&error)?;
                self.report(message.clone(), None)?;
                Ok(Validation {
                    error: if message.is_empty() {
                        "Invalid API key or secret".into()
                    } else {
                        message
                    },
                    balance: None,
                })
            }
        }
    }
    pub async fn fetch_balance(&self) -> Result<std::sync::Arc<Balance>, ExchangeError> {
        let result = async {
            let data = self.primary_balance().await?;
            Ok::<_, ExchangeError>(Balance {
                total: float(data.get("balance"))?,
                free: float(data.get("availableMargin"))?,
                used: float(data.get("usedMargin"))?,
                upnl: float(data.get("unrealizedProfit"))?,
                info: Some(data.into()),
                currencies: None,
                coins: None,
            })
        }
        .await;
        match result {
            Ok(balance) => Ok(std::sync::Arc::new(balance)),
            Err(error) => {
                self.report(error_message(&error)?, None)?;
                Ok(self.base.store.balance_handle())
            }
        }
    }
    pub async fn fetch_markets(&self) -> Result<Vec<Market>, ExchangeError> {
        let result = async {
            let response = self.request(crate::constants::ENDPOINTS[1].1, None).await?;
            let rows = field(Some(&response), "data")?
                .and_then(Value::as_array)
                .ok_or_else(|| type_error("symbols.map is not a function"))?;
            let prepared = rows
                .iter()
                .map(|row| {
                    let base = alias(row, "asset")?.cloned();
                    let quote = alias(row, "currency")?.cloned();
                    let id = match field(Some(row), "symbol")? {
                        Some(Value::String(id)) => id.clone(),
                        None => {
                            return Err(type_error(
                                "Cannot read properties of undefined (reading 'replace')",
                            ));
                        }
                        Some(Value::Null) => {
                            return Err(type_error(
                                "Cannot read properties of null (reading 'replace')",
                            ));
                        }
                        _ => return Err(type_error("m.symbol.replace is not a function")),
                    };
                    let precision = |key| -> Result<f64, ExchangeError> {
                        let value = float(alias(row, key)?)?;
                        Ok(parse_float(
                            &crate::precision::parse_precision(&JsPrimitive::Number(value))
                                .expect("number precision"),
                        ))
                    };
                    let amount = precision("quantityPrecision")?;
                    let price = precision("pricePrecision")?;
                    let min = float(alias(row, "tradeMinLimit")?)?;
                    let leverage = alias(row, "maxLongLeverage")?.cloned();
                    Ok((
                        Market {
                            symbol: id.replace('-', ""),
                            id,
                            base: String::new(),
                            quote: String::new(),
                            active: row.get("status").and_then(js_value_as_f64) == Some(1.0),
                            precision: MarketPrecision { amount, price },
                            limits: MarketLimits {
                                amount: MarketLimitsAmount {
                                    min,
                                    max: f64::INFINITY,
                                },
                                leverage: MarketLimitsLeverage {
                                    min: 1.0,
                                    max: 100.0,
                                },
                                price: None,
                            },
                            pretty_symbol: None,
                            is_read_only: None,
                            contract_size: None,
                            is_expiring: None,
                            is_inverse: None,
                            is_spot: None,
                            is_perpetual: None,
                            info: None,
                            taker_fee: None,
                            maker_fee: None,
                            option: None,
                        },
                        base,
                        quote,
                        leverage,
                    ))
                })
                .collect::<Result<Vec<_>, ExchangeError>>()?;
            let mut markets = Vec::new();
            let mut refusals = Vec::new();
            for (index, (mut market, base, quote, leverage)) in prepared.into_iter().enumerate() {
                let invalid = [("base", base.as_ref()), ("quote", quote.as_ref())]
                    .into_iter()
                    .filter(|(_, value)| value.and_then(Value::as_str).is_none())
                    .map(|(field, value)| {
                        format!(
                            "{field}={}",
                            value.map_or_else(|| "undefined".into(), Value::to_string)
                        )
                    })
                    .collect::<Vec<_>>();
                if !invalid.is_empty() {
                    refusals.push(format!(
                        "Refused BingX market row {index} symbol={}: {}: expected string",
                        market.id,
                        invalid.join(", ")
                    ));
                    continue;
                }
                market.base = strict_string(base.as_ref(), "base")?;
                market.quote = strict_string(quote.as_ref(), "quote")?;
                if js_truthy(leverage.as_ref()) {
                    market.limits.leverage.max = match strict_number(leverage.as_ref(), "max leverage") {
                        Ok(value) => value,
                        Err(cause) => {
                            let value = leverage.as_ref().map_or_else(|| "undefined".into(), Value::to_string);
                            refusals.push(format!("Refused BingX market row {index} symbol={}: limits.leverage.max={value}: {}", market.id, cause.message));
                            continue;
                        }
                    };
                }
                markets.push(market);
            }
            Ok::<_, ExchangeError>((markets, refusals))
        }
        .await;
        match result {
            Ok((markets, refusals)) => {
                for message in refusals {
                    self.report(message, None)?;
                }
                Ok(markets)
            }
            Err(error) => {
                self.report(error_message(&error)?, None)?;
                Ok(self.base.store.markets().to_vec())
            }
        }
    }
    pub async fn fetch_tickers_raw(&self) -> Result<RawTickers, ExchangeError> {
        let result = async {
            let daily = self.request(crate::constants::ENDPOINTS[4].1, None).await?;
            let dailys = field(Some(&daily), "data")?.cloned();
            let price = self.request(crate::constants::ENDPOINTS[5].1, None).await?;
            Ok::<_, ExchangeError>(RawTickers {
                dailys,
                prices: field(Some(&price), "data")?.cloned(),
            })
        }
        .await;
        match result {
            Ok(raw) => Ok(raw),
            Err(error) => {
                self.report(error_message(&error)?, None)?;
                Ok(RawTickers {
                    dailys: Some(json!([])),
                    prices: Some(json!([])),
                })
            }
        }
    }
    pub fn format_tickers(&self, raw: &RawTickers) -> Result<Vec<Ticker>, ExchangeError> {
        let rows = raw
            .dailys
            .as_ref()
            .and_then(Value::as_array)
            .ok_or_else(|| type_error("dailys.reduce is not a function"))?;
        let mut tickers = Vec::new();
        for daily in rows {
            let symbol = field(Some(daily), "symbol")?;
            let key = text(symbol)?;
            let market = self.base.store.markets_by_id().get(&key).cloned();
            let prices = raw
                .prices
                .as_ref()
                .and_then(Value::as_array)
                .ok_or_else(|| type_error("prices.find is not a function"))?;
            let mut price = None;
            for candidate in prices {
                if field(Some(candidate), "symbol")? == symbol {
                    price = Some(candidate);
                    break;
                }
            }
            let (Some(market), Some(price)) = (market, price) else {
                continue;
            };
            let last = float(alias(daily, "lastPrice")?)?;
            let (bid, ask) = crate::ticker::derive_ticker_bid_ask(Some(&market.precision), last);
            tickers.push(Ticker {
                id: market.id,
                symbol: market.symbol,
                bid,
                ask,
                last,
                mark: float(alias(price, "markPrice")?)?,
                index: float(alias(price, "indexPrice")?)?,
                percentage: float(alias(daily, "priceChangePercent")?)?,
                funding_rate: float(alias(price, "lastFundingRate")?)? / 100.0,
                funding_time: float(alias(price, "nextFundingTime")?)?,
                volume: float(daily.get("volume"))?,
                quote_volume: float(alias(daily, "quoteVolume")?)?,
                open_interest: 0.0,
                mid_price: None,
                timestamp: None,
                info: None,
            });
        }
        Ok(tickers)
    }
    pub async fn fetch_tickers(&self) -> Result<Vec<Ticker>, ExchangeError> {
        let raw = self.fetch_tickers_raw().await?;
        if self.base.is_disposed() {
            return Ok(Vec::new());
        }
        let tickers = self.format_tickers(&raw)?;
        self.base
            .store
            .update(
                StoreUpdate {
                    tickers: Some(tickers.clone()),
                    ..Default::default()
                },
                false,
            )
            .map_err(ExchangeError::from)?;
        Ok(tickers)
    }
    fn venue_symbol(&self, symbol: &str) -> String {
        let by_symbol = self
            .base
            .store
            .markets_by_symbol()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>();
        let by_id = self
            .base
            .store
            .markets_by_id()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>();
        crate::market_utils::to_bingx_symbol(symbol, &by_symbol, &by_id)
    }
    pub async fn fetch_leverage_for_symbol(
        &self,
        symbol: &str,
    ) -> Result<Option<FetchedLeverage>, ExchangeError> {
        let result = async {
            let response = self
                .request(
                    crate::constants::ENDPOINTS[3].1,
                    Some(json!({"symbol": self.venue_symbol(symbol)})),
                )
                .await?;
            if field(Some(&response), "code")?.and_then(js_value_as_f64) != Some(0.0) {
                return Err(ExchangeError::new(text(response.get("msg"))?));
            }
            let data = field(Some(&response), "data")?;
            let leverage = FetchedLeverage {
                buy_leverage: strict_number(field(data, "longLeverage")?, "buy leverage")?,
                sell_leverage: strict_number(field(data, "shortLeverage")?, "sell leverage")?,
                margin_type: None,
                position_mode: None,
                is_fetching: None,
            };
            let mut values = self.base.store.fetched_leverage().clone();
            values.insert(symbol.to_owned(), leverage.clone());
            self.base
                .store
                .update(
                    StoreUpdate {
                        fetched_leverage: Some(values),
                        ..Default::default()
                    },
                    false,
                )
                .map_err(ExchangeError::from)?;
            Ok::<_, ExchangeError>(leverage)
        }
        .await;
        match result {
            Ok(leverage) => Ok(Some(leverage)),
            Err(error) => {
                self.report(error_message(&error)?, Some("Failed to fetch leverage"))?;
                Ok(None)
            }
        }
    }
    pub async fn fetch_margin_mode_for_symbol(
        &self,
        symbol: &str,
    ) -> Result<MarginType, ExchangeError> {
        let result = async {
            let response = self
                .request(
                    crate::constants::ENDPOINTS[7].1,
                    Some(json!({"symbol": self.venue_symbol(symbol)})),
                )
                .await?;
            if field(Some(&response), "code")?.and_then(js_value_as_f64) != Some(0.0) {
                return Err(ExchangeError::new(text(response.get("msg"))?));
            }
            Ok(
                if field(field(Some(&response), "data")?, "marginType")?.and_then(Value::as_str)
                    == Some("ISOLATED")
                {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                },
            )
        }
        .await;
        match result {
            Ok(mode) => Ok(mode),
            Err(error) => {
                self.report(error_message(&error)?, None)?;
                Err(error)
            }
        }
    }
    pub async fn get_or_fetch_position_mode(
        &self,
        refetch: bool,
    ) -> Result<PositionMode, ExchangeError> {
        if let Some(mode) = self.account_position_mode.get()
            && !refetch
        {
            return Ok(mode);
        }
        let response = self.request(crate::constants::ENDPOINTS[6].1, None).await?;
        if field(Some(&response), "code")?.and_then(js_value_as_f64) != Some(0.0) {
            let message = text(response.get("msg"))?;
            self.report(message.clone(), None)?;
            return Err(ExchangeError::new(message));
        }
        let mode = if field(field(Some(&response), "data")?, "dualSidePosition")?
            .and_then(Value::as_str)
            == Some("true")
        {
            PositionMode::Hedged
        } else {
            PositionMode::OneWay
        };
        self.account_position_mode.set(Some(mode));
        Ok(mode)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionMetadata {
    pub symbol: String,
    pub side: PositionSide,
    pub leverage: f64,
    pub position_mode: PositionMode,
    pub margin_type: MarginType,
}
fn empty_position(metadata: PositionMetadata) -> Position {
    Position {
        id: safe_cex_types::model_utils::create_position_id(
            &metadata.symbol,
            Some(metadata.side),
            Some(metadata.position_mode),
        ),
        symbol: metadata.symbol,
        side: (metadata.side).into(),
        leverage: metadata.leverage,
        margin_type: (Some(metadata.margin_type)).into(),
        position_mode: (Some(metadata.position_mode)).into(),
        entry_price: 0.0,
        notional: 0.0,
        unrealized_pnl: 0.0,
        contracts: 0.0,
        liquidation_price: 0.0,
        margin_size: (None).into(),
        info: (None).into(),
        realized_pnl: (None).into(),
        extra: (None).into(),
        last_update: (None).into(),
        option: (None).into(),
    }
}
impl BingxDataReads {
    pub async fn fetch_open_positions(&self) -> Result<Vec<Position>, ExchangeError> {
        let result = async {
            let response = self.request(crate::constants::ENDPOINTS[2].1, None).await?;
            if field(Some(&response), "code")?.and_then(js_value_as_f64) != Some(0.0) {
                self.report(text(response.get("msg"))?, None)?;
                return Ok(self.base.store.positions().to_vec());
            }
            let rows = field(Some(&response), "data")?
                .and_then(Value::as_array)
                .ok_or_else(|| type_error("data.data.filter is not a function"))?;
            let mut supported = Vec::new();
            for row in rows {
                // Array.some never evaluates the symbol expression when markets is empty.
                for market in self.base.store.markets().iter() {
                    let raw = strict_string(field(Some(row), "symbol")?, "position symbol")?;
                    if market.symbol == raw.replacen('-', "", 1) {
                        supported.push(row);
                        break;
                    }
                }
            }
            let mode = self.get_or_fetch_position_mode(false).await?;
            let mut positions = Vec::new();
            for row in supported {
                let entry_price = float(alias(row, "avgPrice")?)?;
                let contracts = float(alias(row, "positionAmt")?)?;
                let upnl = float(alias(row, "unrealizedProfit")?)?;
                let raw_side = text(alias(row, "positionSide")?)?;
                let margin_type = if js_truthy(alias(row, "isolated")?) {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                };
                let side_hit =
                    safe_cex_num::js_table_lookup(&crate::constants::POSITION_SIDE, &raw_side);
                let inherited_side = matches!(side_hit, safe_cex_num::TableHit::Inherited(_));
                let side = match side_hit {
                    safe_cex_num::TableHit::Mapped(side) => *side,
                    safe_cex_num::TableHit::Missing => {
                        if contracts > 0.0 {
                            PositionSide::Long
                        } else {
                            PositionSide::Short
                        }
                    }
                    safe_cex_num::TableHit::Inherited(_) => PositionSide::Short,
                };
                let symbol =
                    strict_string(row.get("symbol"), "position symbol")?.replacen('-', "", 1);
                let notional = contracts.abs() * entry_price + upnl;
                let leverage = match row.get("leverage").and_then(js_value_as_f64) {
                    Some(number) => number,
                    None => float(row.get("leverage"))?,
                };
                let isolated = alias(row, "isolatedMargin")?;
                let initial = alias(row, "initialMargin")?;
                let isolated_margin = if js_truthy(isolated) {
                    float(isolated)?
                } else if js_truthy(initial) {
                    float(initial)?
                } else {
                    0.0
                };
                let margin_size = if isolated_margin > 0.0 {
                    Some(isolated_margin)
                } else if leverage > 0.0 {
                    Some(notional / leverage)
                } else {
                    None
                };
                positions.push((
                    Position {
                        id: safe_cex_types::model_utils::create_position_id(
                            &symbol,
                            Some(side),
                            Some(mode),
                        ),
                        symbol,
                        side: side.into(),
                        entry_price,
                        notional,
                        leverage,
                        margin_type: (Some(margin_type)).into(),
                        position_mode: (Some(mode)).into(),
                        unrealized_pnl: upnl,
                        contracts: contracts.abs(),
                        liquidation_price: float(alias(row, "liquidationPrice")?)?,
                        margin_size: margin_size.into(),
                        info: (Some(row.clone().into())).into(),
                        realized_pnl: (None).into(),
                        extra: (None).into(),
                        last_update: (None).into(),
                        option: (None).into(),
                    },
                    inherited_side,
                ));
            }
            if positions.iter().any(|(_, inherited)| *inherited) {
                return Err(type_error("typed PositionSide refuses inherited callable"));
            }
            Ok::<_, ExchangeError>(
                positions
                    .into_iter()
                    .map(|(position, _)| position)
                    .collect(),
            )
        }
        .await;
        match result {
            Ok(positions) => Ok(positions),
            Err(error) => {
                self.report(error_message(&error)?, None)?;
                Ok(self.base.store.positions().to_vec())
            }
        }
    }
    async fn metadata_pair(
        self: &Rc<Self>,
        symbol: &str,
    ) -> Result<(Option<FetchedLeverage>, MarginType), ExchangeError> {
        use std::task::Poll;
        let left = self.clone();
        let left_symbol = symbol.to_owned();
        let right = self.clone();
        let right_symbol = symbol.to_owned();
        let mut leverage: Option<ExchangeFuture<'static, Option<FetchedLeverage>>> =
            Some(Box::pin(async move {
                left.fetch_leverage_for_symbol(&left_symbol).await
            }));
        let mut margin: Option<ExchangeFuture<'static, MarginType>> = Some(Box::pin(async move {
            right.fetch_margin_mode_for_symbol(&right_symbol).await
        }));
        let mut left_value = None;
        let mut right_value = None;
        let result = std::future::poll_fn(|cx| {
            let mut failure = None;
            if let Some(future) = leverage.as_mut()
                && let Poll::Ready(result) = future.as_mut().poll(cx)
            {
                leverage = None;
                match result {
                    Ok(value) => left_value = Some(value),
                    Err(error) => failure = Some(error),
                }
            }
            if let Some(future) = margin.as_mut()
                && let Poll::Ready(result) = future.as_mut().poll(cx)
            {
                margin = None;
                match result {
                    Ok(value) => right_value = Some(value),
                    Err(error) => {
                        if failure.is_none() {
                            failure = Some(error);
                        }
                    }
                }
            }
            if let Some(error) = failure {
                return Poll::Ready(Err(error));
            }
            if left_value.is_some() && right_value.is_some() {
                Poll::Ready(Ok((
                    left_value.take().unwrap(),
                    right_value.take().unwrap(),
                )))
            } else {
                Poll::Pending
            }
        })
        .await;
        if result.is_err() {
            if let Some(future) = leverage {
                let base = self.base.clone();
                self.base.start_owned_operation(async move {
                    if let Err(error) = future.await {
                        base.runtime.report_exception(
                            &error,
                            &json!({"operation":"BingX metadata leverage sibling"}),
                        );
                    }
                });
            }
            if let Some(future) = margin {
                let base = self.base.clone();
                self.base.start_owned_operation(async move {
                    if let Err(error) = future.await {
                        base.runtime.report_exception(
                            &error,
                            &json!({"operation":"BingX metadata margin sibling"}),
                        );
                    }
                });
            }
        }
        result
    }
    pub async fn fetch_position_metadata(
        self: &Rc<Self>,
        symbols: &[String],
    ) -> Result<Vec<PositionMetadata>, ExchangeError> {
        let result = async {
            let mode = self.get_or_fetch_position_mode(false).await?;
            let mut metadata = Vec::new();
            for symbol in symbols {
                if self.base.is_disposed() {
                    continue;
                }
                let (leverage, margin) = self.metadata_pair(symbol).await?;
                if self.base.is_disposed() {
                    continue;
                }
                let Some(leverage) = leverage else {
                    continue;
                };
                metadata.push(PositionMetadata {
                    symbol: symbol.clone(),
                    side: PositionSide::Long,
                    leverage: leverage.buy_leverage,
                    position_mode: mode,
                    margin_type: margin,
                });
                if mode != PositionMode::OneWay {
                    metadata.push(PositionMetadata {
                        symbol: symbol.clone(),
                        side: PositionSide::Short,
                        leverage: leverage.sell_leverage,
                        position_mode: mode,
                        margin_type: margin,
                    });
                }
            }
            Ok::<_, ExchangeError>(metadata)
        }
        .await;
        match result {
            Ok(metadata) => Ok(metadata),
            Err(error) => {
                self.report(
                    error_message(&error)?,
                    Some("Failed to fetch order history"),
                )?;
                Ok(Vec::new())
            }
        }
    }
    pub async fn fetch_positions(
        self: &Rc<Self>,
        symbols: Option<&[String]>,
    ) -> Result<Vec<Position>, ExchangeError> {
        let open = self.fetch_open_positions().await?;
        let Some(symbols) = symbols else {
            return Ok(open);
        };
        let mut selected = open
            .into_iter()
            .filter(|position| symbols.contains(&position.symbol))
            .collect::<Vec<_>>();
        let missing = symbols
            .iter()
            .filter(|symbol| !selected.iter().any(|position| &position.symbol == *symbol))
            .cloned()
            .collect::<Vec<_>>();
        let metadata = self.fetch_position_metadata(&missing).await?;
        selected.extend(metadata.into_iter().map(empty_position));
        Ok(selected)
    }
    pub async fn change_position_mode(self: &Rc<Self>, hedged: bool) -> Result<(), ExchangeError> {
        let result = async {
            let response = self
                .post(
                    crate::constants::ENDPOINTS[6].1,
                    json!({"dualSidePosition": if hedged {"true"} else {"false"}}),
                )
                .await?;
            Self::check_code(&response)?;
            self.get_or_fetch_position_mode(true).await?;
            let positions = self.fetch_positions(None).await?;
            self.base
                .store
                .update(
                    StoreUpdate {
                        positions: Some(positions),
                        ..Default::default()
                    },
                    false,
                )
                .map_err(ExchangeError::from)
        }
        .await;
        if let Err(error) = &result {
            self.report(error_message(error)?, None)?;
        }
        result
    }
    pub async fn change_margin_type(
        self: &Rc<Self>,
        isolated: bool,
        symbol: Option<&str>,
    ) -> Result<(), ExchangeError> {
        let Some(symbol) = symbol.filter(|symbol| !symbol.is_empty()) else {
            self.report("Symbol is required".into(), None)?;
            return Err(ExchangeError::new("Symbol is required"));
        };
        let result = async {
            let response = self.post(crate::constants::ENDPOINTS[7].1, json!({"symbol":self.venue_symbol(symbol), "marginType":if isolated {"ISOLATED"} else {"CROSSED"}})).await?;
            Self::check_code(&response)?;
            self.get_or_fetch_position_mode(true).await?;
            let positions = self.fetch_positions(Some(&[symbol.to_owned()])).await?;
            self.base.store.add_or_update_positions(positions).map_err(ExchangeError::from)
        }.await;
        if let Err(error) = &result {
            self.report(error_message(error)?, None)?;
        }
        result
    }
    pub async fn set_leverage(
        self: &Rc<Self>,
        symbol: &str,
        input: f64,
        side: Option<PositionSide>,
    ) -> Result<(), ExchangeError> {
        let result = async {
            let market = self
                .base
                .store
                .markets_by_symbol()
                .get(symbol)
                .cloned()
                .ok_or_else(|| ExchangeError::new(format!("Market {symbol} not found")))?;
            let mode = self.get_or_fetch_position_mode(false).await?;
            let position_side = if mode == PositionMode::Hedged {
                match side {
                    None => {
                        return Err(ExchangeError::new(
                            "Side is required when position mode is hedged",
                        ));
                    }
                    Some(PositionSide::Long) => "LONG",
                    Some(PositionSide::Short) => "SHORT",
                }
            } else {
                "BOTH"
            };
            let leverage = safe_cex_num::math_min(
                safe_cex_num::math_max(input, market.limits.leverage.min),
                market.limits.leverage.max,
            );
            let mut body = json!({"symbol":self.venue_symbol(symbol),"side":position_side});
            body["leverage"] = safe_cex_types::retained_json::RetainedJsonInput::Float(leverage)
                .into_value()
                .map_err(|_| type_error("typed RestBody refuses NaN leverage"))?;
            let response = self.post(crate::constants::ENDPOINTS[8].1, body).await?;
            Self::check_code(&response)?;
            let positions = self.fetch_positions(Some(&[symbol.to_owned()])).await?;
            self.base
                .store
                .add_or_update_positions(positions)
                .map_err(ExchangeError::from)
        }
        .await;
        if let Err(error) = &result {
            self.report(error_message(error)?, None)?;
        }
        result
    }
    pub async fn cancel_orders(&self, orders: &[Order]) -> Result<(), ExchangeError> {
        let result = async {
            let mut groups: Vec<(String, Vec<String>)> = Vec::new();
            for order in orders {
                if let Some((_, ids)) = groups
                    .iter_mut()
                    .find(|(symbol, _)| symbol == &order.symbol)
                {
                    ids.push(order.id.clone());
                } else {
                    groups.push((order.symbol.clone(), vec![order.id.clone()]));
                }
            }
            groups.sort_by(|a, b| {
                match (
                    safe_cex_num::js_array_index(&a.0),
                    safe_cex_num::js_array_index(&b.0),
                ) {
                    (Some(a), Some(b)) => a.cmp(&b),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            });
            let requests = groups
                .into_iter()
                .map(|(symbol, ids)| (self.venue_symbol(&symbol), ids))
                .collect::<Vec<_>>();
            for (symbol, ids) in requests {
                let mut canceled = Vec::new();
                for id in ids {
                    let result = async {
                        let response = self
                            .configured_request(RestRequest {
                                method: "DELETE".into(),
                                url: crate::constants::ENDPOINTS[11].1.into(),
                                params: Some(json!({"symbol":symbol,"orderId":id})),
                                ..Default::default()
                            })
                            .await?;
                        let code = field(Some(&response), "code")?.and_then(js_value_as_f64);
                        if code == Some(0.0) || code == Some(80016.0) {
                            Ok::<_, ExchangeError>(true)
                        } else {
                            self.report(
                                format!(
                                    "Failed to cancel order {id}: {}",
                                    text(response.get("msg"))?
                                ),
                                None,
                            )?;
                            Ok(false)
                        }
                    }
                    .await;
                    match result {
                        Ok(true) => canceled.push(id),
                        Ok(false) => (),
                        Err(error) => self.report(
                            format!("Failed to cancel order {id}: {}", error_message(&error)?),
                            None,
                        )?,
                    }
                }
                if !canceled.is_empty() {
                    self.base
                        .store
                        .update_orders_status(
                            &canceled.iter().map(String::as_str).collect::<Vec<_>>(),
                            OrderStatus::Canceled,
                        )
                        .map_err(ExchangeError::from)?;
                }
            }
            Ok::<_, ExchangeError>(())
        }
        .await;
        if let Err(error) = &result {
            self.report(error_message(error)?, None)?;
        }
        result
    }
    pub async fn cancel_symbol_orders(&self, symbol: &str) -> Result<(), ExchangeError> {
        let result = async {
            self.configured_request(RestRequest {
                method: "DELETE".into(),
                url: crate::constants::ENDPOINTS[10].1.into(),
                params: Some(json!({"symbol":self.venue_symbol(symbol)})),
                ..Default::default()
            })
            .await?;
            let ids = self
                .base
                .store
                .orders()
                .iter()
                .filter(|order| order.symbol == symbol)
                .map(|order| order.id.clone())
                .collect::<Vec<_>>();
            self.base
                .store
                .update_orders_status(
                    &ids.iter().map(String::as_str).collect::<Vec<_>>(),
                    OrderStatus::Canceled,
                )
                .map_err(ExchangeError::from)
        }
        .await;
        if let Err(error) = &result {
            self.report(error_message(error)?, None)?;
        }
        result
    }
}
