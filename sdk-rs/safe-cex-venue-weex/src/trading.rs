//! Source order/position normalization and trading actions over host primitives.
use crate::types::ENDPOINTS;
use std::{future::Future, pin::Pin};
type F<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
type Map<'a, V, E> = Box<dyn FnMut(V, usize) -> Result<V, E> + 'a>;
type Filter<'a, V, E> = Box<dyn FnMut(V, usize) -> Result<bool, E> + 'a>;
pub struct WeexTradingRuntime<H, V, E> {
    pub get: fn(&H, &V, &str) -> Result<V, E>,
    pub optional_get: fn(&H, &V, &str) -> Result<V, E>,
    pub truthy: fn(&V) -> bool,
    pub nullish: fn(&V) -> bool,
    pub is_undefined: fn(&V) -> bool,
    pub undefined: fn() -> V,
    pub null: fn() -> V,
    pub text: fn(&str) -> V,
    pub string_units: fn(Vec<u16>) -> V,
    pub number: fn(f64) -> V,
    pub boolean: fn(bool) -> V,
    pub object: fn(Vec<(&'static str, V)>) -> V,
    pub array: fn(Vec<V>) -> V,
    pub string: fn(&H, V) -> Result<V, E>,
    pub template: fn(&H, &V) -> Result<Vec<u16>, E>,
    pub uppercase: fn(&H, V) -> Result<V, E>,
    pub equals: fn(&V, &str) -> bool,
    pub strict_true: fn(&V) -> bool,
    pub strict_false: fn(&V) -> bool,
    pub is_string: fn(&V) -> bool,
    pub is_boolean: fn(&V) -> bool,
    pub is_object: fn(&V) -> bool,
    pub is_number: fn(&V) -> bool,
    pub number_value: fn(&V) -> f64,
    pub to_number: fn(&H, V, V) -> Result<V, E>,
    pub integer_string: fn(&H, V) -> Result<V, E>,
    pub enum_normalize: fn(&H, &'static str, V) -> Result<V, E>,
    pub table: fn(&H, &'static str, V) -> Result<V, E>,
    pub timestamp: fn(&H, V) -> Result<V, E>,
    pub position_id: fn(&H, V, V, V) -> Result<V, E>,
    pub copy: fn(&H, V) -> Result<V, E>,
    pub define: fn(&H, &V, &'static str, V) -> Result<(), E>,
    pub omit: fn(&H, V) -> Result<V, E>,
    pub adjust: fn(&H, V, V) -> Result<V, E>,
    pub uuid: fn(&H) -> Result<V, E>,
    pub method: fn(&H, &'static str) -> Result<V, E>,
    pub call: fn(&H, V, Vec<V>) -> Result<V, E>,
    pub call_async: for<'a> fn(&'a H, V, Vec<V>) -> Result<F<'a, V, E>, E>,
    /// Start host-owned work eagerly; dropping its observation must not cancel it.
    pub request: for<'a> fn(&'a H, &'static str, &'static str, V) -> Result<F<'a, V, E>, E>,
    pub error_value: fn(&E) -> V,
    pub error_sink: fn(&H) -> Result<V, E>,
    pub emit: fn(&H, V, V) -> Result<(), E>,
    pub new_error: fn(&H, V) -> E,
    pub map: for<'a> fn(&'a H, &V, Map<'a, V, E>) -> Result<V, E>,
    pub filter: for<'a> fn(&'a H, &V, Filter<'a, V, E>) -> Result<V, E>,
    pub some: for<'a> fn(&'a H, &V, Filter<'a, V, E>) -> Result<bool, E>,
    pub for_each: for<'a> fn(&'a H, &V, Map<'a, V, E>) -> Result<(), E>,
    pub flat_map: for<'a> fn(&'a H, &V, Map<'a, V, E>) -> Result<V, E>,
    pub new_set: fn(&H, &V) -> Result<V, E>,
    pub set_has: fn(&H, &V, &V) -> Result<bool, E>,
    pub new_map: fn(&H) -> Result<V, E>,
    pub map_has: fn(&H, &V, &V) -> Result<bool, E>,
    pub map_get: fn(&H, &V, &V) -> Result<V, E>,
    pub map_set: fn(&H, &V, V, V) -> Result<(), E>,
    pub length: fn(&H, &V) -> Result<usize, E>,
    pub is_array: fn(&H, &V) -> Result<bool, E>,
    pub includes: fn(&H, &V, &V) -> Result<bool, E>,
    pub set_size: fn(&H, &V) -> Result<usize, E>,
    pub push_method: fn(&H, &V) -> Result<V, E>,
    pub push: fn(&H, V, &V, V) -> Result<(), E>,
    pub spread_push: fn(&H, V, &V, V) -> Result<(), E>,
    pub iterator: fn(&H, &V) -> Result<V, E>,
    pub next: fn(&H, &V) -> Result<Option<V>, E>,
    pub close_on_error: fn(&H, &V, &E),
    pub contains: fn(&H, &V, &str) -> Result<bool, E>,
    pub has: fn(&H, &V, &str) -> Result<bool, E>,
    pub valid_order_id: fn(&V) -> bool,
    pub markets_by_symbol: fn(&H) -> Result<V, E>,
    pub lookup: fn(&H, &V, V) -> Result<V, E>,
    pub markets: fn(&H) -> Result<V, E>,
    pub public_only: fn(&H) -> Result<bool, E>,
    pub positions: fn(&H) -> Result<V, E>,
    pub account_mode: fn(&H) -> Result<V, E>,
    pub fake_positions: fn(&H, V, V, V, V) -> Result<V, E>,
    pub extract_array: fn(&H, V) -> Result<V, E>,
    pub join: fn(&H, &V, &str) -> Result<V, E>,
    pub unique_id: fn(&H, V) -> Result<V, E>,
    /// Source array spread, including iterator reads; not Array.prototype.concat.
    pub concat: fn(&H, V, V) -> Result<V, E>,
}
fn prop<H, V, E>(h: &H, r: &WeexTradingRuntime<H, V, E>, v: &V, keys: &[&str]) -> Result<V, E> {
    for (index, key) in keys.iter().enumerate() {
        let x = (r.get)(h, v, key)?;
        if index + 1 == keys.len() || !(r.nullish)(&x) {
            return Ok(x);
        }
    }
    Ok((r.undefined)())
}
fn opt<H, V, E>(h: &H, r: &WeexTradingRuntime<H, V, E>, v: &V, key: &str) -> Result<V, E> {
    (r.optional_get)(h, v, key)
}
fn or<H, V, E>(r: &WeexTradingRuntime<H, V, E>, a: V, b: impl FnOnce() -> V) -> V {
    if (r.truthy)(&a) { a } else { b() }
}
fn num<H, V, E>(h: &H, r: &WeexTradingRuntime<H, V, E>, v: &V, keys: &[&str]) -> Result<V, E> {
    (r.to_number)(h, prop(h, r, v, keys)?, (r.number)(0.0))
}
fn norm<H, V, E>(h: &H, r: &WeexTradingRuntime<H, V, E>, kind: &'static str, v: V) -> Result<V, E> {
    (r.enum_normalize)(h, kind, v)
}
fn upper<H, V, E>(h: &H, r: &WeexTradingRuntime<H, V, E>, v: V) -> Result<V, E> {
    (r.uppercase)(h, (r.string)(h, v)?)
}
fn words<H, V, E>(
    h: &H,
    r: &WeexTradingRuntime<H, V, E>,
    v: &V,
    prefix: &str,
    suffix: &str,
) -> Result<V, E> {
    let mut s: Vec<u16> = prefix.encode_utf16().collect();
    s.extend((r.template)(h, v)?);
    s.extend(suffix.encode_utf16());
    Ok((r.string_units)(s))
}
fn emit<H, V, E>(h: &H, r: &WeexTradingRuntime<H, V, E>, message: V) -> Result<(), E> {
    let sink = (r.error_sink)(h)?;
    (r.emit)(h, sink, message)
}
fn fail<H, V: Clone, E>(h: &H, r: &WeexTradingRuntime<H, V, E>, message: V) -> Result<V, E> {
    emit(h, r, message.clone())?;
    Err((r.new_error)(h, message))
}
fn error_message<H, V: Clone, E>(
    h: &H,
    r: &WeexTradingRuntime<H, V, E>,
    e: &E,
    placement: bool,
    default: Option<&str>,
) -> Result<V, E> {
    let e = (r.error_value)(e);
    for key in if placement {
        &["errorMessage", "msg", "message"][..]
    } else {
        &["msg"][..]
    } {
        let response = opt(h, r, &e, "response")?;
        let data = opt(h, r, &response, "data")?;
        let x = opt(h, r, &data, key)?;
        if (r.truthy)(&x) {
            return Ok(x);
        }
    }
    let x = opt(h, r, &e, "message")?;
    Ok(or(r, x, || {
        default.map(|s| (r.text)(s)).unwrap_or_else(r.undefined)
    }))
}
fn endpoint(key: &str) -> &'static str {
    ENDPOINTS.iter().find(|(k, _)| *k == key).unwrap().1
}
fn invoke<H, V, E>(
    h: &H,
    r: &WeexTradingRuntime<H, V, E>,
    name: &'static str,
    args: Vec<V>,
) -> Result<V, E> {
    let method = (r.method)(h, name)?;
    (r.call)(h, method, args)
}
fn invoke_async<'a, H, V: 'a, E: 'a>(
    h: &'a H,
    r: &'a WeexTradingRuntime<H, V, E>,
    name: &'static str,
    args: Vec<V>,
) -> Result<F<'a, V, E>, E> {
    let method = (r.method)(h, name)?;
    (r.call_async)(h, method, args)
}
async fn js_await<T>(future: impl Future<Output = T>) -> T {
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
    future.await
}
fn eager<'a, T: 'a, E: 'a>(mut future: F<'a, T, E>) -> F<'a, T, E> {
    let waker = std::task::Waker::noop().clone();
    match future
        .as_mut()
        .poll(&mut std::task::Context::from_waker(&waker))
    {
        std::task::Poll::Ready(result) => Box::pin(std::future::ready(result)),
        std::task::Poll::Pending => future,
    }
}

