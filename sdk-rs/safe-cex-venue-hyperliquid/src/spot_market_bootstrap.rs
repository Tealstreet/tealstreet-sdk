//! Primary exchange opt-in spot market bootstrap, preserving coin joins and quote identity.
use crate::{
    helper_tasks::{self, HelperObserver},
    spot_account_reads::SpotAccountHost,
    spot_json::JsString,
};
pub trait SpotMarketBootstrapHost: SpotAccountHost {
    /// Native ToPropertyKey at PutValue, after the fresh assignment RHS is evaluated.
    fn property_key(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn set_key(
        &mut self,
        object: &Self::Value,
        key: &Self::Value,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn unary_plus(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
}
fn lit<H: SpotAccountHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
fn method<H: SpotAccountHost>(
    h: &mut H,
    o: &H::Value,
    k: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.get(o, k)?;
    let a = args(h)?;
    h.call(&f, o, &a)
}
fn free<H: SpotAccountHost>(
    h: &mut H,
    n: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.imported(n)?;
    let a = args(h)?;
    h.call(&f, &h.undefined(), &a)
}
fn intrinsic<H: SpotAccountHost>(
    h: &mut H,
    n: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.global(n)?;
    let a = args(h)?;
    h.call(&f, &h.undefined(), &a)
}
fn constant<H: SpotAccountHost>(h: &mut H, table: &str, name: &str) -> Result<H::Value, H::Error> {
    let table = h.imported(table)?;
    h.get(&table, name)
}
fn optional<H: SpotAccountHost>(h: &mut H, o: &H::Value, key: &str) -> Result<H::Value, H::Error> {
    if h.is_nullish(o) {
        Ok(h.undefined())
    } else {
        h.get(o, key)
    }
}
fn result<H: SpotAccountHost>(
    h: &mut H,
    markets: &H::Value,
    tickers: &H::Value,
) -> Result<H::Value, H::Error> {
    let r = h.object()?;
    h.define_data(&r, "markets", markets)?;
    h.define_data(&r, "tickers", tickers)?;
    Ok(r)
}
fn precision<H: SpotMarketBootstrapHost>(
    h: &mut H,
    value: &H::Value,
) -> Result<H::Value, H::Error> {
    let p = free(h, "parsePrecision", |_| Ok(vec![value.clone()]))?;
    let p = if h.truthy(&p) { p } else { h.number(0.) };
    h.unary_plus(&p)
}
fn pair_symbol<H: SpotAccountHost>(
    h: &mut H,
    base: &H::Value,
    quote: &H::Value,
) -> Result<H::Value, H::Error> {
    let mut units = h.template_string(base)?.0;
    units.push(47);
    units.extend(h.template_string(quote)?.0);
    Ok(h.literal(&JsString(units)))
}
async fn body<H: SpotMarketBootstrapHost>(mut h: H, e: H::Value) -> Result<H::Value, H::Error> {
    let markets = h.array(&[])?;
    let tickers = h.array(&[])?;
    let options = h.get(&e, "options")?;
    let enabled = h.get(&options, "enableSpotMarkets")?;
    if !h.truthy(&enabled) {
        return result(&mut h, &markets, &tickers);
    }
    let attempt = async {
        let xhr = h.get(&e, "infoXhr")?;
        let op = method(&mut h, &xhr, "post", |h| {
            let url = constant(h, "ENDPOINTS", "INFO")?;
            let body = h.object()?;
            let kind = constant(h, "INFO_TYPES", "SPOT_META_AND_ASSET_CTXS")?;
            h.define_data(&body, "type", &kind)?;
            let config = h.object()?;
            let headers = h.object()?;
            h.define_data(&headers, "Content-Type", &lit(h, "application/json"))?;
            h.define_data(&config, "headers", &headers)?;
            Ok(vec![url, body, config])
        })?;
        let response = h.await_value(op).await?;
        let data = h.response_data(&response)?;
        let data = if h.is_nullish(&data) {
            h.array(&[])?
        } else {
            data
        };
        let [meta, assets] = h.pair_binding(&data)?;
        let universe = optional(&mut h, &meta, "universe")?;
        if !h.truthy(&universe) || !h.is_array(&assets)? {
            method(&mut h, &e, "log", |h| {
                let msg = lit(
                    h,
                    "Malformed spotMetaAndAssetCtxs response; skipping spot markets",
                );
                let severity = constant(h, "LogSeverity", "Warning")?;
                Ok(vec![msg, severity])
            })?;
            return result(&mut h, &markets, &tickers).map(Some);
        }
        let token_by_index = h.object()?;
        let mut i = h.number(0.);
        loop {
            let tokens = h.get(&meta, "tokens")?;
            let length = h.get(&tokens, "length")?;
            let before = h.binary("<", &i, &length)?;
            if !h.truthy(&before) {
                break;
            }
            let tokens = h.get(&meta, "tokens")?;
            let token = h.index(&tokens, &i)?;
            let key = h.get(&token, "index")?;
            let tokens = h.get(&meta, "tokens")?;
            let token = h.index(&tokens, &i)?;
            let key = h.property_key(&key)?;
            h.set_key(&token_by_index, &key, &token)?;
            i = h.binary("+", &i, &h.number(1.))?;
        }
        let assets_by_coin = h.object()?;
        let mut i = h.number(0.);
        loop {
            let length = h.get(&assets, "length")?;
            let before = h.binary("<", &i, &length)?;
            if !h.truthy(&before) {
                break;
            }
            let asset = h.index(&assets, &i)?;
            let coin = optional(&mut h, &asset, "coin")?;
            if h.truthy(&coin) {
                let asset = h.index(&assets, &i)?;
                let key = h.property_key(&coin)?;
                h.set_key(&assets_by_coin, &key, &asset)?;
            }
            i = h.binary("+", &i, &h.number(1.))?;
        }
        let constructor = h.global("Set")?;
        let token_names = h.construct(&constructor, &[])?;
        let mut i = h.number(0.);
        loop {
            let tokens = h.get(&meta, "tokens")?;
            let length = h.get(&tokens, "length")?;
            let before = h.binary("<", &i, &length)?;
            if !h.truthy(&before) {
                break;
            }
            method(&mut h, &token_names, "add", |h| {
                let tokens = h.get(&meta, "tokens")?;
                let token = h.index(&tokens, &i)?;
                Ok(vec![h.get(&token, "name")?])
            })?;
            i = h.binary("+", &i, &h.number(1.))?;
        }
        let mut i = h.number(0.);
        loop {
            let universe = h.get(&meta, "universe")?;
            let length = h.get(&universe, "length")?;
            let before = h.binary("<", &i, &length)?;
            if !h.truthy(&before) {
                break;
            }
            let universe = h.get(&meta, "universe")?;
            let source_market = h.index(&universe, &i)?;
            let tokens = h.get(&source_market, "tokens")?;
            let key = if h.is_nullish(&tokens) {
                h.undefined()
            } else {
                h.get(&tokens, "0")?
            };
            let token = h.index(&token_by_index, &key)?;
            let tokens = h.get(&source_market, "tokens")?;
            let key = if h.is_nullish(&tokens) {
                h.undefined()
            } else {
                h.get(&tokens, "1")?
            };
            let quote_token = h.index(&token_by_index, &key)?;
            if !h.truthy(&token) || !h.truthy(&quote_token) {
                i = h.binary("+", &i, &h.number(1.))?;
                continue;
            }
            let name = h.get(&source_market, "name")?;
            let source_ticker = h.index(&assets_by_coin, &name)?;
            let mid = optional(&mut h, &source_ticker, "midPx")?;
            if !h.truthy(&mid) {
                i = h.binary("+", &i, &h.number(1.))?;
                continue;
            }
            let quote = h.get(&quote_token, "name")?;
            let amount_precision_string = free(&mut h, "safeString", |h| {
                Ok(vec![token.clone(), lit(h, "szDecimals")])
            })?;
            let amount_precision = intrinsic(&mut h, "parseInt", |_| {
                Ok(vec![amount_precision_string.clone()])
            })?;
            let mid = intrinsic(&mut h, "parseFloat", |h| {
                let v = free(h, "v", |h| Ok(vec![source_ticker.clone(), lit(h, "midPx")]))?;
                Ok(vec![v])
            })?;
            let price_precision = free(&mut h, "calculatePricePrecision", |h| {
                let max = h.imported("SPOT_MAX_PRICE_DECIMALS")?;
                Ok(vec![mid.clone(), amount_precision, max])
            })?;
            let price_precision_string =
                free(&mut h, "numberToString", |_| Ok(vec![price_precision]))?;
            let id = h.get(&source_market, "name")?;
            let base = h.get(&token, "name")?;
            let symbol = pair_symbol(&mut h, &base, &quote)?;
            let display_base = method(&mut h, &e, "resolveSpotDisplayName", |_| {
                Ok(vec![token.clone(), token_names.clone()])
            })?;
            let display_symbol = pair_symbol(&mut h, &display_base, &quote)?;
            let mark = free(&mut h, "parseFiniteNumber", |h| {
                let v = free(h, "v", |h| {
                    Ok(vec![source_ticker.clone(), lit(h, "markPx")])
                })?;
                Ok(vec![v])
            })?;
            let mark = if h.truthy(&mark) { mark } else { mid.clone() };
            let previous = free(&mut h, "parseFiniteNumber", |h| {
                let v = free(h, "v", |h| {
                    Ok(vec![source_ticker.clone(), lit(h, "prevDayPx")])
                })?;
                Ok(vec![v])
            })?;
            let volumes = free(&mut h, "parseHyperliquidVolumeFields", |_| {
                Ok(vec![source_ticker.clone(), mid.clone()])
            })?;
            let volume = h.get(&volumes, "volume")?;
            let quote_volume = h.get(&volumes, "quoteVolume")?;
            // Resolve push before evaluating the object literal, preserving overrides/getters.
            method(&mut h, &markets, "push", |h| {
                let market = h.object()?;
                h.define_data(&market, "id", &id)?;
                h.define_data(&market, "symbol", &symbol)?;
                let alias = if !h.strict_equal(&display_symbol, &symbol) {
                    let alias = h.object()?;
                    h.define_data(&alias, "prettySymbol", &display_symbol)?;
                    alias
                } else {
                    h.object()?
                };
                h.spread_into(&market, &alias)?;
                h.define_data(&market, "base", &display_base)?;
                h.define_data(&market, "quote", &quote)?;
                h.define_data(&market, "isSpot", &h.boolean(true))?;
                h.define_data(&market, "active", &h.boolean(true))?;
                let p = h.object()?;
                let amount = precision(h, &amount_precision_string)?;
                h.define_data(&p, "amount", &amount)?;
                let price = precision(h, &price_precision_string)?;
                h.define_data(&p, "price", &price)?;
                h.define_data(&market, "precision", &p)?;
                let limits = h.object()?;
                let amount = h.object()?;
                let min = precision(h, &amount_precision_string)?;
                h.define_data(&amount, "min", &min)?;
                let number = h.global("Number")?;
                let max = h.get(&number, "MAX_SAFE_INTEGER")?;
                h.define_data(&amount, "max", &max)?;
                h.define_data(&limits, "amount", &amount)?;
                let leverage = h.object()?;
                h.define_data(&leverage, "min", &h.number(1.))?;
                h.define_data(&leverage, "max", &h.number(1.))?;
                h.define_data(&limits, "leverage", &leverage)?;
                h.define_data(&market, "limits", &limits)?;
                let info = h.object()?;
                let index = h.get(&source_market, "index")?;
                let offset = h.imported("SPOT_ASSET_INDEX_OFFSET")?;
                let index = h.binary("+", &index, &offset)?;
                h.define_data(&info, "hyperliquidId", &index)?;
                h.define_data(&info, "isSpot", &h.boolean(true))?;
                let name = h.get(&token, "name")?;
                h.define_data(&info, "hlTokenName", &name)?;
                let full_name = h.get(&token, "fullName")?;
                let full_name = if h.is_nullish(&full_name) {
                    h.undefined()
                } else {
                    full_name
                };
                h.define_data(&info, "hlTokenFullName", &full_name)?;
                h.spread_into(&info, &source_market)?;
                h.define_data(&market, "info", &info)?;
                Ok(vec![market])
            })?;
            method(&mut h, &tickers, "push", |h| {
                let ticker = h.object()?;
                h.define_data(&ticker, "id", &id)?;
                h.define_data(&ticker, "symbol", &symbol)?;
                for k in ["bid", "ask"] {
                    h.define_data(&ticker, k, &h.number(0.))?
                }
                h.define_data(&ticker, "last", &mid)?;
                h.define_data(&ticker, "mark", &mark)?;
                h.define_data(&ticker, "index", &h.number(0.))?;
                h.define_data(&ticker, "midPrice", &mid)?;
                let change = free(h, "calculatePriceChangePercentage", |_| {
                    Ok(vec![mark.clone(), previous.clone()])
                })?;
                h.define_data(&ticker, "percentage", &change)?;
                for k in ["fundingRate", "fundingTime"] {
                    h.define_data(&ticker, k, &h.number(0.))?
                }
                h.define_data(&ticker, "volume", &volume)?;
                h.define_data(&ticker, "quoteVolume", &quote_volume)?;
                h.define_data(&ticker, "openInterest", &h.number(0.))?;
                h.define_data(&ticker, "info", &source_ticker)?;
                Ok(vec![ticker])
            })?;
            i = h.binary("+", &i, &h.number(1.))?;
        }
        Ok(None)
    }
    .await;
    match attempt {
        Ok(Some(value)) => Ok(value),
        Ok(None) => result(&mut h, &markets, &tickers),
        Err(error) => {
            let error = h.error_value(&error);
            method(&mut h, &e, "log", |h| {
                let message = optional(h, &error, "message")?;
                let message = if h.is_nullish(&message) {
                    error
                } else {
                    message
                };
                let mut units = "Failed to load Hyperliquid spot markets: "
                    .encode_utf16()
                    .collect::<Vec<_>>();
                units.extend(h.template_string(&message)?.0);
                let message = h.literal(&JsString(units));
                let severity = constant(h, "LogSeverity", "Warning")?;
                Ok(vec![message, severity])
            })?;
            let markets = h.array(&[])?;
            let tickers = h.array(&[])?;
            result(&mut h, &markets, &tickers)
        }
    }
}
pub fn fetch_spot_markets_and_tickers<H>(
    host: H,
    exchange: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H: SpotMarketBootstrapHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, body(host, exchange))
}
