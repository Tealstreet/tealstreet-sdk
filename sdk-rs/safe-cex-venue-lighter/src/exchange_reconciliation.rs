//! Private reconciliation/lifecycle bodies. Full JS operations and import calls
//! remain live required hosts, including async-return adoption. This does not
//! construct an exchange, HTTP client, signer, websocket or JS/timer runtime.
//! The inherited global host must capture actual superclass dispose lookup
//! with the owner receiver for super.dispose.
//! Clock/update fields use ordinary Number-or-undefined source domains.
use crate::{
    exchange_account_access::ExchangeAccountAccessHost,
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, PromiseResolver},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::rc::Rc;
pub type ReconciliationCallback = Rc<dyn Fn() -> SigningResult<AccountValue>>;
pub trait ReconciliationHost: ExchangeAccountAccessHost {
    /// Fresh ordinary Array with these ordered elements; full host realm/iterator.
    fn array(&self, values: &[AccountValue]) -> SigningResult<AccountValue>;
    /// Source destructuring of two values, with actual GetIterator/next/close and
    /// original throws. Not JSON indexing or a fixed-array shape cast.
    fn pair(&self, value: AccountValue) -> SigningResult<(AccountValue, AccountValue)>;
    /// Full fresh-object CopyDataProperties plus ordered own overrides, retaining
    /// enumerable unknown UTF16/Symbol keys, getters, descriptors and JS own order.
    fn spread_override(
        &self,
        value: AccountValue,
        fields: &[(&str, AccountValue)],
    ) -> SigningResult<AccountValue>;
    /// Fresh actual callable retaining the live owner closure.
    fn callback(&self, value: ReconciliationCallback) -> SigningResult<AccountValue>;
    /// Pure infallible original thrown value/Error projection, never message-only.
    fn error_value(&self, error: TransactionSigningError) -> AccountValue;
    /// Full template ToString (UTF16 and original coercion throws).
    fn template_string(&self, value: AccountValue) -> SigningResult<Vec<u16>>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn optional(
    h: &dyn ReconciliationHost,
    v: &AccountValue,
    key: &str,
) -> SigningResult<AccountValue> {
    if v.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.get(v, key)
    }
}
fn member_call(
    h: &dyn ReconciliationHost,
    object: &AccountValue,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(object, key)?.call(args)
}
fn call(
    h: &dyn ReconciliationHost,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    member_call(h, &h.owner(), key, args)
}
fn store(h: &dyn ReconciliationHost) -> SigningResult<AccountValue> {
    h.get(&h.owner(), "store")
}
fn number(v: AccountValue) -> f64 {
    match v {
        AccountValue::Number(n) => n,
        AccountValue::Undefined => f64::NAN,
        _ => unreachable!("source ordinary Number-or-undefined clock/update field"),
    }
}
fn undefined() -> SigningResult<Step> {
    Ok(Step::Done(AccountValue::Undefined))
}
type Continuation = Box<dyn FnOnce(SigningResult<AccountValue>) -> SigningResult<Step>>;
enum Step {
    Done(AccountValue),
    Await(AccountValue, Continuation),
}
/// Capture synchronous source throws within try BEFORE creating an await step.
fn try_await(
    value: SigningResult<AccountValue>,
    next: impl FnOnce(SigningResult<AccountValue>) -> SigningResult<Step> + 'static,
) -> SigningResult<Step> {
    match value {
        Ok(v) => Ok(Step::Await(v, Box::new(next))),
        Err(e) => next(Err(e)),
    }
}
fn drive(
    h: Rc<dyn ReconciliationHost>,
    resolver: PromiseResolver<AccountValue>,
    step: SigningResult<Step>,
) {
    match step {
        Err(error) => resolver.settle(Err(error)),
        Ok(Step::Done(value)) => h.resolve_return(value, resolver),
        Ok(Step::Await(value, next)) => match h.await_value(value) {
            Err(error) => drive(h, resolver, next(Err(error))),
            Ok(promise) => promise
                .subscribe(move |value| drive(h, resolver, next(value.map(|v| (*v).clone())))),
        },
    }
}
fn start(
    h: Rc<dyn ReconciliationHost>,
    work: impl FnOnce() -> SigningResult<Step>,
) -> NativePromise<AccountValue> {
    let (p, r) = NativePromise::pending(h.runtime());
    let step = work();
    drive(h, r, step);
    p
}
fn error_message(
    h: &dyn ReconciliationHost,
    error: TransactionSigningError,
) -> SigningResult<AccountValue> {
    let error = h.error_value(error);
    let message = optional(h, &error, "message")?;
    if message.truthy() {
        Ok(message)
    } else {
        h.global("String")?.call(&[error])
    }
}
fn log_error(
    h: &dyn ReconciliationHost,
    error: TransactionSigningError,
    prefix: &str,
) -> SigningResult<Step> {
    let log = h.member(&h.owner(), "log")?;
    let message = error_message(h, error)?;
    let mut value: Vec<u16> = prefix.encode_utf16().collect();
    value.extend(h.template_string(message)?);
    log.call(&[AccountValue::String(value), text("warning")])?;
    undefined()
}
fn update_balance(h: &dyn ReconciliationHost, balance: AccountValue) -> SigningResult<Step> {
    let update = h.member(&store(h)?, "update")?;
    let loaded = h.get(&store(h)?, "loaded")?;
    let loaded = h.spread_override(loaded, &[("balance", AccountValue::Boolean(true))])?;
    let update_value = h.literal(&[("balance", balance), ("loaded", loaded)])?;
    update.call(&[update_value])?;
    undefined()
}
// ts: LighterExchange.fetchAndSetPrivateSnapshot
pub fn fetch_and_set_private_snapshot(
    h: Rc<dyn ReconciliationHost>,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let result = call(h.as_ref(), "fetchAndSetPrivateSnapshotHelper", &[])?;
        try_await(Ok(result), |result| {
            result?;
            undefined()
        })
    })
}
fn snapshot_error(
    h: &dyn ReconciliationHost,
    error: TransactionSigningError,
) -> SigningResult<Step> {
    call(
        h,
        "handlePrivateSnapshotFetchError",
        &[h.error_value(error)],
    )?;
    undefined()
}
// ts: LighterExchange.fetchAndSetPrivateSnapshotHelper
pub fn fetch_and_set_private_snapshot_helper(
    h: Rc<dyn ReconciliationHost>,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let initial = (|| {
            let all = h.global("Promise.all")?;
            let positions = h.member(&h.owner(), "fetchPositions")?;
            let opts = h.literal(&[("allowRecentAccountSnapshot", AccountValue::Boolean(true))])?;
            let positions = positions.call(&[AccountValue::Undefined, opts])?;
            let orders = call(h.as_ref(), "fetchOrders", &[])?;
            let values = h.array(&[positions, orders])?;
            all.call(&[values])
        })();
        let live = h.clone();
        try_await(initial, move |result| {
            let continuation = (|| {
                let (fetched_positions, fetched_orders) = live.pair(result?)?;
                if live.get(&live.owner(), "isDisposed")?.truthy() {
                    return undefined();
                }
                let positions = if !matches!(
                    live.get(&live.owner(), "lastWebsocketPositionsUpdate")?,
                    AccountValue::Undefined
                ) {
                    let now = number(live.global("virtualClock.getCurrentTimeMs")?.call(&[])?);
                    let last = number(live.get(&live.owner(), "lastWebsocketPositionsUpdate")?);
                    if now - last < 1_000.0 {
                        let merge = live.global("mergeUpdatesUsingLastUpdate")?;
                        let current = live.get(&store(live.as_ref())?, "positions")?;
                        merge.call(&[fetched_positions, current])?
                    } else {
                        fetched_positions
                    }
                } else {
                    fetched_positions
                };
                member_call(
                    live.as_ref(),
                    &store(live.as_ref())?,
                    "addOrUpdatePositionsFull",
                    &[positions],
                )?;
                member_call(
                    live.as_ref(),
                    &store(live.as_ref())?,
                    "addOrUpdateOrdersFull",
                    &[fetched_orders],
                )?;
                let update = live.member(&store(live.as_ref())?, "update")?;
                let loaded = live.get(&store(live.as_ref())?, "loaded")?;
                let loaded = live.spread_override(
                    loaded,
                    &[
                        ("positions", AccountValue::Boolean(true)),
                        ("orders", AccountValue::Boolean(true)),
                    ],
                )?;
                update.call(&[live.literal(&[("loaded", loaded)])?])?;
                let fetch = live.member(&live.owner(), "fetchBalance")?;
                let opts =
                    live.literal(&[("allowRecentAccountSnapshot", AccountValue::Boolean(true))])?;
                let balance = fetch.call(&[opts])?;
                let after = live.clone();
                try_await(Ok(balance), move |balance| {
                    let complete = (|| {
                        let balance = balance?;
                        if after.get(&after.owner(), "isDisposed")?.truthy() {
                            return undefined();
                        }
                        update_balance(after.as_ref(), balance)
                    })();
                    match complete {
                        Ok(s) => Ok(s),
                        Err(error) => snapshot_error(after.as_ref(), error),
                    }
                })
            })();
            match continuation {
                Ok(s) => Ok(s),
                Err(error) => snapshot_error(live.as_ref(), error),
            }
        })
    })
}
fn tick_loop(h: Rc<dyn ReconciliationHost>) -> SigningResult<Step> {
    let loop_call = h.global("loop")?;
    let owner = h.clone();
    let callback = h.callback(Rc::new(move || call(owner.as_ref(), "tick", &[])))?;
    let interval = call(h.as_ref(), "getTickInterval", &[])?;
    loop_call.call(&[callback, interval])?;
    undefined()
}
// ts: LighterExchange.tick
pub fn tick(h: Rc<dyn ReconciliationHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        if h.get(&h.owner(), "isDisposed")?.truthy() {
            return undefined();
        }
        let fetched = call(h.as_ref(), "fetchAndSetPrivateSnapshotHelper", &[]);
        let live = h.clone();
        try_await(fetched, move |result| {
            if let Err(error) = result {
                let emit = live.member(&live.owner(), "emitAndLogError")?;
                let error = live.error_value(error);
                let message = optional(live.as_ref(), &error, "message")?;
                emit.call(&[message])?;
            }
            tick_loop(live)
        })
    })
}
// ts: LighterExchange.fetchBalance
pub fn fetch_balance(
    h: Rc<dyn ReconciliationHost>,
    opts: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let opts = if matches!(opts, AccountValue::Undefined) {
            h.literal(&[])?
        } else {
            opts
        };
        let raw = call(h.as_ref(), "fetchRawBalance", &[opts])?;
        let live = h.clone();
        try_await(Ok(raw), move |raw| {
            let raw = raw?;
            if live.get(&live.owner(), "isDisposed")?.truthy() {
                Ok(Step::Done(live.get(&store(live.as_ref())?, "balance")?))
            } else {
                Ok(Step::Done(raw))
            }
        })
    })
}
// ts: LighterExchange.fetchAndSetBalance
pub fn fetch_and_set_balance(
    h: Rc<dyn ReconciliationHost>,
    opts: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let opts = if matches!(opts, AccountValue::Undefined) {
            h.literal(&[])?
        } else {
            opts
        };
        let raw = call(h.as_ref(), "fetchRawBalance", &[opts])?;
        let live = h.clone();
        try_await(Ok(raw), move |balance| {
            let balance = balance?;
            if live.get(&live.owner(), "isDisposed")?.truthy() {
                undefined()
            } else {
                update_balance(live.as_ref(), balance)
            }
        })
    })
}
// ts: LighterExchange#syncLeverageFromExchange
pub fn sync_leverage_from_exchange(
    h: Rc<dyn ReconciliationHost>,
    symbol: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let fetched = call(h.as_ref(), "fetchPositions", &[]);
        let live = h.clone();
        try_await(fetched, move |positions| {
            let complete = (|| {
                let positions = positions?;
                if live.get(&live.owner(), "isDisposed")?.truthy() {
                    return undefined();
                }
                member_call(
                    live.as_ref(),
                    &store(live.as_ref())?,
                    "addOrUpdatePositionsFull",
                    &[positions],
                )?;
                undefined()
            })();
            match complete {
                Ok(s) => Ok(s),
                Err(error) => {
                    let log = live.member(&live.owner(), "log")?;
                    let mut message: Vec<u16> = "[Lighter] Failed to refresh leverage state for "
                        .encode_utf16()
                        .collect();
                    message.extend(live.template_string(symbol)?);
                    message.extend(": ".encode_utf16());
                    let error = error_message(live.as_ref(), error)?;
                    message.extend(live.template_string(error)?);
                    log.call(&[AccountValue::String(message), text("warning")])?;
                    undefined()
                }
            }
        })
    })
}
fn auth_token(h: Rc<dyn ReconciliationHost>, ttl: AccountValue) -> SigningResult<Step> {
    let signer = h.get(&h.owner(), "signerAdapter")?;
    if !signer.truthy() {
        return undefined();
    }
    let result = member_call(h.as_ref(), &signer, "getAuthToken", &[ttl]);
    let live = h.clone();
    try_await(result, move |result| match result {
        Ok(value) => Ok(Step::Done(value)),
        Err(error) => log_error(
            live.as_ref(),
            error,
            "Failed to create private WS auth token: ",
        ),
    })
}
// ts: LighterExchange.getPrivateWsAuthToken
pub fn get_private_ws_auth_token(
    h: Rc<dyn ReconciliationHost>,
    ttl: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let ttl = if matches!(ttl, AccountValue::Undefined) {
            AccountValue::Number(60.0)
        } else {
            ttl
        };
        if !h.get(&h.owner(), "signerAdapter")?.truthy() {
            let result = call(h.as_ref(), "ensureSignerAdapter", &[]);
            let live = h.clone();
            try_await(result, move |result| match result {
                Ok(_) => auth_token(live, ttl),
                Err(error) => {
                    let warn = live.member(&live.owner(), "warnSignerUnavailable")?;
                    let error = error_message(live.as_ref(), error)?;
                    let mut message: Vec<u16> = "Failed to initialize signer for private WS auth: "
                        .encode_utf16()
                        .collect();
                    message.extend(live.template_string(error)?);
                    warn.call(&[AccountValue::String(message)])?;
                    undefined()
                }
            })
        } else {
            auth_token(h, ttl)
        }
    })
}
// ts: LighterExchange.dispose
pub fn dispose(h: &dyn ReconciliationHost) -> SigningResult<AccountValue> {
    h.global("super.dispose")?.call(&[])?;
    for (field, key) in [
        ("ohlcvFallbackFetchByKey", "clear"),
        ("publicWebsocket", "dispose"),
        ("privateWebsocket", "dispose"),
    ] {
        let object = h.get(&h.owner(), field)?;
        member_call(h, &object, key, &[])?;
    }
    let signer = h.get(&h.owner(), "signerAdapter")?;
    if !signer.is_nullish() && h.get(&signer, "dispose")?.truthy() {
        let signer = h.get(&h.owner(), "signerAdapter")?;
        member_call(h, &signer, "dispose", &[])?;
    }
    Ok(AccountValue::Undefined)
}