pub fn normalize_order<H, V: Clone, E>(
    h: &H,
    r: &WeexTradingRuntime<H, V, E>,
    raw: V,
    market: V,
) -> Result<V, E> {
    let id = (r.integer_string)(h, prop(h, r, &raw, &["orderId", "id", "algoId"])?)?;
    let mut symbol = prop(h, r, &raw, &["symbol", "s"])?;
    if (r.nullish)(&symbol) {
        symbol = opt(h, r, &market, "symbol")?;
    }
    if (r.nullish)(&symbol) {
        symbol = (r.text)("");
    }
    let symbol = (r.string)(h, symbol)?;
    let side = norm(h, r, "side", prop(h, r, &raw, &["side", "orderSide"])?)?;
    if !(r.truthy)(&id) || !(r.truthy)(&symbol) || !(r.truthy)(&side) {
        return Ok((r.null)());
    }
    let kind = norm(
        h,
        r,
        "type",
        prop(h, r, &raw, &["type", "orderType", "planType"])?,
    )?;
    let amount = (r.number)(
        (r.number_value)(&num(h, r, &raw, &["origQty", "quantity", "size", "qty"])?).abs(),
    );
    let filled = (r.number)(
        (r.number_value)(&num(
            h,
            r,
            &raw,
            &["executedQty", "cumFillSize", "fillSize"],
        )?)
        .abs(),
    );
    let price = num(h, r, &raw, &["price", "actualPrice", "executePrice"])?;
    let trigger = num(h, r, &raw, &["stopPrice", "triggerPrice"])?;
    let price = or(r, price, || trigger);
    let status = norm(h, r, "status", prop(h, r, &raw, &["status", "algoStatus"])?)?;
    let remaining = (r.number)(safe_cex_num::math_max(
        (r.number_value)(&amount) - (r.number_value)(&filled),
        0.0,
    ));
    let reduce = (r.boolean)((r.truthy)(&(r.get)(h, &raw, "reduceOnly")?));
    let tif = (r.get)(h, &raw, "timeInForce")?;
    let tif = upper(h, r, or(r, tif, || (r.text)("")))?;
    let tif = (r.table)(h, "TIME_IN_FORCE", tif)?;
    let tif = or(r, tif, || (r.text)("GoodTillCancel"));
    let time = (r.timestamp)(
        h,
        prop(
            h,
            r,
            &raw,
            &[
                "updateTime",
                "updatedTime",
                "time",
                "createTime",
                "createdTime",
            ],
        )?,
    )?;
    let trigger_type = |first: &str, second: &str| -> Result<V, E> {
        let a = (r.get)(h, &raw, first)?;
        let condition = if (r.truthy)(&a) {
            true
        } else {
            (r.truthy)(&(r.get)(h, &raw, second)?)
        };
        if condition {
            norm(h, r, "trigger", prop(h, r, &raw, &[first, second])?)
        } else {
            Ok((r.undefined)())
        }
    };
    let tp = trigger_type("tpOrderType", "TpWorkingType")?;
    let sl = trigger_type("workingType", "SlWorkingType")?;
    let take = num(h, r, &raw, &["tpTriggerPrice", "takeProfit"])?;
    let take = or(r, take, r.undefined);
    let stop = num(h, r, &raw, &["slTriggerPrice", "stopLoss"])?;
    let stop = or(r, stop, r.undefined);
    (r.omit)(
        h,
        (r.object)(vec![
            ("id", id),
            ("status", status),
            ("symbol", symbol),
            ("type", kind),
            ("side", side),
            ("price", price),
            ("amount", amount),
            ("filled", filled),
            ("remaining", remaining),
            ("reduceOnly", reduce),
            ("timeInForce", tif),
            ("lastUpdate", time),
            ("info", raw),
            ("tpTriggerType", tp),
            ("slTriggerType", sl),
            ("takeProfit", take),
            ("stopLoss", stop),
        ]),
    )
}

