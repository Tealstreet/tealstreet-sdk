//! Pure exchange decisions, kept independent from lifecycle and transport state.
use safe_cex_exchange::js_runtime::value::{Js, Thrown, arg, from_json};
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_num::{JsCoercionError, js_number, js_string, js_truthy, parse_float};
use safe_cex_transport::{RestAdapter, RestRequest};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{
    Balance, BothPositionSide, ExchangeAccount, Execution, ExecutionPositionSide, OrderBook,
    OrderBookOrders, OrderSide, OrderType, PositionSide,
};
use serde_json::{Map, Value};

// ts: isAsterDEXUnknownOrderError
pub fn is_aster_dex_unknown_order_error(
    value: Option<&Value>,
    error_message: Option<&str>,
) -> Result<bool, JsCoercionError> {
    let message = match error_message {
        Some(message) => message.to_owned(),
        None => js_string(value)?,
    };
    Ok(message.contains("code: -2011") && message.to_lowercase().contains("unknown order"))
}

// ts: AsterDEXExchange.getOrderPositionSide
pub fn get_order_position_side(
    opts: impl Into<safe_cex_types::PreparedOrderOpts>,
    is_hedged: bool,
) -> &'static str {
    let opts = opts.into();
    if !is_hedged {
        return "BOTH";
    }
    let mut long = opts.side == OrderSide::Buy;
    if matches!(
        opts.r#type.into_value(),
        Some(OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss)
    ) || opts.reduce_only == Some(true)
    {
        long = !long;
    }
    if long { "LONG" } else { "SHORT" }
}

// ts: AsterDEXExchange.collectOpenInterestFromPayload
// Mutates incrementally: a later coercion failure leaves earlier entries intact.
pub fn collect_open_interest_from_payload(
    payload: Option<&Value>,
    map: &mut Map<String, Value>,
) -> Result<(), JsCoercionError> {
    fn collect(item: &Value, map: &mut Map<String, Value>) -> Result<(), JsCoercionError> {
        let symbol = item.get("symbol").and_then(Value::as_str).unwrap_or("");
        let value = js_number(item.get("openInterest"))?;
        if !symbol.is_empty() && value.is_finite() {
            map.insert(symbol.to_owned(), Value::from(value));
        }
        Ok(())
    }
    match payload {
        Some(Value::Array(items)) => {
            for item in items {
                collect(item, map)?;
            }
        }
        Some(root @ Value::Object(_)) => match root.get("data") {
            Some(Value::Array(items)) => {
                for item in items {
                    collect(item, map)?;
                }
            }
            Some(item @ Value::Object(_)) => collect(item, map)?,
            _ => collect(root, map)?,
        },
        _ => {}
    }
    Ok(())
}

// ts: AsterDEXExchange.fetchOpenInterestMap
// Bulk failure is logged and may fall back to sequential per-symbol reads;
// per-symbol failures are deliberately silent and do not discard prior data.
pub async fn fetch_open_interest_map(
    adapter: &dyn RestAdapter,
    endpoint: &str,
    symbols: Option<&[String]>,
    allow_per_symbol_fallback: bool,
    mut log_warning: impl FnMut(String),
) -> Map<String, Value> {
    async fn get_data(
        adapter: &dyn RestAdapter,
        endpoint: &str,
        symbol: Option<&str>,
    ) -> Result<Value, String> {
        let mut request = RestRequest {
            method: "GET".into(),
            url: endpoint.to_owned(),
            timeout_ms: Some(2000),
            ..Default::default()
        };
        if let Some(symbol) = symbol {
            request.params = Some(serde_json::json!({"symbol": symbol}));
        }
        let response = adapter
            .execute(request)
            .await
            .map_err(|failure| failure.message)?;
        // Axios's default silent JSON transform keeps non-JSON response text
        // as a string (including an empty body) rather than rejecting it.
        response
            .body
            .transform_axios_response()
            .map_err(|error| error.to_string())?
            .json_value()
            .map_err(|error| error.to_string())
    }

    let mut map = Map::new();
    match get_data(adapter, endpoint, None).await {
        Ok(data) => {
            if let Err(error) = collect_open_interest_from_payload(Some(&data), &mut map) {
                log_warning(format!(
                    "Bulk open-interest fetch failed: {}",
                    error.message
                ));
            }
        }
        Err(error) => log_warning(format!("Bulk open-interest fetch failed: {error}")),
    }

    if !map.is_empty() || !allow_per_symbol_fallback {
        return map;
    }
    let Some(symbols) = symbols.filter(|symbols| !symbols.is_empty()) else {
        return map;
    };
    for symbol in symbols.iter().filter(|symbol| !symbol.is_empty()) {
        if let Ok(data) = get_data(adapter, endpoint, Some(symbol)).await {
            // Matches fetchOpenInterestMap's empty catch for per-symbol failures.
            let _ = collect_open_interest_from_payload(Some(&data), &mut map);
        }
    }
    map
}

