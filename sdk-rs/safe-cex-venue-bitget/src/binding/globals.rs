//! Module-scope values the TS source imports or reads: protocol constants,
//! enums, JS builtins. Host modules register their imported helpers here too.
use super::{
    promise,
    rt::Rt,
    value::{Js, Thrown, error_object, from_json, to_json},
};
use crate::protocol;
use std::rc::{Rc, Weak};

fn enum_object(members: &[(&str, &str)]) -> Js {
    Js::object(members.iter().map(|(k, v)| (*k, Js::str(v))).collect())
}

fn table(entries: &[(&str, &str)]) -> Js {
    enum_object(entries)
}

fn serde_text<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub(crate) fn arg(args: &[Js], i: usize) -> Js {
    args.get(i).cloned().unwrap_or(Js::Undefined)
}

/// Build the module-scope object. `rt` is weak: globals live inside the shared state.
pub(crate) fn build(rt: Weak<super::rt::Shared>) -> Js {
    let g = Js::object(vec![]);
    let def = |k: &str, v: Js| {
        let _ = g.define(k, v);
    };

    // bitget.types.ts
    def("RECV_WINDOW", Js::Number(protocol::RECV_WINDOW as f64));
    def("BROKER_ID", Js::str(protocol::BROKER_ID));
    def("BASE_URL", Js::str(protocol::BASE_URL));
    def("BASE_WS_URL", Js::str(protocol::BASE_WS_URL));
    def("BASE_WS_PRIVATE_URL", Js::str(protocol::BASE_WS_PRIVATE_URL));
    def("BASE_WS_PRIVATE_UTA_URL", Js::str(protocol::BASE_WS_PRIVATE_UTA_URL));
    def(
        "ENDPOINTS",
        Js::object(
            protocol::ENDPOINTS
                .iter()
                .map(|e| (e.name, Js::str(e.path)))
                .collect(),
        ),
    );
    def(
        "GRAFANA_ENDPOINTS_EVERY",
        Js::array(protocol::GRAFANA_ENDPOINTS_EVERY.iter().map(|s| Js::str(s)).collect()),
    );
    def(
        "GRAFANA_ENDPOINTS_PERIODIC",
        Js::array(protocol::GRAFANA_ENDPOINTS_PERIODIC.iter().map(|s| Js::str(s)).collect()),
    );
    def("INTERVAL", table(protocol::INTERVAL));
    def(
        "POSITION_SIDE",
        Js::object(
            protocol::POSITION_SIDE
                .iter()
                .map(|(k, v)| (*k, Js::str(&serde_text(v))))
                .collect(),
        ),
    );
    def(
        "ORDER_STATUS",
        Js::object(
            protocol::ORDER_STATUS
                .iter()
                .map(|(k, v)| (*k, Js::str(&serde_text(v))))
                .collect(),
        ),
    );
    def(
        "ORDER_TYPE",
        Js::object(
            protocol::ORDER_TYPE
                .iter()
                .map(|(k, v)| (*k, Js::str(&serde_text(v))))
                .collect(),
        ),
    );
    def(
        "ORDER_SIDE",
        Js::object(
            protocol::ORDER_SIDE
                .iter()
                .map(|(k, v)| (*k, Js::str(&serde_text(v))))
                .collect(),
        ),
    );
    def(
        "TIME_IN_FORCE",
        Js::object(
            protocol::TIME_IN_FORCE
                .iter()
                .map(|(k, v)| (*k, Js::str(&serde_text(v))))
                .collect(),
        ),
    );
    // bitget.exchange.ts module constants
    def("BILL_PAGE_LIMIT", Js::Number(100.0));
    def("BILL_MAX_WINDOW_MS", Js::Number(30.0 * 24.0 * 60.0 * 60.0 * 1000.0));
    def(
        "BitgetExchange",
        Js::object(vec![("POSITIONING_CACHE_TTL_MS", Js::Number(5000.0))]),
    );
    def("BitgetClientOrderIdPattern", Js::str("^[.A-Z:/a-z0-9_-]{1,32}$"));

    // types.ts enums
    def(
        "OrderType",
        enum_object(&[
            ("Market", "market"),
            ("Limit", "limit"),
            ("Conditional", "conditional"),
            ("StopLoss", "stop_market"),
            ("TakeProfit", "take_profit_market"),
            ("TrailingStopLoss", "trailing_stop_market"),
        ]),
    );
    def(
        "OrderStatus",
        enum_object(&[
            ("Open", "open"),
            ("Closed", "closed"),
            ("Canceled", "canceled"),
            ("Creating", "creating"),
            ("Updating", "updating"),
            ("Canceling", "canceling"),
        ]),
    );
    def("OrderSide", enum_object(&[("Buy", "buy"), ("Sell", "sell")]));
    def(
        "OrderTimeInForce",
        enum_object(&[
            ("GoodTillCancel", "GoodTillCancel"),
            ("ImmediateOrCancel", "ImmediateOrCancel"),
            ("FillOrKill", "FillOrKill"),
            ("PostOnly", "PostOnly"),
        ]),
    );
    def(
        "OrderStopTriggerType",
        enum_object(&[
            ("MarkPrice", "mark_price"),
            ("LastPrice", "last_price"),
            ("IndexPrice", "index_price"),
        ]),
    );
    def("PositionSide", enum_object(&[("Long", "long"), ("Short", "short")]));
    def("PositionMode", enum_object(&[("Hedged", "hedged"), ("OneWay", "oneway")]));
    def("MarginType", enum_object(&[("Isolated", "isolated"), ("Cross", "cross")]));
    def(
        "LogSeverity",
        enum_object(&[
            ("Debug", "debug"),
            ("Info", "info"),
            ("Warning", "warning"),
            ("Error", "error"),
        ]),
    );

    // JS builtins
    def("parseFloat", Js::function("parseFloat", |_, a| Ok(Js::Number(arg(&a, 0).parse_float()?))));
    def(
        "parseInt",
        Js::function("parseInt", |_, a| {
            let text = String::from_utf16_lossy(&arg(&a, 0).to_js_string()?);
            let radix = arg(&a, 1);
            let radix = if radix.is_undefined() { None } else { Some(radix.to_number()? as i32) };
            Ok(Js::Number(safe_cex_num::parse_int(&text, radix)))
        }),
    );
    def("String", Js::function("String", |_, a| {
        Ok(if a.is_empty() { Js::str("") } else { Js::String(a[0].to_js_string()?) })
    }));
    def("Number", Js::function("Number", |_, a| {
        Ok(Js::Number(if a.is_empty() { 0.0 } else { a[0].to_number()? }))
    }));
    def("Boolean", Js::function("Boolean", |_, a| Ok(Js::Bool(arg(&a, 0).truthy()))));
    def(
        "Error",
        Js::constructor("Error", |_, a| {
            let message = arg(&a, 0);
            let message = if message.is_undefined() { String::new() } else { message.to_display() };
            Ok(error_object("Error", &message))
        }),
    );
    def(
        "TypeError",
        Js::constructor("TypeError", |_, a| Ok(error_object("TypeError", &arg(&a, 0).to_display()))),
    );
    def(
        "Array",
        Js::object(vec![(
            "isArray",
            Js::function("isArray", |_, a| Ok(Js::Bool(arg(&a, 0).is_array()))),
        )]),
    );
    def(
        "Object",
        Js::object(vec![
            (
                "keys",
                Js::function("keys", |_, a| {
                    let v = arg(&a, 0);
                    if v.is_nullish() {
                        return Err(Thrown::type_error("Cannot convert undefined or null to object"));
                    }
                    Ok(Js::array(v.own_keys().into_iter().map(Js::String).collect()))
                }),
            ),
            (
                "values",
                Js::function("values", |_, a| {
                    let v = arg(&a, 0);
                    if v.is_nullish() {
                        return Err(Thrown::type_error("Cannot convert undefined or null to object"));
                    }
                    Ok(Js::array(v.own_entries()?.into_iter().map(|(_, v)| v).collect()))
                }),
            ),
            (
                "entries",
                Js::function("entries", |_, a| {
                    let v = arg(&a, 0);
                    if v.is_nullish() {
                        return Err(Thrown::type_error("Cannot convert undefined or null to object"));
                    }
                    Ok(Js::array(
                        v.own_entries()?
                            .into_iter()
                            .map(|(k, v)| Js::array(vec![Js::String(k), v]))
                            .collect(),
                    ))
                }),
            ),
            (
                "assign",
                Js::function("assign", |_, a| {
                    let target = arg(&a, 0);
                    for source in a.iter().skip(1) {
                        if source.is_nullish() {
                            continue;
                        }
                        for (k, v) in source.own_entries()? {
                            target.set_key(&k, v)?;
                        }
                    }
                    Ok(target)
                }),
            ),
        ]),
    );
    def(
        "JSON",
        Js::object(vec![
            (
                "stringify",
                Js::function("stringify", |_, a| {
                    let v = arg(&a, 0);
                    if v.is_undefined() || v.is_callable() {
                        return Ok(Js::Undefined);
                    }
                    Ok(Js::str(&serde_json::to_string(&to_json(&v)?).unwrap_or_default()))
                }),
            ),
            (
                "parse",
                Js::function("parse", |_, a| {
                    let text = String::from_utf16_lossy(&arg(&a, 0).to_js_string()?);
                    serde_json::from_str::<serde_json::Value>(&text)
                        .map(|v| from_json(&v))
                        .map_err(|e| Thrown::error("SyntaxError", &e.to_string()))
                }),
            ),
        ]),
    );
    let math = |name: &'static str, f: fn(f64) -> f64| {
        (name, Js::function(name, move |_, a| Ok(Js::Number(f(arg(&a, 0).to_number()?)))))
    };
    def(
        "Math",
        Js::object(vec![
            math("abs", f64::abs),
            math("floor", f64::floor),
            math("ceil", f64::ceil),
            math("trunc", f64::trunc),
            math("round", safe_cex_num::math_round),
            math("sign", |v| if v.is_nan() || v == 0.0 { v } else { v.signum() }),
            math("log10", safe_cex_num::math_log10),
            (
                "max",
                Js::function("max", |_, a| {
                    let mut out = f64::NEG_INFINITY;
                    for v in &a {
                        out = safe_cex_num::math_max(out, v.to_number()?);
                    }
                    Ok(Js::Number(out))
                }),
            ),
            (
                "min",
                Js::function("min", |_, a| {
                    let mut out = f64::INFINITY;
                    for v in &a {
                        out = safe_cex_num::math_min(out, v.to_number()?);
                    }
                    Ok(Js::Number(out))
                }),
            ),
            (
                "pow",
                Js::function("pow", |_, a| {
                    Ok(Js::Number(safe_cex_num::math_pow(arg(&a, 0).to_number()?, arg(&a, 1).to_number()?)))
                }),
            ),
        ]),
    );
    let promise_rt = rt.clone();
    let settled_rt = rt.clone();
    let resolve_rt = rt.clone();
    let reject_rt = rt.clone();
    def(
        "Promise",
        Js::object(vec![
            (
                "all",
                Js::function("all", move |_, a| {
                    let rt = shared(&promise_rt)?;
                    let values = arg(&a, 0).array_values().unwrap_or_default();
                    Ok(rt.spawn(Box::pin(promise::all(values))))
                }),
            ),
            (
                "allSettled",
                Js::function("allSettled", move |_, a| {
                    let rt = shared(&settled_rt)?;
                    let values = arg(&a, 0).array_values().unwrap_or_default();
                    Ok(rt.spawn(Box::pin(promise::all_settled(values))))
                }),
            ),
            (
                "resolve",
                Js::function("resolve", move |_, a| {
                    let rt = shared(&resolve_rt)?;
                    Ok(match arg(&a, 0) {
                        p @ Js::Promise(_) => p,
                        v => Js::Promise(promise::Promise::settled(&rt.exchange_runtime(), Ok(v))),
                    })
                }),
            ),
            (
                "reject",
                Js::function("reject", move |_, a| {
                    let rt = shared(&reject_rt)?;
                    Ok(Js::Promise(promise::Promise::settled(&rt.exchange_runtime(), Err(Thrown::new(arg(&a, 0))))))
                }),
            ),
        ]),
    );
    let timeout_rt = rt.clone();
    def(
        "setTimeout",
        Js::function("setTimeout", move |_, a| {
            let rt = shared(&timeout_rt)?;
            let callback = arg(&a, 0);
            let delay = arg(&a, 1).to_number().unwrap_or(0.0);
            Ok(rt.set_timeout(Rc::new(move || callback.call(Js::Undefined, vec![]).map(|_| ())), delay))
        }),
    );
    let clear_rt = rt.clone();
    def(
        "clearTimeout",
        Js::function("clearTimeout", move |_, a| {
            shared(&clear_rt)?.clear_timeout(&arg(&a, 0));
            Ok(Js::Undefined)
        }),
    );
    let date_rt = rt.clone();
    def(
        "Date",
        Js::object(vec![(
            "now",
            Js::function("now", move |_, _| Ok(Js::Number(shared(&date_rt)?.now().floor()))),
        )]),
    );
    let clock_rt = rt.clone();
    def(
        "virtualClock",
        Js::object(vec![(
            "getCurrentTimeMs",
            Js::function("getCurrentTimeMs", move |_, _| Ok(Js::Number(shared(&clock_rt)?.now()))),
        )]),
    );
    def("Map", Js::constructor("Map", |_, _| Ok(Js::new_map())));
    def("Set", Js::constructor("Set", |_, a| {
        let set = Js::new_set();
        if let (Js::Set(s), Some(values)) = (&set, arg(&a, 0).array_values()) {
            let mut s = s.borrow_mut();
            for v in values {
                if !s.iter().any(|x| x.same_value_zero(&v)) {
                    s.push(v);
                }
            }
        }
        Ok(set)
    }));
    g
}

/// A detached-handle Rt over the exchange receiver, for builtins that spawn work.
pub(crate) fn shared(weak: &Weak<super::rt::Shared>) -> Result<Rt, Thrown> {
    let shared = weak
        .upgrade()
        .ok_or_else(|| Thrown::error("Error", "Bitget runtime disposed"))?;
    let this = shared.exchange.borrow().clone();
    Ok(Rt { shared, this })
}
