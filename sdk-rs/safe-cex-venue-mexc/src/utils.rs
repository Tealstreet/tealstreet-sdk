//! Pure MEXC wire normalization; JS coercion delegates to the shared primitives.
use safe_cex_num::{
    JsCoercionError, js_number, js_number_string, js_string, js_truthy, math_max, parse_precision,
};
use safe_cex_store::orderbook::{calc_order_book_total, sort_order_book};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{
    MarginType, Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision,
    OrderBook, OrderBookOrders, OrderSide, OrderStatus, OrderType, PositionMode, PositionSide,
};
use serde_json::{Map, Value};

fn pick<'a>(raw: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|key| raw.get(*key).filter(|v| !v.is_null()))
}

pub fn to_number(value: Option<&Value>, fallback: Option<f64>) -> Result<f64, JsCoercionError> {
    retained_number(value.cloned().map(Input::from).as_ref(), fallback)
}

fn retained_number(value: Option<&Input>, fallback: Option<f64>) -> Result<f64, JsCoercionError> {
    let fallback = fallback.unwrap_or(0.0);
    if value.is_none_or(|value| value.is_nullish() || value.text_equals("")) {
        return Ok(fallback);
    }
    let parsed = safe_cex_num::js_retained_parse_float(value)?;
    Ok(if parsed.is_finite() { parsed } else { fallback })
}

fn retained_pick<'a>(raw: &'a Input, keys: &[&str]) -> Option<&'a Input> {
    keys.iter()
        .find_map(|key| raw.get(key).filter(|value| !value.is_nullish()))
}

pub fn to_integer_string(value: Option<&Value>) -> Result<String, JsCoercionError> {
    value
        .filter(|v| !v.is_null())
        .map(|v| js_string(Some(v)))
        .unwrap_or_else(|| Ok(String::new()))
}

pub fn to_mexc_symbol(symbol: &str) -> String {
    if symbol.contains('_') {
        return symbol.into();
    }
    let (base, quote) = safe_cex_types::symbol_converter::parse_binance_symbol(symbol);
    format!("{base}_{quote}")
}

pub fn from_mexc_symbol(symbol: Option<&Value>) -> Result<String, JsCoercionError> {
    Ok(to_integer_string(symbol)?.replace('_', ""))
}

pub fn step_from_scale(scale: Option<&Input>, fallback: f64) -> Result<f64, JsCoercionError> {
    let numeric = safe_cex_num::js_retained_number(scale)?;
    if numeric.is_finite() && numeric.fract() == 0.0 && numeric >= 0.0 {
        let precision = parse_precision(Some(&js_number_string(numeric))).map_err(|message| {
            JsCoercionError {
                name: "RangeError",
                message,
            }
        })?;
        return to_number(
            precision
                .as_ref()
                .map(|s| Value::String(s.clone()))
                .as_ref(),
            Some(fallback),
        );
    }
    Ok(fallback)
}

pub fn get_contract_size(market: Option<&Market>) -> f64 {
    market
        .and_then(|m| m.contract_size)
        .filter(|n| n.is_finite() && *n > 0.0)
        .unwrap_or(1.0)
}

pub fn native_volume_to_amount(
    value: Option<&Value>,
    market: Option<&Market>,
) -> Result<f64, JsCoercionError> {
    Ok(to_number(value, None)? * get_contract_size(market))
}

pub fn amount_to_native_volume(amount: f64, market: Option<&Market>) -> f64 {
    amount / get_contract_size(market)
}

pub fn normalize_market(raw: &Input) -> Result<Market, JsCoercionError> {
    let id = safe_cex_num::js_retained_string(raw.get("symbol"))?;
    let base = retained_pick(raw, &["baseCoin"])
        .map(|v| safe_cex_num::js_retained_string(Some(v)))
        .transpose()?
        .unwrap_or_else(|| id.split('_').next().unwrap_or("").into());
    let quote = retained_pick(raw, &["quoteCoin", "settleCoin"])
        .map(|v| safe_cex_num::js_retained_string(Some(v)))
        .transpose()?
        .unwrap_or_else(|| id.split('_').nth(1).unwrap_or("USDT").into());
    let contract_size = retained_number(raw.get("contractSize"), Some(1.0))?;
    let vol_unit = retained_number(raw.get("volUnit"), Some(1.0))?;
    let amount_step = math_max(contract_size * vol_unit, contract_size);
    let price_step = retained_number(
        raw.get("priceUnit"),
        Some(step_from_scale(raw.get("priceScale"), 0.00000001)?),
    )?;
    let valid_contract_size = if contract_size > 0.0 {
        contract_size
    } else {
        1.0
    };
    let min = math_max(
        retained_number(raw.get("minVol"), None)? * valid_contract_size,
        amount_step,
    );
    let max = match retained_pick(raw, &["maxVol", "limitMaxVol"]) {
        Some(value) => retained_number(Some(value), None)?,
        None => 9_007_199_254_740_991.0,
    } * valid_contract_size;
    Ok(Market {
        symbol: id.replace('_', ""),
        pretty_symbol: Some(id.clone()),
        id,
        base,
        quote,
        active: raw
            .get("state")
            .is_none_or(|value| matches!(value, Input::Undefined))
            || retained_number(raw.get("state"), None)? == 0.0,
        precision: MarketPrecision {
            amount: amount_step,
            price: price_step,
        },
        limits: MarketLimits {
            amount: MarketLimitsAmount { min, max },
            leverage: MarketLimitsLeverage {
                min: retained_number(raw.get("minLeverage"), Some(1.0))?,
                max: retained_number(
                    retained_pick(raw, &["maxLeverage", "regularMaxLeverage"]),
                    Some(1.0),
                )?,
            },
            price: None,
        },
        contract_size: Some(contract_size),
        maker_fee: Some(retained_number(raw.get("makerFeeRate"), None)?),
        taker_fee: Some(retained_number(raw.get("takerFeeRate"), None)?),
        info: Some(raw.clone()),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        option: None,
    })
}