// ts: AsterDEXExchange.isAlgoOrder
pub fn is_algo_order(order_type: OrderType, info: Option<&Input>) -> bool {
    matches!(
        order_type,
        OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
    ) || is_raw_algo_order(info)
}

fn is_raw_algo_order(info: Option<&Input>) -> bool {
    info.and_then(|v| v.get("type")).is_some_and(|v| {
        [
            "STOP",
            "STOP_MARKET",
            "TAKE_PROFIT",
            "TAKE_PROFIT_MARKET",
            "TRAILING_STOP_MARKET",
        ]
        .iter()
        .any(|text| v.text_equals(text))
    })
}

// ts: AsterDEXExchange.reverseMapTimeInForce
pub fn reverse_map_time_in_force(tif: &str) -> &str {
    if tif.is_empty() { "GTC" } else { tif }
}

// ts: AsterDEXExchange.fetchPositionMode
// `data` is the Axios response body. Preserve the source's property-access
// failure for nullish bodies and its JSON.stringify text for other invalid data.
pub fn parse_position_mode_response(data: Option<&Value>) -> Result<bool, (&'static str, String)> {
    let data = match data {
        None => {
            return Err((
                "TypeError",
                "Cannot read properties of undefined (reading 'dualSidePosition')".to_owned(),
            ));
        }
        Some(Value::Null) => {
            return Err((
                "TypeError",
                "Cannot read properties of null (reading 'dualSidePosition')".to_owned(),
            ));
        }
        Some(data) => data,
    };
    let value = data
        .as_object()
        .and_then(|object| object.get("dualSidePosition"));
    match value {
        Some(Value::Bool(true)) => Ok(true),
        Some(Value::String(s)) if s == "true" => Ok(true),
        Some(Value::Bool(false)) => Ok(false),
        Some(Value::String(s)) if s == "false" => Ok(false),
        _ => {
            let rendered = safe_cex_num::js_json_stringify(&data.clone().into())
                .ok()
                .flatten()
                .unwrap_or_else(|| "undefined".to_owned());
            Err((
                "Error",
                format!("Unexpected position mode response: {rendered}"),
            ))
        }
    }
}

// ts: AsterDEXExchange.fetchSymbolMaxLeverage
// The tuple projection keeps Array.find's first-match semantics explicit.
pub fn fetch_symbol_max_leverage(
    markets: &[(String, Option<f64>)],
    symbol: &str,
) -> Result<f64, String> {
    let Some((_, max)) = markets
        .iter()
        .find(|(market_symbol, _)| market_symbol == symbol)
    else {
        return Err(format!("Market not found for symbol: {symbol}"));
    };
    Ok(match max {
        Some(value) if *value != 0.0 && !value.is_nan() => *value,
        _ => 10.0,
    })
}

// ts: AsterDEXExchange#fetchLeverageBrackets
#[allow(clippy::type_complexity)] // Keep the parity result structural without exporting a helper type.
pub fn parse_leverage_brackets_response(
    response: Result<&Value, &str>,
) -> (Vec<(Option<Value>, Option<Value>)>, String, LogSeverity) {
    let mut entries: Vec<(Option<Value>, Option<Value>)> = Vec::new();
    let mut keys: Vec<Js> = Vec::new();
    let parse_result = (|| {
        let data = match response {
            Ok(data) => data,
            Err(error) => return Err(error.to_owned()),
        };
        let items: Vec<&Value> = match data {
            Value::Array(items) => items.iter().collect(),
            item => vec![item],
        };
        for item in items {
            if item.is_null() {
                return Err("Cannot read properties of null (reading 'brackets')".to_owned());
            }
            let Some(brackets) = item.get("brackets").and_then(Value::as_array) else {
                continue;
            };
            let Some(first) = brackets.first() else {
                continue;
            };
            if first.is_null() {
                return Err("Cannot read properties of null (reading 'initialLeverage')".to_owned());
            }
            let key = item.get("symbol").cloned();
            let value = first.get("initialLeverage").cloned();
            // Each object/array in the owned JSON tree has its own source identity.
            // Keep the original JSON metadata alongside the shared Map comparison.
            let map_key = key.as_ref().map(from_json).unwrap_or(Js::Undefined);
            if let Some(index) = keys
                .iter()
                .position(|old_key| old_key.same_value_zero(&map_key))
            {
                entries[index].1 = value;
            } else {
                keys.push(map_key);
                entries.push((key, value));
            }
        }
        Ok(())
    })();

    match parse_result {
        Ok(()) => {
            let log_message = format!("Loaded leverage brackets for {} symbols", entries.len());
            (entries, log_message, LogSeverity::Info)
        }
        Err(error) => (
            entries,
            format!("Failed to fetch leverage brackets: {error}. Using default leverage of 10"),
            LogSeverity::Warning,
        ),
    }
}