pub fn normalize_algo_order<H, V: Clone, E>(
    h: &H,
    r: &WeexTradingRuntime<H, V, E>,
    raw: V,
    market: V,
) -> Result<V, E> {
    let kind = upper(
        h,
        r,
        or_null(r, prop(h, r, &raw, &["orderType", "type"])?, || {
            (r.text)("")
        }),
    )?;
    let plan = upper(
        h,
        r,
        or_null(r, (r.get)(h, &raw, "planType")?, || (r.text)("")),
    )?;
    let algo = upper(
        h,
        r,
        or_null(r, (r.get)(h, &raw, "algoType")?, || (r.text)("")),
    )?;
    let trigger = ["STOP", "STOP_MARKET", "TAKE_PROFIT", "TAKE_PROFIT_MARKET"]
        .iter()
        .any(|s| (r.equals)(&kind, s));
    let protective = (r.strict_true)(&(r.get)(h, &raw, "reduceOnly")?)
        || (r.equals)(&plan, "STOP_LOSS")
        || (r.equals)(&plan, "TAKE_PROFIT")
        || (r.equals)(&algo, "TPSL")
        || (r.equals)(&algo, "TP_SL");
    let normalized =
        if protective && ((r.equals)(&plan, "STOP_LOSS") || (r.equals)(&plan, "TAKE_PROFIT")) {
            plan
        } else if trigger && !protective {
            (r.text)("CONDITIONAL")
        } else {
            prop(h, r, &raw, &["orderType", "planType"])?
        };
    let next = (r.copy)(h, raw.clone())?;
    (r.define)(
        h,
        &next,
        "orderId",
        prop(h, r, &raw, &["algoId", "orderId"])?,
    )?;
    (r.define)(h, &next, "type", normalized)?;
    (r.define)(
        h,
        &next,
        "status",
        prop(h, r, &raw, &["algoStatus", "status"])?,
    )?;
    (r.define)(h, &next, "origQty", (r.get)(h, &raw, "quantity")?)?;
    let price = (r.get)(h, &raw, "price")?;
    let price = if (r.truthy)(&price) {
        price
    } else {
        (r.get)(h, &raw, "actualPrice")?
    };
    (r.define)(h, &next, "price", price)?;
    (r.define)(h, &next, "stopPrice", (r.get)(h, &raw, "triggerPrice")?)?;
    (r.define)(h, &next, "side", (r.get)(h, &raw, "side")?)?;
    let base = normalize_order(h, r, next, market.clone())?;
    let orders = (r.array)(if (r.truthy)(&base) {
        vec![base]
    } else {
        vec![]
    });
    let parent = (r.integer_string)(h, prop(h, r, &raw, &["algoId", "orderId"])?)?;
    let symbol = prop(h, r, &raw, &["symbol"])?;
    let symbol = if (r.nullish)(&symbol) {
        opt(h, r, &market, "symbol")?
    } else {
        symbol
    };
    let symbol = (r.string)(h, or_null(r, symbol, || (r.text)("")))?;
    let side = norm(h, r, "side", prop(h, r, &raw, &["side", "orderSide"])?)?;
    if !(r.truthy)(&parent) || !(r.truthy)(&symbol) || !(r.truthy)(&side) {
        return Ok(orders);
    }
    for (suffix, trigger_field, execution_field, type_first, type_second) in [
        (
            "tp",
            "tpTriggerPrice",
            "tpPrice",
            "tpOrderType",
            "TpWorkingType",
        ),
        (
            "sl",
            "slTriggerPrice",
            "slPrice",
            "workingType",
            "SlWorkingType",
        ),
    ] {
        let trigger = (r.get)(h, &raw, trigger_field)?;
        let execution = (r.get)(h, &raw, execution_field)?;
        let trigger_type = prop(h, r, &raw, &[type_first, type_second])?;
        let price = (r.to_number)(h, trigger, (r.number)(0.0))?;
        if !(r.truthy)(&price) {
            continue;
        }
        let push = (r.push_method)(h, &orders)?;
        let id = words(
            h,
            r,
            &parent,
            "",
            if suffix == "tp" { "__tp" } else { "__sl" },
        )?;
        let status = norm(h, r, "status", prop(h, r, &raw, &["algoStatus", "status"])?)?;
        let amount = (r.number)((r.number_value)(&num(h, r, &raw, &["quantity", "size"])?).abs());
        let remaining =
            (r.number)((r.number_value)(&num(h, r, &raw, &["quantity", "size"])?).abs());
        let tif = (r.get)(h, &raw, "timeInForce")?;
        let tif = upper(h, r, or(r, tif, || (r.text)("")))?;
        let tif = or(r, (r.table)(h, "TIME_IN_FORCE", tif)?, || {
            (r.text)("GoodTillCancel")
        });
        let time = (r.timestamp)(h, prop(h, r, &raw, &["updateTime", "createTime"])?)?;
        let tp = if suffix == "tp" {
            norm(h, r, "trigger", trigger_type.clone())?
        } else {
            (r.undefined)()
        };
        let sl = if suffix == "sl" {
            norm(h, r, "trigger", trigger_type)?
        } else {
            (r.undefined)()
        };
        let notional = (r.to_number)(h, execution, (r.number)(0.0))?;
        let notional = or(r, notional, r.undefined);
        let value = (r.omit)(
            h,
            (r.object)(vec![
                ("id", id),
                ("parentId", parent.clone()),
                ("status", status),
                ("symbol", symbol.clone()),
                (
                    "type",
                    (r.text)(if suffix == "tp" {
                        "take_profit_market"
                    } else {
                        "stop_market"
                    }),
                ),
                ("side", side.clone()),
                ("price", price),
                ("amount", amount),
                ("filled", (r.number)(0.0)),
                ("remaining", remaining),
                ("reduceOnly", (r.boolean)(true)),
                ("timeInForce", tif),
                ("lastUpdate", time),
                ("info", raw.clone()),
                ("tpTriggerType", tp),
                ("slTriggerType", sl),
                ("extra", (r.object)(vec![("remainingNotional", notional)])),
            ]),
        )?;
        (r.push)(h, push, &orders, value)?;
    }
    Ok(orders)
}
fn or_null<H, V, E>(r: &WeexTradingRuntime<H, V, E>, a: V, b: impl FnOnce() -> V) -> V {
    if (r.nullish)(&a) { b() } else { a }
}

