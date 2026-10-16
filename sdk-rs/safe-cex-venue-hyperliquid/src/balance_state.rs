//! Spot/core collateral pricing and balance-state algorithms. Intrinsic objects,
//! operators, iterators and virtual exchange calls remain host contracts.
use crate::{spot_json::JsString, ws_ping_lifecycle::WsPingHost};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalanceOperator {
    Add,
    Subtract,
    Multiply,
    Greater,
}
#[derive(Clone, Debug)]
pub struct SpotMarketPredicate<V> {
    pub upper: V,
    pub token_index: V,
}
pub trait BalanceHost: WsPingHost {
    fn null(&self) -> Self::Value;
    fn number(&self, v: f64) -> Self::Value;
    fn boolean(&self, v: bool) -> Self::Value;
    fn is_nullish(&self, v: &Self::Value) -> bool;
    fn is_undefined(&self, v: &Self::Value) -> bool;
    fn strict_equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn index(&mut self, o: &Self::Value, key: &Self::Value) -> Result<Self::Value, Self::Error>;
    /// Strict ordinary assignment, including __proto__ setter/inherited effects.
    fn set_index(
        &mut self,
        o: &Self::Value,
        key: &Self::Value,
        v: &Self::Value,
    ) -> Result<(), Self::Error>;
    /// Computed literal CreateDataProperty, distinct from ordinary assignment.
    fn define_index(
        &mut self,
        o: &Self::Value,
        key: &Self::Value,
        v: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn array(&mut self) -> Result<Self::Value, Self::Error>;
    fn array_define(
        &mut self,
        o: &Self::Value,
        i: usize,
        v: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn iterator(&mut self, v: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn next(&mut self, i: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    fn close_after_throw(&mut self, i: &Self::Value);
    fn close_after_return(&mut self, i: &Self::Value) -> Result<(), Self::Error>;
    fn pair(&mut self, v: &Self::Value) -> Result<[Self::Value; 2], Self::Error>;
    fn template_string(&mut self, v: &Self::Value) -> Result<JsString, Self::Error>;
    fn operator(
        &mut self,
        op: BalanceOperator,
        a: &Self::Value,
        b: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn spread_into(&mut self, o: &Self::Value, source: &Self::Value) -> Result<(), Self::Error>;
    /// Allocate lexical Array.find predicate; adapter invokes run_spot_market_predicate.
    fn spot_market_predicate(
        &mut self,
        capture: SpotMarketPredicate<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
}
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
}
fn lit<H: WsPingHost>(h: &H, s: &str) -> H::Value {
    h.literal(&text(s))
}
fn method<H: WsPingHost>(
    h: &mut H,
    o: &H::Value,
    n: &str,
    a: &[H::Value],
) -> Result<H::Value, H::Error> {
    let f = h.get(o, n)?;
    h.call(&f, o, a)
}
fn optional<H: BalanceHost>(h: &mut H, o: &H::Value, n: &str) -> Result<H::Value, H::Error> {
    if h.is_nullish(o) {
        Ok(h.undefined())
    } else {
        h.get(o, n)
    }
}
fn free<H: WsPingHost>(h: &mut H, n: &str, a: &[H::Value]) -> Result<H::Value, H::Error> {
    let f = h.global(n)?;
    let u = h.undefined();
    h.call(&f, &u, a)
}
fn suffix<H: BalanceHost>(h: &mut H, v: &H::Value, s: &str) -> Result<H::Value, H::Error> {
    let mut t = h.template_string(v)?.0;
    t.extend(s.encode_utf16());
    Ok(h.literal(&JsString(t)))
}
fn for_each<H: BalanceHost, F: FnMut(&mut H, H::Value) -> Result<(), H::Error>>(
    h: &mut H,
    v: &H::Value,
    mut body: F,
) -> Result<(), H::Error> {
    let i = h.iterator(v)?;
    while let Some(v) = h.next(&i)? {
        if let Err(e) = body(h, v) {
            h.close_after_throw(&i);
            return Err(e);
        }
    }
    Ok(())
}
fn finite<H: BalanceHost>(h: &mut H, v: &H::Value) -> Result<bool, H::Error> {
    let n = h.global("Number")?;
    let result = method(h, &n, "isFinite", &[v.clone()])?;
    Ok(h.truthy(&result))
}
fn normalize<H: BalanceHost>(h: &mut H, coin: &H::Value) -> Result<H::Value, H::Error> {
    let v = if h.truthy(coin) {
        coin.clone()
    } else {
        lit(h, "")
    };
    let t = h.template_string(&v)?;
    let v = h.literal(&t);
    method(h, &v, "trim", &[])
}
fn stable<H: BalanceHost>(h: &H, v: &H::Value) -> bool {
    ["USDC", "USDT", "USD", "USDH"]
        .iter()
        .any(|n| h.strict_equal(v, &lit(h, n)))
}
fn ticker_price<H: BalanceHost>(
    h: &mut H,
    ticker: &H::Value,
    optional_root: bool,
    mid: bool,
) -> Result<H::Value, H::Error> {
    let number = h.global("Number")?;
    let get = |h: &mut H, n| {
        if optional_root {
            optional(h, ticker, n)
        } else {
            h.get(ticker, n)
        }
    };
    let mut value = get(h, "mark")?;
    if h.is_nullish(&value) {
        value = get(h, "last")?;
    }
    if h.is_nullish(&value) {
        value = get(h, "index")?;
    }
    if mid && h.is_nullish(&value) {
        value = get(h, "midPrice")?;
    }
    let u = h.undefined();
    h.call(&number, &u, &[value])
}
fn positive<H: BalanceHost>(h: &mut H, v: &H::Value) -> Result<bool, H::Error> {
    if !finite(h, v)? {
        return Ok(false);
    }
    let zero = h.number(0.0);
    let p = h.operator(BalanceOperator::Greater, v, &zero)?;
    Ok(h.truthy(&p))
}
pub fn get_spot_collateral_mark_price<H: BalanceHost>(
    h: &mut H,
    exchange: &H::Value,
    coin: &H::Value,
) -> Result<H::Value, H::Error> {
    let normal = normalize(h, coin)?;
    if !h.truthy(&normal) {
        return Ok(h.number(0.0));
    }
    let upper = method(h, &normal, "toUpperCase", &[])?;
    if stable(h, &upper) {
        return Ok(h.number(1.0));
    }
    let candidates = h.array()?;
    for i in 0..8 {
        let store = h.get(exchange, "store")?;
        let table = h.get(
            &store,
            if i == 5 || i == 6 {
                "tickersById"
            } else {
                "tickersBySymbol"
            },
        )?;
        let key = match i {
            0 => suffix(h, &normal, "/USDC")?,
            1 => suffix(h, &upper, "/USDC")?,
            2 => suffix(h, &upper, "/USDH")?,
            3 => suffix(h, &upper, "/USDT0")?,
            4 => suffix(h, &upper, "/USDE")?,
            5 => normal.clone(),
            6 => upper.clone(),
            _ => suffix(h, &upper, "-PERP")?,
        };
        let ticker = h.index(&table, &key)?;
        h.array_define(&candidates, i, &ticker)?;
    }
    let iterator = h.iterator(&candidates)?;
    while let Some(ticker) = h.next(&iterator)? {
        let result = (|| {
            if !h.truthy(&ticker) {
                return Ok(None);
            }
            let price = ticker_price(h, &ticker, false, false)?;
            Ok(if positive(h, &price)? {
                Some(price)
            } else {
                None
            })
        })();
        match result {
            Ok(Some(price)) => {
                h.close_after_return(&iterator)?;
                return Ok(price);
            }
            Ok(None) => {}
            Err(error) => {
                h.close_after_throw(&iterator);
                return Err(error);
            }
        }
    }
    Ok(h.number(0.0))
}
fn optional_upper<H: BalanceHost>(
    h: &mut H,
    candidate: &H::Value,
    name: &str,
) -> Result<H::Value, H::Error> {
    let value = h.get(candidate, name)?;
    if h.is_nullish(&value) {
        Ok(h.undefined())
    } else {
        method(h, &value, "toUpperCase", &[])
    }
}
pub fn run_spot_market_predicate<H: BalanceHost>(
    h: &mut H,
    capture: &SpotMarketPredicate<H::Value>,
    candidate: &H::Value,
) -> Result<bool, H::Error> {
    let base = optional_upper(h, candidate, "base")?;
    if h.strict_equal(&base, &capture.upper) {
        return Ok(true);
    }
    let id = optional_upper(h, candidate, "id")?;
    if h.strict_equal(&id, &capture.upper) {
        return Ok(true);
    }
    let symbol = optional_upper(h, candidate, "symbol")?;
    let key = suffix(h, &capture.upper, "/USDC")?;
    if h.strict_equal(&symbol, &key) {
        return Ok(true);
    }
    if h.is_undefined(&capture.token_index) {
        return Ok(false);
    }
    let info = h.get(candidate, "info")?;
    let tokens = optional(h, &info, "tokens")?;
    let first = if h.is_nullish(&tokens) {
        h.undefined()
    } else {
        let zero = h.number(0.0);
        h.index(&tokens, &zero)?
    };
    Ok(h.strict_equal(&first, &capture.token_index))
}
pub fn get_spot_coin_mark_price<H: BalanceHost>(
    h: &mut H,
    exchange: &H::Value,
    coin: &H::Value,
    token_index: &H::Value,
) -> Result<H::Value, H::Error> {
    let normal = normalize(h, coin)?;
    if !h.truthy(&normal) {
        return Ok(h.number(0.0));
    }
    let upper = method(h, &normal, "toUpperCase", &[])?;
    if stable(h, &upper) {
        return Ok(h.number(1.0));
    }
    let keys = h.array()?;
    h.array_define(&keys, 0, &normal)?;
    h.array_define(&keys, 1, &upper)?;
    let key = suffix(h, &normal, "/USDC")?;
    h.array_define(&keys, 2, &key)?;
    let key = suffix(h, &upper, "/USDC")?;
    h.array_define(&keys, 3, &key)?;
    let iterator = h.iterator(&keys)?;
    while let Some(key) = h.next(&iterator)? {
        let result = (|| {
            let store = h.get(exchange, "store")?;
            let by_id = h.get(&store, "tickersById")?;
            let first = h.index(&by_id, &key)?;
            let ticker = if h.truthy(&first) {
                first
            } else {
                let store = h.get(exchange, "store")?;
                let by_symbol = h.get(&store, "tickersBySymbol")?;
                h.index(&by_symbol, &key)?
            };
            let price = ticker_price(h, &ticker, true, true)?;
            Ok(if positive(h, &price)? {
                Some(price)
            } else {
                None
            })
        })();
        match result {
            Ok(Some(price)) => {
                h.close_after_return(&iterator)?;
                return Ok(price);
            }
            Ok(None) => {}
            Err(error) => {
                h.close_after_throw(&iterator);
                return Err(error);
            }
        }
    }
    let store = h.get(exchange, "store")?;
    let markets = h.get(&store, "markets")?;
    let find = h.get(&markets, "find")?;
    let predicate = h.spot_market_predicate(SpotMarketPredicate {
        upper: upper.clone(),
        token_index: token_index.clone(),
    })?;
    let market = h.call(&find, &markets, &[predicate])?;
    if !h.truthy(&market) {
        return Ok(h.number(0.0));
    }
    let store = h.get(exchange, "store")?;
    let by_id = h.get(&store, "tickersById")?;
    let id = h.get(&market, "id")?;
    let first = h.index(&by_id, &id)?;
    let ticker = if h.truthy(&first) {
        first
    } else {
        let store = h.get(exchange, "store")?;
        let by_symbol = h.get(&store, "tickersBySymbol")?;
        let symbol = h.get(&market, "symbol")?;
        h.index(&by_symbol, &symbol)?
    };
    let price = ticker_price(h, &ticker, true, true)?;
    Ok(if positive(h, &price)? {
        price
    } else {
        h.number(0.0)
    })
}
fn number_optional<H: BalanceHost>(h: &mut H, o: &H::Value, n: &str) -> Result<H::Value, H::Error> {
    let f = h.global("Number")?;
    let v = optional(h, o, n)?;
    let u = h.undefined();
    h.call(&f, &u, &[v])
}
fn free_amount<H: BalanceHost>(
    h: &mut H,
    total: &H::Value,
    hold: &H::Value,
) -> Result<H::Value, H::Error> {
    let math = h.global("Math")?;
    let max = h.get(&math, "max")?;
    let difference = h.operator(BalanceOperator::Subtract, total, hold)?;
    let zero = h.number(0.0);
    h.call(&max, &math, &[difference, zero])
}
pub fn build_unified_spot_collateral_snapshot<H: BalanceHost>(
    h: &mut H,
    exchange: &H::Value,
    balances: &H::Value,
) -> Result<H::Value, H::Error> {
    let currencies = h.object()?;
    let coins = h.object()?;
    let mut total = h.number(0.0);
    let mut held = h.number(0.0);
    for_each(h, balances, |h, balance| {
        let coin = optional(h, &balance, "coin")?;
        let coin = normalize(h, &coin)?;
        if !h.truthy(&coin) {
            return Ok(());
        }
        let amount = number_optional(h, &balance, "total")?;
        let hold = number_optional(h, &balance, "hold")?;
        if !finite(h, &amount)? || !finite(h, &hold)? {
            return Ok(());
        }
        let free = free_amount(h, &amount, &hold)?;
        let mark = method(h, exchange, "getSpotCollateralMarkPrice", &[coin.clone()])?;
        let notional = h.operator(BalanceOperator::Multiply, &amount, &mark)?;
        let hold_notional = h.operator(BalanceOperator::Multiply, &hold, &mark)?;
        let currency = h.object()?;
        h.define_data(&currency, "total", &amount)?;
        h.define_data(&currency, "used", &hold)?;
        h.define_data(&currency, "free", &free)?;
        let zero = h.number(0.0);
        h.define_data(&currency, "upnl", &zero)?;
        h.set_index(&currencies, &coin, &currency)?;
        let item = h.object()?;
        h.define_data(&item, "base", &coin)?;
        h.define_data(&item, "amount", &amount)?;
        h.define_data(&item, "notional", &notional)?;
        h.set_index(&coins, &coin, &item)?;
        total = h.operator(BalanceOperator::Add, &total, &notional)?;
        held = h.operator(BalanceOperator::Add, &held, &hold_notional)?;
        Ok(())
    })?;
    let result = h.object()?;
    h.define_data(&result, "total", &total)?;
    h.define_data(&result, "spotHoldNotional", &held)?;
    h.define_data(&result, "spotCurrencies", &currencies)?;
    h.define_data(&result, "coins", &coins)?;
    Ok(result)
}
fn parse_balance_amount<H: BalanceHost>(
    h: &mut H,
    balance: &H::Value,
    name: &str,
) -> Result<H::Value, H::Error> {
    let parse = h.global("parseFloat")?;
    let value = optional(h, balance, name)?;
    let value = if h.truthy(&value) {
        value
    } else {
        h.number(0.0)
    };
    let string = h.template_string(&value)?;
    let value = h.literal(&string);
    let u = h.undefined();
    h.call(&parse, &u, &[value])
}
pub fn build_spot_balance<H: BalanceHost>(
    h: &mut H,
    exchange: &H::Value,
    balances: &H::Value,
) -> Result<H::Value, H::Error> {
    let currencies = h.object()?;
    let coins = h.object()?;
    let mut total = h.number(0.0);
    let mut used = h.number(0.0);
    let mut free = h.number(0.0);
    for_each(h, balances, |h, item| {
        let coin = optional(h, &item, "coin")?;
        let coin = if h.truthy(&coin) { coin } else { lit(h, "") };
        let t = h.template_string(&coin)?;
        let coin = h.literal(&t);
        if !h.truthy(&coin) {
            return Ok(());
        }
        let amount = parse_balance_amount(h, &item, "total")?;
        let hold = parse_balance_amount(h, &item, "hold")?;
        let free_amount = free_amount(h, &amount, &hold)?;
        let currency = h.object()?;
        h.define_data(&currency, "free", &free_amount)?;
        h.define_data(&currency, "used", &hold)?;
        h.define_data(&currency, "total", &amount)?;
        let zero = h.number(0.0);
        h.define_data(&currency, "upnl", &zero)?;
        h.set_index(&currencies, &coin, &currency)?;
        let mark_fn = h.get(exchange, "getSpotCoinMarkPrice")?;
        let token = optional(h, &item, "token")?;
        let mark = h.call(&mark_fn, exchange, &[coin.clone(), token])?;
        let notional = h.operator(BalanceOperator::Multiply, &amount, &mark)?;
        let held_notional = h.operator(BalanceOperator::Multiply, &hold, &mark)?;
        let free_notional = h.operator(BalanceOperator::Multiply, &free_amount, &mark)?;
        let value = h.object()?;
        h.define_data(&value, "base", &coin)?;
        h.define_data(&value, "amount", &amount)?;
        h.define_data(&value, "notional", &notional)?;
        h.set_index(&coins, &coin, &value)?;
        total = h.operator(BalanceOperator::Add, &total, &notional)?;
        used = h.operator(BalanceOperator::Add, &used, &held_notional)?;
        free = h.operator(BalanceOperator::Add, &free, &free_notional)?;
        Ok(())
    })?;
    let result = h.object()?;
    h.define_data(&result, "total", &total)?;
    h.define_data(&result, "used", &used)?;
    h.define_data(&result, "free", &free)?;
    let zero = h.number(0.0);
    h.define_data(&result, "upnl", &zero)?;
    h.define_data(&result, "currencies", &currencies)?;
    h.define_data(&result, "coins", &coins)?;
    Ok(result)
}
pub fn get_authoritative_unified_usdc_availability<H: BalanceHost>(
    h: &mut H,
    state: &H::Value,
) -> Result<H::Value, H::Error> {
    let values = h.get(state, "tokenToAvailableAfterMaintenance")?;
    let array = h.global("Array")?;
    let is_array = method(h, &array, "isArray", &[values.clone()])?;
    if !h.truthy(&is_array) {
        return Ok(h.null());
    }
    let iterator = h.iterator(&values)?;
    while let Some(pair) = h.next(&iterator)? {
        let result = (|| {
            let [token, available] = h.pair(&pair)?;
            let zero = h.number(0.0);
            if !h.strict_equal(&token, &zero) {
                return Ok(None);
            }
            let amount = free(h, "Number", &[available])?;
            Ok(Some(if finite(h, &amount)? {
                amount
            } else {
                h.null()
            }))
        })();
        match result {
            Ok(Some(v)) => {
                h.close_after_return(&iterator)?;
                return Ok(v);
            }
            Ok(None) => {}
            Err(e) => {
                h.close_after_throw(&iterator);
                return Err(e);
            }
        }
    }
    Ok(h.null())
}
fn snapshot<H: BalanceHost>(h: &mut H, exchange: &H::Value) -> Result<H::Value, H::Error> {
    let build = h.get(exchange, "buildUnifiedSpotCollateralSnapshot")?;
    let state = h.get(exchange, "cachedSpotCollateralState")?;
    let balances = h.get(&state, "balances")?;
    h.call(&build, exchange, &[balances])
}
fn define_balance_numbers<H: BalanceHost>(
    h: &mut H,
    b: &H::Value,
    total: &H::Value,
    used: &H::Value,
    free: &H::Value,
    upnl: &H::Value,
) -> Result<(), H::Error> {
    h.define_data(b, "total", total)?;
    h.define_data(b, "used", used)?;
    h.define_data(b, "free", free)?;
    h.define_data(b, "upnl", upnl)
}
pub fn apply_balance_from_current_mode<H: BalanceHost>(
    h: &mut H,
    exchange: &H::Value,
) -> Result<(), H::Error> {
    let use_spot = method(h, exchange, "shouldUseSpotCollateralBalance", &[])?;
    if h.truthy(&use_spot) {
        let has = h.get(exchange, "hasSpotCollateralSnapshot")?;
        if h.truthy(&has) {
            let spot = snapshot(h, exchange)?;
            let upnl = h.get(exchange, "latestCorePerpsUpnl")?;
            let authoritative = h.get(exchange, "latestAuthoritativeUnifiedBalanceSnapshot")?;
            let value = optional(h, &authoritative, "total")?;
            let total = if h.is_nullish(&value) {
                h.get(&spot, "total")?
            } else {
                value
            };
            let value = optional(h, &authoritative, "used")?;
            let used = if h.is_nullish(&value) {
                let margin = h.get(exchange, "latestCorePerpsUsedMargin")?;
                let holds = h.get(&spot, "spotHoldNotional")?;
                h.operator(BalanceOperator::Add, &margin, &holds)?
            } else {
                value
            };
            let value = optional(h, &authoritative, "free")?;
            let free = if h.is_nullish(&value) {
                let spot_total = h.get(&spot, "total")?;
                let sum = h.operator(BalanceOperator::Add, &spot_total, &upnl)?;
                h.operator(BalanceOperator::Subtract, &sum, &used)?
            } else {
                value
            };
            let currencies = h.object()?;
            let source = h.get(&spot, "spotCurrencies")?;
            h.spread_into(&currencies, &source)?;
            let class = h.global("HyperliquidExchange")?;
            let key = h.get(&class, "UNIFIED_MARGIN_CURRENCY")?;
            let currency = h.object()?;
            define_balance_numbers(h, &currency, &total, &used, &free, &upnl)?;
            h.define_index(&currencies, &key, &currency)?;
            let store = h.get(exchange, "store")?;
            let update = h.get(&store, "update")?;
            let payload = h.object()?;
            let balance = h.object()?;
            define_balance_numbers(h, &balance, &total, &used, &free, &upnl)?;
            h.define_data(&balance, "currencies", &currencies)?;
            let coins = h.get(&spot, "coins")?;
            h.define_data(&balance, "coins", &coins)?;
            let info = h.object()?;
            let mode = h.get(exchange, "userAbstractionMode")?;
            h.define_data(&info, "userAbstractionMode", &mode)?;
            let source = lit(h, "spotClearinghouseState+clearinghouseState");
            h.define_data(&info, "source", &source)?;
            let currencies = h.get(&spot, "spotCurrencies")?;
            h.define_data(&info, "spotCollateralCurrencies", &currencies)?;
            h.define_data(&balance, "info", &info)?;
            h.define_data(&payload, "balance", &balance)?;
            h.call(&update, &store, &[payload])?;
            return Ok(());
        }
    }
    let account = h.get(exchange, "latestCorePerpsAccountValue")?;
    let used = h.get(exchange, "latestCorePerpsUsedMargin")?;
    let upnl = h.get(exchange, "latestCorePerpsUpnl")?;
    let use_spot = method(h, exchange, "shouldUseSpotCollateralBalance", &[])?;
    let spot = if !h.truthy(&use_spot) {
        let has = h.get(exchange, "hasSpotCollateralSnapshot")?;
        if h.truthy(&has) {
            {
                let value = snapshot(h, exchange)?;
                if h.truthy(&value) { Some(value) } else { None }
            }
        } else {
            None
        }
    } else {
        None
    };
    let store = h.get(exchange, "store")?;
    let update = h.get(&store, "update")?;
    let payload = h.object()?;
    let balance = h.object()?;
    let total = h.operator(BalanceOperator::Subtract, &account, &upnl)?;
    h.define_data(&balance, "total", &total)?;
    h.define_data(&balance, "used", &used)?;
    let free = h.operator(BalanceOperator::Subtract, &account, &used)?;
    h.define_data(&balance, "free", &free)?;
    h.define_data(&balance, "upnl", &upnl)?;
    let spread = h.object()?;
    if let Some(spot) = &spot {
        let coins = h.get(spot, "coins")?;
        h.define_data(&spread, "coins", &coins)?;
    }
    h.spread_into(&balance, &spread)?;
    let info = h.object()?;
    let mode = h.get(exchange, "userAbstractionMode")?;
    h.define_data(&info, "userAbstractionMode", &mode)?;
    let source = lit(
        h,
        if spot.is_some() {
            "clearinghouseState+spotHoldings"
        } else {
            "clearinghouseState"
        },
    );
    h.define_data(&info, "source", &source)?;
    let spread = h.object()?;
    if let Some(spot) = &spot {
        let currencies = h.get(spot, "spotCurrencies")?;
        h.define_data(&spread, "spotCollateralCurrencies", &currencies)?;
    }
    h.spread_into(&info, &spread)?;
    h.define_data(&balance, "info", &info)?;
    h.define_data(&payload, "balance", &balance)?;
    h.call(&update, &store, &[payload])?;
    Ok(())
}
pub fn apply_core_perps_balance_snapshot<H: BalanceHost>(
    h: &mut H,
    exchange: &H::Value,
    snapshot: &H::Value,
) -> Result<(), H::Error> {
    let mut values = Vec::new();
    for name in ["accountValue", "totalMarginUsed", "upnl"] {
        let number = h.global("Number")?;
        let value = h.get(snapshot, name)?;
        let u = h.undefined();
        values.push(h.call(&number, &u, &[value])?);
    }
    for value in &values {
        if !finite(h, value)? {
            return Ok(());
        }
    }
    for (name, value) in [
        "latestCorePerpsAccountValue",
        "latestCorePerpsUsedMargin",
        "latestCorePerpsUpnl",
    ]
    .into_iter()
    .zip(values)
    {
        h.set(exchange, name, &value)?;
    }
    method(h, exchange, "applyBalanceFromCurrentMode", &[])?;
    Ok(())
}
pub fn apply_spot_balances<H: BalanceHost>(
    h: &mut H,
    exchange: &H::Value,
    balances: &H::Value,
) -> Result<(), H::Error> {
    let array = h.global("Array")?;
    let yes = method(h, &array, "isArray", &[balances.clone()])?;
    if !h.truthy(&yes) {
        return Ok(());
    }
    let store = h.get(exchange, "store")?;
    let update = h.get(&store, "update")?;
    let payload = h.object()?;
    let balance = method(h, exchange, "buildSpotBalance", &[balances.clone()])?;
    h.define_data(&payload, "balance", &balance)?;
    h.call(&update, &store, &[payload])?;
    Ok(())
}
fn request<H: BalanceHost>(
    h: &mut H,
    exchange: &H::Value,
    force: &H::Value,
    name: &str,
) -> Result<(), H::Error> {
    let force = if h.is_undefined(force) {
        h.boolean(false)
    } else {
        force.clone()
    };
    method(h, exchange, name, &[force])?;
    Ok(())
}
pub fn request_unified_spot_collateral_refresh<H: BalanceHost>(
    h: &mut H,
    exchange: &H::Value,
    force: &H::Value,
) -> Result<(), H::Error> {
    request(h, exchange, force, "refreshUnifiedSpotCollateralBalance")
}
pub fn request_spot_balance_refresh<H: BalanceHost>(
    h: &mut H,
    exchange: &H::Value,
    force: &H::Value,
) -> Result<(), H::Error> {
    request(h, exchange, force, "refreshSpotBalance")
}