// ts: AsterDEXExchange.fetchStpMode
// The feature gate and request lifecycle have their own rows; this helper
// mirrors response validation after the actual request succeeds.
pub fn parse_stp_mode_response(
    data: Option<&Value>,
) -> Result<crate::types::AsterDEXStpMode, (&'static str, String)> {
    let mode = data
        .and_then(Value::as_object)
        .and_then(|object| object.get("stpMode"));
    match mode {
        Some(Value::String(text)) => match text.as_str() {
            "EXPIRE_TAKER" => Ok(crate::types::AsterDEXStpMode::V0),
            "EXPIRE_MAKER" => Ok(crate::types::AsterDEXStpMode::V1),
            "EXPIRE_BOTH" => Ok(crate::types::AsterDEXStpMode::V2),
            _ => Err((
                "Error",
                "AsterDEX returned an invalid self-trade-prevention mode".to_owned(),
            )),
        },
        _ => Err((
            "Error",
            "AsterDEX returned an invalid self-trade-prevention mode".to_owned(),
        )),
    }
}

// ts: AsterDEXExchange.changeStpMode
// Returns the validated request effect; transport submission remains owned by
// the exchange API adapter.
pub fn build_change_stp_mode_request(
    stp_mode: Option<&Value>,
) -> Result<Value, (&'static str, String)> {
    let Some(Value::String(mode)) = stp_mode else {
        let rendered =
            js_string(stp_mode).map_err(|error| (error.name, error.message.to_owned()))?;
        return Err((
            "Error",
            format!("Invalid AsterDEX self-trade-prevention mode: {rendered}"),
        ));
    };
    if !matches!(
        mode.as_str(),
        "EXPIRE_TAKER" | "EXPIRE_MAKER" | "EXPIRE_BOTH"
    ) {
        return Err((
            "Error",
            format!("Invalid AsterDEX self-trade-prevention mode: {mode}"),
        ));
    }
    Ok(serde_json::json!({
        "method": "POST",
        "path": crate::helpers::get_aster_dex_fapi_path(
            "stpMode",
            &crate::types::AsterDEXFapiVersion::V3
        ),
        "body": {"stpMode": mode}
    }))
}

// ts: AsterDEXExchange.changeMarginType
// Returns the validated request effect; API submission remains runtime-owned.
pub fn build_change_margin_type_request(
    isolated: bool,
    symbol: Option<&str>,
    api_version: crate::types::AsterDEXAPIVersion,
) -> Result<Value, (&'static str, String)> {
    let Some(symbol) = symbol.filter(|symbol| !symbol.is_empty()) else {
        return Err((
            "Error",
            "Symbol is required for changing margin type".to_owned(),
        ));
    };
    let version = match api_version {
        crate::types::AsterDEXAPIVersion::V1 => crate::types::AsterDEXFapiVersion::V1,
        crate::types::AsterDEXAPIVersion::V3 => crate::types::AsterDEXFapiVersion::V3,
    };
    Ok(serde_json::json!({
        "method": "POST",
        "path": crate::helpers::get_aster_dex_fapi_path("marginType", &version),
        "body": {
            "symbol": symbol,
            "marginType": if isolated { "ISOLATED" } else { "CROSSED" }
        }
    }))
}

fn retained_info<T: serde::Serialize>(
    record: &T,
) -> Result<safe_cex_types::retained_json::RetainedJsonInput, serde_json::Error> {
    safe_cex_num::numeric_ingress::js_capture_output(record)
}

// ts: AsterDEXExchange.transformTicker
pub fn transform_ticker(
    ticker: &crate::types::AsterDEXTickerResponse,
    open_interest: Option<&std::collections::BTreeMap<String, f64>>,
) -> Result<safe_cex_types::Ticker, serde_json::Error> {
    use safe_cex_num::parse_float;
    Ok(safe_cex_types::Ticker {
        id: ticker.symbol.clone(),
        symbol: ticker.symbol.clone(),
        bid: 0.0,
        ask: 0.0,
        last: parse_float(&ticker.last_price),
        mark: 0.0,
        index: 0.0,
        mid_price: None,
        timestamp: None,
        percentage: parse_float(&ticker.price_change_percent),
        funding_rate: 0.0,
        funding_time: 0.0,
        open_interest: open_interest
            .and_then(|map| map.get(&ticker.symbol))
            .copied()
            .unwrap_or(0.0),
        volume: parse_float(&ticker.volume),
        quote_volume: parse_float(&ticker.quote_volume),
        info: Some(retained_info(ticker)?),
    })
}