pub fn is_tradable_mexc_market(raw: &Input) -> Result<bool, JsCoercionError> {
    let coin = safe_cex_num::js_retained_string(
        retained_pick(raw, &["settleCoin", "quoteCoin"]).or(Some(&Input::Text(Vec::new()))),
    )?
    .to_uppercase();
    Ok(matches!(coin.as_str(), "USDT" | "USDC")
        && raw.get("apiAllowed") != Some(&Input::Bool(false))
        && raw.get("isHidden") != Some(&Input::Bool(true))
        && (raw
            .get("state")
            .is_none_or(|value| matches!(value, Input::Undefined))
            || retained_number(raw.get("state"), None)? == 0.0))
}

pub fn normalize_percentage(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    let parsed = to_number(value, None)?;
    Ok(if parsed.abs() <= 1.0 {
        parsed * 100.0
    } else {
        parsed
    })
}

pub fn normalize_orderbook_side(
    rows: Option<&Value>,
    market: &Market,
) -> Result<Vec<OrderBookOrders>, JsCoercionError> {
    let Some(rows) = rows.and_then(Value::as_array) else {
        return Ok(vec![]);
    };
    let mut levels = vec![];
    for row in rows {
        let (price, amount) = match row {
            Value::Array(row) => (row.first(), row.get(1)),
            Value::Object(row) => (
                pick(row, &["p", "price"]),
                pick(row, &["v", "vol", "amount"]),
            ),
            _ => (None, None),
        };
        let price = to_number(price, None)?;
        let amount = native_volume_to_amount(amount, Some(market))?;
        if price > 0.0 && amount >= 0.0 {
            levels.push(OrderBookOrders {
                price,
                amount,
                total: 0.0,
            });
        }
    }
    Ok(levels)
}

pub fn normalize_order_book(
    raw: &Map<String, Value>,
    market: &Market,
) -> Result<OrderBook, JsCoercionError> {
    let data = raw
        .get("data")
        .filter(|v| js_truthy(Some(v)))
        .and_then(Value::as_object);
    // Truthy primitive data has no bids/asks properties; null/false use the outer row.
    let empty = Map::new();
    let data = if raw.get("data").is_some_and(|v| js_truthy(Some(v))) {
        data.unwrap_or(&empty)
    } else {
        raw
    };
    let mut book = OrderBook {
        bids: normalize_orderbook_side(pick(data, &["bids", "bid"]), market)?,
        asks: normalize_orderbook_side(pick(data, &["asks", "ask"]), market)?,
    };
    sort_order_book(&mut book);
    calc_order_book_total(&mut book);
    Ok(book)
}

pub fn normalize_position_side(value: Option<&Value>) -> Result<PositionSide, JsCoercionError> {
    let parsed = to_number(value, Some(f64::NAN))?;
    if parsed.is_finite() {
        return Ok(if parsed == 2.0 {
            PositionSide::Short
        } else {
            PositionSide::Long
        });
    }
    let normalized = if js_truthy(value) {
        js_string(value)?.to_lowercase()
    } else {
        String::new()
    };
    Ok(if normalized.contains("short") || normalized == "sell" {
        PositionSide::Short
    } else {
        PositionSide::Long
    })
}

pub fn normalize_position_mode(value: Option<&Value>) -> Result<PositionMode, JsCoercionError> {
    let parsed = to_number(value, Some(f64::NAN))?;
    if parsed.is_finite() {
        return Ok(if parsed == 2.0 {
            PositionMode::OneWay
        } else {
            PositionMode::Hedged
        });
    }
    let normalized = if js_truthy(value) {
        js_string(value)?.to_lowercase()
    } else {
        String::new()
    };
    Ok(if normalized.contains("one") {
        PositionMode::OneWay
    } else {
        PositionMode::Hedged
    })
}

