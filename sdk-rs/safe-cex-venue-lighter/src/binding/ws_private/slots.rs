//! The feed and stream runtimes' language slots over js_runtime, with the realm
//! as host: globals resolve by name, values cross through the AccountValue bridge.
use super::{
    super::{
        realm::Realm,
        value::{native_to_js, to_account, to_js, to_signing, to_thrown},
    },
    owner::awaited,
};
use crate::{
    sdk_account_api::AccountValue,
    transaction_promise::NativePromise,
    transaction_signing::{SigningResult, TransactionSigningError},
    ws_private_feed_helpers::LighterPrivateFeedRuntime,
    ws_private_stream_handlers::LighterPrivateStreamRuntime,
};
use safe_cex_exchange::js_runtime::{
    ops,
    value::{Js, Thrown},
};
use std::{cell::RefCell, rc::Rc};

type V = AccountValue;
pub(super) type Feed = LighterPrivateFeedRuntime<Realm, V>;
pub(super) type Stream = LighterPrivateStreamRuntime<Realm, V>;

/// Own keys of a bound list method: the receiver and member read at Get time.
const RECEIVER: &str = "\u{1}receiver";
const MEMBER: &str = "\u{1}member";
const LIST_METHODS: [&str; 4] = ["map", "filter", "some", "forEach"];
/// Statics the stream bodies read as `LighterPrivateWebsocket.NAME`.
const CLASS_STATICS: [&str; 2] = [
    "RATE_LIMIT_PING_INTERVAL_MS",
    "RATE_LIMIT_PING_SKIP_LOG_COOLDOWN_MS",
];

fn js<T>(result: Result<T, Thrown>) -> SigningResult<T> {
    result.map_err(to_signing)
}
fn account(result: Result<Js, Thrown>) -> SigningResult<V> {
    js(result).map(|v| to_account(&v))
}
fn global_call(h: &Realm, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    let callee = h.global(name)?;
    if !callee.is_callable() {
        return Err(Thrown::type_error(&format!("{name} is not a function")));
    }
    callee.call(Js::Undefined, args)
}

/// `receiver[key]` read now; calling it later keeps `receiver` as `this`.
fn method(_: &Realm, receiver: V, key: &'static str) -> SigningResult<V> {
    let receiver = to_js(&receiver);
    let member = js(receiver.get(key))?;
    let callee = member.clone();
    let this = receiver.clone();
    let bound = Js::function(key, move |_, args| {
        if !callee.is_callable() {
            return Err(Thrown::type_error(&format!("{key} is not a function")));
        }
        callee.call(this.clone(), args)
    });
    if LIST_METHODS.contains(&key) {
        js(bound.define(RECEIVER, receiver))?;
        js(bound.define(MEMBER, member))?;
    }
    Ok(to_account(&bound))
}
fn call(_: &Realm, callee: V, args: Vec<V>) -> SigningResult<V> {
    account(to_js(&callee).call(Js::Undefined, args.iter().map(to_js).collect()))
}
fn call_async(h: &Realm, callee: V, args: Vec<V>) -> SigningResult<NativePromise<V>> {
    let value = js(to_js(&callee).call(Js::Undefined, args.iter().map(to_js).collect()))?;
    Ok(awaited(h, value))
}

/// The native list body of a bound array method. Array.prototype methods skip
/// holes; a non-array receiver's own method is not a native body to run here.
fn list(callee: &V, name: &str) -> SigningResult<(Js, usize)> {
    let callee = to_js(callee);
    let receiver = js(callee.get(RECEIVER))?;
    let member = js(callee.get(MEMBER))?;
    if !member.is_callable() {
        return Err(to_signing(Thrown::type_error(&format!(
            "{}.{name} is not a function",
            receiver.type_of()
        ))));
    }
    let Js::Array(array) = &receiver else {
        return Err(to_signing(Thrown::type_error(&format!("{name} over a non-array receiver is not bound"))));
    };
    let length = array.len();
    Ok((receiver, length))
}