pub fn normalize_position<H, V: Clone, E>(
    h: &H,
    r: &WeexTradingRuntime<H, V, E>,
    raw: V,
    market: V,
    fallback: V,
) -> Result<V, E> {
    let symbol = (r.get)(h, &raw, "symbol")?;
    let symbol = if (r.nullish)(&symbol) {
        opt(h, r, &market, "symbol")?
    } else {
        symbol
    };
    let symbol = (r.string)(h, or_null(r, symbol, || (r.text)("")))?;
    if !(r.truthy)(&symbol) {
        return Ok((r.null)());
    }
    let side = norm(
        h,
        r,
        "position_side",
        prop(h, r, &raw, &["side", "positionSide"])?,
    )?;
    let mode = upper(
        h,
        r,
        or_null(
            r,
            prop(h, r, &raw, &["separatedMode", "separatedType"])?,
            || (r.text)(""),
        ),
    )?;
    let mode = (r.table)(h, "POSITION_MODE", mode)?;
    let margin = upper(
        h,
        r,
        or_null(r, (r.get)(h, &raw, "marginType")?, || (r.text)("")),
    )?;
    let margin = (r.table)(h, "MARGIN_TYPE", margin)?;
    let mode = if (r.truthy)(&mode) {
        mode
    } else {
        or(r, opt(h, r, &fallback, "positionMode")?, || {
            (r.text)("oneway")
        })
    };
    let margin = if (r.truthy)(&margin) {
        margin
    } else {
        or(r, opt(h, r, &fallback, "marginType")?, || (r.text)("cross"))
    };
    let contracts = (r.number)(
        (r.number_value)(&num(h, r, &raw, &["size", "positionAmt", "positions"])?).abs(),
    );
    let notional =
        (r.number)((r.number_value)(&num(h, r, &raw, &["openValue", "notional"])?).abs());
    let entry = if (r.truthy)(&contracts) {
        (r.number)((r.number_value)(&notional) / (r.number_value)(&contracts))
    } else {
        num(h, r, &raw, &["entryPrice", "averagePrice"])?
    };
    let raw_leverage = (r.get)(h, &raw, "leverage")?;
    let fallback_leverage = or(r, opt(h, r, &fallback, "leverage")?, || (r.number)(1.0));
    let leverage = (r.to_number)(h, raw_leverage, fallback_leverage)?;
    let pnl = num(
        h,
        r,
        &raw,
        &["unrealizePnl", "unRealizedProfit", "unrealizedPnl"],
    )?;
    let id = (r.position_id)(h, symbol.clone(), side.clone(), mode.clone())?;
    let realized = num(h, r, &raw, &["realizedPnl", "realizePnl"])?;
    let liquidation = num(h, r, &raw, &["liquidatePrice", "liquidationPrice"])?;
    let margin_size = num(h, r, &raw, &["marginSize", "isolatedMargin"])?;
    let time = (r.timestamp)(
        h,
        prop(h, r, &raw, &["updatedTime", "updateTime", "createdTime"])?,
    )?;
    Ok((r.object)(vec![
        ("id", id),
        ("symbol", symbol),
        ("side", side),
        ("entryPrice", entry),
        ("notional", notional),
        ("leverage", leverage),
        ("unrealizedPnl", pnl),
        ("realizedPnl", realized),
        ("contracts", contracts),
        ("liquidationPrice", liquidation),
        ("marginType", margin),
        ("positionMode", mode),
        ("marginSize", margin_size),
        ("info", raw),
        ("lastUpdate", time),
    ]))
}

fn adjusted<H, V, E>(
    h: &H,
    r: &WeexTradingRuntime<H, V, E>,
    opts: &V,
    field: &str,
    market: &V,
    step: &str,
) -> Result<V, E> {
    let value = (r.get)(h, opts, field)?;
    let precision = (r.get)(h, market, "precision")?;
    let step = (r.get)(h, &precision, step)?;
    (r.adjust)(h, value, step)
}
fn minted<H, V, E>(h: &H, r: &WeexTradingRuntime<H, V, E>) -> Result<V, E> {
    let uuid = (r.uuid)(h)?;
    let mut value: Vec<u16> = "ts".encode_utf16().collect();
    value.extend((r.template)(h, &uuid)?);
    value.truncate(36);
    Ok((r.string_units)(value))
}
fn reverse<H, V, E>(
    h: &H,
    r: &WeexTradingRuntime<H, V, E>,
    opts: &V,
    field: &str,
    table: &'static str,
) -> Result<V, E> {
    (r.table)(h, table, (r.get)(h, opts, field)?)
}
fn preset<H, V, E>(
    h: &H,
    r: &WeexTradingRuntime<H, V, E>,
    opts: &V,
    field: &str,
    market: &V,
) -> Result<V, E> {
    if (r.truthy)(&(r.get)(h, opts, field)?) {
        let value = adjusted(h, r, opts, field, market, "price")?;
        words(h, r, &value, "", "")
    } else {
        Ok((r.undefined)())
    }
}
fn working<H, V, E>(
    h: &H,
    r: &WeexTradingRuntime<H, V, E>,
    opts: &V,
    condition: &str,
    field: &str,
    default: &str,
) -> Result<V, E> {
    if (r.truthy)(&(r.get)(h, opts, condition)?) {
        let v = or(r, (r.get)(h, opts, field)?, || (r.text)(default));
        (r.table)(h, "REVERSE_ORDER_TRIGGER_TYPE", v)
    } else {
        Ok((r.undefined)())
    }
}