pub fn normalize_margin_type(value: Option<&Value>) -> Result<MarginType, JsCoercionError> {
    let value = value
        .cloned()
        .map(safe_cex_types::retained_json::RetainedJsonInput::from);
    normalize_retained_margin_type(value.as_ref())
}

pub(crate) fn normalize_retained_margin_type(
    value: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
) -> Result<MarginType, JsCoercionError> {
    use safe_cex_exchange::js_runtime::value::Js;
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    let parsed = safe_cex_num::js_retained_parse_float(value)?;
    if parsed.is_finite() {
        return Ok(if parsed == 1.0 {
            MarginType::Isolated
        } else {
            MarginType::Cross
        });
    }
    let units = if value.is_some_and(Input::is_truthy) {
        safe_cex_types::js_coercion::js_retained_string_utf16(value.unwrap())?
    } else {
        Vec::new()
    };
    let normalized = Js::utf16(&units)
        .call_method("toLowerCase", vec![])
        .expect("primitive String lowercase cannot throw");
    let includes = |needle| {
        normalized
            .call_method("includes", vec![Js::str(needle)])
            .expect("primitive String includes cannot throw")
            .truthy()
    };
    Ok(if includes("isolated") || includes("isolate") {
        MarginType::Isolated
    } else {
        MarginType::Cross
    })
}

pub fn normalize_plan_order_type(
    raw: &Map<String, Value>,
    side: OrderSide,
) -> Result<OrderType, JsCoercionError> {
    let trigger = to_number(raw.get("triggerType"), Some(0.0))?;
    Ok(
        if trigger == if side == OrderSide::Sell { 1.0 } else { 2.0 } {
            OrderType::TakeProfit
        } else {
            OrderType::StopLoss
        },
    )
}

pub fn normalize_trigger_order_status(
    raw: &Map<String, Value>,
) -> Result<OrderStatus, JsCoercionError> {
    let state = to_number(pick(raw, &["state", "status"]), Some(f64::NAN))?;
    Ok(if state == 3.0 {
        OrderStatus::Closed
    } else if matches!(state, 2.0 | 4.0 | 5.0) {
        OrderStatus::Canceled
    } else {
        OrderStatus::Open
    })
}

pub fn normalize_stop_order_type(raw: &Map<String, Value>) -> Result<OrderType, JsCoercionError> {
    let side = to_number(raw.get("triggerSide"), Some(0.0))?;
    Ok(if side == 1.0 {
        OrderType::TakeProfit
    } else if side == 2.0 {
        OrderType::StopLoss
    } else if to_number(raw.get("takeProfitPrice"), Some(0.0))? > 0.0 {
        OrderType::TakeProfit
    } else {
        OrderType::StopLoss
    })
}

pub fn normalize_balance(raw_rows: &Value) -> Result<safe_cex_types::Balance, JsCoercionError> {
    fn row_balance(row: &Map<String, Value>) -> Result<safe_cex_types::Balance, JsCoercionError> {
        let free = to_number(pick(row, &["availableBalance", "available", "free"]), None)?;
        let total = to_number(
            pick(row, &["equity", "cashBalance", "balance", "total"]),
            Some(free),
        )?;
        let upnl = to_number(
            pick(row, &["unrealized", "unrealizedPnl", "unRealizedProfit"]),
            None,
        )?;
        let frozen = to_number(pick(row, &["frozenBalance", "frozen"]), None)?;
        Ok(safe_cex_types::Balance {
            free,
            total,
            upnl,
            used: math_max(math_max(total - free, frozen), 0.0),
            info: Some(Value::Object(row.clone()).into()),
            currencies: None,
            coins: None,
        })
    }
    let rows = raw_rows.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let empty = Map::new();
    // Read each row only when JS reaches it; prevalidating the frame changes
    // which coercion/property-access error wins during find and reduce.
    let record = |row: &Value| {
        if row.is_null() {
            Err(JsCoercionError {
                name: "TypeError",
                message: "Cannot read properties of null (reading 'currency')",
            })
        } else {
            Ok(())
        }
    };
    let mut primary = None;
    let mut first_supported = None;
    for raw in rows {
        record(raw)?;
        let row = raw.as_object().unwrap_or(&empty);
        let currency = to_integer_string(pick(row, &["currency", "asset"]))?.to_uppercase();
        if matches!(currency.as_str(), "USDT" | "USDC") {
            first_supported.get_or_insert(row);
            let free = to_number(pick(row, &["availableBalance", "available", "free"]), None)?;
            let total = to_number(
                pick(row, &["equity", "cashBalance", "balance", "total"]),
                Some(free),
            )?;
            if free != 0.0 || total != 0.0 {
                primary = Some(row);
                break;
            }
        }
    }
    let mut balance = row_balance(
        primary
            .or(first_supported)
            .or_else(|| rows.first().and_then(Value::as_object))
            .unwrap_or(&empty),
    )?;
    let mut currencies = safe_cex_types::object_map::JsObjectMap::new();
    for raw in rows {
        record(raw)?;
        let row = raw.as_object().unwrap_or(&empty);
        let currency = to_integer_string(pick(row, &["currency", "asset"]))?;
        if !currency.is_empty() {
            let currency_balance = row_balance(row)?;
            // ts-bug: normalizeBalance __proto__ assignment changes the prototype.
            // Only own-key omission is mirrored; inherited reads are a routed
            // shared dictionary-write FINDING, witnessed before serialization.
            if currency != "__proto__" {
                currencies.insert(
                    currency,
                    safe_cex_types::CurrencyBalance::from(currency_balance),
                );
            }
        }
    }
    balance.info = Some(Value::Array(rows.to_vec()).into());
    balance.currencies = Some(currencies);
    Ok(balance)
}