// ts: AsterDEXExchange.transformPosition
pub fn transform_position(
    pos: &Value,
    existing: &[safe_cex_types::Position],
) -> Result<safe_cex_types::Position, (&'static str, String)> {
    use safe_cex_types::{MarginType, Position, PositionMode, PositionSide};
    let symbol = pos.get("symbol").and_then(Value::as_str).ok_or_else(|| {
        (
            "RepresentationError",
            "transformPosition symbol is outside the shared string boundary".into(),
        )
    })?;
    let number = |field: &str, or_zero: bool| -> Result<f64, (&'static str, String)> {
        let value = pos.get(field);
        let text = if or_zero && !js_truthy(value) {
            "0".into()
        } else {
            js_string(value).map_err(|error| (error.name, error.to_string()))?
        };
        Ok(parse_float(&text))
    };
    let amount = number("positionAmt", false)?;
    let leverage = number("leverage", true)?.abs();
    let leverage = if leverage == 0.0 || leverage.is_nan() {
        1.0
    } else {
        leverage
    };
    let mode = if pos.get("positionSide").and_then(Value::as_str) == Some("BOTH")
        || !js_truthy(pos.get("positionSide"))
    {
        PositionMode::OneWay
    } else {
        PositionMode::Hedged
    };
    let long = if mode == PositionMode::OneWay {
        amount > 0.0
    } else {
        pos.get("positionSide").and_then(Value::as_str) == Some("LONG")
    };
    let side = if long {
        PositionSide::Long
    } else {
        PositionSide::Short
    };
    let id = safe_cex_types::model_utils::create_position_id(symbol, Some(side), Some(mode));
    let realized_pnl = existing
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| (p.realized_pnl).into_value())
        .unwrap_or(0.0);
    Ok(Position {
        id,
        symbol: symbol.to_owned(),
        side: side.into(),
        contracts: amount.abs(),
        entry_price: number("entryPrice", false)?,
        notional: number("notional", false)?,
        leverage,
        unrealized_pnl: number("unRealizedProfit", false)?,
        realized_pnl: Some(realized_pnl).into(),
        margin_type: Some(
            if pos.get("marginType").and_then(Value::as_str) == Some("ISOLATED") {
                MarginType::Isolated
            } else {
                MarginType::Cross
            },
        )
        .into(),
        liquidation_price: number("liquidationPrice", true)?,
        position_mode: Some(mode).into(),
        info: Some(pos.clone().into()).into(),
        margin_size: None.into(),
        extra: None.into(),
        last_update: None.into(),
        option: None.into(),
    })
}

// ts: AsterDEXExchange.getMaxAlgoOrdersLimit
pub fn get_max_algo_orders_limit(
    market: Option<&safe_cex_types::Market>,
) -> Result<Option<f64>, JsCoercionError> {
    let Some(filters) = market
        .and_then(|m| m.info.as_ref())
        .and_then(|v| v.get("filters"))
        .and_then(Input::as_array)
    else {
        return Ok(None);
    };
    for filter in filters {
        // Array.find evaluates each predicate until the first matching filter.
        if filter.is_null() {
            return Err(JsCoercionError {
                name: "TypeError",
                message: "Cannot read properties of null (reading 'filterType')",
            });
        }
        if filter
            .get("filterType")
            .is_some_and(|v| v.text_equals("MAX_NUM_ALGO_ORDERS"))
        {
            let value = safe_cex_num::js_retained_number(filter.get("limit"))?;
            return Ok(value.is_finite().then_some(value));
        }
    }
    Ok(None)
}

// ts: AsterDEXExchange.getOpenAlgoOrdersCount
pub fn get_open_algo_orders_count(symbol: &str, orders: &[safe_cex_types::Order]) -> usize {
    orders
        .iter()
        .filter(|o| {
            o.symbol == symbol
                && o.status == safe_cex_types::OrderStatus::Open
                && match o.r#type.into_value() {
                    Some(kind) => is_algo_order(kind, o.info.as_ref()),
                    None => is_raw_algo_order(o.info.as_ref()),
                }
        })
        .count()
}

// ts: AsterDEXExchange.assertAlgoOrderCapacity
// Error name and message stay separate, including coercions thrown by the limit parser.
pub fn assert_algo_order_capacity(
    symbol: &str,
    new_algo_orders: f64,
    market: Option<&safe_cex_types::Market>,
    orders: &[safe_cex_types::Order],
) -> Result<(), (&'static str, String)> {
    if new_algo_orders <= 0.0 {
        return Ok(());
    }
    let max = get_max_algo_orders_limit(market).map_err(|e| (e.name, e.message.to_owned()))?;
    let Some(max) = max.filter(|n| *n != 0.0) else {
        return Ok(());
    };
    let existing = get_open_algo_orders_count(symbol, orders) as f64;
    if existing + new_algo_orders > max {
        return Err((
            "Error",
            format!(
                "AsterDEX max algo order limit reached for {symbol}: {}/{} open, {} new requested",
                safe_cex_num::js_number_string(existing),
                safe_cex_num::js_number_string(max),
                safe_cex_num::js_number_string(new_algo_orders),
            ),
        ));
    }
    Ok(())
}

