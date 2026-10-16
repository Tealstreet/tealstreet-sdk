//! Market/response selectors and precision callers. Property/iterator/collection,
//! regex, coercion and imported numeric library implementations are host bindings.
use crate::{spot_json::JsString, ws_ping_lifecycle::WsPingHost};
pub trait MarketDataHost: WsPingHost {
    fn null(&self) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn is_nullish(&self, value: &Self::Value) -> bool;
    fn is_string(&self, value: &Self::Value) -> bool;
    fn strict_equal(&self, left: &Self::Value, right: &Self::Value) -> bool;
    /// Native ToPropertyKey/GetV, retaining the original receiver and errors.
    fn index(
        &mut self,
        object: &Self::Value,
        key: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn iterator(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn next(&mut self, iterator: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    fn close_after_throw(&mut self, iterator: &Self::Value);
    fn construct(
        &mut self,
        function: &Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error>;
    /// One-element array binding: consume the first iterator value and apply
    /// source IteratorClose on short destructuring, including error precedence.
    fn first_binding(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    /// Allocate a fresh /^U[A-Z0-9]/ RegExp with no flags; the source .test
    /// method is resolved separately through get, before its argument.
    fn unit_regex(&mut self) -> Result<Self::Value, Self::Error>;
    fn template_string(&mut self, value: &Self::Value) -> Result<JsString, Self::Error>;
    fn subtract(
        &mut self,
        left: &Self::Value,
        right: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
}
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
}
fn literal<H: WsPingHost>(h: &H, s: &str) -> H::Value {
    h.literal(&text(s))
}
fn method<H: WsPingHost>(
    h: &mut H,
    receiver: &H::Value,
    name: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let f = h.get(receiver, name)?;
    h.call(&f, receiver, args)
}
fn free<H: WsPingHost>(h: &mut H, name: &str, args: &[H::Value]) -> Result<H::Value, H::Error> {
    let f = h.global(name)?;
    let u = h.undefined();
    h.call(&f, &u, args)
}
fn optional<H: MarketDataHost>(
    h: &mut H,
    object: &H::Value,
    name: &str,
) -> Result<H::Value, H::Error> {
    if h.is_nullish(object) {
        Ok(h.undefined())
    } else {
        h.get(object, name)
    }
}
fn for_each<H: MarketDataHost, F: FnMut(&mut H, H::Value) -> Result<(), H::Error>>(
    h: &mut H,
    value: &H::Value,
    mut body: F,
) -> Result<(), H::Error> {
    let iterator = h.iterator(value)?;
    while let Some(v) = h.next(&iterator)? {
        if let Err(error) = body(h, v) {
            h.close_after_throw(&iterator);
            return Err(error);
        }
    }
    Ok(())
}
pub fn build_markets_lower_case_map<H: MarketDataHost>(
    h: &mut H,
    exchange: &H::Value,
    markets: &H::Value,
) -> Result<(), H::Error> {
    let map = h.get(exchange, "marketsLowerCaseMap")?;
    method(h, &map, "clear", &[])?;
    for_each(h, markets, |h, market| {
        let map = h.get(exchange, "marketsLowerCaseMap")?;
        let set = h.get(&map, "set")?;
        let id = h.get(&market, "id")?;
        let lower = method(h, &id, "toLowerCase", &[])?;
        h.call(&set, &map, &[lower, market])?;
        Ok(())
    })?;
    let log = h.get(exchange, "log")?;
    let map = h.get(exchange, "marketsLowerCaseMap")?;
    let size = h.get(&map, "size")?;
    let mut message = "Built case-insensitive market lookup map with "
        .encode_utf16()
        .collect::<Vec<_>>();
    message.extend(h.template_string(&size)?.0);
    message.extend(" markets".encode_utf16());
    let message = h.literal(&JsString(message));
    let severity = h.global("LogSeverity")?;
    let debug = h.get(&severity, "Debug")?;
    h.call(&log, exchange, &[message, debug])?;
    Ok(())
}
pub fn resolve_coin_to_symbol<H: MarketDataHost>(
    h: &mut H,
    exchange: &H::Value,
    coin: &H::Value,
) -> Result<H::Value, H::Error> {
    let store = h.get(exchange, "store")?;
    let markets = h.get(&store, "marketsById")?;
    let direct = h.index(&markets, coin)?;
    if h.truthy(&direct) {
        return h.get(&direct, "symbol");
    }
    let map = h.get(exchange, "marketsLowerCaseMap")?;
    let get = h.get(&map, "get")?;
    let lower = method(h, coin, "toLowerCase", &[])?;
    let result = h.call(&get, &map, &[lower])?;
    optional(h, &result, "symbol")
}
pub fn spot_symbol_or_id_to_coin<H: MarketDataHost>(
    h: &mut H,
    value: &H::Value,
) -> Result<H::Value, H::Error> {
    let dash = literal(h, "-");
    let contains = method(h, value, "includes", &[dash.clone()])?;
    if h.truthy(&contains) {
        let split = method(h, value, "split", &[dash])?;
        let zero = h.number(0.0);
        h.index(&split, &zero)
    } else {
        Ok(value.clone())
    }
}
pub fn resolve_spot_display_name<H: MarketDataHost>(
    h: &mut H,
    token: &H::Value,
    names: &H::Value,
) -> Result<H::Value, H::Error> {
    let first = h.get(token, "fullName")?;
    let bridged = if h.is_string(&first) {
        let full = h.get(token, "fullName")?;
        let unit = literal(h, "Unit ");
        let v = method(h, &full, "startsWith", &[unit])?;
        h.truthy(&v)
    } else {
        false
    };
    if !bridged {
        return h.get(token, "name");
    }
    let regex = h.unit_regex()?;
    let test = h.get(&regex, "test")?;
    let name = h.get(token, "name")?;
    let matches = h.call(&test, &regex, &[name])?;
    if !h.truthy(&matches) {
        return h.get(token, "name");
    }
    let name = h.get(token, "name")?;
    let one = h.number(1.0);
    let stripped = method(h, &name, "slice", &[one])?;
    if h.truthy(&stripped) {
        let present = method(h, names, "has", &[stripped.clone()])?;
        if !h.truthy(&present) {
            return Ok(stripped);
        }
    }
    h.get(token, "name")
}
pub fn extract_clearinghouse_balance_summary<H: MarketDataHost>(
    h: &mut H,
    data: &H::Value,
) -> Result<H::Value, H::Error> {
    let margin = optional(h, data, "marginSummary")?;
    let field = if h.truthy(&margin) {
        literal(h, "marginSummary")
    } else {
        let cross = optional(h, data, "crossMarginSummary")?;
        if h.truthy(&cross) {
            literal(h, "crossMarginSummary")
        } else {
            return Ok(h.null());
        }
    };
    let summary = h.index(data, &field)?;
    let number = h.global("Number")?;
    let account = optional(h, &summary, "accountValue")?;
    let undefined = h.undefined();
    let account = h.call(&number, &undefined, &[account])?;
    let number = h.global("Number")?;
    let used = optional(h, &summary, "totalMarginUsed")?;
    let used = h.call(&number, &undefined, &[used])?;
    let number = h.global("Number")?;
    let finite = method(h, &number, "isFinite", &[account.clone()])?;
    if !h.truthy(&finite) {
        return Ok(h.null());
    }
    let number = h.global("Number")?;
    let finite = method(h, &number, "isFinite", &[used.clone()])?;
    if !h.truthy(&finite) {
        return Ok(h.null());
    }
    let result = h.object()?;
    h.define_data(&result, "accountValue", &account)?;
    h.define_data(&result, "totalMarginUsed", &used)?;
    h.define_data(&result, "sourceField", &field)?;
    Ok(result)
}
pub fn response_has_deferred_order_status<H: WsPingHost>(
    h: &mut H,
    data: &H::Value,
) -> Result<H::Value, H::Error> {
    free(
        h,
        "hyperliquidOrderResponseHasDeferredOrderStatus",
        &[data.clone()],
    )
}
pub fn get_stale_check_channel_key<H: MarketDataHost>(
    h: &mut H,
    channel: &H::Value,
    data: &H::Value,
) -> Result<H::Value, H::Error> {
    let clearing = literal(h, "clearinghouseState");
    if h.strict_equal(channel, &clearing) {
        let dex = optional(h, data, "dex")?;
        if h.truthy(&dex) {
            let mut units = h.template_string(channel)?.0;
            units.push(b':' as u16);
            let string = h.global("String")?;
            let dex = h.get(data, "dex")?;
            let undefined = h.undefined();
            let dex = h.call(&string, &undefined, &[dex])?;
            let lower = method(h, &dex, "toLowerCase", &[])?;
            units.extend(h.template_string(&lower)?.0);
            return Ok(h.literal(&JsString(units)));
        }
    }
    Ok(channel.clone())
}
pub fn collect_hip3_dexes_from_fills<H: MarketDataHost>(
    h: &mut H,
    socket: &H::Value,
    fills: &H::Value,
) -> Result<H::Value, H::Error> {
    let set = h.global("Set")?;
    let dexes = h.construct(&set, &[])?;
    let parent = h.get(socket, "parent")?;
    let store = h.get(&parent, "store")?;
    let markets = h.get(&store, "marketsById")?;
    for_each(h, fills, |h, fill| {
        let string = h.global("String")?;
        let coin = optional(h, &fill, "coin")?;
        let coin = if h.truthy(&coin) {
            coin
        } else {
            literal(h, "")
        };
        let undefined = h.undefined();
        let coin = h.call(&string, &undefined, &[coin])?;
        if !h.truthy(&coin) {
            return Ok(());
        }
        let colon = literal(h, ":");
        let includes = method(h, &coin, "includes", &[colon.clone()])?;
        if h.truthy(&includes) {
            let split = method(h, &coin, "split", &[colon])?;
            let prefix = h.first_binding(&split)?;
            if h.truthy(&prefix) {
                let add = h.get(&dexes, "add")?;
                let lower = method(h, &prefix, "toLowerCase", &[])?;
                h.call(&add, &dexes, &[lower])?;
            }
            return Ok(());
        }
        let market = h.index(&markets, &coin)?;
        let info = optional(h, &market, "info")?;
        let dex = optional(h, &info, "hip3Dex")?;
        if h.is_string(&dex) && h.truthy(&dex) {
            let add = h.get(&dexes, "add")?;
            let lower = method(h, &dex, "toLowerCase", &[])?;
            h.call(&add, &dexes, &[lower])?;
        }
        Ok(())
    })?;
    Ok(dexes)
}
fn significant_price<H: MarketDataHost>(
    h: &mut H,
    price: &H::Value,
) -> Result<Option<H::Value>, H::Error> {
    let string = free(h, "numberToString", &[price.clone()])?;
    if !h.truthy(&string) {
        return Ok(None);
    }
    let dot = literal(h, ".");
    let split = method(h, &string, "split", &[dot])?;
    let zero = h.number(0.0);
    let integer = h.index(&split, &zero)?;
    let math = h.global("Math")?;
    let max = h.get(&math, "max")?;
    let five = h.number(5.0);
    let length = h.get(&integer, "length")?;
    let digits = h.call(&max, &math, &[five, length])?;
    let decimal = h.global("decimalToPrecision")?;
    let round = h.global("ROUND")?;
    let significant = h.global("SIGNIFICANT_DIGITS")?;
    let padding = h.global("NO_PADDING")?;
    let undefined = h.undefined();
    Ok(Some(h.call(
        &decimal,
        &undefined,
        &[price.clone(), round, digits, significant, padding],
    )?))
}
fn finish_price<H: MarketDataHost>(
    h: &mut H,
    result: H::Value,
    max: H::Value,
    amount: H::Value,
) -> Result<H::Value, H::Error> {
    // Resolve precisionFromString before evaluating its conditional argument.
    let precision = h.global("precisionFromString")?;
    let string = if h.truthy(&amount) {
        free(h, "String", &[amount])?
    } else {
        literal(h, "1")
    };
    let undefined = h.undefined();
    let precision = h.call(&precision, &undefined, &[string])?;
    let digits = h.subtract(&max, &precision)?;
    let decimal = h.global("decimalToPrecision")?;
    let round = h.global("ROUND")?;
    let places = h.global("DECIMAL_PLACES")?;
    let padding = h.global("NO_PADDING")?;
    h.call(
        &decimal,
        &undefined,
        &[result, round, digits, places, padding],
    )
}
pub fn core_price_to_precision<H: MarketDataHost>(
    h: &mut H,
    exchange: &H::Value,
    symbol: &H::Value,
    price: &H::Value,
) -> Result<H::Value, H::Error> {
    let Some(result) = significant_price(h, price)? else {
        return Ok(literal(h, ""));
    };
    let store = h.get(exchange, "store")?;
    let markets = h.get(&store, "marketsBySymbol")?;
    let market = h.index(&markets, symbol)?;
    let spot = optional(h, &market, "isSpot")?;
    let max = h.global(if h.truthy(&spot) {
        "SPOT_MAX_PRICE_DECIMALS"
    } else {
        "PERP_MAX_PRICE_DECIMALS"
    })?;
    let amount = if h.is_nullish(&market) {
        h.undefined()
    } else {
        let precision = h.get(&market, "precision")?;
        h.get(&precision, "amount")?
    };
    finish_price(h, result, max, amount)
}
pub fn spot_price_to_precision<H: MarketDataHost>(
    h: &mut H,
    exchange: &H::Value,
    symbol: &H::Value,
    price: &H::Value,
) -> Result<H::Value, H::Error> {
    let Some(result) = significant_price(h, price)? else {
        return Ok(literal(h, ""));
    };
    let max = h.number(8.0);
    let store = h.get(exchange, "store")?;
    let markets = h.get(&store, "marketsBySymbol")?;
    let market = h.index(&markets, symbol)?;
    let amount = if h.is_nullish(&market) {
        h.undefined()
    } else {
        let precision = h.get(&market, "precision")?;
        h.get(&precision, "amount")?
    };
    finish_price(h, result, max, amount)
}
