//! Order error/update/cancel bodies with required live language/import bindings.
use crate::{
    exchange_account_access::AccountAccessCall,
    exchange_trading_preflight::TradingHost,
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, PromiseResolver},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::{cell::RefCell, rc::Rc};
pub trait OrderActionIterator {
    /// IteratorStepValue, retaining done state on abrupt next/done/value access.
    fn next(&mut self) -> SigningResult<Option<AccountValue>>;
    /// IteratorClose with Throw completion; retain the original thrown handle.
    fn close_throw(&mut self, error: TransactionSigningError) -> TransactionSigningError;
}
pub trait OrderActionHost: TradingHost {
    /// Parameter object binding: RequireObjectCoercible and ordered Gets/diagnostics.
    fn object_binding_pair(
        &self,
        value: AccountValue,
        keys: (&str, &str),
    ) -> SigningResult<(AccountValue, AccountValue)>;
    fn promise_value(&self, value: NativePromise<AccountValue>) -> AccountValue;
    /// Full HasProperty, including inherited properties and Proxy throws.
    fn has(&self, value: &AccountValue, key: &str) -> SigningResult<bool>;
    /// Fresh CopyDataProperties from the source, including UTF16/Symbol keys.
    fn spread(&self, source: &AccountValue) -> SigningResult<AccountValue>;
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()>;
    fn iterator(&self, value: AccountValue) -> SigningResult<Box<dyn OrderActionIterator>>;
    /// ArgumentListEvaluation of spread then Call of the already captured callee.
    fn call_spread(
        &self,
        callee: Rc<dyn AccountAccessCall>,
        value: AccountValue,
    ) -> SigningResult<AccountValue>;
    fn subtract(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue>;
    fn divide(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue>;
    fn multiply(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue>;
    fn greater(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool>;
    fn less_equal(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool>;
    fn greater_equal(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn arg(args: &[AccountValue], i: usize) -> AccountValue {
    args.get(i).cloned().unwrap_or(AccountValue::Undefined)
}
fn get(h: &dyn OrderActionHost, key: &str) -> SigningResult<AccountValue> {
    h.get(&h.owner(), key)
}
fn call(h: &dyn OrderActionHost, key: &str, args: &[AccountValue]) -> SigningResult<AccountValue> {
    h.member(&h.owner(), key)?.call(args)
}
fn method(
    h: &dyn OrderActionHost,
    value: &AccountValue,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(value, key)?.call(args)
}
fn optional(
    h: &dyn OrderActionHost,
    value: &AccountValue,
    key: &str,
) -> SigningResult<AccountValue> {
    if value.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.get(value, key)
    }
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
fn push(h: &dyn OrderActionHost, buf: &mut Vec<u16>, value: AccountValue) -> SigningResult<()> {
    buf.extend(h.template_string(value)?);
    Ok(())
}
fn suffix(buf: &mut Vec<u16>, s: &str) {
    buf.extend(s.encode_utf16());
}
type Next = Box<dyn FnOnce(SigningResult<AccountValue>) -> SigningResult<Step>>;
enum Step {
    Done(AccountValue),
    Await(AccountValue, Next),
}
fn drive(h: Rc<dyn OrderActionHost>, r: PromiseResolver<AccountValue>, step: SigningResult<Step>) {
    match step {
        Err(e) => r.settle(Err(e)),
        Ok(Step::Done(v)) => h.resolve_return(v, r),
        Ok(Step::Await(v, next)) => match h.await_value(v) {
            Err(e) => drive(h, r, next(Err(e))),
            Ok(p) => p.subscribe(move |v| drive(h, r, next(v.map(|v| (*v).clone())))),
        },
    }
}
fn start(
    h: Rc<dyn OrderActionHost>,
    work: impl FnOnce() -> SigningResult<Step>,
) -> NativePromise<AccountValue> {
    let (p, r) = NativePromise::pending(h.runtime());
    drive(h, r, work());
    p
}
fn included(h: &dyn OrderActionHost, value: &AccountValue, needle: &str) -> SigningResult<bool> {
    Ok(method(h, value, "includes", &[text(needle)])?.truthy())
}
// ts: LighterExchange#normalizePlaceOrderErrorMessage
pub fn normalize_place_order_error_message(
    h: &dyn OrderActionHost,
    opts: AccountValue,
    error: AccountValue,
) -> SigningResult<AccountValue> {
    let raw = call(h, "extractOrderErrorMessage", &[error.clone()])?;
    let normalized = method(h, &raw, "toLowerCase", &[])?;
    if call(h, "isRateLimitOrderError", &[error.clone(), raw.clone()])?.truthy() {
        return call(h, "getAccountRateLimitGuidanceMessage", &[]);
    }
    if call(
        h,
        "isPartnerAttributionTierOrderError",
        &[error.clone(), raw.clone()],
    )?
    .truthy()
    {
        return h.constant("LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE");
    }
    if call(
        h,
        "isPartnerAttributionApprovalMissingOrderError",
        &[error.clone(), raw.clone()],
    )?
    .truthy()
    {
        return h.constant("LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE");
    }
    if call(h, "isInvalidMarginModeOrderError", &[error, raw.clone()])?.truthy() {
        let f = h.member(&h.owner(), "buildInvalidMarginModeOrderMessage")?;
        return f.call(&[h.get(&opts, "symbol")?]);
    }
    let accidental = included(h, &normalized, "accidental price")?
        || (included(h, &normalized, "accidental")? && included(h, &normalized, "price")?);
    if accidental
        && eq(&h.get(&opts, "type")?, &h.constant("OrderType.Limit")?)
        && matches!(h.get(&opts, "price")?, AccountValue::Number(_))
        && h.greater(h.get(&opts, "price")?, AccountValue::Number(0.0))?
    {
        let store = get(h, "store")?;
        let tickers = h.get(&store, "tickersBySymbol")?;
        let ticker = h.index(&tickers, h.get(&opts, "symbol")?)?;
        let mark = optional(h, &ticker, "mark")?;
        let mark = if mark.truthy() {
            mark
        } else {
            let last = optional(h, &ticker, "last")?;
            if last.truthy() {
                last
            } else {
                optional(h, &ticker, "index")?
            }
        };
        let has_mark = matches!(mark, AccountValue::Number(_))
            && h.global("Number.isFinite")?.call(&[mark.clone()])?.truthy()
            && h.greater(mark.clone(), AccountValue::Number(0.0))?;
        let deviation = if has_mark {
            let abs = h.global("Math.abs")?;
            let difference = h.subtract(h.get(&opts, "price")?, mark.clone())?;
            let ratio = h.divide(difference, mark.clone())?;
            let magnitude = abs.call(&[ratio])?;
            h.multiply(magnitude, AccountValue::Number(100.0))?
        } else {
            AccountValue::Undefined
        };
        let rounded = if !matches!(deviation, AccountValue::Undefined)
            && h.global("Number.isFinite")?
                .call(&[deviation.clone()])?
                .truthy()
        {
            let number = h.global("Number")?;
            let fixed = method(h, &deviation, "toFixed", &[AccountValue::Number(2.0)])?;
            number.call(&[fixed])?
        } else {
            AccountValue::Undefined
        };
        let mut context = vec![];
        if has_mark {
            suffix(&mut context, " Current mark is ~");
            push(h, &mut context, mark)?;
            if !matches!(rounded, AccountValue::Undefined) {
                suffix(&mut context, " (deviation ");
                push(h, &mut context, rounded)?;
                suffix(&mut context, "%).");
            } else {
                suffix(&mut context, ".");
            }
        }
        let mut message = vec![];
        push(h, &mut message, raw)?;
        suffix(&mut message, ". Submitted ");
        push(h, &mut message, h.get(&opts, "side")?)?;
        suffix(&mut message, " limit price ");
        push(h, &mut message, h.get(&opts, "price")?)?;
        suffix(&mut message, " on ");
        push(h, &mut message, h.get(&opts, "symbol")?)?;
        suffix(&mut message, ".");
        message.extend(context);
        suffix(
            &mut message,
            " Verify decimal placement and keep scaled bounds near market.",
        );
        return Ok(AccountValue::String(message));
    }
    let reduce = eq(&h.get(&opts, "type")?, &h.constant("OrderType.Limit")?)
        && h.global("Boolean")?
            .call(&[h.get(&opts, "reduceOnly")?])?
            .truthy()
        && (included(h, &normalized, "reduce only")?
            || included(h, &normalized, "reduce-only")?
            || included(h, &normalized, "nothing to reduce")?
            || included(h, &normalized, "position does not exist")?
            || included(h, &normalized, "no position")?
            || (included(h, &normalized, "position")? && included(h, &normalized, "close")?));
    if !reduce {
        return Ok(raw);
    }
    let f = h.member(&h.owner(), "buildNoPositionReduceOnlyMessage")?;
    f.call(&[h.get(&opts, "symbol")?, h.get(&opts, "side")?])
}
type IteratorRef = Rc<RefCell<Box<dyn OrderActionIterator>>>;
fn close(it: &IteratorRef, e: TransactionSigningError) -> SigningResult<Step> {
    Err(it.borrow_mut().close_throw(e))
}
fn sequence_next(
    h: Rc<dyn OrderActionHost>,
    it: IteratorRef,
    ids: AccountValue,
) -> SigningResult<Step> {
    let order = it.borrow_mut().next()?;
    let Some(order) = order else {
        return Ok(Step::Done(ids));
    };
    let push = match h.member(&ids, "push") {
        Ok(v) => v,
        Err(e) => return close(&it, e),
    };
    let placed = match call(h.as_ref(), "placeOrder", &[order]) {
        Ok(v) => v,
        Err(e) => return close(&it, e),
    };
    Ok(Step::Await(
        placed,
        Box::new(move |result| {
            let values = match result {
                Ok(v) => v,
                Err(e) => return close(&it, e),
            };
            if let Err(e) = h.call_spread(push, values) {
                return close(&it, e);
            }
            sequence_next(h, it, ids)
        }),
    ))
}
// ts: LighterExchange#placeOrdersSequentially
pub fn place_orders_sequentially(
    h: Rc<dyn OrderActionHost>,
    orders: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let ids = h.array(&[])?;
        let it = Rc::new(RefCell::new(h.iterator(orders)?));
        sequence_next(h, it, ids)
    })
}
// ts: LighterExchange.updateOrder
pub fn update_order(h: Rc<dyn OrderActionHost>, opts: AccountValue) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let (order, update) = h.object_binding_pair(opts, ("order", "update"))?;
        let infer = h.member(&h.owner(), "inferStopLossTypeFromReduceOnlyLimit")?;
        let inferred_opts = h.spread(&order)?;
        let price = if h.has(&update, "price")? {
            h.get(&update, "price")?
        } else {
            h.get(&order, "price")?
        };
        h.define_data(&inferred_opts, "price", price)?;
        let inferred = infer.call(&[inferred_opts])?;
        let symbol = h.get(&order, "symbol")?;
        let kind = if inferred.is_nullish() {
            h.get(&order, "type")?
        } else {
            inferred.clone()
        };
        let side = h.get(&order, "side")?;
        let price = h.get(&order, "price")?;
        let amount = h.get(&order, "amount")?;
        let reduce = h.get(&order, "reduceOnly")?;
        let new = h.literal(&[
            ("symbol", symbol),
            ("type", kind),
            ("side", side),
            ("price", price),
            ("amount", amount),
            (
                "reduceOnly",
                if reduce.truthy() {
                    reduce
                } else {
                    AccountValue::Boolean(false)
                },
            ),
        ])?;
        let slippage = h.get(&order, "maxSlippagePct")?;
        if !matches!(slippage, AccountValue::Undefined) {
            h.set(&new, "maxSlippagePct", slippage)?;
        }
        for key in ["tpTriggerType", "slTriggerType"] {
            if h.has(&order, key)? {
                h.set(&new, key, h.get(&order, key)?)?;
            }
        }
        for key in ["price", "amount"] {
            if h.has(&update, key)? {
                h.set(&new, key, h.get(&update, key)?)?;
            }
        }
        if inferred.truthy() && h.has(&update, "price")? {
            let store = get(h.as_ref(), "store")?;
            let tickers = h.get(&store, "tickersBySymbol")?;
            let ticker = h.index(&tickers, h.get(&order, "symbol")?)?;
            let positive = h.member(&h.owner(), "getPositivePriceCandidate")?;
            let reference = positive.call(&[
                optional(h.as_ref(), &ticker, "mark")?,
                optional(h.as_ref(), &ticker, "last")?,
                optional(h.as_ref(), &ticker, "index")?,
            ])?;
            if reference.truthy() {
                let sell = eq(&h.get(&order, "side")?, &h.constant("OrderSide.Sell")?);
                let sl = if sell {
                    h.less_equal(h.get(&update, "price")?, reference.clone())?
                } else {
                    h.greater_equal(h.get(&update, "price")?, reference.clone())?
                };
                let tp = if sell {
                    h.greater_equal(h.get(&update, "price")?, reference)?
                } else {
                    h.less_equal(h.get(&update, "price")?, reference)?
                };
                let valid = if eq(&inferred, &h.constant("OrderType.StopLoss")?) {
                    sl
                } else if eq(&inferred, &h.constant("OrderType.TakeProfit")?) {
                    tp
                } else {
                    true
                };
                if !valid {
                    let e = h.global("Error")?.construct(&[text(
                        "Invalid Lighter protective trigger price for current market",
                    )])?;
                    return Err(h.throw_value(e));
                }
            }
        }
        let cancel = h.member(&h.owner(), "cancelOrders")?;
        let cancelled = cancel.call(&[h.array(&[order])?])?;
        Ok(Step::Await(
            cancelled,
            Box::new(move |result| {
                result?;
                let placed = call(h.as_ref(), "placeOrder", &[new])?;
                Ok(Step::Await(
                    placed,
                    Box::new(|result| Ok(Step::Done(result?))),
                ))
            }),
        ))
    })
}
fn cancel_one(
    h: Rc<dyn OrderActionHost>,
    targets: AccountValue,
    signer: AccountValue,
    order_id: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let find = h.member(&targets, "find")?;
        let callback_h = h.clone();
        let candidate_id = order_id.clone();
        let predicate = h.function(Rc::new(move |args| {
            Ok(AccountValue::Boolean(eq(
                &callback_h.get(&arg(args, 0), "orderId")?,
                &candidate_id,
            )))
        }))?;
        let target = find.call(&[predicate])?;
        if !target.truthy() {
            let warn = h.global("console.warn")?;
            let mut message = "[Lighter] Cancel target not found for order "
                .encode_utf16()
                .collect::<Vec<_>>();
            push(h.as_ref(), &mut message, order_id)?;
            warn.call(&[AccountValue::String(message)])?;
            return Ok(Step::Done(AccountValue::Undefined));
        }
        let cancel = h.member(&signer, "cancelOrder")?;
        let id = h.global("BigInt")?.call(&[order_id])?;
        let market = h.get(&target, "marketId")?;
        let request = h.literal(&[("orderId", id), ("marketId", market)])?;
        let sent = cancel.call(&[request])?;
        Ok(Step::Await(
            sent,
            Box::new(|result| {
                result?;
                Ok(Step::Done(AccountValue::Undefined))
            }),
        ))
    })
}
fn cancel_lot(
    h: Rc<dyn OrderActionHost>,
    targets: AccountValue,
    signer: AccountValue,
    ids: AccountValue,
) -> SigningResult<Step> {
    let series = h.global("forEachSeries")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let promise = cancel_one(live.clone(), targets.clone(), signer.clone(), arg(args, 0));
        Ok(live.promise_value(promise))
    }))?;
    let sent = series.call(&[ids, callback])?;
    Ok(Step::Await(
        sent,
        Box::new(|result| {
            result?;
            Ok(Step::Done(AccountValue::Undefined))
        }),
    ))
}
// ts: LighterExchange.cancelOrders
pub fn cancel_orders(
    h: Rc<dyn OrderActionHost>,
    orders: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let find = h.member(&orders, "find")?;
        let live = h.clone();
        let callback = h.function(Rc::new(move |args| {
            let parse = live.global("parseLighterPositiveIntegerString")?;
            Ok(AccountValue::Boolean(
                !parse.call(&[live.get(&arg(args, 0), "id")?])?.truthy(),
            ))
        }))?;
        let invalid = find.call(&[callback])?;
        if invalid.truthy() {
            let error = h.global("Error")?;
            let mut message = "Cannot cancel Lighter order "
                .encode_utf16()
                .collect::<Vec<_>>();
            push(h.as_ref(), &mut message, h.get(&invalid, "id")?)?;
            suffix(
                &mut message,
                ": expected a numeric client order id. Please wait for open-order sync before moving or cancelling this order.",
            );
            return Err(h.throw_value(error.construct(&[AccountValue::String(message)])?));
        }
        let signer = call(h.as_ref(), "ensureSignerAdapter", &[])?;
        Ok(Step::Await(
            signer,
            Box::new(move |result| {
                let signer = result?;
                let resolve = h.global("resolveLighterCancelTargets")?;
                let store = get(h.as_ref(), "store")?;
                let targets = resolve.call(&[orders.clone(), h.get(&store, "marketsBySymbol")?])?;
                let map = h.member(&targets, "map")?;
                let live = h.clone();
                let cb = h.function(Rc::new(move |args| live.get(&arg(args, 0), "orderId")))?;
                let ids = map.call(&[cb])?;
                let map = h.member(&orders, "map")?;
                let live = h.clone();
                let cb = h.function(Rc::new(move |args| live.get(&arg(args, 0), "id")))?;
                let all_ids = map.call(&[cb])?;
                let now = h.global("Date.now")?.call(&[])?;
                let each = h.member(&ids, "forEach")?;
                let live = h.clone();
                let cb = h.function(Rc::new(move |args| {
                    let map = get(live.as_ref(), "recentlyCancelledOrders")?;
                    let set = live.member(&map, "set")?;
                    set.call(&[arg(args, 0), now.clone()])?;
                    Ok(AccountValue::Undefined)
                }))?;
                each.call(&[cb])?;
                let store = get(h.as_ref(), "store")?;
                let remove = h.member(&store, "removeOrders")?;
                let map = h.member(&all_ids, "map")?;
                let live = h.clone();
                let cb = h.function(Rc::new(move |args| live.literal(&[("id", arg(args, 0))])))?;
                remove.call(&[map.call(&[cb])?])?;
                let lots = h
                    .global("chunk")?
                    .call(&[ids, AccountValue::Number(10.0)])?;
                let series = h.global("forEachSeries")?;
                let live = h.clone();
                let cb = h.function(Rc::new(move |args| {
                    let ids = arg(args, 0);
                    let index = arg(args, 1);
                    let h = live.clone();
                    let targets = targets.clone();
                    let signer = signer.clone();
                    let promise = start(h.clone(), || {
                        if h.greater(index, AccountValue::Number(0.0))? {
                            let sleeping =
                                h.global("sleep")?.call(&[AccountValue::Number(1000.0)])?;
                            Ok(Step::Await(
                                sleeping,
                                Box::new(move |result| {
                                    result?;
                                    cancel_lot(h, targets, signer, ids)
                                }),
                            ))
                        } else {
                            cancel_lot(h, targets, signer, ids)
                        }
                    });
                    Ok(live.promise_value(promise))
                }))?;
                let sent = series.call(&[lots, cb])?;
                Ok(Step::Await(
                    sent,
                    Box::new(move |result| {
                        result?;
                        let socket = get(h.as_ref(), "privateWebsocket")?;
                        h.member(&socket, "throttleFetchOrders")?.call(&[])?;
                        Ok(Step::Done(AccountValue::Undefined))
                    }),
                ))
            }),
        ))
    })
}