#[derive(Clone, Debug)]
pub enum MexcLeverage {
    Number(f64),
    Rails {
        long: Option<f64>,
        short: Option<f64>,
    },
}

pub fn fake_positions_for_market(
    market: &Market,
    position_mode: Option<PositionMode>,
    margin_type: Option<MarginType>,
    leverage: Option<&MexcLeverage>,
) -> Vec<safe_cex_types::Position> {
    let mode = position_mode.unwrap_or(PositionMode::Hedged);
    let margin = margin_type.unwrap_or(MarginType::Cross);
    let sides: &[PositionSide] = if mode == PositionMode::OneWay {
        &[PositionSide::Long]
    } else {
        &[PositionSide::Long, PositionSide::Short]
    };
    sides
        .iter()
        .map(|side| {
            let leverage = match leverage {
                None => 20.0,
                Some(MexcLeverage::Number(n)) => *n,
                Some(MexcLeverage::Rails { long, short }) => {
                    if *side == PositionSide::Long {
                        long.unwrap_or(20.0)
                    } else {
                        short.unwrap_or(20.0)
                    }
                }
            };
            fake_position(market, *side, mode, margin, leverage)
        })
        .collect()
}

fn fake_position(
    market: &Market,
    side: PositionSide,
    mode: PositionMode,
    margin: MarginType,
    leverage: f64,
) -> safe_cex_types::Position {
    safe_cex_types::Position {
        id: safe_cex_types::model_utils::create_position_id(&market.symbol, Some(side), Some(mode)),
        symbol: market.symbol.clone(),
        side: side.into(),
        entry_price: 0.0,
        notional: 0.0,
        leverage,
        margin_type: (Some(margin)).into(),
        position_mode: (Some(mode)).into(),
        unrealized_pnl: 0.0,
        realized_pnl: (Some(0.0)).into(),
        contracts: 0.0,
        liquidation_price: 0.0,
        margin_size: (Some(0.0)).into(),
        info: (Some(Value::Object(Map::new()).into())).into(),
        extra: (Some(safe_cex_types::PositionExtra {
            contracts_native: Some(0.0),
            unrealized_pnl_native: Some(0.0),
        }))
        .into(),
        last_update: (Some(0.0)).into(),
        option: (None).into(),
    }
}