pub fn format_order_payload<H, V: Clone, E>(
    h: &H,
    r: &WeexTradingRuntime<H, V, E>,
    opts: V,
) -> Result<V, E> {
    let markets = (r.markets_by_symbol)(h)?;
    let market = (r.lookup)(h, &markets, (r.get)(h, &opts, "symbol")?)?;
    if !(r.truthy)(&market) {
        let message = words(h, r, &(r.get)(h, &opts, "symbol")?, "Market ", " not found")?;
        return fail(h, r, message);
    }
    let amount = adjusted(h, r, &opts, "amount", &market, "amount")?;
    let price = if (r.truthy)(&(r.get)(h, &opts, "price")?) {
        adjusted(h, r, &opts, "price", &market, "price")?
    } else {
        (r.undefined)()
    };
    let side = invoke(h, r, "inferPositionSide", vec![opts.clone()])?;
    let position_side = (r.table)(h, "REVERSE_POSITION_SIDE", side)?;
    if (r.equals)(&(r.get)(h, &opts, "type")?, "conditional") {
        let trigger = (r.get)(h, &opts, "triggerPrice")?;
        if !(r.is_number)(&trigger)
            || !(r.number_value)(&trigger).is_finite()
            || (r.number_value)(&trigger) <= 0.0
        {
            return fail(
                h,
                r,
                (r.text)("Trigger price is required for WEEX opening conditional orders"),
            );
        }
        let symbol = (r.get)(h, &opts, "symbol")?;
        let side = reverse(h, r, &opts, "side", "REVERSE_ORDER_SIDE")?;
        let method = (r.method)(h, "inferPositionSide")?;
        let inferred = (r.call)(
            h,
            method,
            vec![(r.object)(vec![
                ("side", (r.get)(h, &opts, "side")?),
                ("reduceOnly", (r.boolean)(false)),
            ])],
        )?;
        let ps = (r.table)(h, "REVERSE_POSITION_SIDE", inferred)?;
        let kind = (r.text)(if (r.truthy)(&(r.get)(h, &opts, "price")?) {
            "STOP"
        } else {
            "STOP_MARKET"
        });
        let qty = words(h, r, &amount, "", "")?;
        let price = if (r.truthy)(&(r.get)(h, &opts, "price")?) {
            words(h, r, &price, "", "")?
        } else {
            (r.undefined)()
        };
        let precision = (r.get)(h, &market, "precision")?;
        let step = (r.get)(h, &precision, "price")?;
        let trigger = (r.adjust)(h, trigger, step)?;
        let trigger = words(h, r, &trigger, "", "")?;
        let client = minted(h, r)?;
        let tp = preset(h, r, &opts, "takeProfit", &market)?;
        let sl = preset(h, r, &opts, "stopLoss", &market)?;
        let tp_work = working(h, r, &opts, "takeProfit", "tpTriggerType", "last_price")?;
        let sl_work = working(h, r, &opts, "stopLoss", "slTriggerType", "mark_price")?;
        return (r.omit)(
            h,
            (r.object)(vec![
                ("symbol", symbol),
                ("side", side),
                ("positionSide", ps),
                ("type", kind),
                ("quantity", qty),
                ("price", price),
                ("triggerPrice", trigger),
                ("clientAlgoId", client),
                ("presetTakeProfitPrice", tp),
                ("presetStopLossPrice", sl),
                ("TpWorkingType", tp_work),
                ("SlWorkingType", sl_work),
            ]),
        );
    }
    if (r.equals)(&(r.get)(h, &opts, "type")?, "stop_market")
        || (r.equals)(&(r.get)(h, &opts, "type")?, "take_profit_market")
    {
        if !(r.truthy)(&(r.get)(h, &opts, "price")?) {
            return fail(
                h,
                r,
                (r.text)("Trigger price is required for WEEX TP/SL orders"),
            );
        }
        let symbol = (r.get)(h, &opts, "symbol")?;
        let client = minted(h, r)?;
        let kind = (r.text)(
            if (r.equals)(&(r.get)(h, &opts, "type")?, "take_profit_market") {
                "TAKE_PROFIT"
            } else {
                "STOP_LOSS"
            },
        );
        let trigger = words(h, r, &price, "", "")?;
        let qty = words(h, r, &amount, "", "")?;
        let typ = if (r.equals)(&(r.get)(h, &opts, "type")?, "take_profit_market") {
            or(r, (r.get)(h, &opts, "tpTriggerType")?, || {
                (r.text)("last_price")
            })
        } else {
            or(r, (r.get)(h, &opts, "slTriggerType")?, || {
                (r.text)("mark_price")
            })
        };
        let typ = (r.table)(h, "REVERSE_ORDER_TRIGGER_TYPE", typ)?;
        return (r.omit)(
            h,
            (r.object)(vec![
                ("symbol", symbol),
                ("clientAlgoId", client),
                ("planType", kind),
                ("triggerPrice", trigger),
                ("executePrice", (r.text)("0")),
                ("quantity", qty),
                ("positionSide", position_side),
                ("triggerPriceType", typ),
            ]),
        );
    }
    if (r.equals)(&(r.get)(h, &opts, "type")?, "trailing_stop_market") {
        return fail(h, r, (r.text)("WEEX trailing stop loss is not supported"));
    }
    let symbol = (r.get)(h, &opts, "symbol")?;
    let side = reverse(h, r, &opts, "side", "REVERSE_ORDER_SIDE")?;
    let kind = reverse(h, r, &opts, "type", "REVERSE_ORDER_TYPE")?;
    let tif = if (r.equals)(&(r.get)(h, &opts, "type")?, "limit") {
        (r.table)(
            h,
            "REVERSE_TIME_IN_FORCE",
            or(r, (r.get)(h, &opts, "timeInForce")?, || {
                (r.text)("GoodTillCancel")
            }),
        )?
    } else {
        (r.undefined)()
    };
    let qty = words(h, r, &amount, "", "")?;
    let price = if (r.equals)(&(r.get)(h, &opts, "type")?, "limit") {
        words(h, r, &price, "", "")?
    } else {
        (r.undefined)()
    };
    let client = invoke(h, r, "resolveCallerClientOrderId", vec![opts.clone()])?;
    let client = if (r.nullish)(&client) {
        minted(h, r)?
    } else {
        client
    };
    let reduce = or(r, (r.get)(h, &opts, "reduceOnly")?, r.undefined);
    let tp = preset(h, r, &opts, "takeProfit", &market)?;
    let sl = preset(h, r, &opts, "stopLoss", &market)?;
    let tp_work = working(h, r, &opts, "takeProfit", "tpTriggerType", "last_price")?;
    let sl_work = working(h, r, &opts, "stopLoss", "slTriggerType", "mark_price")?;
    (r.omit)(
        h,
        (r.object)(vec![
            ("symbol", symbol),
            ("side", side),
            ("positionSide", position_side),
            ("type", kind),
            ("timeInForce", tif),
            ("quantity", qty),
            ("price", price),
            ("newClientOrderId", client),
            ("reduceOnly", reduce),
            ("tpTriggerPrice", tp),
            ("slTriggerPrice", sl),
            ("TpWorkingType", tp_work),
            ("SlWorkingType", sl_work),
        ]),
    )
}

