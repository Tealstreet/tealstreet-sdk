//! Module bindings both Lighter socket classes import that the shared builtins
//! and library do not provide, installed once before either class.
use super::{
    ops::spread,
    realm::{Realm, Shared},
};
use safe_cex_exchange::{
    js_runtime::{
        sockets::base_websocket_class,
        value::{Js, Thrown, arg, from_typed, to_typed},
    },
    runtime::ExchangeTask,
};
use safe_cex_transport::RoutingOptions;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

fn shared(weak: &Weak<Shared>) -> Result<Rc<Shared>, Thrown> {
    weak.upgrade()
        .ok_or_else(|| Thrown::error("Error", "Lighter scope was dropped"))
}

fn define_absent(owner: &Js, name: &str, value: impl FnOnce() -> Js) -> Result<(), Thrown> {
    if owner.has_own(name) {
        return Ok(());
    }
    owner.define(name, value())
}

pub(crate) fn install(realm: &Realm) -> Result<(), Thrown> {
    let globals = &realm.0.globals;
    let weak = Rc::downgrade(&realm.0);
    define_absent(globals, "BaseWebSocket", || {
        base_websocket_class(&realm.0.sockets)
    })?;
    // ts: config.ts — the host samples 1000 + random * 2000 once per realm.
    define_absent(globals, "WS_MIN_RECONNECTION_DELAY", || {
        Js::Number(realm.0.runtime.min_reconnection_delay_ms())
    })?;
    define_absent(globals, "ENABLE_RECONNECTING_WEBSOCKET", || {
        Js::Bool(realm.0.runtime.enable_reconnecting())
    })?;
    define_absent(globals, "resolveTransportRouting", || {
        resolve_transport_routing(weak.clone())
    })?;
    define_absent(globals, "jsonParse", || json_parse(weak.clone()))?;
    install_intervals(globals, &weak)?;
    define_absent(globals, "performance", || {
        let weak = weak.clone();
        Js::object(vec![(
            "now",
            Js::function("now", move |_, _| {
                Ok(Js::Number(shared(&weak)?.runtime.monotonic_now_ms()))
            }),
        )])
    })?;
    if !globals.has_own("console") {
        globals.define("console", Js::object(vec![]))?;
    }
    let array = globals.get("Array")?;
    if array.get("from")?.is_undefined() {
        array.define("from", array_from())?;
    }
    let weak = weak.clone();
    define_absent(&globals.get("console")?, "error", move || {
        Js::function("error", move |_, a| {
            let line: Vec<String> = a.iter().map(console_text).collect();
            shared(&weak)?.runtime.console("error", &line.join(" "));
            Ok(Js::Undefined)
        })
    })
}

/// util.format's rendering of the values the private socket logs: strings raw,
/// errors as `name: message`, anything else by its display form.
fn console_text(value: &Js) -> String {
    if let Js::String(s) = value {
        return String::from_utf16_lossy(s);
    }
    if value.is_object()
        && let Ok(message) = value.get("message")
        && let Some(message) = message.as_str()
    {
        let name = value
            .get("name")
            .ok()
            .and_then(|n| n.as_str().map(|s| String::from_utf16_lossy(s)))
            .unwrap_or_else(|| "Error".into());
        return format!("{name}: {}", String::from_utf16_lossy(message));
    }
    value.to_display()
}

// ts: transport/registry.ts resolveTransportRouting — through the realm's WS transport.
fn resolve_transport_routing(weak: Weak<Shared>) -> Js {
    Js::function("resolveTransportRouting", move |_, a| {
        let exchange: ExchangeName = to_typed(&arg(&a, 0))?;
        let options: ExchangeOptions = to_typed(&arg(&a, 1))?;
        // ts: getTransport(options) — options never carry an instance provider
        // across JS, so the realm's registered default applies.
        let routing = shared(&weak)?
            .extra
            .registry
            .get_transport(None)
            .ws_routing(&exchange, &RoutingOptions::from(&options));
        from_typed(&routing)
    })
}