/// The host supplies its local-wall-time to UTC conversion, including DST.
/// Numeric positive timestamps deliberately bypass Date's TimeClip.
// ts: normalizeTimestampToMs
pub fn normalize_timestamp_to_ms(
    value: Option<&Value>,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<f64, JsCoercionError> {
    if value.is_none_or(|v| v.is_null() || v.as_str() == Some("")) {
        return Ok(0.0);
    }
    let numeric = js_number(value)?;
    if numeric.is_finite() && numeric > 0.0 {
        return Ok(if numeric >= 1_000_000_000_000.0 {
            numeric.floor()
        } else {
            (numeric * 1000.0).floor()
        });
    }
    let parsed = safe_cex_num::js_date_parse(&js_string(value)?, local_to_utc);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

// ts: normalizeTimestampToSeconds
pub fn normalize_timestamp_to_seconds(
    value: Option<&Value>,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<f64, JsCoercionError> {
    Ok((normalize_timestamp_to_ms(value, local_to_utc)? / 1000.0).floor())
}

// ts: normalizeTicker
pub fn normalize_ticker(
    raw: &Map<String, Value>,
    market: &Market,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<safe_cex_types::Ticker, JsCoercionError> {
    let last = to_number(pick(raw, &["lastPrice", "last"]), None)?;
    let mark = to_number(pick(raw, &["fairPrice", "markPrice"]), Some(last))?;
    let open_interest_base =
        native_volume_to_amount(pick(raw, &["holdVol", "openInterest"]), Some(market))?;
    Ok(safe_cex_types::Ticker {
        id: market.id.clone(),
        symbol: market.symbol.clone(),
        bid: to_number(pick(raw, &["bid1", "bidPrice"]), Some(last))?,
        ask: to_number(pick(raw, &["ask1", "askPrice"]), Some(last))?,
        last,
        mark,
        index: to_number(raw.get("indexPrice"), Some(last))?,
        mid_price: None,
        timestamp: None,
        percentage: normalize_percentage(pick(raw, &["riseFallRate", "priceChangePercent"]))?,
        open_interest: open_interest_base
            * if mark != 0.0 && !mark.is_nan() {
                mark
            } else {
                last
            },
        funding_rate: to_number(raw.get("fundingRate"), None)?,
        funding_time: normalize_timestamp_to_ms(
            pick(raw, &["fundingTime", "nextSettleTime"]),
            local_to_utc,
        )?,
        volume: native_volume_to_amount(pick(raw, &["volume24", "volume"]), Some(market))?,
        quote_volume: to_number(pick(raw, &["amount24", "quoteVolume"]), None)?,
        info: Some(Value::Object(raw.clone()).into()),
    })
}

// ts: normalizeCandle
pub fn normalize_candle(
    raw: &Value,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<Option<safe_cex_types::Candle>, JsCoercionError> {
    if raw.is_null() {
        return Err(JsCoercionError {
            name: "TypeError",
            message: "Cannot read properties of null (reading 'time')",
        });
    }
    let empty = Map::new();
    let raw = raw.as_object().unwrap_or(&empty);
    let timestamp =
        normalize_timestamp_to_seconds(pick(raw, &["time", "t", "timestamp"]), local_to_utc)?;
    if timestamp == 0.0 || timestamp.is_nan() {
        return Ok(None);
    }
    Ok(Some(safe_cex_types::Candle {
        timestamp,
        open: to_number(pick(raw, &["open", "o"]), None)?,
        high: to_number(pick(raw, &["high", "h"]), None)?,
        low: to_number(pick(raw, &["low", "l"]), None)?,
        close: to_number(pick(raw, &["close", "c"]), None)?,
        volume: to_number(pick(raw, &["vol", "volume", "q", "amount"]), None)?,
        last_side: None,
    }))
}

// These indexed values flow only into toNumber. A transient UTF-16 surrogate
// has parseFloat NaN and therefore becomes the same numeric fallback zero.
// Preserve that result without rejecting an otherwise valid scalar string.
fn kline_index(value: Option<&Value>, index: usize) -> Option<Value> {
    match value {
        Some(Value::Array(a)) => a.get(index).cloned(),
        Some(Value::Object(o)) => o.get(&index.to_string()).cloned(),
        Some(Value::String(s)) => s.encode_utf16().nth(index).map(|unit| {
            char::from_u32(u32::from(unit))
                .map_or_else(|| Value::from(0), |c| Value::String(c.to_string()))
        }),
        _ => None,
    }
}

// ts: normalizeKlineResponse
pub fn normalize_kline_response(
    payload: &Value,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<Vec<safe_cex_types::Candle>, JsCoercionError> {
    let data = payload
        .get("data")
        .filter(|v| !v.is_null())
        .unwrap_or(payload);
    let mut result = Vec::new();
    if let Some(rows) = data.as_array() {
        for row in rows {
            if let Some(candle) = normalize_candle(row, local_to_utc)? {
                result.push(candle);
            }
        }
    } else if let Some(times) = data.get("time").and_then(Value::as_array) {
        for (index, time) in times.iter().enumerate() {
            let mut row = Map::new();
            row.insert("time".into(), time.clone());
            for key in ["open", "high", "low", "close"] {
                if let Some(value) = kline_index(data.get(key), index) {
                    row.insert(key.into(), value);
                }
            }
            let vol = kline_index(data.get("vol"), index);
            let vol = if vol.as_ref().is_none_or(Value::is_null) {
                kline_index(data.get("amount"), index)
            } else {
                vol
            };
            if let Some(vol) = vol {
                row.insert("vol".into(), vol);
            }
            if let Some(candle) = normalize_candle(&Value::Object(row), local_to_utc)? {
                result.push(candle);
            }
        }
    }
    Ok(result)
}

// Numeric coercion makes Object.prototype names unreachable at all mapper tables.
fn numeric_table<T: serde::de::DeserializeOwned>(
    table: &Value,
    key: f64,
) -> Result<Option<T>, JsCoercionError> {
    table
        .get(js_number_string(key))
        .map(|value| {
            serde_json::from_value(value.clone()).map_err(|_| JsCoercionError {
                name: "TypeError",
                message: "Invalid MEXC protocol table value",
            })
        })
        .transpose()
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderSideInfo {
    side: OrderSide,
    position_side: PositionSide,
    reduce_only: bool,
}

// ts: normalizePosition
pub fn normalize_position(
    raw: &Map<String, Value>,
    market: Option<&Market>,
    account_mode: Option<PositionMode>,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<Option<safe_cex_types::Position>, JsCoercionError> {
    let symbol = from_mexc_symbol(pick(raw, &["symbol", "contract"]))?;
    if symbol.is_empty() {
        return Ok(None);
    }
    let side = normalize_position_side(pick(raw, &["positionType", "positionSide", "side"]))?;
    let mode = if js_truthy(raw.get("positionMode")) {
        normalize_position_mode(raw.get("positionMode"))?
    } else {
        account_mode.unwrap_or(PositionMode::Hedged)
    };
    let volume = pick(raw, &["holdVol", "positionVol", "vol", "size"]);
    let contracts = native_volume_to_amount(volume, market)?;
    let entry_price = to_number(
        pick(
            raw,
            &["holdAvgPrice", "openAvgPrice", "entryPrice", "avgPrice"],
        ),
        None,
    )?;
    let unrealized_pnl = to_number(
        pick(raw, &["unrealized", "unrealizedPnl", "unRealizedProfit"]),
        None,
    )?;
    let notional = contracts.abs() * entry_price;
    let minimum = market
        .map(|m| m.limits.leverage.min)
        .filter(|n| *n != 0.0 && !n.is_nan())
        .unwrap_or(1.0);
    let leverage = to_number(raw.get("leverage"), Some(minimum))?;
    Ok(Some(safe_cex_types::Position {
        id: safe_cex_types::model_utils::create_position_id(&symbol, Some(side), Some(mode)),
        symbol,
        side: side.into(),
        entry_price,
        notional,
        leverage,
        margin_type: (Some(normalize_margin_type(pick(
            raw,
            &["openType", "marginType"],
        ))?))
        .into(),
        position_mode: (Some(mode)).into(),
        unrealized_pnl,
        realized_pnl: (Some(to_number(pick(raw, &["realised", "realizedPnl"]), None)?)).into(),
        contracts,
        liquidation_price: to_number(pick(raw, &["liquidatePrice", "liquidationPrice"]), None)?,
        margin_size: (Some(to_number(
            pick(raw, &["im", "margin", "positionMargin"]),
            Some(
                if notional != 0.0 && !notional.is_nan() && leverage != 0.0 && !leverage.is_nan() {
                    notional / leverage
                } else {
                    0.0
                },
            ),
        )?))
        .into(),
        info: (Some(Value::Object(raw.clone()).into())).into(),
        extra: (Some(safe_cex_types::PositionExtra {
            contracts_native: Some(to_number(volume, None)?),
            unrealized_pnl_native: Some(unrealized_pnl),
        }))
        .into(),
        last_update: (Some(normalize_timestamp_to_ms(
            pick(raw, &["updateTime", "modifyTime", "createTime"]),
            local_to_utc,
        )?))
        .into(),
        option: (None).into(),
    }))
}

fn empty_order(id: String, symbol: String, side: OrderSide) -> safe_cex_types::Order {
    safe_cex_types::Order {
        client_order_id: (None).into(),
        id,
        parent_id: (None).into(),
        status: safe_cex_types::ModelField::Value(OrderStatus::Open),
        symbol,
        r#type: safe_cex_types::ModelField::Value(OrderType::Limit),
        side: side.into(),
        price: (0.0).into(),
        amount: 0.0,
        filled: 0.0,
        remaining: 0.0,
        reduce_only: false.into(),
        close: (None).into(),
        info: (None).into(),
        time_in_force: (Some(safe_cex_types::OrderTimeInForce::GoodTillCancel)).into(),
        last_update: 0.0,
        trigger_type: (None).into(),
        tp_trigger_type: (None).into(),
        sl_trigger_type: (None).into(),
        take_profit: (None).into(),
        stop_loss: (None).into(),
        extra: (None).into(),
    }
}
fn order_extra(remaining: f64, filled: Option<f64>) -> safe_cex_types::OrderExtra {
    safe_cex_types::OrderExtra {
        remaining_native: Some(remaining),
        filled_in_placement_units: filled,
        remaining_notional: None,
        conditional_order_price: None,
        reject_reason: None,
        is_from_chaser: None,
        chaser_id: None,
        chaser_attempts: None,
        chaser_max_attempts: None,
    }
}

// ts: normalizeOrder
pub fn normalize_order(
    raw: &Map<String, Value>,
    market: Option<&Market>,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<Option<safe_cex_types::Order>, JsCoercionError> {
    let id = to_integer_string(pick(raw, &["orderId", "id"]))?;
    let symbol = from_mexc_symbol(pick(raw, &["symbol", "contract"]))?;
    let side_info: Option<OrderSideInfo> = numeric_table(
        &crate::types::ORDER_SIDE,
        to_number(raw.get("side"), Some(f64::NAN))?,
    )?;
    let Some(side_info) = side_info.filter(|_| !id.is_empty() && !symbol.is_empty()) else {
        return Ok(None);
    };
    let type_code = to_number(pick(raw, &["type", "orderType"]), Some(1.0))?;
    let amount_native = to_number(pick(raw, &["vol", "quantity", "origQty"]), None)?;
    // TypeScript re-coerces the numeric intermediate, canonicalizing -0.
    let amount = native_volume_to_amount(Some(&Value::from(amount_native)), market)?;
    let filled_native = to_number(pick(raw, &["dealVol", "filledQty", "executedQty"]), None)?;
    let filled = native_volume_to_amount(Some(&Value::from(filled_native)), market)?;
    let status = numeric_table(
        &crate::types::ORDER_STATUS,
        to_number(pick(raw, &["state", "status"]), Some(f64::NAN))?,
    )?
    .unwrap_or(OrderStatus::Open);
    let order_type =
        numeric_table(&crate::types::ORDER_TYPE, type_code)?.unwrap_or(OrderType::Limit);
    let price = to_number(pick(raw, &["price", "orderPrice"]), None)?;
    let trigger_price = to_number(pick(raw, &["triggerPrice", "stopPrice"]), None)?;
    let mut order = empty_order(id, symbol, side_info.side);
    order.status = (status).into();
    order.r#type = (order_type).into();
    order.price = if price != 0.0 {
        (price).into()
    } else {
        (trigger_price).into()
    };
    order.amount = amount;
    order.filled = filled;
    order.remaining = math_max(amount - filled, 0.0);
    order.reduce_only = (js_truthy(raw.get("reduceOnly")) || side_info.reduce_only).into();
    order.time_in_force = (Some(
        numeric_table(&crate::types::ORDER_TIME_IN_FORCE, type_code)?
            .unwrap_or(safe_cex_types::OrderTimeInForce::GoodTillCancel),
    ))
    .into();
    order.last_update = normalize_timestamp_to_ms(
        pick(raw, &["updateTime", "modifyTime", "createTime"]),
        local_to_utc,
    )?;
    order.info = (Some(Value::Object(raw.clone()).into())).into();
    order.extra = (Some(order_extra(
        math_max(amount_native - filled_native, 0.0),
        Some(filled_native),
    )))
    .into();
    Ok(Some(order))
}

// ts: normalizePlanOrder
pub fn normalize_plan_order(
    raw: &Map<String, Value>,
    market: Option<&Market>,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<Option<safe_cex_types::Order>, JsCoercionError> {
    let id = to_integer_string(pick(raw, &["id", "orderId"]))?;
    let symbol = from_mexc_symbol(pick(raw, &["symbol", "contract"]))?;
    let side_info: Option<OrderSideInfo> = numeric_table(
        &crate::types::ORDER_SIDE,
        to_number(raw.get("side"), Some(f64::NAN))?,
    )?;
    let Some(side_info) = side_info.filter(|_| !id.is_empty() && !symbol.is_empty()) else {
        return Ok(None);
    };
    let volume = pick(raw, &["vol", "quantity", "origQty"]);
    let amount = native_volume_to_amount(volume, market)?;
    let status = normalize_trigger_order_status(raw)?;
    let price = to_number(pick(raw, &["triggerPrice", "stopPrice", "price"]), None)?;
    let mut order = empty_order(id, symbol, side_info.side);
    order.status = (status).into();
    order.r#type = (normalize_plan_order_type(raw, side_info.side)?).into();
    order.price = (price).into();
    order.amount = amount;
    order.remaining = amount;
    order.reduce_only = (js_truthy(raw.get("reduceOnly")) || side_info.reduce_only).into();
    order.last_update =
        normalize_timestamp_to_ms(pick(raw, &["updateTime", "createTime"]), local_to_utc)?;
    let mut info = raw.clone();
    info.insert("__mexcPlanOrder".into(), Value::Bool(true));
    order.info = (Some(Value::Object(info).into())).into();
    order.extra = (Some(order_extra(to_number(volume, None)?, None))).into();
    Ok(Some(order))
}

// ts: normalizeStopOrder
pub fn normalize_stop_order(
    raw: &Map<String, Value>,
    market: Option<&Market>,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<Option<safe_cex_types::Order>, JsCoercionError> {
    let id = to_integer_string(pick(raw, &["id", "stopPlanOrderId"]))?;
    let symbol = from_mexc_symbol(pick(raw, &["symbol", "contract"]))?;
    let side: Option<PositionSide> = numeric_table(
        &crate::types::POSITION_SIDE,
        to_number(pick(raw, &["positionType", "positionSide"]), Some(f64::NAN))?,
    )?;
    let Some(side) = side.filter(|_| !id.is_empty() && !symbol.is_empty()) else {
        return Ok(None);
    };
    let order_type = normalize_stop_order_type(raw)?;
    let full_position = to_number(raw.get("volType"), Some(1.0))? == 2.0;
    let native_amount = to_number(pick(raw, &["vol", "quantity", "origQty"]), None)?;
    let amount = if full_position {
        0.0
    } else {
        native_volume_to_amount(Some(&Value::from(native_amount)), market)?
    };
    let price = to_number(
        pick(
            raw,
            if order_type == OrderType::TakeProfit {
                &["takeProfitPrice", "triggerPrice", "price"]
            } else {
                &["stopLossPrice", "triggerPrice", "price"]
            },
        ),
        None,
    )?;
    let mut order = empty_order(
        id,
        symbol,
        if side == PositionSide::Long {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        },
    );
    order.status = (normalize_trigger_order_status(raw)?).into();
    order.r#type = (order_type).into();
    order.price = (price).into();
    order.amount = amount;
    order.remaining = amount;
    order.reduce_only = true.into();
    order.last_update =
        normalize_timestamp_to_ms(pick(raw, &["updateTime", "createTime"]), local_to_utc)?;
    let mut info = raw.clone();
    info.insert("__mexcStopOrder".into(), Value::Bool(true));
    order.info = (Some(Value::Object(info).into())).into();
    order.extra = (Some(order_extra(
        if full_position { 0.0 } else { native_amount },
        None,
    )))
    .into();
    Ok(Some(order))
}

pub(crate) const EXECUTION_ASSET_BOUNDARY: JsCoercionError = JsCoercionError {
    name: "TypeError",
    message: "MEXC commissionAsset is outside the shared Execution string boundary",
};

// ts: normalizeExecution
pub fn normalize_execution(
    raw: &Map<String, Value>,
    market: Option<&Market>,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<Option<safe_cex_types::Execution>, JsCoercionError> {
    normalize_execution_record(raw, market, local_to_utc, true)
}

// handleFill projects no commissionAsset. Source accepts arbitrary asset values,
// so the closed Execution output refusal must not reject this smaller event.
pub(crate) fn normalize_execution_for_fill(
    raw: &Map<String, Value>,
    market: Option<&Market>,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<Option<safe_cex_types::Execution>, JsCoercionError> {
    normalize_execution_record(raw, market, local_to_utc, false)
}
fn normalize_execution_record(
    raw: &Map<String, Value>,
    market: Option<&Market>,
    local_to_utc: &dyn Fn(f64) -> f64,
    include_commission_asset: bool,
) -> Result<Option<safe_cex_types::Execution>, JsCoercionError> {
    let symbol = from_mexc_symbol(pick(raw, &["symbol", "contract"]))?;
    if symbol.is_empty() {
        return Ok(None);
    }
    let side_info: Option<OrderSideInfo> = numeric_table(
        &crate::types::ORDER_SIDE,
        to_number(raw.get("side"), Some(f64::NAN))?,
    )?;
    let qty = native_volume_to_amount(pick(raw, &["vol", "quantity", "qty"]), market)?;
    if qty <= 0.0 {
        return Ok(None);
    }
    let id = to_integer_string(pick(raw, &["tradeId", "id"]))?;
    let timestamp = normalize_timestamp_to_seconds(
        pick(raw, &["createTime", "time", "tradeTime"]),
        local_to_utc,
    )?;
    let order_id = to_integer_string(raw.get("orderId"))?;
    let price = to_number(raw.get("price"), None)?;
    let side = if let Some(info) = &side_info {
        info.side
    } else {
        let taker = raw.get("taker").filter(|v| js_truthy(Some(v)));
        if taker
            .map(|v| js_string(Some(v)))
            .transpose()?
            .unwrap_or_default()
            .to_lowercase()
            == "sell"
        {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        }
    };
    let commission = to_number(raw.get("fee"), None)?.abs();
    // ts-bug: normalizeExecution retains non-string commissionAsset values.
    // CHOSEN closed-typed-output-boundary: refuse the whole record, with
    // explicit live witnesses, rather than stringify or erase those values.
    let asset = raw
        .get("feeCurrency")
        .filter(|v| !v.is_null())
        .or_else(|| raw.get("feeAsset"));
    // Evaluate later source fields before applying the public typed refusal.
    let commission_asset = if include_commission_asset {
        asset
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or(EXECUTION_ASSET_BOUNDARY)
            })
            .transpose()
    } else {
        Ok(None)
    };
    let realized_pnl = to_number(pick(raw, &["profit", "realizedPnl"]), None)?;
    let maker = raw
        .get("role")
        .map(|v| to_number(Some(v), None).map(|n| n == 1.0))
        .transpose()?;
    let commission_asset = commission_asset?;
    Ok(Some(safe_cex_types::Execution {
        id: Some(id),
        timestamp,
        symbol,
        order_id: Some(order_id),
        price,
        qty,
        side,
        commission: Some(commission),
        commission_asset,
        position_side: side_info
            .map(|info| safe_cex_types::ExecutionPositionSide::Side(info.position_side)),
        realized_pnl: Some(realized_pnl),
        maker,
        venue_reported_revenue: None,
        venue_reported_revenue_asset: None,
    }))
}
