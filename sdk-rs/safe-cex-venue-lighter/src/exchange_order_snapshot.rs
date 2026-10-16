//! The active-order snapshot body and its local parsing/fallback helpers.
use crate::{
    exchange_trading_preflight::TradingHost,
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, PromiseResolver},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::rc::Rc;
pub trait CancelledOrderEntries {
    /// Actual for-of advance and [id, timestamp] binding. Abrupt binding closes
    /// the outer iterator; abrupt iterator advance follows its own JS semantics.
    fn next_entry(&mut self) -> SigningResult<Option<(AccountValue, AccountValue)>>;
    fn close_on_throw(&mut self, error: TransactionSigningError) -> TransactionSigningError;
}
pub type CandidateVisitor = Rc<dyn Fn(AccountValue) -> SigningResult<Option<AccountValue>>>;
pub trait OrderSnapshotHost: TradingHost {
    /// For-of with early return/throw completion and actual IteratorClose.
    /// None means exhaustion; the visitor performs the Rust body for each value.
    fn visit_until(
        &self,
        iterable: AccountValue,
        visitor: CandidateVisitor,
    ) -> SigningResult<Option<AccountValue>>;
    /// Infallible realm binding retaining the exact Promise identity and queued
    /// settlement/adoption. It must not copy or eagerly unwrap the result.
    fn promise_value(&self, promise: NativePromise<AccountValue>) -> AccountValue;
    fn regex(&self, pattern: &[u16], flags: &[u16]) -> SigningResult<AccountValue>;
    fn has_key(&self, object: &AccountValue, key: &str) -> SigningResult<bool>;
    fn global_value(&self, name: &str) -> SigningResult<AccountValue>;
    fn instance_of(&self, value: &AccountValue, constructor: AccountValue) -> SigningResult<bool>;
    fn subtract(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue>;
    fn greater_than(&self, left: AccountValue, right: AccountValue) -> SigningResult<bool>;
    fn greater_or_equal(&self, left: AccountValue, right: AccountValue) -> SigningResult<bool>;
    fn cancelled_entries(
        &self,
        value: AccountValue,
    ) -> SigningResult<Box<dyn CancelledOrderEntries>>;
}
fn text(value: &str) -> AccountValue {
    AccountValue::String(value.encode_utf16().collect())
}
fn arg(args: &[AccountValue], index: usize) -> AccountValue {
    args.get(index).cloned().unwrap_or(AccountValue::Undefined)
}
fn get(h: &dyn OrderSnapshotHost, key: &str) -> SigningResult<AccountValue> {
    h.get(&h.owner(), key)
}
fn set(h: &dyn OrderSnapshotHost, key: &str, value: AccountValue) -> SigningResult<()> {
    h.set(&h.owner(), key, value)
}
fn call(
    h: &dyn OrderSnapshotHost,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(&h.owner(), key)?.call(args)
}
fn method(
    h: &dyn OrderSnapshotHost,
    value: &AccountValue,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(value, key)?.call(args)
}
fn optional_path(
    h: &dyn OrderSnapshotHost,
    value: &AccountValue,
    keys: &[&str],
) -> SigningResult<AccountValue> {
    let mut value = value.clone();
    for key in keys {
        if value.is_nullish() {
            return Ok(AccountValue::Undefined);
        }
        value = h.get(&value, key)?;
    }
    Ok(value)
}
fn is_object(value: &AccountValue) -> bool {
    matches!(value, AccountValue::Object(_))
}
fn is_array(h: &dyn OrderSnapshotHost, value: AccountValue) -> SigningResult<bool> {
    Ok(h.global("Array.isArray")?.call(&[value])?.truthy())
}
fn store_orders(h: &dyn OrderSnapshotHost) -> SigningResult<AccountValue> {
    h.get(&get(h, "store")?, "orders")
}
type Next = Box<dyn FnOnce(SigningResult<AccountValue>) -> SigningResult<Step>>;
enum Step {
    Done(AccountValue),
    Await(AccountValue, Next),
}
fn drive(
    h: Rc<dyn OrderSnapshotHost>,
    resolver: PromiseResolver<AccountValue>,
    step: SigningResult<Step>,
) {
    match step {
        Err(e) => resolver.settle(Err(e)),
        Ok(Step::Done(value)) => h.resolve_return(value, resolver),
        Ok(Step::Await(value, next)) => match h.await_value(value) {
            Err(e) => drive(h, resolver, next(Err(e))),
            Ok(p) => p.subscribe(move |value| {
                drive(h, resolver, next(value.map(|value| (*value).clone())))
            }),
        },
    }
}
fn start(
    h: Rc<dyn OrderSnapshotHost>,
    work: impl FnOnce() -> SigningResult<Step>,
) -> NativePromise<AccountValue> {
    let (p, resolver) = NativePromise::pending(h.runtime());
    drive(h, resolver, work());
    p
}
fn warning_index(h: &dyn OrderSnapshotHost, error: AccountValue) -> SigningResult<AccountValue> {
    if !get(h, "hasWarnedMissingOrderAccountIndex")?.truthy() {
        set(
            h,
            "hasWarnedMissingOrderAccountIndex",
            AccountValue::Boolean(true),
        )?;
        let log = h.member(&h.owner(), "log")?;
        let message = optional_path(h, &error, &["message"])?;
        let message = if message.truthy() {
            message
        } else {
            h.global("String")?.call(&[error])?
        };
        let mut units="Lighter account index is unavailable for accountActiveOrders; private order sync is temporarily disabled (".encode_utf16().collect::<Vec<_>>();
        units.extend(h.template_string(message)?);
        units.extend(")".encode_utf16());
        log.call(&[
            AccountValue::String(units),
            h.constant("LogSeverity.Warning")?,
        ])?;
    }
    Ok(AccountValue::Undefined)
}
// ts-local: isInvalidAuthAccountError
fn is_invalid_auth_account_error(
    h: Rc<dyn OrderSnapshotHost>,
    error: AccountValue,
) -> SigningResult<bool> {
    let candidates = h.array(&[
        optional_path(h.as_ref(), &error, &["message"])?,
        optional_path(h.as_ref(), &error, &["response", "data", "message"])?,
        optional_path(
            h.as_ref(),
            &error,
            &["response", "data", "error", "message"],
        )?,
    ])?;
    let some = h.member(&candidates, "some")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let regex = live.regex(
            &r"invalid auth:\s*couldn'?t find account"
                .encode_utf16()
                .collect::<Vec<_>>(),
            &"i".encode_utf16().collect::<Vec<_>>(),
        )?;
        let test = live.member(&regex, "test")?;
        let string = live.global("String")?;
        let message = arg(args, 0);
        let message = string.call(&[if message.truthy() { message } else { text("") }])?;
        test.call(&[message])
    }))?;
    Ok(some.call(&[callback])?.truthy())
}
// ts-local: handleInvalidAuthAccountError
fn handle_invalid_auth_account_error(
    h: &dyn OrderSnapshotHost,
    error: AccountValue,
) -> SigningResult<AccountValue> {
    if !get(h, "hasWarnedInvalidOrderAuthAccount")?.truthy() {
        set(
            h,
            "hasWarnedInvalidOrderAuthAccount",
            AccountValue::Boolean(true),
        )?;
        let constructor = h.global_value("Error")?;
        let message = if h.instance_of(&error, constructor)? {
            h.get(&error, "message")?
        } else {
            h.global("String")?.call(&[error])?
        };
        let log = h.member(&h.owner(), "log")?;
        let mut units="Lighter accountActiveOrders auth was rejected; private order sync is temporarily disabled. Verify the account index, API key index, and private key all belong to the same Lighter deployment (".encode_utf16().collect::<Vec<_>>();
        units.extend(h.template_string(message)?);
        units.extend(")".encode_utf16());
        log.call(&[
            AccountValue::String(units),
            h.constant("LogSeverity.Warning")?,
        ])?;
    }
    store_orders(h)
}
// ts-local: flattenCandidate
fn flatten_candidate(
    h: Rc<dyn OrderSnapshotHost>,
    candidate: AccountValue,
) -> SigningResult<AccountValue> {
    if is_array(h.as_ref(), candidate.clone())? {
        return Ok(candidate);
    }
    if candidate.truthy() && is_object(&candidate) {
        let values = h.global("Object.values")?.call(&[candidate])?;
        let flat_map = h.member(&values, "flatMap")?;
        let live = h.clone();
        let callback = h.function(Rc::new(move |args| {
            let entry = arg(args, 0);
            if is_array(live.as_ref(), entry.clone())? {
                Ok(entry)
            } else if entry.truthy() && is_object(&entry) {
                live.array(&[entry])
            } else {
                live.array(&[])
            }
        }))?;
        return flat_map.call(&[callback]);
    }
    h.array(&[])
}
// ts-local: looksLikeOrder
fn looks_like_order(h: &dyn OrderSnapshotHost, entry: AccountValue) -> SigningResult<AccountValue> {
    if !entry.truthy() || !is_object(&entry) {
        return Ok(AccountValue::Boolean(false));
    }
    for key in [
        "orderId",
        "order_id",
        "orderIndex",
        "order_index",
        "OrderId",
        "OrderIndex",
        "marketId",
        "market_id",
        "marketIndex",
        "MarketIndex",
        "symbol",
        "order",
    ] {
        if h.has_key(&entry, key)? {
            return Ok(AccountValue::Boolean(true));
        }
    }
    Ok(AccountValue::Boolean(false))
}
// ts-local: readOrders
fn read_orders(h: Rc<dyn OrderSnapshotHost>, payload: AccountValue) -> SigningResult<AccountValue> {
    let candidates = [
        optional_path(h.as_ref(), &payload, &["orders"])?,
        optional_path(h.as_ref(), &payload, &["data", "orders"])?,
        optional_path(h.as_ref(), &payload, &["result", "orders"])?,
        optional_path(h.as_ref(), &payload, &["data"])?,
        optional_path(h.as_ref(), &payload, &["result"])?,
        payload,
    ];
    // The array literal's actual iterator is observable in the source for-of.
    let candidates = h.array(&candidates)?;
    read_candidates(h, candidates)
}
fn read_candidates(
    h: Rc<dyn OrderSnapshotHost>,
    candidates: AccountValue,
) -> SigningResult<AccountValue> {
    let live = h.clone();
    let found = h.visit_until(
        candidates,
        Rc::new(move |candidate| {
            let parsed = flatten_candidate(live.clone(), candidate)?;
            if !live.greater_than(live.get(&parsed, "length")?, AccountValue::Number(0.0))? {
                return Ok(None);
            }
            let filter = live.member(&parsed, "filter")?;
            let body = live.clone();
            let callback = live.function(Rc::new(move |args| {
                looks_like_order(body.as_ref(), arg(args, 0))
            }))?;
            let order_like = filter.call(&[callback])?;
            let found = if live
                .greater_than(live.get(&order_like, "length")?, AccountValue::Number(0.0))?
            {
                order_like
            } else {
                parsed
            };
            Ok(Some(found))
        }),
    )?;
    match found {
        Some(value) => Ok(value),
        None => h.array(&[]),
    }
}
// ts-local: fetchOrdersForMarket
fn fetch_orders_for_market(
    h: Rc<dyn OrderSnapshotHost>,
    index: AccountValue,
    auth: AccountValue,
    market: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let client = get(h.as_ref(), "lighterApiClient")?;
        let request = h.member(&client, "get")?;
        let endpoint = h.constant("ENDPOINTS.OPEN_ORDERS")?;
        let index = method(h.as_ref(), &index, "toString", &[])?;
        let config = h.literal(&[(
            "params",
            h.literal(&[
                ("account_index", index),
                ("market_id", market),
                ("auth", auth),
            ])?,
        )])?;
        let payload = request.call(&[endpoint, config])?;
        Ok(Step::Await(
            payload,
            Box::new(move |payload| Ok(Step::Done(read_orders(h, payload?)?))),
        ))
    })
}
fn after_orders(h: Rc<dyn OrderSnapshotHost>, orders: AccountValue) -> SigningResult<Step> {
    set(
        h.as_ref(),
        "hasWarnedInvalidOrderAuthAccount",
        AccountValue::Boolean(false),
    )?;
    let now = h.global("Date.now")?.call(&[])?;
    let entries = method(
        h.as_ref(),
        &get(h.as_ref(), "recentlyCancelledOrders")?,
        "entries",
        &[],
    )?;
    let mut entries = h.cancelled_entries(entries)?;
    while let Some((id, timestamp)) = entries.next_entry()? {
        let work = (|| {
            let elapsed = h.subtract(now.clone(), timestamp)?;
            if h.greater_than(elapsed, AccountValue::Number(30_000.0))? {
                let cancelled = get(h.as_ref(), "recentlyCancelledOrders")?;
                h.member(&cancelled, "delete")?.call(&[id])?;
            }
            Ok(())
        })();
        if let Err(error) = work {
            return Err(entries.close_on_throw(error));
        }
    }
    let filter = h.member(&orders, "filter")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let order = arg(args, 0);
        let string = live.global("String")?;
        let mut id = AccountValue::Undefined;
        for path in [
            &["orderId"][..],
            &["order_id"],
            &["orderIndex"],
            &["order_index"],
            &["OrderId"],
            &["OrderIndex"],
            &["Index"],
            &["index"],
            &["order", "orderId"],
            &["order", "order_id"],
            &["order", "orderIndex"],
            &["order", "order_index"],
            &["order", "OrderId"],
            &["order", "OrderIndex"],
        ] {
            id = optional_path(live.as_ref(), &order, path)?;
            if !id.is_nullish() {
                break;
            }
        }
        let id = string.call(&[if id.is_nullish() { text("") } else { id }])?;
        let cancelled = get(live.as_ref(), "recentlyCancelledOrders")?;
        Ok(AccountValue::Boolean(
            !live.member(&cancelled, "has")?.call(&[id])?.truthy(),
        ))
    }))?;
    let filtered = filter.call(&[callback])?;
    let flat_map = h.member(&filtered, "flatMap")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        call(live.as_ref(), "mapOrder", &[arg(args, 0)])
    }))?;
    Ok(Step::Done(flat_map.call(&[callback])?))
}
fn fallback_orders(
    h: Rc<dyn OrderSnapshotHost>,
    index: AccountValue,
    auth: AccountValue,
) -> SigningResult<Step> {
    let uniq = h.global("uniq")?;
    let store = get(h.as_ref(), "store")?;
    let markets = h.get(&store, "markets")?;
    let map = h.member(&markets, "map")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let parse = live.global("Number.parseInt")?;
        let string = live.global("String")?;
        let id = string.call(&[live.get(&arg(args, 0), "id")?])?;
        parse.call(&[id, AccountValue::Number(10.0)])
    }))?;
    let ids = map.call(&[callback])?;
    let filter = h.member(&ids, "filter")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let id = arg(args, 0);
        Ok(AccountValue::Boolean(
            live.global("Number.isFinite")?
                .call(&[id.clone()])?
                .truthy()
                && live.greater_or_equal(id, AccountValue::Number(0.0))?,
        ))
    }))?;
    let ids = uniq.call(&[filter.call(&[callback])?])?;
    let all = h.global("Promise.allSettled")?;
    let map = h.member(&ids, "map")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let inner =
            fetch_orders_for_market(live.clone(), index.clone(), auth.clone(), arg(args, 0));
        let value = live.promise_value(inner);
        let outer = start(live.clone(), || {
            Ok(Step::Await(value, Box::new(|value| Ok(Step::Done(value?)))))
        });
        Ok(live.promise_value(outer))
    }))?;
    let values = all.call(&[map.call(&[callback])?])?;
    Ok(Step::Await(
        values,
        Box::new(move |values| {
            let values = values?;
            let flat_map = h.member(&values, "flatMap")?;
            let live = h.clone();
            let callback=h.function(Rc::new(move|args|{
            let result=arg(args,0);
            if matches!(live.get(&result,"status")?,AccountValue::String(value) if value=="fulfilled".encode_utf16().collect::<Vec<_>>()){
                live.get(&result,"value")
            }else{live.array(&[])}
        }))?;
            after_orders(h, flat_map.call(&[callback])?)
        }),
    ))
}
fn primary_failure(
    h: Rc<dyn OrderSnapshotHost>,
    index: AccountValue,
    auth: AccountValue,
    error: TransactionSigningError,
) -> SigningResult<Step> {
    let original = error.clone();
    let error = h.error_value(error);
    let string = h.global("String")?;
    let message = optional_path(h.as_ref(), &error, &["message"])?;
    let message = string.call(&[if message.truthy() { message } else { text("") }])?;
    let lower = method(h.as_ref(), &message, "toLowerCase", &[])?;
    let fallback = if method(h.as_ref(), &lower, "includes", &[text("invalid param")])?.truthy() {
        true
    } else {
        let lower = method(h.as_ref(), &message, "toLowerCase", &[])?;
        method(h.as_ref(), &lower, "includes", &[text("market")])?.truthy()
    };
    if !fallback {
        if is_invalid_auth_account_error(h.clone(), error.clone())? {
            return Ok(Step::Done(handle_invalid_auth_account_error(
                h.as_ref(),
                error,
            )?));
        }
        return Err(original);
    }
    fallback_orders(h, index, auth)
}
fn fetch_authorized(
    h: Rc<dyn OrderSnapshotHost>,
    index: AccountValue,
    auth: AccountValue,
) -> SigningResult<Step> {
    let _initial_orders = h.array(&[])?;
    let value = h.promise_value(fetch_orders_for_market(
        h.clone(),
        index.clone(),
        auth.clone(),
        AccountValue::Number(255.0),
    ));
    Ok(Step::Await(
        value,
        Box::new(move |orders| match orders {
            Ok(orders) => after_orders(h, orders),
            Err(error) => primary_failure(h, index, auth, error),
        }),
    ))
}
// ts: LighterExchange#fetchOrdersOnce
pub fn fetch_orders_once(h: Rc<dyn OrderSnapshotHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let index = call(h.as_ref(), "ensureAccountIndex", &[])?;
        let catch = h.member(&index, "catch")?;
        let live = h.clone();
        let callback = h.function(Rc::new(move |args| {
            warning_index(live.as_ref(), arg(args, 0))
        }))?;
        let index = catch.call(&[callback])?;
        Ok(Step::Await(
            index,
            Box::new(move |index| {
                let index = index?;
                if matches!(index, AccountValue::Undefined) {
                    return Ok(Step::Done(store_orders(h.as_ref())?));
                }
                set(
                    h.as_ref(),
                    "hasWarnedMissingOrderAccountIndex",
                    AccountValue::Boolean(false),
                )?;
                let auth = call(
                    h.as_ref(),
                    "getPrivateWsAuthToken",
                    &[AccountValue::Number(60.0)],
                )?;
                Ok(Step::Await(
                    auth,
                    Box::new(move |auth| {
                        let auth = auth?;
                        if !auth.truthy() {
                            if !get(h.as_ref(), "hasWarnedMissingOrderAuth")?.truthy() {
                                set(
                                    h.as_ref(),
                                    "hasWarnedMissingOrderAuth",
                                    AccountValue::Boolean(true),
                                )?;
                                let log = h.member(&h.owner(), "log")?;
                                log.call(&[text("Lighter auth token unavailable for accountActiveOrders; private order sync is temporarily disabled"),h.constant("LogSeverity.Warning")?])?;
                            }
                            return Ok(Step::Done(store_orders(h.as_ref())?));
                        }
                        set(
                            h.as_ref(),
                            "hasWarnedMissingOrderAuth",
                            AccountValue::Boolean(false),
                        )?;
                        fetch_authorized(h, index, auth)
                    }),
                ))
            }),
        ))
    })
}