// ts: AsterDEXExchange.getTickSize
pub fn get_tick_size(market: &safe_cex_types::Market) -> Result<f64, JsCoercionError> {
    let filters = market.info.as_ref().and_then(|v| v.get("filters"));
    if filters.is_some_and(Input::is_truthy) {
        let Some(filters) = filters.and_then(Input::as_array) else {
            return Err(JsCoercionError {
                name: "TypeError",
                message: "market.info.filters.find is not a function",
            });
        };
        for filter in filters {
            if filter.is_null() {
                return Err(JsCoercionError {
                    name: "TypeError",
                    message: "Cannot read properties of null (reading 'filterType')",
                });
            }
            if filter
                .get("filterType")
                .is_some_and(|v| v.text_equals("PRICE_FILTER"))
            {
                let tick = filter.get("tickSize");
                if tick.is_some_and(Input::is_truthy) {
                    return Ok(safe_cex_num::js_retained_parse_float(tick)?);
                }
                break;
            }
        }
    }
    Ok(market.precision.price)
}

// ts: AsterDEXExchange.mapOrderType
pub fn map_order_type(order_type: OrderType) -> Result<&'static str, &'static str> {
    let key = match order_type {
        OrderType::TrailingStopLoss => {
            return Err(
                "AsterDEX trailing stop-loss orders are unsupported because callbackRate is unavailable",
            );
        }
        OrderType::Limit => "limit",
        OrderType::Market => "market",
        OrderType::Conditional => "conditional",
        OrderType::StopLoss => "stop_market",
        OrderType::TakeProfit => "take_profit_market",
    };
    Ok(crate::tables::ASTERDEX_ORDER_TYPE_MAP
        .iter()
        .find(|(k, _)| *k == key)
        .map_or(key, |(_, value)| *value))
}

// ts: AsterDEXExchange.mapTimeframe
pub fn map_timeframe(timeframe: safe_cex_types::AnyTimeframe) -> &'static str {
    match timeframe {
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M1) => "1m",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M3) => "3m",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M5) => "5m",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M15) => "15m",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M30) => "30m",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H1) => "1h",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H2) => "2h",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H4) => "4h",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H6) => "6h",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H12) => "12h",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::D1) => "1d",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::W1) => "1w",
        _ => "1h",
    }
}

// ts: AsterDEXExchange.canSign
// Host signer presence is explicit because runtime handles do not serialize in options.
pub fn can_sign(
    options: &safe_cex_types::ExchangeOptions,
    version: &crate::types::AsterDEXAPIVersion,
    has_remote_signer: bool,
) -> bool {
    if options.key.is_empty() {
        return false;
    }
    if has_remote_signer {
        return true;
    }
    if *version == crate::types::AsterDEXAPIVersion::V3 {
        return crate::helpers::has_aster_dex_v3_auth_context(options);
    }
    !options.secret.is_empty()
}

// ts: AsterDEXExchange.getFapiPath
pub fn get_fapi_path(
    endpoint: &str,
    version: &crate::types::AsterDEXAPIVersion,
    version_override: Option<crate::types::AsterDEXFapiVersion>,
) -> String {
    use crate::types::{AsterDEXAPIVersion, AsterDEXFapiVersion};
    let version = version_override.unwrap_or(match version {
        AsterDEXAPIVersion::V1 => AsterDEXFapiVersion::V1,
        AsterDEXAPIVersion::V3 => AsterDEXFapiVersion::V3,
    });
    crate::helpers::get_aster_dex_fapi_path(endpoint, &version)
}

// ts: AsterDEXExchange.getAccountEndpoint
pub fn get_account_endpoint(version: &crate::types::AsterDEXAPIVersion) -> String {
    get_fapi_path(
        "account",
        version,
        if *version == crate::types::AsterDEXAPIVersion::V3 {
            None
        } else {
            Some(crate::types::AsterDEXFapiVersion::V4)
        },
    )
}

// ts: AsterDEXExchange.getAccount
pub fn get_account_from_alias(account_alias: &str, api_key: Option<&str>) -> ExchangeAccount {
    ExchangeAccount {
        user_id: if account_alias.is_empty() {
            api_key.unwrap_or_default().to_owned()
        } else {
            account_alias.to_owned()
        },
        sub_id: None,
        affiliate_id: None,
    }
}

// ts: AsterDEXExchange.fetchOrderBook
pub fn transform_order_book(bids: &[(String, String)], asks: &[(String, String)]) -> OrderBook {
    fn levels(values: &[(String, String)], descending: bool) -> Vec<OrderBookOrders> {
        let mut levels: Vec<_> = values
            .iter()
            .map(|(price, amount)| OrderBookOrders {
                price: parse_float(price),
                amount: parse_float(amount),
                total: 0.0,
            })
            .collect();
        levels.sort_by(|left, right| {
            let order = left
                .price
                .partial_cmp(&right.price)
                .unwrap_or(std::cmp::Ordering::Equal);
            if descending { order.reverse() } else { order }
        });
        let mut total = 0.0;
        for level in &mut levels {
            total += level.price * level.amount;
            level.total = total;
        }
        levels
    }
    OrderBook {
        bids: levels(bids, true),
        asks: levels(asks, false),
    }
}