// ts: @tealstreet/json-parse jsonParse — JSON.parse(str), null on any throw.
fn json_parse(weak: Weak<Shared>) -> Js {
    Js::function("jsonParse", move |_, a| {
        let parse = shared(&weak)?.globals.get("JSON")?.get("parse")?;
        Ok(parse
            .call(Js::Undefined, vec![arg(&a, 0)])
            .unwrap_or(Js::Null))
    })
}

type Intervals = Rc<RefCell<Vec<(Js, Rc<Cell<bool>>, Box<dyn ExchangeTask>)>>>;

/// setInterval / clearInterval: a repeating runtime task per handle; clearing
/// cancels it. clearInterval of a setTimeout handle clears that timeout, as in JS.
fn install_intervals(globals: &Js, weak: &Weak<Shared>) -> Result<(), Thrown> {
    let intervals: Intervals = Rc::default();
    {
        let (weak, intervals) = (weak.clone(), intervals.clone());
        define_absent(globals, "setInterval", move || {
            Js::function("setInterval", move |_, a| {
                let shared = shared(&weak)?;
                let callback = arg(&a, 0);
                let delay = arg(&a, 1).to_number()?;
                let delay = if delay.is_finite() && delay >= 1.0 {
                    delay as u64
                } else {
                    1
                };
                let cancelled: Rc<Cell<bool>> = Rc::default();
                let runtime = shared.runtime.clone();
                let stop = cancelled.clone();
                let task = runtime.clone().spawn_local(Box::pin(async move {
                    loop {
                        if runtime.sleep(delay).await.is_err() || stop.get() {
                            return;
                        }
                        if let Err(error) = callback.call(Js::Undefined, vec![]) {
                            runtime.report_exception(
                                &error.to_exchange_error(),
                                &serde_json::json!({"source": "setInterval"}).into(),
                            );
                        }
                        if stop.get() {
                            return;
                        }
                    }
                }));
                let handle = Js::object_with_class("Timeout", vec![]);
                intervals
                    .borrow_mut()
                    .push((handle.clone(), cancelled, task));
                Ok(handle)
            })
        })?;
    }
    let weak = weak.clone();
    define_absent(globals, "clearInterval", move || {
        Js::function("clearInterval", move |_, a| {
            let handle = arg(&a, 0);
            let found = {
                let mut list = intervals.borrow_mut();
                let index = list.iter().position(|(h, _, _)| h.same_reference(&handle));
                index.map(|i| list.remove(i))
            };
            match found {
                Some((_, cancelled, task)) => {
                    cancelled.set(true);
                    task.cancel();
                }
                None => {
                    let realm = Realm(shared(&weak)?);
                    realm.scope().clear_timeout(&handle);
                }
            }
            Ok(Js::Undefined)
        })
    })
}

const MS_PER_DAY: i64 = 86_400_000;

/// `new Date(ms).toISOString()`, including its RangeError for an invalid time.
pub(crate) fn iso_string(ms: f64) -> Result<Vec<u16>, Thrown> {
    let clipped = safe_cex_num::js_time_clip(ms);
    if clipped.is_nan() {
        return Err(Thrown::error("RangeError", "Invalid time value"));
    }
    let t = clipped as i64;
    let (days, rem) = (t.div_euclid(MS_PER_DAY), t.rem_euclid(MS_PER_DAY));
    let (year, month, day) = civil_from_days(days);
    let year = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else if year < 0 {
        format!("-{:06}", -year)
    } else {
        format!("+{year:06}")
    };
    let text = format!(
        "{year}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    );
    Ok(text.encode_utf16().collect())
}

/// Proleptic Gregorian date of a day count from 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `Array.from(iterable, mapFn?)`, which the shared builtins do not yet bind.
fn array_from() -> Js {
    Js::function("from", |_, a| {
        let source = arg(&a, 0);
        if source.is_nullish() {
            return Err(Thrown::type_error(&format!(
                "{} is not iterable (cannot read property Symbol(Symbol.iterator))",
                source.to_display()
            )));
        }
        let items = spread(&source)?;
        let map = arg(&a, 1);
        if map.is_undefined() {
            return Ok(Js::array(items));
        }
        let mapped = items
            .into_iter()
            .enumerate()
            .map(|(i, v)| map.call(Js::Undefined, vec![v, Js::Number(i as f64)]))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Js::array(mapped))
    })
}