fn map(_: &Realm, callee: V, f: &mut dyn FnMut(V) -> SigningResult<V>) -> SigningResult<V> {
    let (receiver, length) = list(&callee, "map")?;
    let mut out = vec![];
    for index in 0..length {
        out.push(if receiver.has_index(index) { Some(to_js(&f(to_account(&js(receiver.get(&index.to_string()))?))?)) } else { None });
    }
    Ok(to_account(&Js::holey_array(out)))
}
fn filter(_: &Realm, callee: V, f: &mut dyn FnMut(V) -> SigningResult<bool>) -> SigningResult<V> {
    let (receiver, length) = list(&callee, "filter")?;
    let mut out = vec![];
    for index in 0..length {
        if receiver.has_index(index) {
            let item = js(receiver.get(&index.to_string()))?;
            if f(to_account(&item))? { out.push(item); }
        }
    }
    Ok(to_account(&Js::array(out)))
}
fn some(_: &Realm, callee: V, f: &mut dyn FnMut(V) -> SigningResult<V>) -> SigningResult<V> {
    let (receiver, length) = list(&callee, "some")?;
    for index in 0..length {
        if receiver.has_index(index) && f(to_account(&js(receiver.get(&index.to_string()))?))?.truthy() { return Ok(V::Boolean(true)); }
    }
    Ok(V::Boolean(false))
}
fn for_each(_: &Realm, callee: V, f: &mut dyn FnMut(V) -> SigningResult<V>) -> SigningResult<V> {
    let (receiver, length) = list(&callee, "forEach")?;
    for index in 0..length {
        if receiver.has_index(index) { f(to_account(&js(receiver.get(&index.to_string()))?))?; }
    }
    Ok(V::Undefined)
}

// For-of consumes the shared GetIterator protocol lazily. IteratorStepValue
// errors escape without closing; a body throw closes but keeps its original cause.
fn for_of(_: &Realm, data: V, f: &mut dyn FnMut(V) -> SigningResult<()>) -> SigningResult<()> {
    let iterator = js(to_js(&data).iterator())?;
    let next = js(iterator.get("next"))?;
    loop {
        let step = js(next.call(iterator.clone(), vec![]))?;
        if !step.is_object() {
            return Err(to_signing(Thrown::type_error(
                "Iterator result is not an object",
            )));
        }
        if js(step.get("done"))?.truthy() {
            return Ok(());
        }
        let value = js(step.get("value"))?;
        if let Err(error) = f(to_account(&value)) {
            // A throw completion wins over return getter/call/result failures.
            if let Ok(method) = iterator.get("return") {
                if !method.is_nullish() {
                    let _ = method.call(iterator.clone(), vec![]);
                }
            }
            return Err(error);
        }
    }
}
fn spread(_: &Realm, value: V) -> SigningResult<Vec<V>> {
    Ok(js(to_js(&value).iterable_values())?
        .iter()
        .map(to_account)
        .collect())
}

/// A JS function running a native task once; its promise is returned to JS.
fn task_function(task: Box<dyn FnOnce() -> SigningResult<NativePromise<V>>>, h: &Realm) -> Js {
    let slot = RefCell::new(Some(task));
    let runtime = h.exchange_runtime();
    Js::function("", move |_, _| match slot.borrow_mut().take() {
        Some(task) => Ok(native_to_js(&runtime, task().map_err(to_thrown)?)),
        None => Ok(Js::Undefined),
    })
}

fn set_timeout(
    h: &Realm,
    callback: Box<dyn FnOnce() -> NativePromise<V>>,
    delay: V,
) -> SigningResult<V> {
    let slot = RefCell::new(Some(callback));
    let realm = h.clone();
    let function = Js::function("", move |_, _| {
        let Some(callback) = slot.borrow_mut().take() else {
            return Ok(Js::Undefined);
        };
        let promise = callback();
        // The source's async timer callback rejects unobserved; report it.
        let reporter = realm.clone();
        promise.subscribe(move |result| {
            if let Err(error) = result {
                reporter
                    .scope()
                    .report(&to_thrown(error), "unhandledRejection");
            }
        });
        Ok(Js::Undefined)
    });
    account(global_call(h, "setTimeout", vec![function, to_js(&delay)]))
}