// ts: AsterDEXExchange#fetchBalance
pub fn balance_from_account_response(account: &Value) -> Result<Balance, JsCoercionError> {
    fn amount(account: &Value, field: &str) -> Result<f64, JsCoercionError> {
        let text = match account.get(field).filter(|value| js_truthy(Some(*value))) {
            Some(value) => js_string(Some(value))?,
            None => "0".to_owned(),
        };
        Ok(parse_float(&text))
    }
    Ok(Balance {
        total: amount(account, "totalWalletBalance")?,
        free: amount(account, "availableBalance")?,
        used: amount(account, "totalInitialMargin")?
            + amount(account, "totalOpenOrderInitialMargin")?,
        upnl: amount(account, "totalUnrealizedProfit")?,
        info: None,
        currencies: None,
        coins: None,
    })
}

// ts: AsterDEXExchange.fetchOHLCV
pub fn transform_klines(rows: &Js) -> Result<Js, Thrown> {
    rows.call_method(
        "map",
        vec![Js::function("", |_, args| {
            let row = arg(&args, 0);
            Ok(Js::object(vec![
                ("timestamp", row.get("0")?),
                ("open", Js::Number(row.get("1")?.parse_float()?)),
                ("high", Js::Number(row.get("2")?.parse_float()?)),
                ("low", Js::Number(row.get("3")?.parse_float()?)),
                ("close", Js::Number(row.get("4")?.parse_float()?)),
                ("volume", Js::Number(row.get("5")?.parse_float()?)),
            ]))
        })],
    )
}

// ts: AsterDEXExchange.fetchHistoricalFills
pub fn historical_fills_from_trades(
    trades: &[crate::types::AsterDEXUserTradeResponse],
) -> Vec<Execution> {
    let realized_pnls = crate::helpers::normalize_aster_dex_realized_pnl_deltas(trades);
    let mut executions = Vec::new();
    for (index, trade) in trades.iter().enumerate() {
        let price = parse_float(&trade.price);
        let qty = parse_float(&trade.qty);
        if !price.is_finite() || !qty.is_finite() || qty <= 0.0 {
            continue;
        }

        let raw_position_side = trade
            .position_side
            .as_deref()
            .filter(|side| !side.is_empty())
            .unwrap_or("")
            .to_uppercase();
        let position_side = match raw_position_side.as_str() {
            "LONG" => ExecutionPositionSide::Side(PositionSide::Long),
            "SHORT" => ExecutionPositionSide::Side(PositionSide::Short),
            _ => ExecutionPositionSide::Both(BothPositionSide::Both),
        };
        executions.push(Execution {
            id: Some(safe_cex_num::js_number_string(trade.id)),
            timestamp: (trade.time / 1000.0).floor(),
            symbol: trade.symbol.clone(),
            order_id: Some(safe_cex_num::js_number_string(trade.order_id)),
            price,
            qty,
            side: if trade.side == "BUY" {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            },
            realized_pnl: realized_pnls[index],
            position_side: Some(position_side),
            maker: trade.maker,
            commission: trade
                .commission
                .as_deref()
                .filter(|commission| !commission.is_empty())
                .map(parse_float),
            commission_asset: trade.commission_asset.clone(),
            venue_reported_revenue: None,
            venue_reported_revenue_asset: None,
        });
    }
    executions
}

// ts: AsterDEXExchange.reverseMapOrderStatus
// ts-bug: prototype status names suppress the Open fallback; preserve the value
// in TableHit so a later store bridge cannot silently substitute a typed status.
pub fn reverse_map_order_status(
    status: &str,
) -> safe_cex_num::TableHit<safe_cex_types::OrderStatus> {
    use safe_cex_num::{TableHit, js_table_lookup};
    use safe_cex_types::OrderStatus;
    let mapping = [
        ("NEW", OrderStatus::Open),
        ("PARTIALLY_FILLED", OrderStatus::Open),
        ("FILLED", OrderStatus::Closed),
        ("CANCELED", OrderStatus::Canceled),
        ("REJECTED", OrderStatus::Canceled),
        ("EXPIRED", OrderStatus::Canceled),
    ];
    match js_table_lookup(&mapping, status) {
        TableHit::Mapped(value) => TableHit::Mapped(*value),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        // All mapped statuses are nonempty strings and all inherited members
        // are truthy: only Missing reaches TypeScript's result-OR fallback.
        TableHit::Missing => TableHit::Mapped(OrderStatus::Open),
    }
}

