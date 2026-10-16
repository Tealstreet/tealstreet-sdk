//! OHLCV request/fallback/polling bodies, retaining required realm/JS bindings.
use crate::{
    exchange_trading_preflight::TradingHost,
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, PromiseResolver},
    transaction_signing::SigningResult,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub type OhlcvSelfCallback =
    Rc<dyn Fn(AccountValue, &[AccountValue]) -> SigningResult<AccountValue>>;
pub trait OhlcvHost: TradingHost {
    /// Fresh callable supplying its actual callee identity without retaining itself.
    fn self_function(&self, callback: OhlcvSelfCallback) -> SigningResult<AccountValue>;
    /// Infallible binding of this actual native Promise into the host realm.
    fn promise_value(&self, promise: NativePromise<AccountValue>) -> AccountValue;
    /// Full Call with undefined this, including callable check and original throw.
    fn call_value(
        &self,
        callee: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue>;
    /// Fresh literal RegExp, without looking up the mutable global constructor.
    fn regexp(&self, pattern: &str, flags: &str) -> SigningResult<AccountValue>;
    /// Actual [,amount,unit] binding: GetIterator, three steps, IteratorClose.
    fn interval_binding(&self, value: AccountValue) -> SigningResult<(AccountValue, AccountValue)>;
    /// Fresh CopyDataProperties then own data overrides, including symbols/UTF16.
    fn spread_override(
        &self,
        value: AccountValue,
        fields: &[(&str, AccountValue)],
    ) -> SigningResult<AccountValue>;
    fn subtract(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue>;
    fn add(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue>;
    fn multiply(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue>;
    fn divide(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue>;
    fn less(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool>;
    fn less_equal(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool>;
    fn greater(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn get(h: &dyn OhlcvHost, key: &str) -> SigningResult<AccountValue> {
    h.get(&h.owner(), key)
}
fn optional(h: &dyn OhlcvHost, v: &AccountValue, key: &str) -> SigningResult<AccountValue> {
    if v.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.get(v, key)
    }
}
fn call(h: &dyn OhlcvHost, key: &str, args: &[AccountValue]) -> SigningResult<AccountValue> {
    h.member(&h.owner(), key)?.call(args)
}
fn eq(a: &AccountValue, b: &AccountValue) -> bool {
    match (a, b) {
        (AccountValue::Undefined, AccountValue::Undefined)
        | (AccountValue::Null, AccountValue::Null) => true,
        (AccountValue::Boolean(a), AccountValue::Boolean(b)) => a == b,
        (AccountValue::Number(a), AccountValue::Number(b)) => a == b,
        (AccountValue::String(a), AccountValue::String(b)) => a == b,
        (AccountValue::BigInt(a), AccountValue::BigInt(b)) => a == b,
        (AccountValue::Object(a), AccountValue::Object(b)) => Rc::ptr_eq(a, b),
        (AccountValue::Function(a), AccountValue::Function(b))
        | (AccountValue::Symbol(a), AccountValue::Symbol(b)) => a.ptr_eq(b),
        _ => false,
    }
}
type Next = Box<dyn FnOnce(SigningResult<AccountValue>) -> SigningResult<Step>>;
enum Step {
    Done(AccountValue),
    Await(AccountValue, Next),
}
fn wait(
    value: SigningResult<AccountValue>,
    next: impl FnOnce(SigningResult<AccountValue>) -> SigningResult<Step> + 'static,
) -> SigningResult<Step> {
    match value {
        Ok(value) => Ok(Step::Await(value, Box::new(next))),
        Err(e) => next(Err(e)),
    }
}
fn drive(h: Rc<dyn OhlcvHost>, r: PromiseResolver<AccountValue>, step: SigningResult<Step>) {
    match step {
        Err(e) => r.settle(Err(e)),
        Ok(Step::Done(value)) => h.resolve_return(value, r),
        Ok(Step::Await(value, next)) => match h.await_value(value) {
            Err(e) => drive(h, r, next(Err(e))),
            Ok(p) => p.subscribe(move |v| drive(h, r, next(v.map(|v| (*v).clone())))),
        },
    }
}
fn start(
    h: Rc<dyn OhlcvHost>,
    work: impl FnOnce() -> SigningResult<Step>,
) -> NativePromise<AccountValue> {
    let (p, r) = NativePromise::pending(h.runtime());
    drive(h, r, work());
    p
}
fn now(h: &dyn OhlcvHost) -> SigningResult<AccountValue> {
    h.global("virtualClock.getCurrentTimeMs")?.call(&[])
}

fn nullish(
    h: &dyn OhlcvHost,
    v: &AccountValue,
    keys: &[&str],
    default: AccountValue,
) -> SigningResult<AccountValue> {
    for key in keys {
        let value = h.get(v, key)?;
        if !value.is_nullish() {
            return Ok(value);
        }
    }
    Ok(default)
}
fn interpolated(
    h: &dyn OhlcvHost,
    parts: &[&str],
    values: &[AccountValue],
) -> SigningResult<AccountValue> {
    let mut result = parts[0].encode_utf16().collect::<Vec<_>>();
    for (i, value) in values.iter().enumerate() {
        result.extend(h.template_string(value.clone())?);
        result.extend(parts[i + 1].encode_utf16());
    }
    Ok(AccountValue::String(result))
}
fn candle_scalar(
    h: &dyn OhlcvHost,
    candle: &AccountValue,
    keys: &[&str],
) -> SigningResult<AccountValue> {
    let parse = h.global("parseFloat")?;
    let value = nullish(h, candle, keys, text("0"))?;
    parse.call(&[value])
}
// ts: LighterExchange.fetchOHLCV
pub fn fetch_ohlcv(h: Rc<dyn OhlcvHost>, opts: AccountValue) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let store = get(h.as_ref(), "store")?;
        let markets = h.get(&store, "marketsBySymbol")?;
        let symbol = h.get(&opts, "symbol")?;
        let market = h.index(&markets, symbol)?;
        if !market.truthy() {
            let error = h.global("Error")?;
            let symbol = h.get(&opts, "symbol")?;
            let message = interpolated(
                h.as_ref(),
                &["Market not found for symbol: ", ""],
                &[symbol],
            )?;
            return Err(h.throw_value(error.construct(&[message])?));
        }
        let min = h.global("Math.min")?;
        let limit = h.get(&opts, "limit")?;
        let limit = min.call(&[
            if limit.truthy() {
                limit
            } else {
                AccountValue::Number(500.0)
            },
            AccountValue::Number(1500.0),
        ])?;
        let resolve = h.global("resolveLighterInterval")?;
        let interval = resolve.call(&[h.get(&opts, "interval")?])?;
        let raw_interval = h.get(&opts, "interval")?;
        let split = h.member(&raw_interval, "split")?;
        let regex = h.regexp("(\\d+)", "")?;
        let split = split.call(&[regex])?;
        let (amount, unit) = h.interval_binding(split)?;
        let end = if h.get(&opts, "to")?.truthy() {
            let date = h.global("dayjs")?;
            date.call(&[h.get(&opts, "to")?])?
        } else {
            h.global("dayjs")?.call(&[])?
        };
        // Both source end ternary arms reference the same hypotheticalEnd.
        let start = if !h.get(&opts, "limit")?.truthy() && h.get(&opts, "from")?.truthy() {
            let date = h.global("dayjs")?;
            date.call(&[h.get(&opts, "from")?])?
        } else {
            let subtract = h.member(&end, "subtract")?;
            let amount = h.global("parseFloat")?.call(&[amount])?;
            let amount = h.multiply(amount, limit.clone())?;
            subtract.call(&[amount, unit])?
        };
        let parse = h.global("parseInt")?;
        let market_id = parse.call(&[h.get(&market, "id")?, AccountValue::Number(10.0)])?;
        let client = get(h.as_ref(), "lighterApiClient")?;
        let get = h.member(&client, "get")?;
        let endpoint = h.constant("ENDPOINTS.CANDLES")?;
        let start = start_value(h.as_ref(), &start)?;
        let end = start_value(h.as_ref(), &end)?;
        let params = h.literal(&[
            ("market_id", market_id),
            ("resolution", interval),
            ("start_timestamp", start),
            ("end_timestamp", end),
            ("count_back", limit),
        ])?;
        let config = h.literal(&[
            ("params", params),
            ("__suppressExchangeError", AccountValue::Boolean(true)),
        ])?;
        let fetched = get.call(&[endpoint, config])?;
        let live = h.clone();
        Ok(Step::Await(
            fetched,
            Box::new(move |data| {
                let data = data?;
                let mut raw = optional(live.as_ref(), &data, "c")?;
                if !raw.truthy() {
                    raw = optional(live.as_ref(), &data, "candles")?
                }
                if !raw.truthy() {
                    raw = optional(live.as_ref(), &data, "candlesticks")?
                }
                if !raw.truthy() {
                    raw = live.array(&[])?
                }
                let array = live.global("Array.isArray")?.call(&[raw.clone()])?;
                if !array.truthy() || eq(&live.get(&raw, "length")?, &AccountValue::Number(0.0)) {
                    let error = live.global("console.error")?;
                    let symbol = live.get(&opts, "symbol")?;
                    let message =
                        interpolated(live.as_ref(), &["No data for symbol ", ""], &[symbol])?;
                    error.call(&[message])?;
                    return Ok(Step::Done(live.array(&[])?));
                }
                let order = live.global("orderBy")?;
                let map = live.member(&raw, "map")?;
                let callback_host = live.clone();
                let callback = live.function(Rc::new(move |args| {
                    let candle = args.first().cloned().unwrap_or(AccountValue::Undefined);
                    let number = callback_host.global("Number")?;
                    let timestamp = nullish(
                        callback_host.as_ref(),
                        &candle,
                        &["t", "timestamp"],
                        AccountValue::Number(0.0),
                    )?;
                    let timestamp = number.call(&[timestamp])?;
                    let timestamp =
                        callback_host.divide(timestamp, AccountValue::Number(1000.0))?;
                    let open = candle_scalar(callback_host.as_ref(), &candle, &["o", "open"])?;
                    let high = candle_scalar(callback_host.as_ref(), &candle, &["h", "high"])?;
                    let low = candle_scalar(callback_host.as_ref(), &candle, &["l", "low"])?;
                    let close = candle_scalar(callback_host.as_ref(), &candle, &["c", "close"])?;
                    let volume = candle_scalar(callback_host.as_ref(), &candle, &["v", "volume"])?;
                    callback_host.literal(&[
                        ("timestamp", timestamp),
                        ("open", open),
                        ("high", high),
                        ("low", low),
                        ("close", close),
                        ("volume", volume),
                    ])
                }))?;
                let mapped = map.call(&[callback])?;
                let keys = live.array(&[text("timestamp")])?;
                let directions = live.array(&[text("asc")])?;
                Ok(Step::Done(order.call(&[mapped, keys, directions])?))
            }),
        ))
    })
}
fn start_value(h: &dyn OhlcvHost, value: &AccountValue) -> SigningResult<AccountValue> {
    h.member(value, "valueOf")?.call(&[])
}
fn map(
    h: &dyn OhlcvHost,
    key: &str,
    method: &str,
) -> SigningResult<Rc<dyn crate::exchange_account_access::AccountAccessCall>> {
    let value = get(h, key)?;
    h.member(&value, method)
}
fn shared_error(
    h: &dyn OhlcvHost,
    opts: &AccountValue,
    key: AccountValue,
    error: crate::transaction_signing::TransactionSigningError,
) -> SigningResult<AccountValue> {
    let error = h.error_value(error);
    let number = h.global("Number")?;
    let response = optional(h, &error, "response")?;
    let status = optional(h, &response, "status")?;
    let status = if status.is_nullish() {
        optional(h, &error, "status")?
    } else {
        status
    };
    let status = number.call(&[status])?;
    if eq(&status, &AccountValue::Number(403.0)) {
        let set = map(h, "ohlcvFallbackCooldownUntilByKey", "set")?;
        let current = now(h)?;
        let delay = h.constant("LIGHTER_OHLCV_FALLBACK_403_COOLDOWN_MS")?;
        let until = h.add(current, delay)?;
        set.call(&[key, until])?;
        let log = h.member(&h.owner(), "log")?;
        let mut message = "[Lighter] listenOHLCV REST fallback returned 403 for "
            .encode_utf16()
            .collect::<Vec<_>>();
        message.extend(h.template_string(h.get(opts, "symbol")?)?);
        message.push(32);
        message.extend(h.template_string(h.get(opts, "interval")?)?);
        message.extend("; pausing fallback requests.".encode_utf16());
        let severity = h.constant("LogSeverity.Warning")?;
        log.call(&[AccountValue::String(message), severity])?;
        return Ok(AccountValue::Undefined);
    }
    let log = h.member(&h.owner(), "log")?;
    let mut message = "[Lighter] listenOHLCV REST fallback failed for "
        .encode_utf16()
        .collect::<Vec<_>>();
    message.extend(h.template_string(h.get(opts, "symbol")?)?);
    message.push(32);
    message.extend(h.template_string(h.get(opts, "interval")?)?);
    message.extend(": ".encode_utf16());
    let detail = optional(h, &error, "message")?;
    let detail = if detail.truthy() {
        detail
    } else {
        h.global("String")?.call(&[error])?
    };
    message.extend(h.template_string(detail)?);
    let severity = h.constant("LogSeverity.Warning")?;
    log.call(&[AccountValue::String(message), severity])?;
    Ok(AccountValue::Undefined)
}
fn shared_result(
    h: Rc<dyn OhlcvHost>,
    opts: AccountValue,
    key: AccountValue,
    result: SigningResult<AccountValue>,
) -> SigningResult<Step> {
    let attempt = (|| {
        let candles = result?;
        let length = h.get(&candles, "length")?;
        let last = h.subtract(length, AccountValue::Number(1.0))?;
        h.index(&candles, last)
    })();
    let result = match attempt {
        Ok(value) => Ok(value),
        Err(error) => shared_error(h.as_ref(), &opts, key.clone(), error),
    };
    map(h.as_ref(), "ohlcvFallbackFetchByKey", "delete")?.call(&[key])?;
    Ok(Step::Done(result?))
}
// ts: LighterExchange#fetchLatestOhlcvWithSharedFallback
pub fn fetch_latest_ohlcv_with_shared_fallback(
    h: Rc<dyn OhlcvHost>,
    opts: AccountValue,
    now_ms: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let key = call(h.as_ref(), "getOhlcvFallbackKey", &[opts.clone()])?;
        let cooldown =
            map(h.as_ref(), "ohlcvFallbackCooldownUntilByKey", "get")?.call(&[key.clone()])?;
        let cooldown = if cooldown.truthy() {
            cooldown
        } else {
            AccountValue::Number(0.0)
        };
        let current = now(h.as_ref())?;
        if h.less(current, cooldown)? {
            return Ok(Step::Done(AccountValue::Undefined));
        }
        let existing = map(h.as_ref(), "ohlcvFallbackFetchByKey", "get")?.call(&[key.clone()])?;
        if existing.truthy() {
            return Ok(Step::Done(existing));
        }
        let live = h.clone();
        let inner_opts = opts.clone();
        let inner_key = key.clone();
        let inner = start(h.clone(), move || {
            let fetched = (|| {
                let fetch = live.member(&live.owner(), "fetchOHLCV")?;
                let request = live.spread_override(
                    inner_opts.clone(),
                    &[("limit", AccountValue::Number(3.0)), ("to", now_ms)],
                )?;
                fetch.call(&[request])
            })();
            wait(fetched, move |value| {
                shared_result(live, inner_opts, inner_key, value)
            })
        });
        let promise = h.promise_value(inner);
        map(h.as_ref(), "ohlcvFallbackFetchByKey", "set")?.call(&[key, promise.clone()])?;
        Ok(Step::Done(promise))
    })
}
struct Listener {
    active: Cell<bool>,
    timeout: RefCell<AccountValue>,
    last_ws: RefCell<AccountValue>,
    fingerprint: RefCell<Vec<u16>>,
}
fn emit(
    h: &dyn OhlcvHost,
    state: &Listener,
    callback: &AccountValue,
    candle: AccountValue,
) -> SigningResult<AccountValue> {
    let mut fingerprint = Vec::new();
    for (i, key) in ["timestamp", "open", "high", "low", "close", "volume"]
        .into_iter()
        .enumerate()
    {
        if i > 0 {
            fingerprint.push(58)
        }
        fingerprint.extend(h.template_string(h.get(&candle, key)?)?)
    }
    if *state.fingerprint.borrow() == fingerprint {
        return Ok(AccountValue::Undefined);
    }
    *state.fingerprint.borrow_mut() = fingerprint;
    h.call_value(callback.clone(), &[candle])?;
    Ok(AccountValue::Undefined)
}
fn poll(
    h: Rc<dyn OhlcvHost>,
    state: Rc<Listener>,
    opts: AccountValue,
    callback: AccountValue,
    poll_callback: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        if !state.active.get() || get(h.as_ref(), "isDisposed")?.truthy() {
            return Ok(Step::Done(AccountValue::Undefined));
        }
        let current = now(h.as_ref())?;
        let last_ws = state.last_ws.borrow().clone();
        let positive = h.greater(last_ws, AccountValue::Number(0.0))?;
        let fresh = if positive {
            let last_ws = state.last_ws.borrow().clone();
            let age = h.subtract(current.clone(), last_ws)?;
            let stale = h.constant("LIGHTER_OHLCV_WS_STALE_MS")?;
            h.less_equal(age, stale)?
        } else {
            false
        };
        if fresh {
            schedule(h.as_ref(), &state, &poll_callback)?;
            return Ok(Step::Done(AccountValue::Undefined));
        }
        let socket = get(h.as_ref(), "publicWebsocket")?;
        let recover = h.member(&socket, "maybeRecoverOhlcvStream")?;
        let mut message = "no WS candle update for "
            .encode_utf16()
            .collect::<Vec<_>>();
        message.extend(h.template_string(h.get(&opts, "symbol")?)?);
        message.push(32);
        message.extend(h.template_string(h.get(&opts, "interval")?)?);
        message.extend("; using REST fallback".encode_utf16());
        recover.call(&[opts.clone(), AccountValue::String(message)])?;
        let fetched = call(
            h.as_ref(),
            "fetchLatestOhlcvWithSharedFallback",
            &[opts, current],
        )?;
        Ok(Step::Await(
            fetched,
            Box::new(move |candle| {
                let candle = candle?;
                if !state.active.get() || get(h.as_ref(), "isDisposed")?.truthy() {
                    return Ok(Step::Done(AccountValue::Undefined));
                }
                if candle.truthy() {
                    emit(h.as_ref(), &state, &callback, candle)?;
                }
                schedule(h.as_ref(), &state, &poll_callback)?;
                Ok(Step::Done(AccountValue::Undefined))
            }),
        ))
    })
}
fn schedule(h: &dyn OhlcvHost, state: &Listener, callback: &AccountValue) -> SigningResult<()> {
    let timeout = h.global("setTimeout")?;
    let interval = h.constant("LIGHTER_OHLCV_POLL_INTERVAL_MS")?;
    let value = timeout.call(&[callback.clone(), interval])?;
    *state.timeout.borrow_mut() = value;
    Ok(())
}
// ts: LighterExchange.listenOHLCV
pub fn listen_ohlcv(
    h: Rc<dyn OhlcvHost>,
    opts: AccountValue,
    callback: AccountValue,
) -> SigningResult<AccountValue> {
    let state = Rc::new(Listener {
        active: Cell::new(true),
        timeout: RefCell::new(AccountValue::Undefined),
        last_ws: RefCell::new(AccountValue::Number(0.0)),
        fingerprint: RefCell::new(vec![]),
    });
    let socket = get(h.as_ref(), "publicWebsocket")?;
    let listen = h.member(&socket, "listenOHLCV")?;
    let live = h.clone();
    let ws_state = state.clone();
    let ws_callback = callback.clone();
    let handler = h.function(Rc::new(move |args| {
        let current = now(live.as_ref())?;
        *ws_state.last_ws.borrow_mut() = current;
        emit(
            live.as_ref(),
            &ws_state,
            &ws_callback,
            args.first().cloned().unwrap_or(AccountValue::Undefined),
        )
    }))?;
    let unsubscribe = listen.call(&[opts.clone(), handler])?;
    let live = h.clone();
    let poll_state = state.clone();
    let poll_callback = h.self_function(Rc::new(move |callee, _| {
        let promise = poll(
            live.clone(),
            poll_state.clone(),
            opts.clone(),
            callback.clone(),
            callee,
        );
        Ok(live.promise_value(promise))
    }))?;
    h.call_value(poll_callback, &[])?;
    let live = h.clone();
    h.function(Rc::new(move |_| {
        state.active.set(false);
        if !unsubscribe.is_nullish() {
            live.call_value(unsubscribe.clone(), &[])?;
        }
        let present = state.timeout.borrow().truthy();
        if present {
            let clear = live.global("clearTimeout")?;
            let timeout = state.timeout.borrow().clone();
            clear.call(&[timeout])?;
            *state.timeout.borrow_mut() = AccountValue::Undefined;
        }
        Ok(AccountValue::Undefined)
    }))
}
// ts: LighterExchange.listenOrderBook
pub fn listen_order_book(
    h: &dyn OhlcvHost,
    symbol: AccountValue,
    callback: AccountValue,
) -> SigningResult<AccountValue> {
    let socket = get(h, "publicWebsocket")?;
    h.member(&socket, "listenOrderBook")?
        .call(&[symbol, callback])
}
