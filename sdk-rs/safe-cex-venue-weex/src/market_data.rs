//! WEEX market and ticker projections; no Store mutation or notification.
use crate::utils::{WeexUtilityError, WeexUtilityInput, normalize_percentage, to_number};
use safe_cex_num::{js_number_string, js_string, parse_int, parse_precision};
use safe_cex_types::{
    Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision, Ticker,
};
use serde_json::{Map, Value};

#[derive(Debug)]
pub(crate) struct MarketProjectionRefusal {
    pub error: WeexUtilityError,
    pub value: Option<Value>,
}

pub fn normalize_market(raw: &WeexUtilityInput) -> Result<Market, WeexUtilityError> {
    normalize_market_row(raw)?.map_err(|refusal| refusal.error)
}

// Finish source coercions before refusing a closed output field. The caller can
// discard only this row; source failures still reach fetchMarkets' outer catch.
pub(crate) fn normalize_market_row(
    raw: &WeexUtilityInput,
) -> Result<Result<Market, MarketProjectionRefusal>, WeexUtilityError> {
    let value = raw.as_value();
    if value.is_null() {
        return Err(WeexUtilityError {
            name: "TypeError",
            message: "Cannot read properties of null (reading 'pricePrecision')".to_owned(),
        });
    }
    // ccxt-compat's parsePrecision uses radix inference and emits "0.1" for
    // negative/NaN counts, unlike safe-cex's parsePrecision. Adapt its argument
    // before delegating string materialization to the shared helper.
    let step = |keys: &[&str]| -> Result<f64, WeexUtilityError> {
        let input = raw.pick_retained(keys);
        let text = match input.as_ref() {
            Some(input) => js_string(Some(input.as_value()))?,
            None => "8".to_owned(),
        };
        let digits = parse_int(&text, None);
        let digits = if digits.is_nan() || digits < 0.0 {
            1.0
        } else {
            digits
        };
        // Guard before Number-to-String: huge integral counts stringify in
        // exponent form, which parsePrecision would otherwise re-read as 1.
        // This is the shared materializer's existing string-length ceiling.
        if digits > 536_870_888.0 {
            return Err(WeexUtilityError {
                name: "RangeError",
                message: "Invalid string length".to_owned(),
            });
        }
        let precision = parse_precision(Some(&js_number_string(digits))).map_err(|message| {
            WeexUtilityError {
                name: "RangeError",
                message: message.to_owned(),
            }
        })?;
        Ok(precision
            .as_deref()
            .map_or(0.00000001, safe_cex_num::parse_float))
    };
    let price_step = step(&["pricePrecision", "quotePrecision"])?;
    // The nested fallback is evaluated even when minOrderSize is present.
    let quantity_step = step(&["quantityPrecision", "baseAssetPrecision"])?;
    let amount_step = to_number(
        raw.pick_retained(&["minOrderSize"]).as_ref(),
        Some(quantity_step),
    )?;
    let contract_val = to_number(raw.pick_retained(&["contractVal"]).as_ref(), Some(1.0))?;
    // Source assigns these fields directly. Do not invent String coercion;
    // malformed records are refused at this typed Market projection boundary.
    let text = |key: &str| -> Result<String, MarketProjectionRefusal> {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| MarketProjectionRefusal {
                error: WeexUtilityError {
                    name: "TypeError",
                    message: format!("Market.{key} requires a string"),
                },
                value: value.get(key).cloned(),
            })
    };
    let min_amount = to_number(
        raw.pick_retained(&["minOrderSize"]).as_ref(),
        Some(amount_step),
    )?;
    let max_amount = to_number(
        raw.pick_retained(&["maxOrderSize"]).as_ref(),
        Some(9_007_199_254_740_991.0),
    )?;
    let min_leverage = to_number(raw.pick_retained(&["minLeverage"]).as_ref(), Some(1.0))?;
    let max_leverage = to_number(raw.pick_retained(&["maxLeverage"]).as_ref(), Some(1.0))?;
    let maker_fee = to_number(raw.pick_retained(&["makerFeeRate"]).as_ref(), None)?;
    let taker_fee = to_number(raw.pick_retained(&["takerFeeRate"]).as_ref(), None)?;
    let strings = (|| Ok((text("symbol")?, text("baseAsset")?, text("quoteAsset")?)))();
    let (symbol, base, quote) = match strings {
        Ok(fields) => fields,
        Err(refusal) => return Ok(Err(refusal)),
    };
    Ok(Ok(Market {
        id: symbol.clone(),
        symbol,
        base,
        quote,
        active: value.get("forwardContractFlag") != Some(&Value::Bool(false)),
        precision: MarketPrecision {
            amount: amount_step,
            price: price_step,
        },
        limits: MarketLimits {
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
        contract_size: Some(contract_val),
        maker_fee: Some(maker_fee),
        taker_fee: Some(taker_fee),
        info: Some(value.clone().into()),
        pretty_symbol: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        option: None,
    }))
}

pub fn normalize_ticker(
    raw_ticker: Option<&WeexUtilityInput>,
    market: &Market,
    raw_book: Option<&WeexUtilityInput>,
) -> Result<Ticker, WeexUtilityError> {
    let pick = |keys: &[&str]| raw_ticker.and_then(|raw| raw.pick_retained(keys));
    let last = to_number(pick(&["lastPrice", "c", "last"]).as_ref(), None)?;
    let open = to_number(pick(&["openPrice", "o"]).as_ref(), None)?;
    let mark = to_number(pick(&["markPrice", "m"]).as_ref(), Some(last))?;
    let index = to_number(pick(&["indexPrice", "i"]).as_ref(), Some(mark))?;
    let bid = raw_book
        .and_then(|raw| raw.pick_retained(&["bidPrice"]))
        .or_else(|| pick(&["bidPrice", "b"]));
    let ask = raw_book
        .and_then(|raw| raw.pick_retained(&["askPrice"]))
        .or_else(|| pick(&["askPrice", "a"]));
    let mut info = Map::new();
    if let Some(raw) = raw_ticker {
        info.insert("ticker".to_owned(), raw.as_value().clone());
    }
    if let Some(raw) = raw_book {
        info.insert("bookTicker".to_owned(), raw.as_value().clone());
    }
    Ok(Ticker {
        id: market.id.clone(),
        symbol: market.symbol.clone(),
        bid: to_number(bid.as_ref(), Some(last))?,
        ask: to_number(ask.as_ref(), Some(last))?,
        last,
        mark,
        index,
        mid_price: None,
        percentage: normalize_percentage(pick(&["priceChangePercent", "P"]).as_ref(), last, open)?,
        open_interest: to_number(pick(&["openInterest"]).as_ref(), None)?,
        funding_rate: to_number(pick(&["fundingRate"]).as_ref(), None)?,
        funding_time: to_number(pick(&["fundingTime"]).as_ref(), None)?,
        volume: to_number(pick(&["volume", "v"]).as_ref(), None)?,
        quote_volume: to_number(pick(&["quoteVolume", "q"]).as_ref(), None)?,
        timestamp: None,
        info: Some(Value::Object(info).into()),
    })
}