// ts: AsterDEXExchange.reverseMapOrderType
// ts-bug: unknown strings escape the OrderType cast, and prototype keys return
// inherited values. Keep both until the separate closed Order output boundary.
pub fn reverse_map_order_type(kind: &str) -> safe_cex_num::TableHit<String> {
    use safe_cex_num::{TableHit, js_table_lookup};
    let mapping = [
        ("LIMIT", "limit"),
        ("MARKET", "market"),
        ("STOP", "stop_market"),
        ("STOP_MARKET", "stop_market"),
        ("TAKE_PROFIT", "take_profit_market"),
        ("TAKE_PROFIT_MARKET", "take_profit_market"),
        ("TRAILING_STOP", "trailing_stop_market"),
        ("TRAILING_STOP_MARKET", "trailing_stop_market"),
    ];
    match js_table_lookup(&mapping, kind) {
        TableHit::Mapped(value) => TableHit::Mapped((*value).to_owned()),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Mapped(kind.to_owned()),
    }
}

// ts: AsterDEXExchange.extractFilterValue
pub fn extract_filter_value(
    filters: &[Value],
    filter_type: &str,
    field: &str,
) -> Result<f64, JsCoercionError> {
    use safe_cex_num::{TableHit, js_table_lookup, js_truthy, parse_float};
    for filter in filters {
        if filter.is_null() {
            return Err(JsCoercionError {
                name: "TypeError",
                message: "Cannot read properties of null (reading 'filterType')",
            });
        }
        if filter.get("filterType").and_then(Value::as_str) != Some(filter_type) {
            continue;
        }
        // A matching JSON filter must be an object; find stops here even when
        // the field is absent or falsy, rather than searching later duplicates.
        let entries = filter
            .as_object()
            .into_iter()
            .flat_map(|v| v.iter())
            .collect::<Vec<_>>();
        return match js_table_lookup(&entries, field) {
            TableHit::Mapped(value) if js_truthy(Some(value)) => {
                Ok(parse_float(&js_string(Some(value))?))
            }
            // ts-bug: inherited field values pass the truthiness guard. The
            // eleven native functions and Object.prototype all parse to NaN;
            // no closed enum or serialized function boundary is involved.
            TableHit::Inherited(member) => Ok(parse_float(member.js_string())),
            _ => Ok(0.0),
        };
    }
    Ok(0.0)
}

// ts: AsterDEXExchange.transformMarket
pub fn transform_market(
    market: &crate::types::AsterDEXMarketResponse,
    leverage_brackets: &std::collections::BTreeMap<String, Js>,
) -> Result<safe_cex_types::Market, (&'static str, String)> {
    use safe_cex_num::{ccxt_precision::precision_to_string, parse_float};
    use safe_cex_types::{
        Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision,
    };
    let price = parse_float(&precision_to_string(market.price_precision));
    let amount = parse_float(&precision_to_string(market.quantity_precision));
    let max_leverage = match leverage_brackets
        .get(&market.symbol)
        .filter(|value| value.truthy())
    {
        Some(value) => value
            .to_number()
            .map_err(|error| ("TypeError", error.message()))?,
        None => 10.0,
    };
    let filters = market
        .filters
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ("Error", e.to_string()))?;
    let min = extract_filter_value(&filters, "LOT_SIZE", "minQty")
        .map_err(|e| (e.name, e.message.to_owned()))?;
    let max = extract_filter_value(&filters, "LOT_SIZE", "maxQty")
        .map_err(|e| (e.name, e.message.to_owned()))?;
    Ok(Market {
        id: market.symbol.clone(),
        symbol: market.symbol.clone(),
        pretty_symbol: None,
        base: market.base_asset.clone(),
        quote: market.quote_asset.clone(),
        active: market.status == "TRADING",
        contract_size: Some(1.0),
        precision: MarketPrecision { price, amount },
        limits: MarketLimits {
            amount: MarketLimitsAmount { min, max },
            leverage: MarketLimitsLeverage {
                min: 1.0,
                max: max_leverage,
            },
            price: None,
        },
        info: Some(retained_info(market).map_err(|e| ("Error", e.to_string()))?),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
        option: None,
    })
}

// ts: AsterDEXExchange.getAccountPositionMode
pub fn get_account_position_mode(is_hedged: bool) -> safe_cex_types::PositionMode {
    if is_hedged {
        safe_cex_types::PositionMode::Hedged
    } else {
        safe_cex_types::PositionMode::OneWay
    }
}

