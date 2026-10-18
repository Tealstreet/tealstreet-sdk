//! Inert Starbase private-state contracts; no venue transport or activation.
use crate::starbase::CoinbaseStarbaseProtocolError;
use serde::Serialize;
use serde_json::Value;

// ts: CoinbaseStarbaseMarginModel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseMarginModel {
    #[serde(rename = "cross_pm")]
    CrossPm,
    #[serde(rename = "cross_sm")]
    CrossSm,
    #[serde(rename = "segregated_pm")]
    SegregatedPm,
    #[serde(rename = "segregated_sm")]
    SegregatedSm,
}

// ts: CoinbaseStarbaseOrderDirection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseOrderDirection {
    #[serde(rename = "buy")]
    Buy,
    #[serde(rename = "sell")]
    Sell,
}

// ts: CoinbaseStarbaseOrderState
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseOrderState {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "filled")]
    Filled,
    #[serde(rename = "rejected")]
    Rejected,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "untriggered")]
    Untriggered,
    #[serde(rename = "triggered")]
    Triggered,
}

// ts: CoinbaseStarbaseOrderType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseOrderType {
    #[serde(rename = "market")]
    Market,
    #[serde(rename = "limit")]
    Limit,
    #[serde(rename = "stop_market")]
    StopMarket,
    #[serde(rename = "stop_limit")]
    StopLimit,
    #[serde(rename = "take_market")]
    TakeMarket,
    #[serde(rename = "take_limit")]
    TakeLimit,
    #[serde(rename = "trailing_stop")]
    TrailingStop,
}

// ts: CoinbaseStarbaseTimeInForce
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseTimeInForce {
    #[serde(rename = "good_til_cancelled")]
    GoodTilCancelled,
    #[serde(rename = "good_til_day")]
    GoodTilDay,
    #[serde(rename = "fill_or_kill")]
    FillOrKill,
    #[serde(rename = "immediate_or_cancel")]
    ImmediateOrCancel,
}

// ts: CoinbaseStarbaseTradeState
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseTradeState {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "filled")]
    Filled,
    #[serde(rename = "rejected")]
    Rejected,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "untriggered")]
    Untriggered,
    #[serde(rename = "archive")]
    Archive,
}

// ts: CoinbaseStarbaseTradeType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseTradeType {
    #[serde(rename = "limit")]
    Limit,
    #[serde(rename = "market")]
    Market,
    #[serde(rename = "liquidation")]
    Liquidation,
}

// ts: CoinbaseStarbaseTriggerType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseTriggerType {
    #[serde(rename = "index_price")]
    IndexPrice,
    #[serde(rename = "mark_price")]
    MarkPrice,
    #[serde(rename = "last_price")]
    LastPrice,
}

// ts: CoinbaseStarbaseFeeCurrency
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseFeeCurrency {
    #[serde(rename = "BTC")]
    Btc,
    #[serde(rename = "ETH")]
    Eth,
    #[serde(rename = "USDC")]
    Usdc,
    #[serde(rename = "USDT")]
    Usdt,
    #[serde(rename = "EURR")]
    Eurr,
}

// ts: CoinbaseStarbaseParityBlocker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseParityBlocker {
    #[serde(rename = "account-pnl-unit-unpublished")]
    AccountPnlUnitUnpublished,
    #[serde(rename = "filled-amount-unit-conflict")]
    FilledAmountUnitConflict,
    #[serde(rename = "gateway-amount-missing")]
    GatewayAmountMissing,
    #[serde(rename = "gateway-filled-amount-missing")]
    GatewayFilledAmountMissing,
    #[serde(rename = "good-til-day-unsupported")]
    GoodTilDayUnsupported,
    #[serde(rename = "position-margin-pnl-units-unpublished")]
    PositionMarginPnlUnitsUnpublished,
    #[serde(rename = "reduce-only-missing")]
    ReduceOnlyMissing,
    #[serde(rename = "stop-limit-shape-unsupported")]
    StopLimitShapeUnsupported,
    #[serde(rename = "take-limit-shape-unsupported")]
    TakeLimitShapeUnsupported,
    #[serde(rename = "trade-amount-unit-conflict")]
    TradeAmountUnitConflict,
    #[serde(rename = "trigger-missing")]
    TriggerMissing,
}

// Anonymous currency singleton in CoinbaseStarbaseAccountSummary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseAccountCurrency {
    #[serde(rename = "USDC")]
    Usdc,
}

// Anonymous direction union in CoinbaseStarbasePrivatePosition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbasePositionDirection {
    #[serde(rename = "buy")]
    Buy,
    #[serde(rename = "sell")]
    Sell,
    #[serde(rename = "zero")]
    Zero,
}

// ts: CoinbaseStarbasePrivateRequest
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbasePrivateRequest<
    Method: AsRef<str>,
    Params: Into<serde_json::Map<String, serde_json::Value>>,
> {
    pub method: Method,
    pub params: Params,
}

// ts: CoinbaseStarbaseAccountSummary
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseAccountSummary {
    pub currency: CoinbaseStarbaseAccountCurrency,
    #[serde(rename = "balanceUsdc")]
    pub balance_usdc: f64,
    #[serde(rename = "equityUsdc")]
    pub equity_usdc: f64,
    #[serde(rename = "availableFundsUsdc")]
    pub available_funds_usdc: f64,
    #[serde(rename = "availableWithdrawalFundsUsdc")]
    pub available_withdrawal_funds_usdc: f64,
    #[serde(rename = "initialMarginUsdc")]
    pub initial_margin_usdc: f64,
    #[serde(rename = "maintenanceMarginUsdc")]
    pub maintenance_margin_usdc: f64,
    #[serde(rename = "marginBalanceUsdc")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_balance_usdc: Option<f64>,
    #[serde(rename = "totalEquityUsd")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_equity_usd: Option<f64>,
    #[serde(rename = "totalInitialMarginUsd")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_initial_margin_usd: Option<f64>,
    #[serde(rename = "totalMaintenanceMarginUsd")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_maintenance_margin_usd: Option<f64>,
    #[serde(rename = "totalMarginBalanceUsd")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_margin_balance_usd: Option<f64>,
    #[serde(rename = "crossCollateralEnabled")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_collateral_enabled: Option<bool>,
    #[serde(rename = "portfolioMarginingEnabled")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portfolio_margining_enabled: Option<bool>,
    #[serde(rename = "marginModel")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_model: Option<CoinbaseStarbaseMarginModel>,
    #[serde(rename = "gatewayFuturesPnl")]
    pub gateway_futures_pnl: f64,
    #[serde(rename = "gatewayFuturesSessionRealizedPnl")]
    pub gateway_futures_session_realized_pnl: f64,
    #[serde(rename = "gatewayFuturesSessionUnrealizedPnl")]
    pub gateway_futures_session_unrealized_pnl: f64,
    #[serde(rename = "gatewaySessionRealizedPnl")]
    pub gateway_session_realized_pnl: f64,
    #[serde(rename = "gatewaySessionUnrealizedPnl")]
    pub gateway_session_unrealized_pnl: f64,
    #[serde(rename = "gatewayTotalPnl")]
    pub gateway_total_pnl: f64,
}

// ts: CoinbaseStarbasePrivatePosition
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbasePrivatePosition {
    #[serde(rename = "instrumentName")]
    pub instrument_name: String,
    #[serde(rename = "legacySavedSymbol")]
    pub legacy_saved_symbol: String,
    pub direction: CoinbaseStarbasePositionDirection,
    #[serde(rename = "baseAmount")]
    pub base_amount: f64,
    #[serde(rename = "gatewaySizeUsd")]
    pub gateway_size_usd: f64,
    #[serde(rename = "entryPriceUsd")]
    pub entry_price_usd: f64,
    #[serde(rename = "markPriceUsd")]
    pub mark_price_usd: f64,
    #[serde(rename = "indexPriceUsd")]
    pub index_price_usd: f64,
    #[serde(rename = "currentNotionalUsd")]
    pub current_notional_usd: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leverage: Option<f64>,
    #[serde(rename = "liquidationPriceUsd")]
    pub liquidation_price_usd: f64,
    #[serde(rename = "gatewayInitialMargin")]
    pub gateway_initial_margin: f64,
    #[serde(rename = "gatewayMaintenanceMargin")]
    pub gateway_maintenance_margin: f64,
    #[serde(rename = "gatewayOpenOrdersMargin")]
    pub gateway_open_orders_margin: f64,
    #[serde(rename = "gatewayFloatingPnl")]
    pub gateway_floating_pnl: f64,
    #[serde(rename = "gatewayRealizedPnl")]
    pub gateway_realized_pnl: f64,
    #[serde(rename = "gatewayTotalPnl")]
    pub gateway_total_pnl: f64,
}