fn schedule_call(
    r: &Rc<Feed>,
    scheduler: V,
    key: &'static str,
    delay: V,
    task: Box<dyn FnOnce() -> SigningResult<NativePromise<V>>>,
    prefix: V,
) -> SigningResult<V> {
    let task = task_function(task, &r.host);
    account(to_js(&scheduler).call(
        Js::Undefined,
        vec![Js::str(key), to_js(&delay), task, to_js(&prefix)],
    ))
}

fn console_error(h: &Realm, prefix: V, error: &TransactionSigningError) -> SigningResult<V> {
    let error = to_thrown(error.clone()).value;
    account(global_call(h, "console.error", vec![to_js(&prefix), error]))
}

fn object(entries: Vec<(&'static str, V)>) -> V {
    to_account(&Js::object(
        entries.iter().map(|(k, v)| (*k, to_js(v))).collect(),
    ))
}
fn array(values: Vec<V>) -> V {
    to_account(&Js::array(values.iter().map(to_js).collect()))
}
fn text(value: &str) -> V {
    V::String(value.encode_utf16().collect())
}
fn to_string(_: &Realm, value: V) -> SigningResult<Vec<u16>> {
    js(to_js(&value).to_js_string()).map(|s| s.to_vec())
}
fn number_op(a: V, b: V, op: fn(&Js, &Js) -> Result<bool, Thrown>) -> SigningResult<bool> {
    js(op(&to_js(&a), &to_js(&b)))
}

pub(super) fn feed(realm: &Realm) -> Feed {
    LighterPrivateFeedRuntime {
        promise_runtime: realm.0.extra.microtasks.clone(),
        host: realm.clone(),
        get: |_, value, k| account(to_js(value).get_key(k)),
        define: |_, value, k, field| js(to_js(value).set_key(k, to_js(&field))),
        method,
        call,
        call_async,
        clear_timeout: |h, timer| account(global_call(h, "clearTimeout", vec![to_js(&timer)])),
        set_timeout,
        schedule_call,
        console_error,
        stringify: |h, value| account(global_call(h, "JSON.stringify", vec![to_js(&value)])),
        to_string,
        trim_upper: |_, units| {
            let upper = js(Js::utf16(&units)
                .call_method("trim", vec![])
                .and_then(|t| t.call_method("toUpperCase", vec![])))?;
            js(upper.to_js_string()).map(|s| s.to_vec())
        },
        to_number: |_, value| js(to_js(&value).to_number()),
        parse_float: |_, units| js(Js::utf16(&units).parse_float()),
        copy: |_, value| account(to_js(&value).spread_object()),
        map,
        filter,
        now: |h| {
            js(global_call(h, "virtualClock.getCurrentTimeMs", vec![]).and_then(|v| v.to_number()))
        },
        number: V::Number,
        text,
        utf16: V::String,
        boolean: V::Boolean,
        undefined: || V::Undefined,
        object,
        array,
        truthy: V::truthy,
        nullish: V::is_nullish,
        is_undefined: |v| matches!(v, V::Undefined),
        strict_same: |a, b| to_js(a).strict_equals(&to_js(b)),
    }
}

pub(super) fn stream(feed: Rc<Feed>) -> Stream {
    LighterPrivateStreamRuntime {
        feed,
        some,
        for_each,
        for_of,
        spread,
        key_value: |h, object, k| account(global_call(h, "v", vec![to_js(object), Js::str(k)])),
        number_value: |value| match value {
            V::Number(n) => Some(*n),
            _ => None,
        },
        date_now: |h| account(global_call(h, "Date.now", vec![])),
        performance_now: |h| account(global_call(h, "performance.now", vec![])),
        constant: |h, name| {
            if CLASS_STATICS.contains(&name) {
                account(
                    h.global("LighterPrivateWebsocket")
                        .and_then(|c| c.get(name)),
                )
            } else {
                account(h.global(name))
            }
        },
        error_value: |_, error| to_account(&to_thrown(error).value),
        template_string: to_string,
        parse_float_value: |_, value| js(to_js(&value).parse_float()),
        subtract: |_, a, b| account(ops::subtract(&to_js(&a), &to_js(&b))),
        less_than: |_, a, b| number_op(a, b, ops::less_than),
        less_equal: |_, a, b| number_op(a, b, ops::less_equal),
        greater_equal: |_, a, b| number_op(a, b, ops::greater_equal),
    }
}
