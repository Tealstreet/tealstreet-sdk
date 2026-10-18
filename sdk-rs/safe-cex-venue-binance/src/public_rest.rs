//! Extracted Futures/PM/Spot public REST bodies over decoded responses.
//! Bindings supply current HTTP/Store/error methods and the source Dayjs dependency.
//! Dense JSON data, stock prototypes, typed OHLCV options and string-keyed market
//! identities are the boundary; executable properties/custom collections and
//! concrete class/client/date-library composition remain pending.
use crate::{
    exchange_error::{ErrorEmission, ExceptionValue, emit_binance_exchange_error},
    request_weight::{Failure, type_error},
};
use safe_cex_num::{
    get_key::{GetKeyUnknownValue, v_unknown},
    js_number, js_string, js_truthy, parse_float,
};
use safe_cex_types::{
    AnyTimeframe, Candle, ExtendedTimeframe, MarketLimitsAmount, MarketPrecision, OHLCVOptions,
    SecondsTimeframe, Ticker, Timeframe,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestVenue {
    Futures,
    PM,
    Spot,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestClient {
    Public,
    Private,
    Papi,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CandleParams {
    pub symbol: String,
    pub interval: &'static str,
    pub start_time: f64,
    pub end_time: f64,
    pub limit: Option<f64>,
}
/// Retains uncoerced source identity/base/quote and leverage values (a bracket's
/// string "125" stays a string). Precision/amount limits are actual native DTOs.
#[derive(Clone, Debug, PartialEq)]
pub struct RestMarket {
    pub id: String,
    pub symbol: Option<Value>,
    pub base: Option<Value>,
    pub quote: Option<Value>,
    pub active: bool,
    pub is_spot: Option<bool>,
    pub precision: MarketPrecision,
    pub amount: MarketLimitsAmount,
    pub leverage_min: f64,
    pub leverage_max: Value,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketIdentity {
    pub id: String,
    pub symbol: String,
}
pub type RestFuture<'a> = Pin<Box<dyn Future<Output = Result<Option<Value>, Failure>> + 'a>>;
/// Rc fallback handles preserve the actual Store array identity. GET returns a
/// decoded response.data, including undefined; synchronous throws are separate
/// from future rejection. Methods are read at the source call sites, after awaits.
pub trait RestBindings {
    fn get(
        &mut self,
        client: RestClient,
        endpoint: &'static str,
        params: Option<CandleParams>,
    ) -> Result<RestFuture<'_>, Failure>;
    fn is_public_data_only(&mut self) -> Result<bool, Failure>;
    fn markets_by_symbol(&mut self) -> Result<Rc<BTreeMap<String, MarketIdentity>>, Failure>;
    fn emit(&mut self, error: ErrorEmission) -> Result<(), Failure>;
    fn fallback_markets(&mut self) -> Result<Rc<Vec<RestMarket>>, Failure>;
    fn fallback_tickers(&mut self) -> Result<Rc<Vec<Ticker>>, Failure>;
    /// Configured dayjs(value), or dayjs() for None, returning valueOf(). Preserve
    /// time clipping, local-calendar semantics and failures of the actual adapter.
    fn dayjs(&mut self, value: Option<f64>) -> Result<f64, Failure>;
    fn subtract(&mut self, end: f64, amount: f64, unit: char) -> Result<f64, Failure>;
}
fn endpoint(venue: RestVenue, key: &str) -> &'static str {
    let entries = match venue {
        RestVenue::Futures => crate::binance_types::ENDPOINTS,
        RestVenue::PM => crate::binancepm_types::ENDPOINTS,
        RestVenue::Spot => crate::binancespot_types::ENDPOINTS,
    };
    entries
        .iter()
        .find(|(k, _)| *k == key)
        .expect("generated endpoint")
        .1
}
fn response_error(error: crate::response_shape::BinanceResponseError) -> Failure {
    let mut e = safe_cex_exchange::runtime::ExchangeError::new(error.message);
    e.name = error.name.into();
    Rc::new(e)
}
fn rows<'a>(venue: RestVenue, ep: &str, value: Option<&'a Value>) -> Result<&'a [Value], Failure> {
    match venue {
        RestVenue::Futures => {
            crate::response_shape::require_binance_array_response(ep, value).map_err(response_error)
        }
        RestVenue::PM => crate::response_shape::require_binance_pm_array_response(ep, value)
            .map_err(response_error),
        RestVenue::Spot => value
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .ok_or_else(|| type_error("data.reduce is not a function")),
    }
}
fn prop<'a>(value: Option<&'a Value>, key: &str) -> Result<Option<&'a Value>, Failure> {
    if value.is_none() || value.is_some_and(Value::is_null) {
        return Err(type_error(format!(
            "Cannot read properties of {} (reading '{key}')",
            if value.is_none() { "undefined" } else { "null" }
        )));
    }
    Ok(value.and_then(|v| v.get(key)))
}
fn keyed<'a>(value: Option<&'a Value>, key: &str) -> Result<Option<&'a Value>, Failure> {
    match v_unknown(value, key).map_err(|e| type_error(e.message))? {
        GetKeyUnknownValue::Undefined => Ok(None),
        GetKeyUnknownValue::Json(v) => Ok(Some(v)),
        _ => unreachable!("fixed source data keys have no stock prototype member"),
    }
}
fn float(value: Option<&Value>) -> Result<f64, Failure> {
    Ok(parse_float(
        &js_string(value).map_err(|e| type_error(e.message))?,
    ))
}
fn vf(value: Option<&Value>, key: &str) -> Result<f64, Failure> {
    float(keyed(value, key)?)
}
fn float_or(value: Option<&Value>, fallback: f64) -> Result<f64, Failure> {
    let n = float(value)?;
    Ok(if n != 0.0 && !n.is_nan() { n } else { fallback })
}
// JSON primitive strict equality; independently decoded object/array referents
// are distinct, even if structurally equal. Shared cross-response object graphs
// cannot be represented by the decoded HTTP JSON contract.
fn strict_eq(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(Value::Null), Some(Value::Null)) => true,
        (Some(Value::Bool(a)), Some(Value::Bool(b))) => a == b,
        (Some(Value::Number(a)), Some(Value::Number(b))) => a.as_f64() == b.as_f64(),
        (Some(Value::String(a)), Some(Value::String(b))) => a == b,
        (Some(a), Some(b)) if a.is_object() || a.is_array() => std::ptr::eq(a, b),
        _ => false,
    }
}
fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b && a == 0.0 {
        if a.is_sign_positive() || b.is_sign_positive() {
            0.0
        } else {
            -0.0
        }
    } else {
        a.max(b)
    }
}
fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b && a == 0.0 {
        if a.is_sign_negative() || b.is_sign_negative() {
            -0.0
        } else {
            0.0
        }
    } else {
        a.min(b)
    }
}
fn catch(
    bindings: &mut impl RestBindings,
    venue: RestVenue,
    error: Failure,
) -> Result<(), Failure> {
    let properties = json!({"message":error.message,"response":error.response_json_view()});
    if venue == RestVenue::Futures {
        emit_binance_exchange_error(
            &ExceptionValue::Error {
                original: error,
                properties,
            },
            |e| bindings.emit(e),
        )
    } else {
        let response_message = properties
            .get("response")
            .and_then(|v| v.get("data"))
            .and_then(|v| v.get("msg"));
        let message = response_message
            .filter(|v| js_truthy(Some(v)))
            .or_else(|| properties.get("message"));
        bindings.emit(ErrorEmission::Message(message.cloned()))
    }
}
fn eligible(m: &Value) -> Result<bool, Failure> {
    let contract = keyed(Some(m), "contractType")?;
    if !matches!(
        contract.and_then(Value::as_str),
        Some("PERPETUAL" | "TRADIFI_PERPETUAL")
    ) {
        return Ok(false);
    }
    Ok(matches!(
        keyed(Some(m), "marginAsset")?.and_then(Value::as_str),
        Some("USDT" | "USDC")
    ))
}
fn filter<'a>(filters: &'a [Value], kind: &str, spot: bool) -> Result<Option<&'a Value>, Failure> {
    for f in filters {
        let value = if spot {
            prop(Some(f), "filterType")?
        } else {
            keyed(Some(f), "filterType")?
        };
        if value.and_then(Value::as_str) == Some(kind) {
            return Ok(Some(f));
        }
    }
    Ok(None)
}
fn derivative_market(
    venue: RestVenue,
    m: &Value,
    brackets: &[Value],
) -> Result<Option<RestMarket>, Failure> {
    let filters = prop(Some(m), "filters")?
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let p = filter(filters, "PRICE_FILTER", false)?;
    let amt = filter(filters, "LOT_SIZE", false)?;
    let m_amt = filter(filters, "MARKET_LOT_SIZE", false)?;
    if venue == RestVenue::PM && (p.is_none() || amt.is_none() || m_amt.is_none()) {
        return Ok(None);
    }
    let symbol = prop(Some(m), "symbol")?;
    let mut found = None;
    for b in brackets {
        if strict_eq(prop(Some(b), "symbol")?, symbol) {
            found = prop(Some(b), "brackets")?;
            break;
        }
    }
    let string_first = found
        .and_then(Value::as_str)
        .and_then(|s| s.chars().next())
        .map(|ch| Value::String(ch.to_string()));
    let first = found.filter(|v| !v.is_null()).and_then(|v| match v {
        Value::Array(a) => a.first(),
        Value::Object(o) => o.get("0"),
        Value::String(_) => string_first.as_ref(),
        _ => None,
    });
    // Stock JSON indexing includes string brackets; lone UTF16 error text from
    // a leading non-BMP character remains outside the UTF8 error boundary.
    let base = keyed(Some(m), "baseAsset")?;
    let quote = keyed(Some(m), "quoteAsset")?;
    let margin = keyed(Some(m), "marginAsset")?;
    Ok(Some(RestMarket {
        id: format!(
            "{}/{}:{}",
            js_string(base).map_err(|e| type_error(e.message))?,
            js_string(quote).map_err(|e| type_error(e.message))?,
            js_string(margin).map_err(|e| type_error(e.message))?
        ),
        symbol: symbol.cloned(),
        base: base.cloned(),
        quote: quote.cloned(),
        active: prop(Some(m), "status")?.and_then(Value::as_str) == Some("TRADING"),
        is_spot: None,
        precision: MarketPrecision {
            base: None,
            quote: None,
            amount: vf(amt, "stepSize")?,
            price: vf(p, "tickSize")?,
        },
        amount: MarketLimitsAmount {
            min: max(vf(amt, "minQty")?, vf(m_amt, "minQty")?),
            max: (min(vf(amt, "maxQty")?, vf(m_amt, "maxQty")?)).into(),
        },
        leverage_min: 1.0,
        leverage_max: keyed(first, "initialLeverage")?
            .filter(|v| js_truthy(Some(v)))
            .cloned()
            .unwrap_or(json!(1)),
    }))
}
fn spot_market(m: &Value) -> Result<RestMarket, Failure> {
    let base = keyed(Some(m), "baseAsset")?;
    let quote = keyed(Some(m), "quoteAsset")?;
    let filters = prop(Some(m), "filters")?;
    let filters = match filters {
        None | Some(Value::Null) => &[][..],
        Some(Value::Array(a)) => a.as_slice(),
        _ => return Err(type_error("m.filters?.find is not a function")),
    };
    let lot = filter(filters, "LOT_SIZE", true)?;
    let price = filter(filters, "PRICE_FILTER", true)?;
    let amount_step = float(lot.and_then(|v| v.get("stepSize")))?;
    let price_step = float(price.and_then(|v| v.get("tickSize")))?;
    Ok(RestMarket {
        id: format!(
            "{}/{}",
            js_string(base).map_err(|e| type_error(e.message))?,
            js_string(quote).map_err(|e| type_error(e.message))?
        ),
        symbol: prop(Some(m), "symbol")?.cloned(),
        base: base.cloned(),
        quote: quote.cloned(),
        active: prop(Some(m), "status")?.and_then(Value::as_str) == Some("TRADING"),
        is_spot: Some(true),
        precision: MarketPrecision {
            base: None,
            quote: None,
            amount: if amount_step > 0.0 {
                amount_step
            } else {
                10.0_f64.powf(-vf(Some(m), "baseAssetPrecision")?)
            },
            price: if price_step > 0.0 {
                price_step
            } else {
                10.0_f64.powf(-vf(Some(m), "quoteAssetPrecision")?)
            },
        },
        amount: MarketLimitsAmount {
            min: float_or(lot.and_then(|v| v.get("minQty")), 0.0)?,
            max: (float_or(lot.and_then(|v| v.get("maxQty")), 999_999_999.0)?).into(),
        },
        leverage_min: 1.0,
        leverage_max: json!(1),
    })
}
async fn markets(
    bindings: &mut impl RestBindings,
    venue: RestVenue,
) -> Result<Rc<Vec<RestMarket>>, Failure> {
    let result = async {
        let ep = endpoint(venue, "MARKETS");
        let data = bindings
            .get(
                if venue == RestVenue::Spot {
                    RestClient::Private
                } else {
                    RestClient::Public
                },
                ep,
                None,
            )?
            .await?;
        let symbols = if venue == RestVenue::Spot {
            prop(data.as_ref(), "symbols")?
        } else {
            data.as_ref()
                .filter(|v| !v.is_null())
                .and_then(|v| v.get("symbols"))
        };
        if venue == RestVenue::Spot {
            let symbols = symbols.and_then(Value::as_array).ok_or_else(|| {
                type_error(if symbols.is_none() {
                    "Cannot read properties of undefined (reading 'map')"
                } else if symbols.is_some_and(Value::is_null) {
                    "Cannot read properties of null (reading 'map')"
                } else {
                    "symbols.map is not a function"
                })
            })?;
            return symbols
                .iter()
                .map(spot_market)
                .collect::<Result<Vec<_>, _>>()
                .map(Rc::new);
        }
        let symbols = rows(venue, &format!("{ep}.symbols"), symbols)?;
        let bracket_ep = endpoint(venue, "LEVERAGE_BRACKET");
        let bracket_data = if bindings.is_public_data_only()? {
            Some(json!([]))
        } else {
            bindings
                .get(
                    if venue == RestVenue::PM {
                        RestClient::Papi
                    } else {
                        RestClient::Private
                    },
                    bracket_ep,
                    None,
                )?
                .await?
        };
        let brackets = rows(venue, bracket_ep, bracket_data.as_ref())?;
        let mut out = Vec::new();
        if venue == RestVenue::Futures {
            let selected = symbols
                .iter()
                .filter_map(|m| match eligible(m) {
                    Ok(true) => Some(Ok(m)),
                    Ok(false) => None,
                    Err(e) => Some(Err(e)),
                })
                .collect::<Result<Vec<_>, _>>()?;
            for m in selected {
                if let Some(m) = derivative_market(venue, m, brackets)? {
                    out.push(m)
                }
            }
        } else {
            for m in symbols {
                if eligible(m)? {
                    if let Some(m) = derivative_market(venue, m, brackets)? {
                        out.push(m)
                    }
                }
            }
        }
        Ok(Rc::new(out))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            catch(bindings, venue, e)?;
            bindings.fallback_markets()
        }
    }
}
// ts: BinanceExchange.fetchMarkets
pub async fn fetch_futures_markets(
    bindings: &mut impl RestBindings,
) -> Result<Rc<Vec<RestMarket>>, Failure> {
    markets(bindings, RestVenue::Futures).await
}
// ts: BinancePMExchange.fetchMarkets
pub async fn fetch_pm_markets(
    bindings: &mut impl RestBindings,
) -> Result<Rc<Vec<RestMarket>>, Failure> {
    markets(bindings, RestVenue::PM).await
}
// ts: BinanceSpotExchange.fetchMarkets
pub async fn fetch_spot_markets(
    bindings: &mut impl RestBindings,
) -> Result<Rc<Vec<RestMarket>>, Failure> {
    markets(bindings, RestVenue::Spot).await
}