// ts: CoinbaseStarbaseOrderHistoryOptions
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseOrderHistoryOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<f64>,
    #[serde(rename = "includeOld")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_old: Option<bool>,
    #[serde(rename = "includeUnfilled")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_unfilled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub historical: Option<bool>,
}

// ts: CoinbaseStarbaseOrderHistoryPageParams
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseOrderHistoryPageParams {
    pub count: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_old: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_unfilled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub historical: Option<bool>,
}

// Anonymous market-price sentinel in CoinbaseStarbasePrivateOrder.price.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseMarketPrice {
    #[serde(rename = "market_price")]
    MarketPrice,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CoinbaseStarbaseOrderPrice {
    Number(f64),
    Market(CoinbaseStarbaseMarketPrice),
}

// Anonymous liquidity union in CoinbaseStarbasePrivateTrade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseTradeLiquidity {
    #[serde(rename = "M")]
    Maker,
    #[serde(rename = "T")]
    Taker,
}

// ts: CoinbaseStarbaseParityResult
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status")]
pub enum CoinbaseStarbaseParityResult<T, Gateway> {
    #[serde(rename = "ready")]
    Ready { value: T },
    #[serde(rename = "blocked")]
    Blocked {
        gateway: Gateway,
        blockers: Vec<CoinbaseStarbaseParityBlocker>,
    },
}

// ts: CoinbaseStarbasePrivateOrder
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbasePrivateOrder {
    pub id: String,
    #[serde(rename = "instrumentName")]
    pub instrument_name: String,
    #[serde(rename = "legacySavedSymbol")]
    pub legacy_saved_symbol: String,
    pub state: CoinbaseStarbaseOrderState,
    pub r#type: CoinbaseStarbaseOrderType,
    #[serde(rename = "timeInForce")]
    pub time_in_force: CoinbaseStarbaseTimeInForce,
    pub direction: CoinbaseStarbaseOrderDirection,
    pub price: CoinbaseStarbaseOrderPrice,
    #[serde(rename = "averagePrice")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_price: Option<f64>,
    pub label: String,
    #[serde(rename = "postOnly")]
    pub post_only: bool,
    #[serde(rename = "reduceOnly")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(rename = "creationTimestampMs")]
    pub creation_timestamp_ms: f64,
    #[serde(rename = "lastUpdateTimestampMs")]
    pub last_update_timestamp_ms: f64,
    #[serde(rename = "gatewayAmount")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_amount: Option<f64>,
    #[serde(rename = "gatewayFilledAmount")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_filled_amount: Option<f64>,
    #[serde(rename = "gatewayContracts")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_contracts: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<CoinbaseStarbaseTriggerType>,
    #[serde(rename = "triggerPrice")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
    #[serde(rename = "triggerOrderId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_order_id: Option<String>,
}

// ts: CoinbaseStarbasePrivateTrade
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbasePrivateTrade {
    pub id: String,
    pub sequence: f64,
    #[serde(rename = "orderId")]
    pub order_id: String,
    #[serde(rename = "instrumentName")]
    pub instrument_name: String,
    #[serde(rename = "legacySavedSymbol")]
    pub legacy_saved_symbol: String,
    #[serde(rename = "timestampMs")]
    pub timestamp_ms: f64,
    pub direction: CoinbaseStarbaseOrderDirection,
    pub r#type: CoinbaseStarbaseTradeType,
    pub state: CoinbaseStarbaseTradeState,
    #[serde(rename = "priceUsd")]
    pub price_usd: f64,
    #[serde(rename = "markPriceUsd")]
    pub mark_price_usd: f64,
    #[serde(rename = "indexPriceUsd")]
    pub index_price_usd: f64,
    #[serde(rename = "gatewayAmount")]
    pub gateway_amount: f64,
    #[serde(rename = "gatewayContracts")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_contracts: Option<f64>,
    #[serde(rename = "gatewayProfitLoss")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_profit_loss: Option<f64>,
    pub fee: f64,
    #[serde(rename = "feeCurrency")]
    pub fee_currency: CoinbaseStarbaseFeeCurrency,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liquidity: Option<CoinbaseStarbaseTradeLiquidity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

// ts: CoinbaseStarbaseTradePage
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseTradePage {
    #[serde(rename = "hasMore")]
    pub has_more: bool,
    pub trades: Vec<CoinbaseStarbasePrivateTrade>,
}

// ts: isRecord
pub fn is_record(value: Option<&Value>) -> bool {
    crate::starbase::is_record(value)
}

// ts: protocolError
pub fn protocol_error<T>(message: impl Into<String>) -> Result<T, CoinbaseStarbaseProtocolError> {
    Err(CoinbaseStarbaseProtocolError::new(message))
}

// ts: requireRecord
pub fn require_record<'a>(
    value: Option<&'a Value>,
    context: &str,
) -> Result<&'a serde_json::Map<String, Value>, CoinbaseStarbaseProtocolError> {
    crate::starbase_public::require_record(value, context)
}

// ts: requireArray
pub fn require_array<'a>(
    value: Option<&'a Value>,
    context: &str,
) -> Result<&'a Vec<Value>, CoinbaseStarbaseProtocolError> {
    value.and_then(Value::as_array).ok_or_else(|| {
        CoinbaseStarbaseProtocolError::new(format!("Invalid {context}: expected an array"))
    })
}

// ts: requireString
pub fn require_string<'a>(
    value: Option<&'a Value>,
    context: &str,
    allow_empty: bool,
) -> Result<&'a str, CoinbaseStarbaseProtocolError> {
    value
        .and_then(Value::as_str)
        .filter(|value| allow_empty || !value.is_empty())
        .ok_or_else(|| {
            CoinbaseStarbaseProtocolError::new(format!(
                "Invalid {context}: expected {}",
                if allow_empty {
                    "a string"
                } else {
                    "a non-empty string"
                }
            ))
        })
}

// ts: optionalString
pub fn optional_string<'a>(
    value: Option<&'a Value>,
    context: &str,
) -> Result<Option<&'a str>, CoinbaseStarbaseProtocolError> {
    value
        .map(|value| require_string(Some(value), context, true))
        .transpose()
}

// ts: requireFinite
pub fn require_finite(
    value: Option<&Value>,
    context: &str,
) -> Result<f64, CoinbaseStarbaseProtocolError> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| {
            CoinbaseStarbaseProtocolError::new(format!(
                "Invalid {context}: expected a finite number"
            ))
        })
}

// ts: requireNonNegative
pub fn require_non_negative(
    value: Option<&Value>,
    context: &str,
) -> Result<f64, CoinbaseStarbaseProtocolError> {
    let parsed = require_finite(value, context)?;
    if parsed < 0.0 {
        return protocol_error(format!("Invalid {context}: expected a non-negative number"));
    }
    Ok(parsed)
}

// ts: requirePositive
pub fn require_positive(
    value: Option<&Value>,
    context: &str,
) -> Result<f64, CoinbaseStarbaseProtocolError> {
    let parsed = require_finite(value, context)?;
    if parsed <= 0.0 {
        return protocol_error(format!("Invalid {context}: expected a positive number"));
    }
    Ok(parsed)
}

// ts: requireTimestamp
pub fn require_timestamp(
    value: Option<&Value>,
    context: &str,
) -> Result<f64, CoinbaseStarbaseProtocolError> {
    crate::starbase_public::require_timestamp(value, context)
}