pub fn place_order<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a WeexTradingRuntime<H, V, E>,
    opts: V,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        invoke(h, r, "resolveCallerClientOrderId", vec![opts.clone()])?;
        let payload = invoke(h, r, "formatOrderPayload", vec![opts.clone()])?;
        let route = if (r.equals)(&(r.get)(h, &opts, "type")?, "conditional") {
            "ALGO_ORDER"
        } else if (r.equals)(&(r.get)(h, &opts, "type")?, "stop_market")
            || (r.equals)(&(r.get)(h, &opts, "type")?, "take_profit_market")
        {
            "PLACE_TP_SL_ORDER"
        } else {
            "ORDER"
        };
        let outcome = async {
            let response =
                js_await((r.request)(h, "POST", endpoint(route), payload.clone())?).await?;
            (r.get)(h, &response, "data")
        }
        .await;
        let data = match outcome {
            Ok(v) => v,
            Err(e) => {
                let message = error_message(h, r, &e, true, Some("WEEX order placement failed"))?;
                emit(h, r, message)?;
                return Err(e);
            }
        };
        let rows = if (r.is_array)(h, &data)? {
            data
        } else {
            (r.array)(vec![data])
        };
        let ids = (r.map)(
            h,
            &rows,
            Box::new(|result, _| {
                let method = (r.method)(h, "parseOrderResult")?;
                let client = (r.get)(h, &payload, "newClientOrderId")?;
                let client = if (r.truthy)(&client) {
                    client
                } else {
                    (r.get)(h, &payload, "clientAlgoId")?
                };
                (r.call)(h, method, vec![result, client])
            }),
        )?;
        if (r.equals)(&(r.get)(h, &opts, "timeInForce")?, "PostOnly") {
            (r.for_each)(
                h,
                &ids,
                Box::new(|id, _| {
                    let method = (r.method)(h, "updateStoredOrder")?;
                    (r.call)(
                        h,
                        method,
                        vec![
                            (r.object)(vec![("id", id)]),
                            (r.object)(vec![("timeInForce", (r.text)("PostOnly"))]),
                        ],
                    )
                }),
            )?;
        }
        Ok(ids)
    }))
}
fn flush<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a WeexTradingRuntime<H, V, E>,
    ids: &'a V,
    batch: &'a mut V,
) -> F<'a, (), E> {
    eager(Box::pin(async move {
        if (r.length)(h, batch)? == 0 {
            return Ok(());
        }
        let push = (r.push_method)(h, ids)?;
        let result = js_await(invoke_async(
            h,
            r,
            "placeRegularOrderBatch",
            vec![batch.clone()],
        )?)
        .await?;
        (r.spread_push)(h, push, ids, result)?;
        *batch = (r.array)(vec![]);
        Ok(())
    }))
}
pub fn place_orders<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a WeexTradingRuntime<H, V, E>,
    orders: V,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        invoke(h, r, "assertCallerClientOrderIds", vec![orders.clone()])?;
        let ids = (r.array)(vec![]);
        let mut batch = (r.array)(vec![]);
        let it = (r.iterator)(h, &orders)?;
        while let Some(order) = (r.next)(h, &it)? {
            let outcome = async {
                let batchable = invoke(h, r, "isBatchableRegularOrder", vec![order.clone()])?;
                if !(r.truthy)(&batchable) {
                    js_await(flush(h, r, &ids, &mut batch)).await?;
                    let push = (r.push_method)(h, &ids)?;
                    let result = js_await(invoke_async(h, r, "placeOrder", vec![order])?).await?;
                    (r.spread_push)(h, push, &ids, result)?;
                } else {
                    let push = (r.push_method)(h, &batch)?;
                    (r.push)(h, push, &batch, order)?;
                    if (r.length)(h, &batch)? == 5 {
                        js_await(flush(h, r, &ids, &mut batch)).await?;
                    }
                }
                Ok(())
            }
            .await;
            if let Err(e) = outcome {
                (r.close_on_error)(h, &it, &e);
                return Err(e);
            }
        }
        js_await(flush(h, r, &ids, &mut batch)).await?;
        Ok(ids)
    }))
}
fn malformed<H, V: Clone, E>(h: &H, r: &WeexTradingRuntime<H, V, E>, reason: V) -> Result<V, E> {
    let message = words(h, r, &reason, "WEEX batch order response malformed: ", "")?;
    fail(h, r, message)
}