// ts: AsterDEXExchange.transformOrder
// ts-bug: reverse mappings can return inherited members or unknown strings.
// Preserve lookup decisions, then refuse the whole record at the shared closed
// output boundary; TS instead retains a record with those out-of-contract fields.
pub fn transform_order(
    order: &crate::types::AsterDEXOrderResponse,
) -> Result<safe_cex_types::Order, (&'static str, String)> {
    use safe_cex_num::{TableHit, js_number_string, parse_float};
    let stop = order.stop_price.as_deref().map(parse_float);
    let price = stop
        .filter(|value| *value > 0.0)
        .unwrap_or_else(|| parse_float(&order.price));
    let kind = match reverse_map_order_type(&order.r#type) {
        TableHit::Mapped(value) => serde_json::from_value::<OrderType>(Value::String(value))
            .map_err(|_| {
                (
                    "RepresentationError",
                    "transformOrder type is outside the shared closed enum".to_owned(),
                )
            })?,
        TableHit::Inherited(_) | TableHit::Missing => {
            return Err((
                "RepresentationError",
                "transformOrder type is outside the shared closed enum".to_owned(),
            ));
        }
    };
    let status = match reverse_map_order_status(&order.status) {
        TableHit::Mapped(value) => value,
        TableHit::Inherited(_) | TableHit::Missing => {
            return Err((
                "RepresentationError",
                "transformOrder status is outside the shared closed enum".to_owned(),
            ));
        }
    };
    let amount = parse_float(&order.orig_qty);
    let filled = parse_float(&order.executed_qty);
    Ok(safe_cex_types::Order {
        client_order_id: (None).into(),
        id: js_number_string(order.order_id),
        parent_id: (None).into(),
        symbol: order.symbol.clone(),
        r#type: (kind).into(),
        side: if order.side == "BUY" {
            safe_cex_types::ModelField::Value(OrderSide::Buy)
        } else {
            safe_cex_types::ModelField::Value(OrderSide::Sell)
        },
        price: price.into(),
        amount,
        filled,
        remaining: amount - filled,
        status: status.into(),
        reduce_only: (order.close_position == Some(true) || order.reduce_only == Some(true)).into(),
        last_update: order.time,
        info: (Some(retained_info(order).map_err(|error| ("Error", error.to_string()))?)).into(),
        close: (None).into(),
        time_in_force: (None).into(),
        trigger_type: (None).into(),
        tp_trigger_type: (None).into(),
        sl_trigger_type: (None).into(),
        take_profit: (None).into(),
        stop_loss: (None).into(),
        extra: (None).into(),
    })
}

// ts: AsterDEXExchange.fetchPublicTrades
// The source's trade mapper casts uppercase BUY/SELL strings into a shared
// closed OrderSide enum. Keep the source values and refuse at that output
// boundary rather than silently lowercasing them.
pub fn map_public_trade(
    trade: &crate::types::AsterDEXTradeResponse,
    symbol: &str,
) -> Result<safe_cex_types::PublicTrade, (&'static str, String)> {
    use safe_cex_num::{js_number_string, parse_float};
    let side = if trade.is_buyer_maker { "SELL" } else { "BUY" };
    let typed_side =
        serde_json::from_value::<OrderSide>(Value::String(side.to_owned())).map_err(|_| {
            (
                "RepresentationError",
                format!("fetchPublicTrades side {side} is outside the shared closed enum"),
            )
        })?;
    Ok(safe_cex_types::PublicTrade {
        id: js_number_string(trade.id),
        symbol: symbol.to_owned(),
        price: parse_float(&trade.price),
        qty: parse_float(&trade.qty),
        quote_qty: None,
        timestamp: trade.time,
        side: typed_side,
    })
}

// ts: AsterDEXExchange.afterMethodCalledInWorker
pub fn after_method_called_in_worker(
    method: &str,
    first_arg: Option<&Value>,
    set_setting: impl FnOnce(&str, bool) -> Result<(), safe_cex_exchange::runtime::ExchangeError>,
) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
    if method == "changePositionMode" {
        set_setting("isHedged", safe_cex_num::js_truthy(first_arg))?;
    }
    Ok(())
}

// ts: AsterDEXExchange.assertV3Feature
pub fn assert_v3_feature(
    base: &safe_cex_exchange::exchange::BaseExchange,
    version: crate::types::AsterDEXAPIVersion,
    feature: &str,
) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
    if version == crate::types::AsterDEXAPIVersion::V3 {
        return Ok(());
    }
    let message = format!("AsterDEX {feature} requires API v3");
    base.reject_preflight(message)
}

// ts: AsterDEXExchange.markPositionStateUpdatedByWebsocket
// Keep JS binary64 increment semantics, including loss of unit precision at 2^53.
pub fn mark_position_state_updated_by_websocket(version: &mut f64) {
    *version += 1.0;
}
// ts: AsterDEXExchange.getPositionStateVersion
pub fn get_position_state_version(version: f64) -> f64 {
    version
}
// ts: AsterDEXExchange.markBalanceStateUpdatedByWebsocket
pub fn mark_balance_state_updated_by_websocket(version: &mut f64) -> f64 {
    *version += 1.0;
    *version
}
// ts: AsterDEXExchange.getBalanceStateVersion
pub fn get_balance_state_version(version: f64) -> f64 {
    version
}