// ts: requirePositiveInteger
pub fn require_positive_integer(
    value: Option<&Value>,
    context: &str,
) -> Result<f64, CoinbaseStarbaseProtocolError> {
    crate::starbase_public::require_positive_integer(value, context)
}

// ts: optionalBoolean
pub fn optional_boolean(
    value: Option<&Value>,
    context: &str,
) -> Result<Option<bool>, CoinbaseStarbaseProtocolError> {
    value
        .map(|value| {
            value.as_bool().ok_or_else(|| {
                CoinbaseStarbaseProtocolError::new(format!("Invalid {context}: expected a boolean"))
            })
        })
        .transpose()
}

// ts: parseEnum
pub fn parse_enum<'a>(
    value: Option<&'a Value>,
    values: &[&str],
    context: &str,
) -> Result<&'a str, CoinbaseStarbaseProtocolError> {
    value
        .and_then(Value::as_str)
        .filter(|value| values.contains(value))
        .ok_or_else(|| {
            CoinbaseStarbaseProtocolError::new(format!("Invalid {context}: unknown enum value"))
        })
}

// ts: validateCount
pub fn validate_count(
    value: Option<&Value>,
    context: &str,
) -> Result<f64, CoinbaseStarbaseProtocolError> {
    value
        .and_then(Value::as_f64)
        .filter(|count| count.is_finite() && count.fract() == 0.0 && (1.0..=1000.0).contains(count))
        .ok_or_else(|| {
            CoinbaseStarbaseProtocolError::new(format!(
                "Starbase {context} count must be an integer from 1 to 1000"
            ))
        })
}

type PrivateRequest = CoinbaseStarbasePrivateRequest<&'static str, serde_json::Map<String, Value>>;

// ts: buildOrderHistoryPageParams
pub fn build_order_history_page_params(
    options: &serde_json::Map<String, Value>,
) -> Result<CoinbaseStarbaseOrderHistoryPageParams, CoinbaseStarbaseProtocolError> {
    let default_count = Value::from(100);
    let count = validate_count(
        Some(
            options
                .get("count")
                .filter(|v| !v.is_null())
                .unwrap_or(&default_count),
        ),
        "order-history",
    )?;
    let offset = options
        .get("offset")
        .map(|value| {
            require_timestamp(Some(value), "").map_err(|_| {
                CoinbaseStarbaseProtocolError::new(
                    "Starbase order-history offset must be a non-negative safe integer",
                )
            })
        })
        .transpose()?;
    let include_old = optional_boolean(
        options.get("includeOld"),
        "Starbase order-history include_old",
    )?;
    let include_unfilled = optional_boolean(
        options.get("includeUnfilled"),
        "Starbase order-history include_unfilled",
    )?;
    let historical = optional_boolean(
        options.get("historical"),
        "Starbase order-history historical",
    )?;
    Ok(CoinbaseStarbaseOrderHistoryPageParams {
        count,
        offset,
        include_old,
        include_unfilled,
        historical,
    })
}

// Outbound adapter consumes the DTO serializer, preserving its omission behavior.
fn history_params(
    options: Option<&serde_json::Map<String, Value>>,
) -> Result<serde_json::Map<String, Value>, CoinbaseStarbaseProtocolError> {
    let empty = serde_json::Map::new();
    let params = build_order_history_page_params(options.unwrap_or(&empty))?;
    match serde_json::to_value(params)
        .map_err(|error| CoinbaseStarbaseProtocolError::new(error.to_string()))?
    {
        Value::Object(params) => Ok(params),
        _ => protocol_error("Starbase history parameters must serialize as an object"),
    }
}

// ts: buildCoinbaseStarbaseAccountSummary
pub fn build_coinbase_starbase_account_summary() -> PrivateRequest {
    CoinbaseStarbasePrivateRequest {
        method: "private/get_account_summary",
        params: serde_json::Map::from_iter([
            ("currency".into(), Value::from("USDC")),
            ("extended".into(), Value::from(false)),
        ]),
    }
}

// ts: buildCoinbaseStarbasePositions
pub fn build_coinbase_starbase_positions() -> PrivateRequest {
    CoinbaseStarbasePrivateRequest {
        method: "private/get_positions",
        params: serde_json::Map::from_iter([
            ("currency".into(), Value::from("USDC")),
            ("kind".into(), Value::from("future")),
        ]),
    }
}

// ts: buildCoinbaseStarbaseOpenOrders
pub fn build_coinbase_starbase_open_orders() -> PrivateRequest {
    CoinbaseStarbasePrivateRequest {
        method: "private/get_open_orders_by_currency",
        params: serde_json::Map::from_iter([
            ("currency".into(), Value::from("USDC")),
            ("kind".into(), Value::from("future")),
            ("type".into(), Value::from("all")),
        ]),
    }
}

// ts: buildCoinbaseStarbaseOrderState
pub fn build_coinbase_starbase_order_state(
    order_id: Option<&Value>,
) -> Result<PrivateRequest, CoinbaseStarbaseProtocolError> {
    let order_id = require_string(order_id, "Starbase order-state order_id", false)?;
    Ok(CoinbaseStarbasePrivateRequest {
        method: "private/get_order_state",
        params: serde_json::Map::from_iter([("order_id".into(), Value::from(order_id))]),
    })
}

// ts: buildCoinbaseStarbaseOrderHistoryByCurrency
pub fn build_coinbase_starbase_order_history_by_currency(
    options: Option<&serde_json::Map<String, Value>>,
) -> Result<PrivateRequest, CoinbaseStarbaseProtocolError> {
    let mut params = serde_json::Map::from_iter([
        ("currency".into(), Value::from("USDC")),
        ("kind".into(), Value::from("future")),
    ]);
    params.extend(history_params(options)?);
    Ok(CoinbaseStarbasePrivateRequest {
        method: "private/get_order_history_by_currency",
        params,
    })
}

// ts: buildCoinbaseStarbaseOrderHistoryByInstrument
pub fn build_coinbase_starbase_order_history_by_instrument(
    symbol: &str,
    options: Option<&serde_json::Map<String, Value>>,
) -> Result<PrivateRequest, CoinbaseStarbaseProtocolError> {
    let instrument_name = crate::starbase_public::to_coinbase_starbase_instrument_name(symbol)?;
    let mut params =
        serde_json::Map::from_iter([("instrument_name".into(), Value::from(instrument_name))]);
    params.extend(history_params(options)?);
    Ok(CoinbaseStarbasePrivateRequest {
        method: "private/get_order_history_by_instrument",
        params,
    })
}

// ts: buildCoinbaseStarbaseUserTrades
pub fn build_coinbase_starbase_user_trades(
    count: Option<&Value>,
) -> Result<PrivateRequest, CoinbaseStarbaseProtocolError> {
    let default_count = Value::from(100);
    let count = validate_count(Some(count.unwrap_or(&default_count)), "user-trade")?;
    Ok(CoinbaseStarbasePrivateRequest {
        method: "private/get_user_trades_by_currency",
        params: serde_json::Map::from_iter([
            ("count".into(), Value::from(count)),
            ("currency".into(), Value::from("USDC")),
            ("historical".into(), Value::from(false)),
            ("kind".into(), Value::from("future")),
            ("sorting".into(), Value::from("desc")),
        ]),
    })
}

// ts: buildCoinbaseStarbaseUserTradesByTime
pub fn build_coinbase_starbase_user_trades_by_time(
    start_timestamp_ms: Option<&Value>,
    end_timestamp_ms: Option<&Value>,
    count: Option<&Value>,
) -> Result<PrivateRequest, CoinbaseStarbaseProtocolError> {
    let start = require_timestamp(start_timestamp_ms, "Starbase trades start_timestamp")?;
    let end = require_timestamp(end_timestamp_ms, "Starbase trades end_timestamp")?;
    if end < start {
        return protocol_error("Starbase trades end_timestamp precedes start_timestamp");
    }
    let default_count = Value::from(100);
    let count = validate_count(Some(count.unwrap_or(&default_count)), "user-trade")?;
    Ok(CoinbaseStarbasePrivateRequest {
        method: "private/get_user_trades_by_currency_and_time",
        params: serde_json::Map::from_iter([
            ("count".into(), Value::from(count)),
            ("currency".into(), Value::from("USDC")),
            ("end_timestamp".into(), Value::from(end)),
            ("historical".into(), Value::from(false)),
            ("kind".into(), Value::from("future")),
            ("sorting".into(), Value::from("asc")),
            ("start_timestamp".into(), Value::from(start)),
        ]),
    })
}

