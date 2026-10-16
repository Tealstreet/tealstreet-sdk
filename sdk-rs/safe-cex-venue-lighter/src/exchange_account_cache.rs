//! Account snapshot/limits cache bodies with live owner/Promise bindings.
use crate::{
    exchange_trading_preflight::TradingHost,
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, PromiseResolver},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::rc::Rc;

pub trait AccountCacheHost: TradingHost {
    /// Infallible realm binding of the actual native Promise; no user operation.
    fn promise_value(&self, promise: NativePromise<AccountValue>) -> AccountValue;
    /// Full ECMAScript operations, retaining ToPrimitive order and thrown values.
    fn subtract(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue>;
    fn add(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue>;
    fn less(&self, left: AccountValue, right: AccountValue) -> SigningResult<bool>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn get(h: &dyn AccountCacheHost, key: &str) -> SigningResult<AccountValue> {
    h.get(&h.owner(), key)
}
fn set(h: &dyn AccountCacheHost, key: &str, value: AccountValue) -> SigningResult<()> {
    h.set(&h.owner(), key, value)
}
fn optional(h: &dyn AccountCacheHost, v: &AccountValue, key: &str) -> SigningResult<AccountValue> {
    if v.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.get(v, key)
    }
}
fn call(h: &dyn AccountCacheHost, key: &str, args: &[AccountValue]) -> SigningResult<AccountValue> {
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
fn fail(h: &dyn AccountCacheHost, message: AccountValue) -> SigningResult<Step> {
    let error = h.global("Error")?.construct(&[message])?;
    Err(h.throw_value(error))
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
fn drive(h: Rc<dyn AccountCacheHost>, r: PromiseResolver<AccountValue>, step: SigningResult<Step>) {
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
    h: Rc<dyn AccountCacheHost>,
    work: impl FnOnce() -> SigningResult<Step>,
) -> NativePromise<AccountValue> {
    let (p, r) = NativePromise::pending(h.runtime());
    drive(h, r, work());
    p
}
fn now(h: &dyn AccountCacheHost) -> SigningResult<AccountValue> {
    h.global("virtualClock.getCurrentTimeMs")?.call(&[])
}
fn remember(
    h: &dyn AccountCacheHost,
    value: AccountValue,
    version: AccountValue,
) -> SigningResult<Step> {
    call(
        h,
        "rememberRecentAccountSnapshot",
        &[value.clone(), version],
    )?;
    Ok(Step::Done(value))
}
fn snapshot_payload(
    h: Rc<dyn AccountCacheHost>,
    payload: AccountValue,
    version: AccountValue,
) -> SigningResult<Step> {
    if eq(
        &optional(h.as_ref(), &payload, "code")?,
        &AccountValue::Number(200.0),
    ) {
        let is_array = h.global("Array.isArray")?;
        if is_array
            .call(&[optional(h.as_ref(), &payload, "accounts")?])?
            .truthy()
        {
            let accounts = h.get(&payload, "accounts")?;
            let account = h.index(&accounts, AccountValue::Number(0.0))?;
            if account.truthy() {
                return remember(h.as_ref(), account, version);
            }
            return fail(
                h.as_ref(),
                text("Lighter account lookup returned no accounts"),
            );
        }
    }
    if eq(
        &optional(h.as_ref(), &payload, "success")?,
        &AccountValue::Boolean(true),
    ) && optional(h.as_ref(), &payload, "data")?.truthy()
    {
        let remember = h.member(&h.owner(), "rememberRecentAccountSnapshot")?;
        let data = h.get(&payload, "data")?;
        remember.call(&[data, version])?;
        return Ok(Step::Done(h.get(&payload, "data")?));
    }
    let error = h.global("Error")?;
    let mut message = optional(h.as_ref(), &payload, "message")?;
    if !message.truthy() {
        message = optional(
            h.as_ref(),
            &optional(h.as_ref(), &payload, "error")?,
            "message",
        )?
    }
    if !message.truthy() {
        message = text("Failed to fetch account snapshot")
    }
    Err(h.throw_value(error.construct(&[message])?))
}
// ts: LighterExchange#fetchAccountSnapshot
pub fn fetch_account_snapshot(
    h: Rc<dyn AccountCacheHost>,
    opts: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let opts = if matches!(opts, AccountValue::Undefined) {
            h.literal(&[])?
        } else {
            opts
        };
        let now = now(h.as_ref())?;
        if h.get(&opts, "allowRecent")?.truthy()
            && get(h.as_ref(), "recentAccountSnapshot")?.truthy()
        {
            let recent = get(h.as_ref(), "recentAccountSnapshot")?;
            let version = h.get(&recent, "cacheVersion")?;
            if eq(&version, &get(h.as_ref(), "accountSnapshotCacheVersion")?) {
                let recent = get(h.as_ref(), "recentAccountSnapshot")?;
                let at = h.get(&recent, "fetchedAt")?;
                let age = h.subtract(now, at)?;
                let threshold = h.constant("LIGHTER_ACCOUNT_SNAPSHOT_REUSE_MS")?;
                if h.less(age, threshold)? {
                    let recent = get(h.as_ref(), "recentAccountSnapshot")?;
                    return Ok(Step::Done(h.get(&recent, "value")?));
                }
            }
        }
        if get(h.as_ref(), "accountSnapshotFetchPromise")?.truthy() {
            let version = get(h.as_ref(), "accountSnapshotFetchVersion")?;
            if eq(&version, &get(h.as_ref(), "accountSnapshotCacheVersion")?) {
                return Ok(Step::Done(get(h.as_ref(), "accountSnapshotFetchPromise")?));
            }
        }
        let version = get(h.as_ref(), "accountSnapshotCacheVersion")?;
        set(h.as_ref(), "accountSnapshotFetchVersion", version.clone())?;
        let live = h.clone();
        let inner_version = version.clone();
        let inner = start(h.clone(), move || {
            wait(
                call(live.as_ref(), "ensureAccountIndex", &[]),
                move |account| {
                    let account = account?;
                    let client = get(live.as_ref(), "lighterApiClient")?;
                    let get = live.member(&client, "get")?;
                    let endpoint = live.constant("ENDPOINTS.ACCOUNT")?;
                    let value = live.member(&account, "toString")?.call(&[])?;
                    let params = live.literal(&[("by", text("index")), ("value", value)])?;
                    let config = live.literal(&[("params", params)])?;
                    wait(get.call(&[endpoint, config]), move |payload| {
                        snapshot_payload(live, payload?, inner_version)
                    })
                },
            )
        });
        let value = h.promise_value(inner);
        let finally = h.member(&value, "finally")?;
        let live = h.clone();
        let callback = h.function(Rc::new(move |_| {
            if eq(
                &get(live.as_ref(), "accountSnapshotFetchVersion")?,
                &version,
            ) {
                set(
                    live.as_ref(),
                    "accountSnapshotFetchPromise",
                    AccountValue::Undefined,
                )?
            }
            Ok(AccountValue::Undefined)
        }))?;
        let promise = finally.call(&[callback])?;
        set(h.as_ref(), "accountSnapshotFetchPromise", promise)?;
        Ok(Step::Done(get(h.as_ref(), "accountSnapshotFetchPromise")?))
    })
}
fn retry(h: &dyn AccountCacheHost, now: AccountValue) -> SigningResult<()> {
    let delay = h.constant("LIGHTER_ACCOUNT_LIMITS_RETRY_MS")?;
    let next = h.add(now, delay)?;
    set(h, "accountLimitsNextFetchAt", next)
}
fn limits_error(
    h: Rc<dyn AccountCacheHost>,
    now: AccountValue,
    error: TransactionSigningError,
) -> SigningResult<Step> {
    retry(h.as_ref(), now)?;
    if !get(h.as_ref(), "hasWarnedAccountLimitsLookup")?.truthy() {
        set(
            h.as_ref(),
            "hasWarnedAccountLimitsLookup",
            AccountValue::Boolean(true),
        )?;
        let log = h.member(&h.owner(), "log")?;
        let error = h.error_value(error);
        let message = optional(h.as_ref(), &error, "message")?;
        let message = if message.truthy() {
            message
        } else {
            h.global("String")?.call(&[error])?
        };
        let mut units = "Lighter account tier lookup failed: "
            .encode_utf16()
            .collect::<Vec<_>>();
        units.extend(h.template_string(message)?);
        let severity = h.constant("LogSeverity.Debug")?;
        log.call(&[AccountValue::String(units), severity])?;
    }
    Ok(Step::Done(get(h.as_ref(), "accountLimits")?))
}
fn limits_payload(
    h: Rc<dyn AccountCacheHost>,
    now: AccountValue,
    result: SigningResult<AccountValue>,
) -> SigningResult<Step> {
    let outcome = (|| {
        let payload = result?;
        if eq(
            &optional(h.as_ref(), &payload, "code")?,
            &AccountValue::Number(200.0),
        ) {
            set(h.as_ref(), "accountLimits", payload.clone())?;
            let duration = h.constant("LIGHTER_ACCOUNT_LIMITS_CACHE_MS")?;
            let next = h.add(now.clone(), duration)?;
            set(h.as_ref(), "accountLimitsNextFetchAt", next)?;
            return Ok(Step::Done(payload));
        }
        let error = h.global("Error")?;
        let message = optional(h.as_ref(), &payload, "message")?;
        let message = if message.truthy() {
            message
        } else {
            text("Lighter account limits lookup failed")
        };
        Err(h.throw_value(error.construct(&[message])?))
    })();
    match outcome {
        Ok(value) => Ok(value),
        Err(error) => limits_error(h, now, error),
    }
}
fn limits_request(
    h: Rc<dyn AccountCacheHost>,
    now: AccountValue,
    index: AccountValue,
    auth: AccountValue,
) -> SigningResult<Step> {
    let request = (|| {
        let value = h.member(&index, "toString")?.call(&[])?;
        let params = h.literal(&[("account_index", value)])?;
        let headers = h.literal(&[("authorization", auth)])?;
        let config = h.literal(&[
            ("params", params),
            ("headers", headers),
            ("__suppressExchangeError", AccountValue::Boolean(true)),
        ])?;
        let client = get(h.as_ref(), "lighterApiClient")?;
        let get = h.member(&client, "get")?;
        let endpoint = h.constant("ENDPOINTS.ACCOUNT_LIMITS")?;
        get.call(&[endpoint, config])
    })();
    wait(request, move |payload| limits_payload(h, now, payload))
}
// ts: LighterExchange#fetchAccountLimits
pub fn fetch_account_limits(h: Rc<dyn AccountCacheHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let now = now(h.as_ref())?;
        if h.less(now.clone(), get(h.as_ref(), "accountLimitsNextFetchAt")?)? {
            return Ok(Step::Done(get(h.as_ref(), "accountLimits")?));
        }
        if get(h.as_ref(), "accountLimitsFetchPromise")?.truthy() {
            return Ok(Step::Done(get(h.as_ref(), "accountLimitsFetchPromise")?));
        }
        let live = h.clone();
        let inner = start(h.clone(), move || {
            wait(
                call(live.as_ref(), "ensureAccountIndex", &[]),
                move |index| {
                    let index = index?;
                    wait(
                        call(
                            live.as_ref(),
                            "getPrivateWsAuthToken",
                            &[AccountValue::Number(60.0)],
                        ),
                        move |auth| {
                            let auth = auth?;
                            if !auth.truthy() {
                                retry(live.as_ref(), now)?;
                                return Ok(Step::Done(get(live.as_ref(), "accountLimits")?));
                            }
                            limits_request(live, now, index, auth)
                        },
                    )
                },
            )
        });
        let value = h.promise_value(inner);
        let finally = h.member(&value, "finally")?;
        let live = h.clone();
        let callback = h.function(Rc::new(move |_| {
            set(
                live.as_ref(),
                "accountLimitsFetchPromise",
                AccountValue::Undefined,
            )?;
            Ok(AccountValue::Undefined)
        }))?;
        let promise = finally.call(&[callback])?;
        set(h.as_ref(), "accountLimitsFetchPromise", promise)?;
        Ok(Step::Done(get(h.as_ref(), "accountLimitsFetchPromise")?))
    })
}