pub fn place_regular_order_batch<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a WeexTradingRuntime<H, V, E>,
    orders: V,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        let payloads = (r.map)(
            h,
            &orders,
            Box::new(|order, _| invoke(h, r, "formatOrderPayload", vec![order])),
        )?;
        let clients = (r.map)(
            h,
            &payloads,
            Box::new(|payload, _| (r.get)(h, &payload, "newClientOrderId")),
        )?;
        let bad = (r.some)(
            h,
            &clients,
            Box::new(|id, _| Ok(!(r.is_string)(&id) || !(r.truthy)(&id))),
        )?;
        if bad || (r.set_size)(h, &clients)? != (r.length)(h, &clients)? {
            return fail(
                h,
                r,
                (r.text)("WEEX batch order request has missing or duplicate client order IDs"),
            );
        }
        let outcome = async {
            let response = js_await((r.request)(
                h,
                "POST",
                endpoint("BATCH_ORDERS"),
                (r.object)(vec![("batchOrders", payloads.clone())]),
            )?)
            .await?;
            (r.get)(h, &response, "data")
        }
        .await;
        let data = match outcome {
            Ok(v) => v,
            Err(e) => {
                let message =
                    error_message(h, r, &e, true, Some("WEEX batch order placement failed"))?;
                emit(h, r, message)?;
                return Err(e);
            }
        };
        if !(r.is_array)(h, &data)? {
            return malformed(h, r, (r.text)("expected an array"));
        }
        let n = (r.length)(h, &data)?;
        let expected = (r.length)(h, &payloads)?;
        if n != expected {
            return malformed(
                h,
                r,
                (r.text)(&format!(
                    "expected {} rows, received {}",
                    (r.length)(h, &payloads)?,
                    (r.length)(h, &data)?
                )),
            );
        }
        let results = (r.new_map)(h)?;
        let it = (r.iterator)(h, &data)?;
        while let Some(result) = (r.next)(h, &it)? {
            let check = (|| {
                if !(r.truthy)(&result) || !(r.is_object)(&result) || (r.is_array)(h, &result)? {
                    return malformed(h, r, (r.text)("each row must be an object"));
                }
                let client = (r.get)(h, &result, "clientOrderId")?;
                let client = if (r.is_string)(&client)
                    && (r.truthy)(&(r.get)(h, &result, "clientOrderId")?)
                {
                    (r.get)(h, &result, "clientOrderId")?
                } else {
                    return malformed(h, r, (r.text)("each row must echo a clientOrderId"));
                };
                if !(r.includes)(h, &clients, &client)? {
                    return malformed(h, r, words(h, r, &client, "unknown clientOrderId ", "")?);
                }
                if (r.map_has)(h, &results, &client)? {
                    return malformed(h, r, words(h, r, &client, "duplicate clientOrderId ", "")?);
                }
                if !(r.is_boolean)(&(r.get)(h, &result, "success")?) {
                    return malformed(h, r, words(h, r, &client, "missing success flag for ", "")?);
                }
                if !is_undefined(h, r, &result, "errorCode")?
                    && !(r.is_string)(&(r.get)(h, &result, "errorCode")?)
                {
                    return malformed(h, r, words(h, r, &client, "invalid errorCode for ", "")?);
                }
                if !is_undefined(h, r, &result, "errorMessage")?
                    && !(r.is_string)(&(r.get)(h, &result, "errorMessage")?)
                {
                    return malformed(h, r, words(h, r, &client, "invalid errorMessage for ", "")?);
                }
                if (r.truthy)(&(r.get)(h, &result, "success")?) {
                    let id = (r.get)(h, &result, "orderId")?;
                    if !(r.is_string)(&id) || !(r.valid_order_id)(&(r.get)(h, &result, "orderId")?)
                    {
                        return malformed(h, r, words(h, r, &client, "invalid orderId for ", "")?);
                    }
                }
                (r.map_set)(h, &results, client, result)?;
                Ok((r.undefined)())
            })();
            if let Err(e) = check {
                (r.close_on_error)(h, &it, &e);
                return Err(e);
            }
        }
        let ordered = (r.map)(
            h,
            &clients,
            Box::new(|client, _| {
                let found = (r.map_get)(h, &results, &client)?;
                if (r.truthy)(&found) {
                    Ok(found)
                } else {
                    malformed(h, r, words(h, r, &client, "missing clientOrderId ", "")?)
                }
            }),
        )?;
        (r.for_each)(
            h,
            &ordered,
            Box::new(|result, index| {
                if (r.truthy)(&(r.get)(h, &result, "success")?) {
                    let order = (r.get)(h, &orders, &index.to_string())?;
                    if (r.equals)(&(r.get)(h, &order, "timeInForce")?, "PostOnly") {
                        let method = (r.method)(h, "updateStoredOrder")?;
                        let id = (r.get)(h, &result, "orderId")?;
                        (r.call)(
                            h,
                            method,
                            vec![
                                (r.object)(vec![("id", id)]),
                                (r.object)(vec![("timeInForce", (r.text)("PostOnly"))]),
                            ],
                        )?;
                    }
                }
                Ok(result)
            }),
        )?;
        let rejected = (r.filter)(
            h,
            &ordered,
            Box::new(|result, _| Ok(!(r.truthy)(&(r.get)(h, &result, "success")?))),
        )?;
        if (r.length)(h, &rejected)? != 0 {
            let messages = (r.map)(
                h,
                &rejected,
                Box::new(|result, _| {
                    let id = (r.get)(h, &result, "clientOrderId")?;
                    let mut s = (r.template)(h, &id)?;
                    s.extend(": ".encode_utf16());
                    let msg = (r.get)(h, &result, "errorMessage")?;
                    let msg = if (r.truthy)(&msg) {
                        msg
                    } else {
                        or(r, (r.get)(h, &result, "errorCode")?, || {
                            (r.text)("WEEX order rejected")
                        })
                    };
                    s.extend((r.template)(h, &msg)?);
                    Ok((r.string_units)(s))
                }),
            )?;
            let message = (r.join)(h, &messages, "; ")?;
            return fail(h, r, message);
        }
        (r.map)(
            h,
            &ordered,
            Box::new(|result, _| (r.get)(h, &result, "orderId")),
        )
    }))
}
fn is_undefined<H, V, E>(
    h: &H,
    r: &WeexTradingRuntime<H, V, E>,
    v: &V,
    key: &str,
) -> Result<bool, E> {
    let value = (r.get)(h, v, key)?;
    Ok((r.is_undefined)(&value))
}