// ts: decodeCoinbaseStarbaseAccountSummary
pub fn decode_coinbase_starbase_account_summary(
    value: Option<&Value>,
) -> Result<CoinbaseStarbaseAccountSummary, CoinbaseStarbaseProtocolError> {
    let raw = require_record(value, "Starbase account summary")?;
    if raw.get("currency").and_then(Value::as_str) != Some("USDC") {
        return protocol_error("Unsupported Starbase account-summary currency");
    }
    let margin_model = raw
        .get("margin_model")
        .map(|value| match value.as_str() {
            Some("cross_pm") => Ok(CoinbaseStarbaseMarginModel::CrossPm),
            Some("cross_sm") => Ok(CoinbaseStarbaseMarginModel::CrossSm),
            Some("segregated_pm") => Ok(CoinbaseStarbaseMarginModel::SegregatedPm),
            Some("segregated_sm") => Ok(CoinbaseStarbaseMarginModel::SegregatedSm),
            _ => protocol_error("Invalid Starbase account margin_model: unknown enum value"),
        })
        .transpose()?;
    Ok(CoinbaseStarbaseAccountSummary {
        currency: CoinbaseStarbaseAccountCurrency::Usdc,
        balance_usdc: require_finite(raw.get("balance"), "Starbase account balance")?,
        equity_usdc: require_finite(raw.get("equity"), "Starbase account equity")?,
        available_funds_usdc: require_finite(
            raw.get("available_funds"),
            "Starbase account available_funds",
        )?,
        available_withdrawal_funds_usdc: require_finite(
            raw.get("available_withdrawal_funds"),
            "Starbase account available_withdrawal_funds",
        )?,
        initial_margin_usdc: require_non_negative(
            raw.get("initial_margin"),
            "Starbase account initial_margin",
        )?,
        maintenance_margin_usdc: require_non_negative(
            raw.get("maintenance_margin"),
            "Starbase account maintenance_margin",
        )?,
        margin_balance_usdc: raw
            .get("margin_balance")
            .map(|value| require_finite(Some(value), "Starbase account margin_balance"))
            .transpose()?,
        total_equity_usd: raw
            .get("total_equity_usd")
            .map(|value| require_finite(Some(value), "Starbase account total_equity_usd"))
            .transpose()?,
        total_initial_margin_usd: raw
            .get("total_initial_margin_usd")
            .map(|value| {
                require_non_negative(Some(value), "Starbase account total_initial_margin_usd")
            })
            .transpose()?,
        total_maintenance_margin_usd: raw
            .get("total_maintenance_margin_usd")
            .map(|value| {
                require_non_negative(Some(value), "Starbase account total_maintenance_margin_usd")
            })
            .transpose()?,
        total_margin_balance_usd: raw
            .get("total_margin_balance_usd")
            .map(|value| require_finite(Some(value), "Starbase account total_margin_balance_usd"))
            .transpose()?,
        cross_collateral_enabled: optional_boolean(
            raw.get("cross_collateral_enabled"),
            "Starbase account cross_collateral_enabled",
        )?,
        portfolio_margining_enabled: optional_boolean(
            raw.get("portfolio_margining_enabled"),
            "Starbase account portfolio_margining_enabled",
        )?,
        margin_model,
        gateway_futures_pnl: require_finite(raw.get("futures_pl"), "Starbase account futures_pl")?,
        gateway_futures_session_realized_pnl: require_finite(
            raw.get("futures_session_rpl"),
            "Starbase account futures_session_rpl",
        )?,
        gateway_futures_session_unrealized_pnl: require_finite(
            raw.get("futures_session_upl"),
            "Starbase account futures_session_upl",
        )?,
        gateway_session_realized_pnl: require_finite(
            raw.get("session_rpl"),
            "Starbase account session_rpl",
        )?,
        gateway_session_unrealized_pnl: require_finite(
            raw.get("session_upl"),
            "Starbase account session_upl",
        )?,
        gateway_total_pnl: require_finite(raw.get("total_pl"), "Starbase account total_pl")?,
    })
}

// The source balance assessment has a never-valued ready branch.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum CoinbaseStarbaseNever {}

// ts: assessCoinbaseStarbaseBalanceParity
pub fn assess_coinbase_starbase_balance_parity(
    account: &CoinbaseStarbaseAccountSummary,
) -> CoinbaseStarbaseParityResult<CoinbaseStarbaseNever, &CoinbaseStarbaseAccountSummary> {
    CoinbaseStarbaseParityResult::Blocked {
        gateway: account,
        blockers: vec![CoinbaseStarbaseParityBlocker::AccountPnlUnitUnpublished],
    }
}

// Anonymous return record of parseStarbaseInstrument.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbasePrivateInstrument {
    #[serde(rename = "instrumentName")]
    pub instrument_name: String,
    #[serde(rename = "legacySavedSymbol")]
    pub legacy_saved_symbol: String,
}

// ts: parseStarbaseInstrument
pub fn parse_starbase_instrument(
    value: Option<&Value>,
    context: &str,
) -> Result<CoinbaseStarbasePrivateInstrument, CoinbaseStarbaseProtocolError> {
    let instrument_name = require_string(value, context, false)?;
    Ok(CoinbaseStarbasePrivateInstrument {
        instrument_name: instrument_name.into(),
        legacy_saved_symbol: crate::starbase_public::to_coinbase_legacy_intx_symbol(
            instrument_name,
        )?,
    })
}

// ts: assertUnique
pub fn assert_unique<T, E: From<CoinbaseStarbaseProtocolError>>(
    values: &[T],
    mut id: impl FnMut(&T) -> Result<String, E>,
    context: &str,
) -> Result<(), E> {
    let mut ids = std::collections::HashSet::new();
    for value in values {
        let key = id(value)?;
        if ids.contains(&key) {
            return Err(
                CoinbaseStarbaseProtocolError::new(format!("Duplicate {context}: {key}")).into(),
            );
        }
        ids.insert(key);
    }
    Ok(())
}

// ts: decodeCoinbaseStarbasePosition
pub fn decode_coinbase_starbase_position(
    value: Option<&Value>,
) -> Result<CoinbaseStarbasePrivatePosition, CoinbaseStarbaseProtocolError> {
    let raw = require_record(value, "Starbase position")?;
    if raw.get("kind").and_then(Value::as_str) != Some("future") {
        return protocol_error("Unsupported Starbase position kind");
    }
    let identity = parse_starbase_instrument(
        raw.get("instrument_name"),
        "Starbase position instrument_name",
    )?;
    let direction = match parse_enum(
        raw.get("direction"),
        &["buy", "sell", "zero"],
        "Starbase position direction",
    )? {
        "buy" => CoinbaseStarbasePositionDirection::Buy,
        "sell" => CoinbaseStarbasePositionDirection::Sell,
        _ => CoinbaseStarbasePositionDirection::Zero,
    };
    let base_amount = require_finite(raw.get("size_currency"), "Starbase position size_currency")?;
    let gateway_size_usd = require_finite(raw.get("size"), "Starbase position size")?;
    if direction == CoinbaseStarbasePositionDirection::Zero
        && (base_amount != 0.0 || gateway_size_usd != 0.0)
    {
        return protocol_error("Invalid Starbase zero position with non-zero size");
    }
    if direction != CoinbaseStarbasePositionDirection::Zero
        && (base_amount == 0.0 || gateway_size_usd == 0.0)
    {
        return protocol_error("Invalid Starbase non-zero position with zero size");
    }
    let mark_price_usd = require_positive(raw.get("mark_price"), "Starbase position mark_price")?;
    let leverage = raw
        .get("leverage")
        .map(|value| require_positive_integer(Some(value), "Starbase position leverage"))
        .transpose()?;
    Ok(CoinbaseStarbasePrivatePosition {
        instrument_name: identity.instrument_name,
        legacy_saved_symbol: identity.legacy_saved_symbol,
        direction,
        base_amount: base_amount.abs(),
        gateway_size_usd: gateway_size_usd.abs(),
        entry_price_usd: require_non_negative(
            raw.get("average_price"),
            "Starbase position average_price",
        )?,
        mark_price_usd,
        index_price_usd: require_positive(raw.get("index_price"), "Starbase position index_price")?,
        current_notional_usd: base_amount.abs() * mark_price_usd,
        leverage,
        liquidation_price_usd: require_non_negative(
            raw.get("estimated_liquidation_price"),
            "Starbase position estimated_liquidation_price",
        )?,
        gateway_initial_margin: require_non_negative(
            raw.get("initial_margin"),
            "Starbase position initial_margin",
        )?,
        gateway_maintenance_margin: require_non_negative(
            raw.get("maintenance_margin"),
            "Starbase position maintenance_margin",
        )?,
        gateway_open_orders_margin: require_non_negative(
            raw.get("open_orders_margin"),
            "Starbase position open_orders_margin",
        )?,
        gateway_floating_pnl: require_finite(
            raw.get("floating_profit_loss"),
            "Starbase position floating_profit_loss",
        )?,
        gateway_realized_pnl: require_finite(
            raw.get("realized_profit_loss"),
            "Starbase position realized_profit_loss",
        )?,
        gateway_total_pnl: require_finite(
            raw.get("total_profit_loss"),
            "Starbase position total_profit_loss",
        )?,
    })
}