fn ticker(
    m: &MarketIdentity,
    book: &Value,
    daily: &Value,
    price: &Value,
    spot: bool,
) -> Result<Ticker, Failure> {
    Ok(Ticker {
        id: m.id.clone(),
        symbol: m.symbol.clone(),
        bid: vf(Some(book), "bidPrice")?,
        ask: vf(Some(book), "askPrice")?,
        last: vf(Some(daily), "lastPrice")?,
        mark: vf(Some(price), if spot { "lastPrice" } else { "markPrice" })?,
        index: vf(Some(price), if spot { "lastPrice" } else { "indexPrice" })?,
        percentage: vf(Some(daily), "priceChangePercent")?,
        funding_rate: if spot {
            0.0
        } else {
            vf(Some(price), "lastFundingRate")?
        },
        funding_time: if spot {
            0.0
        } else {
            vf(Some(price), "nextFundingTime")?
        },
        volume: float(prop(Some(daily), "volume")?)?,
        quote_volume: vf(Some(daily), "quoteVolume")?,
        open_interest: 0.0,
        mid_price: None,
        timestamp: None,
        info: None,
    })
}
async fn tickers(
    bindings: &mut impl RestBindings,
    venue: RestVenue,
) -> Result<Rc<Vec<Ticker>>, Failure> {
    let result = async {
        let daily_ep = endpoint(venue, "TICKERS_24H");
        let daily_data = bindings
            .get(
                if venue == RestVenue::Spot {
                    RestClient::Private
                } else {
                    RestClient::Public
                },
                daily_ep,
                None,
            )?
            .await?;
        let dailys = if venue == RestVenue::Spot {
            match daily_data.as_ref() {
                Some(Value::Array(a)) => a.as_slice(),
                None => {
                    return Err(type_error(
                        "Cannot read properties of undefined (reading 'reduce')",
                    ));
                }
                Some(Value::Null) => {
                    return Err(type_error(
                        "Cannot read properties of null (reading 'reduce')",
                    ));
                }
                _ => return Err(type_error("dailys.reduce is not a function")),
            }
        } else {
            rows(venue, daily_ep, daily_data.as_ref())?
        };
        if venue == RestVenue::Spot {
            let mut out = Vec::new();
            for daily in dailys {
                let map = bindings.markets_by_symbol()?;
                let key =
                    js_string(prop(Some(daily), "symbol")?).map_err(|e| type_error(e.message))?;
                if let Some(m) = map.get(&key) {
                    if !js_truthy(Some(daily)) {
                        continue;
                    }
                    out.push(ticker(m, daily, daily, daily, true)?);
                }
            }
            return Ok(Rc::new(out));
        }
        let book_ep = endpoint(venue, "TICKERS_BOOK");
        let book_data = bindings.get(RestClient::Public, book_ep, None)?.await?;
        let books = rows(venue, book_ep, book_data.as_ref())?;
        let price_ep = endpoint(venue, "TICKERS_PRICE");
        let price_data = bindings.get(RestClient::Public, price_ep, None)?.await?;
        let prices = rows(venue, price_ep, price_data.as_ref())?;
        let captured = if venue == RestVenue::Futures {
            Some(bindings.markets_by_symbol()?)
        } else {
            None
        };
        let mut out = Vec::new();
        for book in books {
            let current = if let Some(captured) = &captured {
                captured.clone()
            } else {
                bindings.markets_by_symbol()?
            };
            let symbol = prop(Some(book), "symbol")?;
            let key = js_string(symbol).map_err(|e| type_error(e.message))?;
            let market = current.get(&key);
            let mut daily = None;
            for d in dailys {
                if strict_eq(prop(Some(d), "symbol")?, symbol) {
                    daily = Some(d);
                    break;
                }
            }
            let mut price = None;
            for p in prices {
                if strict_eq(prop(Some(p), "symbol")?, symbol) {
                    price = Some(p);
                    break;
                }
            }
            if let (Some(m), Some(d), Some(p)) = (market, daily, price) {
                if js_truthy(Some(d)) && js_truthy(Some(p)) {
                    out.push(ticker(m, book, d, p, false)?);
                }
            }
        }
        Ok(Rc::new(out))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            catch(bindings, venue, e)?;
            bindings.fallback_tickers()
        }
    }
}
// ts: BinanceExchange.fetchTickers
pub async fn fetch_futures_tickers(
    bindings: &mut impl RestBindings,
) -> Result<Rc<Vec<Ticker>>, Failure> {
    tickers(bindings, RestVenue::Futures).await
}
// ts: BinancePMExchange.fetchTickers
pub async fn fetch_pm_tickers(
    bindings: &mut impl RestBindings,
) -> Result<Rc<Vec<Ticker>>, Failure> {
    tickers(bindings, RestVenue::PM).await
}
// ts: BinanceSpotExchange.fetchTickers
pub async fn fetch_spot_tickers(
    bindings: &mut impl RestBindings,
) -> Result<Rc<Vec<Ticker>>, Failure> {
    tickers(bindings, RestVenue::Spot).await
}
fn interval(tf: &AnyTimeframe) -> (&'static str, f64, char) {
    match tf {
        AnyTimeframe::Variant1(Timeframe::M1) => ("1m", 1.0, 'm'),
        AnyTimeframe::Variant1(Timeframe::M3) => ("3m", 3.0, 'm'),
        AnyTimeframe::Variant1(Timeframe::M5) => ("5m", 5.0, 'm'),
        AnyTimeframe::Variant1(Timeframe::M15) => ("15m", 15.0, 'm'),
        AnyTimeframe::Variant1(Timeframe::M30) => ("30m", 30.0, 'm'),
        AnyTimeframe::Variant1(Timeframe::H1) => ("1h", 1.0, 'h'),
        AnyTimeframe::Variant1(Timeframe::H2) => ("2h", 2.0, 'h'),
        AnyTimeframe::Variant1(Timeframe::H4) => ("4h", 4.0, 'h'),
        AnyTimeframe::Variant1(Timeframe::H6) => ("6h", 6.0, 'h'),
        AnyTimeframe::Variant1(Timeframe::H12) => ("12h", 12.0, 'h'),
        AnyTimeframe::Variant1(Timeframe::D1) => ("1d", 1.0, 'd'),
        AnyTimeframe::Variant1(Timeframe::W1) => ("1w", 1.0, 'w'),
        AnyTimeframe::Variant0(SecondsTimeframe::S1) => ("1s", 1.0, 's'),
        AnyTimeframe::Variant0(SecondsTimeframe::S5) => ("5s", 5.0, 's'),
        AnyTimeframe::Variant0(SecondsTimeframe::S15) => ("15s", 15.0, 's'),
        AnyTimeframe::Variant0(SecondsTimeframe::S30) => ("30s", 30.0, 's'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H3) => ("3h", 3.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H5) => ("5h", 5.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H7) => ("7h", 7.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H8) => ("8h", 8.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H9) => ("9h", 9.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H10) => ("10h", 10.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H11) => ("11h", 11.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H24) => ("24h", 24.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::D2) => ("2d", 2.0, 'd'),
        AnyTimeframe::Variant2(ExtendedTimeframe::D3) => ("3d", 3.0, 'd'),
        AnyTimeframe::Variant2(ExtendedTimeframe::MN1) => ("1M", 1.0, 'M'),
    }
}
fn truthy(n: f64) -> bool {
    n != 0.0 && !n.is_nan()
}
async fn ohlcv(
    bindings: &mut impl RestBindings,
    venue: RestVenue,
    opts: &OHLCVOptions,
) -> Result<Vec<Candle>, Failure> {
    let (interval, amount, unit) = interval(&opts.interval);
    let supplied = opts.limit.filter(|n| truthy(*n));
    let limit = min(supplied.unwrap_or(500.0), 1500.0);
    let end = bindings.dayjs(opts.to.filter(|n| truthy(*n)))?;
    let start = if supplied.is_none() && opts.from.is_some_and(truthy) {
        bindings.dayjs(opts.from)?
    } else {
        bindings.subtract(end, amount * limit, unit)?
    };
    let ep = endpoint(venue, "KLINE");
    let data = bindings
        .get(
            if venue == RestVenue::Spot {
                RestClient::Private
            } else {
                RestClient::Public
            },
            ep,
            Some(CandleParams {
                symbol: opts.symbol.clone(),
                interval,
                start_time: start,
                end_time: end,
                limit: supplied.map(|_| limit),
            }),
        )?
        .await?;
    let rows = match venue {
        RestVenue::Futures => match data.as_ref() {
            Some(Value::Array(a)) => a.as_slice(),
            _ => return Ok(Vec::new()),
        },
        RestVenue::PM => rows(venue, ep, data.as_ref())?,
        RestVenue::Spot => match data.as_ref() {
            Some(Value::Array(a)) => a.as_slice(),
            None => {
                return Err(type_error(
                    "Cannot read properties of undefined (reading 'map')",
                ));
            }
            Some(Value::Null) => {
                return Err(type_error("Cannot read properties of null (reading 'map')"));
            }
            _ => return Err(type_error("data.map is not a function")),
        },
    };
    rows.iter()
        .map(|row| {
            let string_values;
            let values = match row {
                Value::String(text) => {
                    string_values = text
                        .chars()
                        .map(|ch| Value::String(ch.to_string()))
                        .collect::<Vec<_>>();
                    string_values.as_slice()
                }
                Value::Array(a) => a.as_slice(),
                Value::Null=>return Err(type_error("object null is not iterable (cannot read property Symbol(Symbol.iterator))")),
                _ => {
                    return Err(type_error(
                        "object is not iterable (cannot read property Symbol(Symbol.iterator))",
                    ));
                }
            };
            Ok(Candle {
                timestamp: js_number(values.first()).map_err(|e| type_error(e.message))? / 1000.0,
                open: float(values.get(1))?,
                high: float(values.get(2))?,
                low: float(values.get(3))?,
                close: float(values.get(4))?,
                volume: float(values.get(5))?,
                last_side: None,
            })
        })
        .collect()
}
// ts: BinanceExchange.fetchOHLCV
pub async fn fetch_futures_ohlcv(
    bindings: &mut impl RestBindings,
    opts: &OHLCVOptions,
) -> Result<Vec<Candle>, Failure> {
    ohlcv(bindings, RestVenue::Futures, opts).await
}
// ts: BinancePMExchange.fetchOHLCV
pub async fn fetch_pm_ohlcv(
    bindings: &mut impl RestBindings,
    opts: &OHLCVOptions,
) -> Result<Vec<Candle>, Failure> {
    ohlcv(bindings, RestVenue::PM, opts).await
}
// ts: BinanceSpotExchange.fetchOHLCV
pub async fn fetch_spot_ohlcv(
    bindings: &mut impl RestBindings,
    opts: &OHLCVOptions,
) -> Result<Vec<Candle>, Failure> {
    ohlcv(bindings, RestVenue::Spot, opts).await
}