pub fn update_order<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a WeexTradingRuntime<H, V, E>,
    opts: V,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        let order = (r.get)(h, &opts, "order")?;
        let update = (r.get)(h, &opts, "update")?;
        if (r.equals)(&(r.get)(h, &order, "type")?, "stop_market")
            || (r.equals)(&(r.get)(h, &order, "type")?, "take_profit_market")
        {
            if !(r.has)(h, &update, "price")? {
                return fail(
                    h,
                    r,
                    (r.text)("WEEX TP/SL quantity amendment is not supported"),
                );
            }
            if (r.contains)(h, &(r.get)(h, &order, "id")?, "__")? {
                return fail(
                    h,
                    r,
                    (r.text)("WEEX virtual TP/SL amendment requires the protective order ID"),
                );
            }
            let markets = (r.markets_by_symbol)(h)?;
            let market = (r.lookup)(h, &markets, (r.get)(h, &order, "symbol")?)?;
            if !(r.truthy)(&market) {
                return fail(
                    h,
                    r,
                    words(
                        h,
                        r,
                        &(r.get)(h, &order, "symbol")?,
                        "Market ",
                        " not found",
                    )?,
                );
            }
            let id = (r.get)(h, &order, "id")?;
            let trigger = adjusted(h, r, &update, "price", &market, "price")?;
            let trigger = words(h, r, &trigger, "", "")?;
            let info = (r.get)(h, &order, "info")?;
            let execution = (r.to_number)(h, opt(h, r, &info, "executePrice")?, (r.number)(0.0))?;
            let execution = words(h, r, &execution, "", "")?;
            let typ = if (r.equals)(&(r.get)(h, &order, "type")?, "take_profit_market") {
                or(r, (r.get)(h, &order, "tpTriggerType")?, || {
                    (r.text)("last_price")
                })
            } else {
                or(r, (r.get)(h, &order, "slTriggerType")?, || {
                    (r.text)("mark_price")
                })
            };
            let typ = (r.table)(h, "REVERSE_ORDER_TRIGGER_TYPE", typ)?;
            let payload = (r.object)(vec![
                ("orderId", id),
                ("triggerPrice", trigger),
                ("executePrice", execution),
                ("triggerPriceType", typ),
            ]);
            let outcome = async {
                let response = js_await((r.request)(
                    h,
                    "POST",
                    endpoint("MODIFY_TP_SL_ORDER"),
                    payload,
                )?)
                .await?;
                let data = (r.get)(h, &response, "data")?;
                if (r.strict_false)(&opt(h, r, &data, "success")?) {
                    fail(h, r, (r.text)("WEEX TP/SL amendment rejected"))?;
                }
                Ok(())
            }
            .await;
            if let Err(e) = outcome {
                let value = (r.error_value)(&e);
                if (r.equals)(
                    &opt(h, r, &value, "message")?,
                    "WEEX TP/SL amendment rejected",
                ) {
                    return Err(e);
                }
                let message = error_message(h, r, &e, true, Some("WEEX TP/SL amendment failed"))?;
                emit(h, r, message)?;
                return Err(e);
            }
            let method = (r.method)(h, "updateStoredOrder")?;
            let id = (r.get)(h, &order, "id")?;
            let price = (r.get)(h, &update, "price")?;
            (r.call)(
                h,
                method,
                vec![
                    (r.object)(vec![("id", id)]),
                    (r.object)(vec![("price", price)]),
                ],
            )?;
            return Ok((r.array)(vec![(r.get)(h, &order, "id")?]));
        }
        let conditional = (r.equals)(&(r.get)(h, &order, "type")?, "conditional");
        let symbol = (r.get)(h, &order, "symbol")?;
        let kind = (r.get)(h, &order, "type")?;
        let side = (r.get)(h, &order, "side")?;
        let amount = if (r.has)(h, &update, "amount")? {
            (r.get)(h, &update, "amount")?
        } else {
            (r.get)(h, &order, "amount")?
        };
        let price = if conditional {
            let info = (r.get)(h, &order, "info")?;
            let value = (r.to_number)(h, opt(h, r, &info, "price")?, (r.number)(0.0))?;
            or(r, value, r.undefined)
        } else if (r.has)(h, &update, "price")? {
            (r.get)(h, &update, "price")?
        } else {
            (r.get)(h, &order, "price")?
        };
        let trigger = if conditional {
            if (r.has)(h, &update, "price")? {
                (r.get)(h, &update, "price")?
            } else {
                (r.get)(h, &order, "price")?
            }
        } else {
            (r.undefined)()
        };
        let mut fields = vec![
            ("symbol", symbol),
            ("type", kind),
            ("side", side),
            ("amount", amount),
            ("price", price),
            ("triggerPrice", trigger),
        ];
        for name in [
            "reduceOnly",
            "timeInForce",
            "tpTriggerType",
            "slTriggerType",
            "takeProfit",
            "stopLoss",
        ] {
            fields.push((name, (r.get)(h, &order, name)?));
        }
        let next = (r.object)(fields);
        js_await(invoke_async(
            h,
            r,
            "cancelOrders",
            vec![(r.array)(vec![order])],
        )?)
        .await?;
        js_await(invoke_async(h, r, "placeOrder", vec![next])?).await
    }))
}

pub fn fetch_positions<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a WeexTradingRuntime<H, V, E>,
    symbols: V,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        if (r.public_only)(h)? {
            return Ok((r.array)(vec![]));
        }
        let outcome = async {
            let response = js_await((r.request)(
                h,
                "GET",
                endpoint("POSITIONS"),
                (r.undefined)(),
            )?)
            .await?;
            let data = (r.get)(h, &response, "data")?;
            let length = opt(h, r, &symbols, "length")?;
            let requested = if (r.truthy)(&length) {
                symbols.clone()
            } else {
                let markets = (r.markets)(h)?;
                (r.map)(
                    h,
                    &markets,
                    Box::new(|market, _| (r.get)(h, &market, "symbol")),
                )?
            };
            let requested_set = (r.new_set)(h, &requested)?;
            let rows = (r.extract_array)(h, data)?;
            let rows = (r.map)(
                h,
                &rows,
                Box::new(|raw, _| {
                    let markets = (r.markets_by_symbol)(h)?;
                    let market = (r.lookup)(h, &markets, (r.get)(h, &raw, "symbol")?)?;
                    let config_method = (r.method)(h, "getSymbolConfigFromCache")?;
                    let config = (r.call)(h, config_method, vec![(r.get)(h, &raw, "symbol")?])?;
                    normalize_position(h, r, raw, market, config)
                }),
            )?;
            let open = (r.filter)(
                h,
                &rows,
                Box::new(|position, _| {
                    Ok((r.truthy)(&position)
                        && (r.set_has)(h, &requested_set, &(r.get)(h, &position, "symbol")?)?)
                }),
            )?;
            let ids = (r.map)(
                h,
                &open,
                Box::new(|position, _| (r.get)(h, &position, "id")),
            )?;
            let existing_ids = (r.new_set)(h, &ids)?;
            let fake = (r.flat_map)(
                h,
                &requested,
                Box::new(|symbol, _| {
                    let markets = (r.markets_by_symbol)(h)?;
                    let market = (r.lookup)(h, &markets, symbol.clone())?;
                    if !(r.truthy)(&market) {
                        return Ok((r.array)(vec![]));
                    }
                    let cfg = invoke(h, r, "getSymbolConfigFromCache", vec![symbol])?;
                    let mode = opt(h, r, &cfg, "positionMode")?;
                    let mode = if (r.truthy)(&mode) {
                        mode
                    } else {
                        (r.account_mode)(h)?
                    };
                    let margin = or(r, opt(h, r, &cfg, "marginType")?, || (r.text)("cross"));
                    let cross = opt(h, r, &cfg, "crossLeverage")?;
                    let leverage = if (r.truthy)(&cross) {
                        cross
                    } else {
                        or(r, opt(h, r, &cfg, "isolatedLongLeverage")?, || {
                            (r.number)(20.0)
                        })
                    };
                    let positions = (r.fake_positions)(h, market, mode, margin, leverage)?;
                    let selected = (r.filter)(
                        h,
                        &positions,
                        Box::new(|position, _| {
                            Ok(!(r.set_has)(
                                h,
                                &existing_ids,
                                &(r.get)(h, &position, "id")?,
                            )?)
                        }),
                    )?;
                    Ok(selected)
                }),
            )?;
            let combined = (r.concat)(h, open, fake)?;
            (r.unique_id)(h, combined)
        }
        .await;
        match outcome {
            Ok(v) => Ok(v),
            Err(e) => {
                let sink = (r.error_sink)(h)?;
                let message = error_message(h, r, &e, false, None)?;
                (r.emit)(h, sink, message)?;
                let positions = (r.positions)(h)?;
                if (r.truthy)(&symbols) {
                    (r.filter)(
                        h,
                        &positions,
                        Box::new(|position, _| {
                            (r.includes)(h, &symbols, &(r.get)(h, &position, "symbol")?)
                        }),
                    )
                } else {
                    Ok(positions)
                }
            }
        }
    }))
}