// ts: decodeCoinbaseStarbasePositions
pub fn decode_coinbase_starbase_positions(
    value: Option<&Value>,
) -> Result<Vec<CoinbaseStarbasePrivatePosition>, CoinbaseStarbaseProtocolError> {
    let positions = require_array(value, "private/get_positions result")?
        .iter()
        .map(|value| decode_coinbase_starbase_position(Some(value)))
        .collect::<Result<Vec<_>, _>>()?;
    assert_unique(
        &positions,
        |position| Ok::<_, CoinbaseStarbaseProtocolError>(position.instrument_name.clone()),
        "Starbase position instrument_name",
    )?;
    Ok(positions)
}

// ts: assessCoinbaseStarbasePositionParity
pub fn assess_coinbase_starbase_position_parity(
    position: &CoinbaseStarbasePrivatePosition,
) -> CoinbaseStarbaseParityResult<CoinbaseStarbaseNever, &CoinbaseStarbasePrivatePosition> {
    CoinbaseStarbaseParityResult::Blocked {
        gateway: position,
        blockers: vec![CoinbaseStarbaseParityBlocker::PositionMarginPnlUnitsUnpublished],
    }
}

// ts: parseOrderPrice
pub fn parse_order_price(
    value: Option<&Value>,
) -> Result<CoinbaseStarbaseOrderPrice, CoinbaseStarbaseProtocolError> {
    if value.and_then(Value::as_str) == Some("market_price") {
        return Ok(CoinbaseStarbaseOrderPrice::Market(
            CoinbaseStarbaseMarketPrice::MarketPrice,
        ));
    }
    require_non_negative(value, "Starbase order price").map(CoinbaseStarbaseOrderPrice::Number)
}

// ts: decodeCoinbaseStarbaseOrder
pub fn decode_coinbase_starbase_order(
    value: Option<&Value>,
) -> Result<CoinbaseStarbasePrivateOrder, CoinbaseStarbaseProtocolError> {
    let raw = require_record(value, "Starbase order")?;
    let identity =
        parse_starbase_instrument(raw.get("instrument_name"), "Starbase order instrument_name")?;
    let trigger = raw
        .get("trigger")
        .map(|value| match value.as_str() {
            Some("index_price") => Ok(CoinbaseStarbaseTriggerType::IndexPrice),
            Some("mark_price") => Ok(CoinbaseStarbaseTriggerType::MarkPrice),
            Some("last_price") => Ok(CoinbaseStarbaseTriggerType::LastPrice),
            _ => protocol_error("Invalid Starbase order trigger: unknown enum value"),
        })
        .transpose()?;
    let Some(post_only) = raw.get("post_only").and_then(Value::as_bool) else {
        return protocol_error("Invalid Starbase order post_only: expected a boolean");
    };
    Ok(CoinbaseStarbasePrivateOrder {
        id: require_string(raw.get("order_id"), "Starbase order order_id", false)?.into(),
        instrument_name: identity.instrument_name,
        legacy_saved_symbol: identity.legacy_saved_symbol,
        state: match raw.get("order_state").and_then(Value::as_str) {
            Some("open") => CoinbaseStarbaseOrderState::Open,
            Some("filled") => CoinbaseStarbaseOrderState::Filled,
            Some("rejected") => CoinbaseStarbaseOrderState::Rejected,
            Some("cancelled") => CoinbaseStarbaseOrderState::Cancelled,
            Some("untriggered") => CoinbaseStarbaseOrderState::Untriggered,
            Some("triggered") => CoinbaseStarbaseOrderState::Triggered,
            _ => return protocol_error("Invalid Starbase order order_state: unknown enum value"),
        },
        r#type: match raw.get("order_type").and_then(Value::as_str) {
            Some("market") => CoinbaseStarbaseOrderType::Market,
            Some("limit") => CoinbaseStarbaseOrderType::Limit,
            Some("stop_market") => CoinbaseStarbaseOrderType::StopMarket,
            Some("stop_limit") => CoinbaseStarbaseOrderType::StopLimit,
            Some("take_market") => CoinbaseStarbaseOrderType::TakeMarket,
            Some("take_limit") => CoinbaseStarbaseOrderType::TakeLimit,
            Some("trailing_stop") => CoinbaseStarbaseOrderType::TrailingStop,
            _ => return protocol_error("Invalid Starbase order order_type: unknown enum value"),
        },
        time_in_force: match raw.get("time_in_force").and_then(Value::as_str) {
            Some("good_til_cancelled") => CoinbaseStarbaseTimeInForce::GoodTilCancelled,
            Some("good_til_day") => CoinbaseStarbaseTimeInForce::GoodTilDay,
            Some("fill_or_kill") => CoinbaseStarbaseTimeInForce::FillOrKill,
            Some("immediate_or_cancel") => CoinbaseStarbaseTimeInForce::ImmediateOrCancel,
            _ => return protocol_error("Invalid Starbase order time_in_force: unknown enum value"),
        },
        direction: match raw.get("direction").and_then(Value::as_str) {
            Some("buy") => CoinbaseStarbaseOrderDirection::Buy,
            Some("sell") => CoinbaseStarbaseOrderDirection::Sell,
            _ => return protocol_error("Invalid Starbase order direction: unknown enum value"),
        },
        price: parse_order_price(raw.get("price"))?,
        average_price: raw
            .get("average_price")
            .map(|value| require_non_negative(Some(value), "Starbase order average_price"))
            .transpose()?,
        label: require_string(raw.get("label"), "Starbase order label", true)?.into(),
        post_only,
        reduce_only: optional_boolean(raw.get("reduce_only"), "Starbase order reduce_only")?,
        creation_timestamp_ms: require_timestamp(
            raw.get("creation_timestamp"),
            "Starbase order creation_timestamp",
        )?,
        last_update_timestamp_ms: require_timestamp(
            raw.get("last_update_timestamp"),
            "Starbase order last_update_timestamp",
        )?,
        gateway_amount: raw
            .get("amount")
            .map(|value| require_non_negative(Some(value), "Starbase order amount"))
            .transpose()?,
        gateway_filled_amount: raw
            .get("filled_amount")
            .map(|value| require_non_negative(Some(value), "Starbase order filled_amount"))
            .transpose()?,
        gateway_contracts: raw
            .get("contracts")
            .map(|value| require_non_negative(Some(value), "Starbase order contracts"))
            .transpose()?,
        trigger,
        trigger_price: raw
            .get("trigger_price")
            .map(|value| require_positive(Some(value), "Starbase order trigger_price"))
            .transpose()?,
        trigger_order_id: raw
            .get("trigger_order_id")
            .map(|value| {
                require_string(Some(value), "Starbase order trigger_order_id", false)
                    .map(str::to_owned)
            })
            .transpose()?,
    })
}

// ts: decodeCoinbaseStarbaseOpenOrders
pub fn decode_coinbase_starbase_open_orders(
    value: Option<&Value>,
) -> Result<Vec<CoinbaseStarbasePrivateOrder>, CoinbaseStarbaseProtocolError> {
    let orders = require_array(value, "private/get_open_orders result")?
        .iter()
        .map(|value| decode_coinbase_starbase_order(Some(value)))
        .collect::<Result<Vec<_>, _>>()?;
    assert_unique(
        &orders,
        |order| Ok::<_, CoinbaseStarbaseProtocolError>(order.id.clone()),
        "Starbase order_id",
    )?;
    Ok(orders)
}

// ts: decodeCoinbaseStarbaseOrderState
pub fn decode_coinbase_starbase_order_state(
    value: Option<&Value>,
    expected_order_id: Option<&Value>,
) -> Result<CoinbaseStarbasePrivateOrder, CoinbaseStarbaseProtocolError> {
    let expected = require_string(
        expected_order_id,
        "Starbase order-state expected order_id",
        false,
    )?;
    let order = decode_coinbase_starbase_order(value)?;
    if order.id != expected {
        return protocol_error(format!(
            "Starbase order-state order_id mismatch: expected {expected}, received {}",
            order.id
        ));
    }
    Ok(order)
}

// ts: decodeCoinbaseStarbaseOrderHistory
pub fn decode_coinbase_starbase_order_history(
    value: Option<&Value>,
) -> Result<Vec<CoinbaseStarbasePrivateOrder>, CoinbaseStarbaseProtocolError> {
    let orders = require_array(value, "private/get_order_history result")?
        .iter()
        .map(|value| decode_coinbase_starbase_order(Some(value)))
        .collect::<Result<Vec<_>, _>>()?;
    assert_unique(
        &orders,
        |order| Ok::<_, CoinbaseStarbaseProtocolError>(order.id.clone()),
        "Starbase order-history order_id",
    )?;
    Ok(orders)
}

// ts: decodeCoinbaseStarbaseOrderHistoryByInstrument
pub fn decode_coinbase_starbase_order_history_by_instrument(
    value: Option<&Value>,
    expected_symbol: &str,
) -> Result<Vec<CoinbaseStarbasePrivateOrder>, CoinbaseStarbaseProtocolError> {
    let expected = crate::starbase_public::to_coinbase_starbase_instrument_name(expected_symbol)?;
    let orders = decode_coinbase_starbase_order_history(value)?;
    for order in &orders {
        if order.instrument_name != expected {
            return protocol_error(format!(
                "Starbase order-history instrument mismatch: expected {expected}, received {}",
                order.instrument_name
            ));
        }
    }
    Ok(orders)
}

// ts: decodeCoinbaseStarbaseTrade
pub fn decode_coinbase_starbase_trade(
    value: Option<&Value>,
) -> Result<CoinbaseStarbasePrivateTrade, CoinbaseStarbaseProtocolError> {
    let raw = require_record(value, "Starbase user trade")?;
    let identity = parse_starbase_instrument(
        raw.get("instrument_name"),
        "Starbase user trade instrument_name",
    )?;
    if raw.get("matching_id") != Some(&Value::Null) {
        return protocol_error("Invalid Starbase user trade matching_id: expected null");
    }
    let liquidity = raw
        .get("liquidity")
        .map(|value| match value.as_str() {
            Some("M") => Ok(CoinbaseStarbaseTradeLiquidity::Maker),
            Some("T") => Ok(CoinbaseStarbaseTradeLiquidity::Taker),
            _ => protocol_error("Invalid Starbase user trade liquidity: unknown enum value"),
        })
        .transpose()?;
    let kind = match raw.get("order_type").and_then(Value::as_str) {
        Some("limit") => CoinbaseStarbaseTradeType::Limit,
        Some("market") => CoinbaseStarbaseTradeType::Market,
        Some("liquidation") => CoinbaseStarbaseTradeType::Liquidation,
        _ => return protocol_error("Invalid Starbase user trade order_type: unknown enum value"),
    };
    let fee_currency = match raw.get("fee_currency").and_then(Value::as_str) {
        Some("BTC") => CoinbaseStarbaseFeeCurrency::Btc,
        Some("ETH") => CoinbaseStarbaseFeeCurrency::Eth,
        Some("USDC") => CoinbaseStarbaseFeeCurrency::Usdc,
        Some("USDT") => CoinbaseStarbaseFeeCurrency::Usdt,
        Some("EURR") => CoinbaseStarbaseFeeCurrency::Eurr,
        _ => return protocol_error("Invalid Starbase user trade fee_currency: unknown enum value"),
    };
    Ok(CoinbaseStarbasePrivateTrade {
        id: require_string(raw.get("trade_id"), "Starbase user trade trade_id", false)?.into(),
        sequence: require_positive_integer(raw.get("trade_seq"), "Starbase user trade trade_seq")?,
        order_id: require_string(raw.get("order_id"), "Starbase user trade order_id", false)?
            .into(),
        instrument_name: identity.instrument_name,
        legacy_saved_symbol: identity.legacy_saved_symbol,
        timestamp_ms: require_timestamp(raw.get("timestamp"), "Starbase user trade timestamp")?,
        direction: match raw.get("direction").and_then(Value::as_str) {
            Some("buy") => CoinbaseStarbaseOrderDirection::Buy,
            Some("sell") => CoinbaseStarbaseOrderDirection::Sell,
            _ => {
                return protocol_error("Invalid Starbase user trade direction: unknown enum value");
            }
        },
        r#type: kind,
        state: match raw.get("state").and_then(Value::as_str) {
            Some("open") => CoinbaseStarbaseTradeState::Open,
            Some("filled") => CoinbaseStarbaseTradeState::Filled,
            Some("rejected") => CoinbaseStarbaseTradeState::Rejected,
            Some("cancelled") => CoinbaseStarbaseTradeState::Cancelled,
            Some("untriggered") => CoinbaseStarbaseTradeState::Untriggered,
            Some("archive") => CoinbaseStarbaseTradeState::Archive,
            _ => return protocol_error("Invalid Starbase user trade state: unknown enum value"),
        },
        price_usd: require_positive(raw.get("price"), "Starbase user trade price")?,
        mark_price_usd: require_positive(raw.get("mark_price"), "Starbase user trade mark_price")?,
        index_price_usd: require_positive(
            raw.get("index_price"),
            "Starbase user trade index_price",
        )?,
        gateway_amount: require_positive(raw.get("amount"), "Starbase user trade amount")?,
        gateway_contracts: raw
            .get("contracts")
            .map(|value| require_positive(Some(value), "Starbase user trade contracts"))
            .transpose()?,
        gateway_profit_loss: raw
            .get("profit_loss")
            .map(|value| require_finite(Some(value), "Starbase user trade profit_loss"))
            .transpose()?,
        fee: require_finite(raw.get("fee"), "Starbase user trade fee")?,
        fee_currency,
        liquidity,
        label: optional_string(raw.get("label"), "Starbase user trade label")?.map(str::to_owned),
    })
}

// ts: decodeCoinbaseStarbaseTradePage
pub fn decode_coinbase_starbase_trade_page(
    value: Option<&Value>,
) -> Result<CoinbaseStarbaseTradePage, CoinbaseStarbaseProtocolError> {
    let raw = require_record(value, "private/get_user_trades result")?;
    let Some(has_more) = optional_boolean(raw.get("has_more"), "Starbase user trades has_more")?
    else {
        return protocol_error("Invalid Starbase user trades has_more");
    };
    let trades = require_array(raw.get("trades"), "Starbase user trades trades")?
        .iter()
        .map(|value| decode_coinbase_starbase_trade(Some(value)))
        .collect::<Result<Vec<_>, _>>()?;
    assert_unique(
        &trades,
        |trade| Ok::<_, CoinbaseStarbaseProtocolError>(trade.id.clone()),
        "Starbase trade_id",
    )?;
    Ok(CoinbaseStarbaseTradePage { has_more, trades })
}

// ts: normalizeCoinbaseStarbaseTrade
pub fn normalize_coinbase_starbase_trade(
    gateway: &CoinbaseStarbasePrivateTrade,
) -> CoinbaseStarbaseParityResult<safe_cex_types::HistoricalTrade, &CoinbaseStarbasePrivateTrade> {
    CoinbaseStarbaseParityResult::Blocked {
        gateway,
        blockers: vec![CoinbaseStarbaseParityBlocker::TradeAmountUnitConflict],
    }
}

// ts: mapOrderStatus
pub fn map_order_status(state: CoinbaseStarbaseOrderState) -> safe_cex_types::OrderStatus {
    match state {
        CoinbaseStarbaseOrderState::Filled => safe_cex_types::OrderStatus::Closed,
        CoinbaseStarbaseOrderState::Cancelled | CoinbaseStarbaseOrderState::Rejected => {
            safe_cex_types::OrderStatus::Canceled
        }
        _ => safe_cex_types::OrderStatus::Open,
    }
}

// ts: mapOrderSide
pub fn map_order_side(direction: CoinbaseStarbaseOrderDirection) -> safe_cex_types::OrderSide {
    match direction {
        CoinbaseStarbaseOrderDirection::Buy => safe_cex_types::OrderSide::Buy,
        CoinbaseStarbaseOrderDirection::Sell => safe_cex_types::OrderSide::Sell,
    }
}

// ts: mapOrderType
pub fn map_order_type(kind: CoinbaseStarbaseOrderType) -> Option<safe_cex_types::OrderType> {
    match kind {
        CoinbaseStarbaseOrderType::Market => Some(safe_cex_types::OrderType::Market),
        CoinbaseStarbaseOrderType::Limit => Some(safe_cex_types::OrderType::Limit),
        CoinbaseStarbaseOrderType::StopMarket => Some(safe_cex_types::OrderType::StopLoss),
        CoinbaseStarbaseOrderType::TakeMarket => Some(safe_cex_types::OrderType::TakeProfit),
        CoinbaseStarbaseOrderType::TrailingStop => {
            Some(safe_cex_types::OrderType::TrailingStopLoss)
        }
        CoinbaseStarbaseOrderType::StopLimit | CoinbaseStarbaseOrderType::TakeLimit => None,
    }
}

// ts: mapTimeInForce
pub fn map_time_in_force(
    value: CoinbaseStarbaseTimeInForce,
    post_only: bool,
) -> Option<safe_cex_types::OrderTimeInForce> {
    if post_only {
        return Some(safe_cex_types::OrderTimeInForce::PostOnly);
    }
    match value {
        CoinbaseStarbaseTimeInForce::GoodTilCancelled => {
            Some(safe_cex_types::OrderTimeInForce::GoodTillCancel)
        }
        CoinbaseStarbaseTimeInForce::FillOrKill => {
            Some(safe_cex_types::OrderTimeInForce::FillOrKill)
        }
        CoinbaseStarbaseTimeInForce::ImmediateOrCancel => {
            Some(safe_cex_types::OrderTimeInForce::ImmediateOrCancel)
        }
        CoinbaseStarbaseTimeInForce::GoodTilDay => None,
    }
}

// ts: mapTrigger
pub fn map_trigger(value: CoinbaseStarbaseTriggerType) -> safe_cex_types::OrderStopTriggerType {
    match value {
        CoinbaseStarbaseTriggerType::IndexPrice => safe_cex_types::OrderStopTriggerType::IndexPrice,
        CoinbaseStarbaseTriggerType::MarkPrice => safe_cex_types::OrderStopTriggerType::MarkPrice,
        CoinbaseStarbaseTriggerType::LastPrice => safe_cex_types::OrderStopTriggerType::LastPrice,
    }
}

// ts: normalizeCoinbaseStarbaseOrder
pub fn normalize_coinbase_starbase_order(
    gateway: &CoinbaseStarbasePrivateOrder,
) -> CoinbaseStarbaseParityResult<safe_cex_types::Order, &CoinbaseStarbasePrivateOrder> {
    use CoinbaseStarbaseOrderType as Type;
    let mut blockers = Vec::new();
    if gateway.gateway_amount.is_none() {
        blockers.push(CoinbaseStarbaseParityBlocker::GatewayAmountMissing);
    }
    if gateway.gateway_filled_amount.is_none() {
        blockers.push(CoinbaseStarbaseParityBlocker::GatewayFilledAmountMissing);
    }
    if gateway
        .gateway_filled_amount
        .is_some_and(|amount| amount != 0.0)
    {
        blockers.push(CoinbaseStarbaseParityBlocker::FilledAmountUnitConflict);
    }
    if gateway.reduce_only.is_none() {
        blockers.push(CoinbaseStarbaseParityBlocker::ReduceOnlyMissing);
    }
    if gateway.r#type == Type::StopLimit {
        blockers.push(CoinbaseStarbaseParityBlocker::StopLimitShapeUnsupported);
    }
    if gateway.r#type == Type::TakeLimit {
        blockers.push(CoinbaseStarbaseParityBlocker::TakeLimitShapeUnsupported);
    }
    if gateway.time_in_force == CoinbaseStarbaseTimeInForce::GoodTilDay {
        blockers.push(CoinbaseStarbaseParityBlocker::GoodTilDayUnsupported);
    }
    if matches!(
        gateway.r#type,
        Type::StopMarket
            | Type::StopLimit
            | Type::TakeMarket
            | Type::TakeLimit
            | Type::TrailingStop
    ) && gateway.trigger.is_none()
    {
        blockers.push(CoinbaseStarbaseParityBlocker::TriggerMissing);
    }
    if !blockers.is_empty() {
        return CoinbaseStarbaseParityResult::Blocked { gateway, blockers };
    }

    let kind = map_order_type(gateway.r#type).unwrap_or(safe_cex_types::OrderType::Limit);
    let time_in_force = map_time_in_force(gateway.time_in_force, gateway.post_only);
    let trigger_type = gateway.trigger.map(map_trigger);
    let amount = gateway.gateway_amount.unwrap_or_default();
    let info = private_order_info(gateway);
    CoinbaseStarbaseParityResult::Ready {
        value: safe_cex_types::Order {
            client_order_id: (None).into(),
            id: gateway.id.clone(),
            parent_id: (None).into(),
            status: (map_order_status(gateway.state)).into(),
            symbol: gateway.legacy_saved_symbol.clone(),
            r#type: (kind).into(),
            side: (map_order_side(gateway.direction)).into(),
            price: match gateway.price {
                CoinbaseStarbaseOrderPrice::Number(value) => (value).into(),
                CoinbaseStarbaseOrderPrice::Market(_) => (0.0).into(),
            },
            amount,
            filled: 0.0,
            remaining: amount,
            reduce_only: gateway.reduce_only.unwrap_or(false).into(),
            close: (None).into(),
            info: (Some(info.into())).into(),
            time_in_force: time_in_force.into(),
            last_update: gateway.last_update_timestamp_ms,
            trigger_type: (None).into(),
            tp_trigger_type: ((kind == safe_cex_types::OrderType::TakeProfit)
                .then_some(trigger_type)
                .flatten())
            .into(),
            sl_trigger_type: ((kind == safe_cex_types::OrderType::StopLoss)
                .then_some(trigger_type)
                .flatten())
            .into(),
            take_profit: ((kind == safe_cex_types::OrderType::TakeProfit)
                .then_some(gateway.trigger_price)
                .flatten())
            .into(),
            stop_loss: ((kind == safe_cex_types::OrderType::StopLoss)
                .then_some(gateway.trigger_price)
                .flatten())
            .into(),
            extra: (None).into(),
        },
    }
}

fn private_order_info(gateway: &CoinbaseStarbasePrivateOrder) -> Value {
    use CoinbaseStarbaseOrderPrice as Price;
    let mut info = serde_json::Map::new();
    info.insert("id".into(), Value::String(gateway.id.clone()));
    info.insert(
        "instrumentName".into(),
        Value::String(gateway.instrument_name.clone()),
    );
    info.insert(
        "legacySavedSymbol".into(),
        Value::String(gateway.legacy_saved_symbol.clone()),
    );
    info.insert(
        "state".into(),
        Value::String(order_state_text(gateway.state).into()),
    );
    info.insert(
        "type".into(),
        Value::String(order_type_text(gateway.r#type).into()),
    );
    info.insert(
        "timeInForce".into(),
        Value::String(time_in_force_text(gateway.time_in_force).into()),
    );
    info.insert(
        "direction".into(),
        Value::String(direction_text(gateway.direction).into()),
    );
    let price = match gateway.price {
        Price::Number(value) => Value::from(value),
        Price::Market(_) => Value::String("market_price".into()),
    };
    info.insert("price".into(), price);
    insert_optional(&mut info, "averagePrice", gateway.average_price);
    info.insert("label".into(), Value::String(gateway.label.clone()));
    info.insert("postOnly".into(), Value::Bool(gateway.post_only));
    insert_optional(&mut info, "reduceOnly", gateway.reduce_only);
    info.insert(
        "creationTimestampMs".into(),
        Value::from(gateway.creation_timestamp_ms),
    );
    info.insert(
        "lastUpdateTimestampMs".into(),
        Value::from(gateway.last_update_timestamp_ms),
    );
    insert_optional(&mut info, "gatewayAmount", gateway.gateway_amount);
    insert_optional(
        &mut info,
        "gatewayFilledAmount",
        gateway.gateway_filled_amount,
    );
    insert_optional(&mut info, "gatewayContracts", gateway.gateway_contracts);
    if let Some(value) = gateway.trigger {
        info.insert("trigger".into(), Value::String(trigger_text(value).into()));
    }
    insert_optional(&mut info, "triggerPrice", gateway.trigger_price);
    if let Some(value) = &gateway.trigger_order_id {
        info.insert("triggerOrderId".into(), Value::String(value.clone()));
    }
    Value::Object(info)
}

fn insert_optional<T: Into<Value>>(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
    value: Option<T>,
) {
    if let Some(value) = value {
        object.insert(key.into(), value.into());
    }
}

fn order_state_text(value: CoinbaseStarbaseOrderState) -> &'static str {
    match value {
        CoinbaseStarbaseOrderState::Open => "open",
        CoinbaseStarbaseOrderState::Filled => "filled",
        CoinbaseStarbaseOrderState::Rejected => "rejected",
        CoinbaseStarbaseOrderState::Cancelled => "cancelled",
        CoinbaseStarbaseOrderState::Untriggered => "untriggered",
        CoinbaseStarbaseOrderState::Triggered => "triggered",
    }
}

fn order_type_text(value: CoinbaseStarbaseOrderType) -> &'static str {
    match value {
        CoinbaseStarbaseOrderType::Market => "market",
        CoinbaseStarbaseOrderType::Limit => "limit",
        CoinbaseStarbaseOrderType::StopMarket => "stop_market",
        CoinbaseStarbaseOrderType::StopLimit => "stop_limit",
        CoinbaseStarbaseOrderType::TakeMarket => "take_market",
        CoinbaseStarbaseOrderType::TakeLimit => "take_limit",
        CoinbaseStarbaseOrderType::TrailingStop => "trailing_stop",
    }
}

fn time_in_force_text(value: CoinbaseStarbaseTimeInForce) -> &'static str {
    match value {
        CoinbaseStarbaseTimeInForce::GoodTilCancelled => "good_til_cancelled",
        CoinbaseStarbaseTimeInForce::GoodTilDay => "good_til_day",
        CoinbaseStarbaseTimeInForce::FillOrKill => "fill_or_kill",
        CoinbaseStarbaseTimeInForce::ImmediateOrCancel => "immediate_or_cancel",
    }
}

fn direction_text(value: CoinbaseStarbaseOrderDirection) -> &'static str {
    match value {
        CoinbaseStarbaseOrderDirection::Buy => "buy",
        CoinbaseStarbaseOrderDirection::Sell => "sell",
    }
}

fn trigger_text(value: CoinbaseStarbaseTriggerType) -> &'static str {
    match value {
        CoinbaseStarbaseTriggerType::IndexPrice => "index_price",
        CoinbaseStarbaseTriggerType::MarkPrice => "mark_price",
        CoinbaseStarbaseTriggerType::LastPrice => "last_price",
    }
}

// ts: CoinbaseStarbaseReadOnlyState
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseReadOnlyState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<CoinbaseStarbaseAccountSummary>,
    pub positions: Vec<CoinbaseStarbasePrivatePosition>,
    pub orders: Vec<CoinbaseStarbasePrivateOrder>,
    #[serde(rename = "orderState", skip_serializing_if = "Option::is_none")]
    pub order_state: Option<CoinbaseStarbasePrivateOrder>,
    #[serde(rename = "orderHistory")]
    pub order_history: Vec<CoinbaseStarbasePrivateOrder>,
    #[serde(rename = "tradePage")]
    pub trade_page: CoinbaseStarbaseTradePage,
}

impl Default for CoinbaseStarbaseReadOnlyState {
    fn default() -> Self {
        Self {
            account: None,
            positions: Vec::new(),
            orders: Vec::new(),
            order_state: None,
            order_history: Vec::new(),
            trade_page: CoinbaseStarbaseTradePage {
                has_more: false,
                trades: Vec::new(),
            },
        }
    }
}

impl CoinbaseStarbaseReadOnlyState {
    pub fn apply_account_summary<'a>(
        &'a mut self,
        value: Option<&Value>,
    ) -> Result<&'a CoinbaseStarbaseAccountSummary, CoinbaseStarbaseProtocolError> {
        let next = decode_coinbase_starbase_account_summary(value)?;
        Ok(self.account.insert(next))
    }

    pub fn apply_positions<'a>(
        &'a mut self,
        value: Option<&Value>,
    ) -> Result<&'a [CoinbaseStarbasePrivatePosition], CoinbaseStarbaseProtocolError> {
        let next = decode_coinbase_starbase_positions(value)?;
        self.positions = next;
        Ok(&self.positions)
    }

    pub fn apply_open_orders<'a>(
        &'a mut self,
        value: Option<&Value>,
    ) -> Result<&'a [CoinbaseStarbasePrivateOrder], CoinbaseStarbaseProtocolError> {
        let next = decode_coinbase_starbase_open_orders(value)?;
        self.orders = next;
        Ok(&self.orders)
    }

    pub fn apply_order_state<'a>(
        &'a mut self,
        order_id: &str,
        value: Option<&Value>,
    ) -> Result<&'a CoinbaseStarbasePrivateOrder, CoinbaseStarbaseProtocolError> {
        let id = Value::String(order_id.to_owned());
        let next = decode_coinbase_starbase_order_state(value, Some(&id))?;
        Ok(self.order_state.insert(next))
    }

    pub fn apply_order_history<'a>(
        &'a mut self,
        value: Option<&Value>,
    ) -> Result<&'a [CoinbaseStarbasePrivateOrder], CoinbaseStarbaseProtocolError> {
        let next = decode_coinbase_starbase_order_history(value)?;
        self.order_history = next;
        Ok(&self.order_history)
    }

    pub fn apply_order_history_by_instrument<'a>(
        &'a mut self,
        symbol: &str,
        value: Option<&Value>,
    ) -> Result<&'a [CoinbaseStarbasePrivateOrder], CoinbaseStarbaseProtocolError> {
        let next = decode_coinbase_starbase_order_history_by_instrument(value, symbol)?;
        self.order_history = next;
        Ok(&self.order_history)
    }

    pub fn apply_trade_page<'a>(
        &'a mut self,
        value: Option<&Value>,
    ) -> Result<&'a CoinbaseStarbaseTradePage, CoinbaseStarbaseProtocolError> {
        let next = decode_coinbase_starbase_trade_page(value)?;
        self.trade_page = next;
        Ok(&self.trade_page)
    }
}
