//! Batched placement body. Required live realm operations retain source Get,
//! callee/receiver capture, numeric coercion, iterator close and queued awaits.
use crate::{
    exchange_trading_preflight::TradingHost,
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, PromiseResolver},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::rc::Rc;
#[derive(Clone, Copy)]
pub enum BatchBinary {
    Add,
    Multiply,
    Remainder,
    LessThan,
    GreaterThan,
}
pub type BatchVisitor = Rc<dyn Fn(AccountValue) -> SigningResult<()>>;
pub trait BatchPlacementHost: TradingHost {
    /// Actual JS operator including ordered ToPrimitive/ToNumeric, BigInt mixing
    /// errors and relational comparison. Comparison results are Boolean.
    fn binary(
        &self,
        op: BatchBinary,
        left: AccountValue,
        right: AccountValue,
    ) -> SigningResult<AccountValue>;
    /// Actual for-of, including iterator binding, close and completion precedence.
    /// The visitor executes the Rust loop body; no result/callback substitution.
    fn for_each(&self, value: AccountValue, visitor: BatchVisitor) -> SigningResult<()>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn bigint(s: &str) -> AccountValue {
    AccountValue::BigInt(s.parse().expect("internal decimal BigInt literal"))
}
fn get(h: &dyn BatchPlacementHost, key: &str) -> SigningResult<AccountValue> {
    h.get(&h.owner(), key)
}
fn call(
    h: &dyn BatchPlacementHost,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(&h.owner(), key)?.call(args)
}
fn method(
    h: &dyn BatchPlacementHost,
    value: &AccountValue,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(value, key)?.call(args)
}
fn arg(args: &[AccountValue], i: usize) -> AccountValue {
    args.get(i).cloned().unwrap_or(AccountValue::Undefined)
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
        (AccountValue::Function(a), AccountValue::Function(b)) => a.ptr_eq(b),
        (AccountValue::Symbol(a), AccountValue::Symbol(b)) => a.ptr_eq(b),
        _ => false,
    }
}
fn optional(
    h: &dyn BatchPlacementHost,
    v: &AccountValue,
    key: &str,
) -> SigningResult<AccountValue> {
    if v.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.get(v, key)
    }
}
type Continuation = Box<dyn FnOnce(SigningResult<AccountValue>) -> SigningResult<Step>>;
enum Step {
    Done(AccountValue),
    Await(AccountValue, Continuation),
}
fn drive(
    h: Rc<dyn BatchPlacementHost>,
    resolver: PromiseResolver<AccountValue>,
    step: SigningResult<Step>,
) {
    match step {
        Err(e) => resolver.settle(Err(e)),
        Ok(Step::Done(v)) => h.resolve_return(v, resolver),
        Ok(Step::Await(value, next)) => match h.await_value(value) {
            Err(e) => drive(h, resolver, next(Err(e))),
            Ok(p) => p.subscribe(move |v| drive(h, resolver, next(v.map(|v| (*v).clone())))),
        },
    }
}
fn start(
    h: Rc<dyn BatchPlacementHost>,
    work: impl FnOnce() -> SigningResult<Step>,
) -> NativePromise<AccountValue> {
    let (p, r) = NativePromise::pending(h.runtime());
    let step = work();
    drive(h, r, step);
    p
}
fn wait(
    value: SigningResult<AccountValue>,
    next: impl FnOnce(SigningResult<AccountValue>) -> SigningResult<Step> + 'static,
) -> SigningResult<Step> {
    match value {
        Ok(v) => Ok(Step::Await(v, Box::new(next))),
        Err(e) => next(Err(e)),
    }
}
fn sequential(h: Rc<dyn BatchPlacementHost>, orders: AccountValue) -> SigningResult<Step> {
    let value = call(h.as_ref(), "placeOrdersSequentially", &[orders])?;
    Ok(Step::Await(value, Box::new(|v| Ok(Step::Done(v?)))))
}
struct Batch {
    orders: AccountValue,
    max_size: AccountValue,
    p_price: AccountValue,
    p_amount: AccountValue,
    amount_decimals: AccountValue,
    price_decimals: AccountValue,
    min_base: AccountValue,
    min_quote: AccountValue,
    market_id: AccountValue,
    config: AccountValue,
}
fn prepare(h: Rc<dyn BatchPlacementHost>, b: Rc<Batch>) -> SigningResult<Step> {
    let max = bigint("281474976710655");
    let integer = h.global("BigInt")?;
    let now = h.global("Date.now")?.call(&[])?;
    let left = h.binary(BatchBinary::Multiply, integer.call(&[now])?, bigint("1000"))?;
    let integer = h.global("BigInt")?;
    let floor = h.global("Math.floor")?;
    let random = h.global("Math.random")?.call(&[])?;
    let random = h.binary(BatchBinary::Multiply, random, AccountValue::Number(1000.0))?;
    let right = integer.call(&[floor.call(&[random])?])?;
    let base = h.binary(BatchBinary::Add, left, right)?;
    let base = h.binary(BatchBinary::Remainder, base, max.clone())?;
    let prepared = h.array(&[])?;
    let mut index = 0.0;
    loop {
        let length = h.get(&b.orders, "length")?;
        if !h
            .binary(BatchBinary::LessThan, AccountValue::Number(index), length)?
            .truthy()
        {
            break;
        }
        let order = h.index(&b.orders, AccountValue::Number(index))?;
        let adjust = h.global("adjust")?;
        let amount = adjust.call(&[h.get(&order, "amount")?, b.p_amount.clone()])?;
        let adjust = h.global("adjust")?;
        let price = adjust.call(&[h.get(&order, "price")?, b.p_price.clone()])?;
        if h.binary(BatchBinary::GreaterThan, amount.clone(), b.max_size.clone())?
            .truthy()
        {
            return sequential(h, b.orders.clone());
        }
        let reasons = call(
            h.as_ref(),
            "getOrderMinimumViolationReasons",
            &[
                amount.clone(),
                price.clone(),
                b.min_base.clone(),
                b.min_quote.clone(),
            ],
        )?;
        if reasons.truthy() {
            let constructor = h.global("Error")?;
            let mut message = "Lighter order amount is below exchange minimums for "
                .encode_utf16()
                .collect::<Vec<_>>();
            message.extend(h.template_string(h.get(&order, "symbol")?)?);
            message.extend(" (".encode_utf16());
            message.extend(h.template_string(reasons)?);
            message.extend("). Increase order size or place closer to mark price.".encode_utf16());
            return Err(h.throw_value(constructor.construct(&[AccountValue::String(message)])?));
        }
        let integer = h.global("BigInt")?;
        let candidate = h.binary(
            BatchBinary::Add,
            base.clone(),
            integer.call(&[AccountValue::Number(index)])?,
        )?;
        let candidate = h.binary(BatchBinary::Add, candidate, bigint("1"))?;
        let candidate = h.binary(BatchBinary::Remainder, candidate, max.clone())?;
        let order_id = if eq(&candidate, &bigint("0")) {
            h.global("BigInt")?
                .call(&[AccountValue::Number(index + 1.0)])?
        } else {
            candidate
        };
        let tif = h.get(&order, "timeInForce")?;
        let push = h.member(&prepared, "push")?;
        let hint = h.literal(&[
            ("type", h.get(&order, "type")?),
            ("side", h.get(&order, "side")?),
            ("reduceOnly", AccountValue::Boolean(false)),
            ("timeInForce", tif.clone()),
            ("price", price.clone()),
            ("symbol", h.get(&order, "symbol")?),
        ])?;
        let mut fields = vec![
            ("orderId", order_id.clone()),
            ("marketId", b.market_id.clone()),
            (
                "isBuy",
                AccountValue::Boolean(eq(&h.get(&order, "side")?, &h.constant("OrderSide.Buy")?)),
            ),
            (
                "price",
                h.global("toLighterScaledInt")?
                    .call(&[price, b.price_decimals.clone()])?,
            ),
            (
                "amount",
                h.global("toLighterScaledInt")?
                    .call(&[amount, b.amount_decimals.clone()])?,
            ),
            (
                "postOnly",
                AccountValue::Boolean(eq(
                    &h.get(&order, "timeInForce")?,
                    &h.constant("OrderTimeInForce.PostOnly")?,
                )),
            ),
            ("reduceOnly", AccountValue::Boolean(false)),
            ("orderType", text("limit")),
            ("timeInForce", tif),
        ];
        if b.config.truthy() {
            for key in [
                "integratorAccountIndex",
                "integratorMakerFee",
                "integratorTakerFee",
            ] {
                fields.push((key, h.get(&b.config, key)?));
            }
        }
        let request = h.literal(&fields)?;
        let row = h.literal(&[
            ("orderId", order_id),
            ("sourceOrder", order),
            ("hint", hint),
            ("request", request),
        ])?;
        push.call(&[row])?;
        index += 1.0;
    }
    let signer = call(h.as_ref(), "ensureSignerAdapter", &[])?;
    Ok(Step::Await(
        signer,
        Box::new(move |signer| {
            let signer = signer?;
            let live = h.clone();
            h.for_each(
                prepared.clone(),
                Rc::new(move |row| {
                    let remember = live.member(&live.owner(), "rememberPlacedOrderHint")?;
                    let id = live.get(&row, "orderId")?;
                    let hint = live.get(&row, "hint")?;
                    remember.call(&[id, hint])?;
                    Ok(())
                }),
            )?;
            submit(h, b, prepared, signer)
        }),
    ))
}
fn submit(
    h: Rc<dyn BatchPlacementHost>,
    b: Rc<Batch>,
    prepared: AccountValue,
    signer: AccountValue,
) -> SigningResult<Step> {
    let value = (|| {
        let create = h.member(&signer, "createOrders")?;
        let map = h.member(&prepared, "map")?;
        let live = h.clone();
        let callback = h.function(Rc::new(move |args| live.get(&arg(args, 0), "request")))?;
        create.call(&[map.call(&[callback])?])
    })();
    wait(value, move |result| {
        let after = (|| {
            result?;
            let ws = get(h.as_ref(), "privateWebsocket")?;
            method(h.as_ref(), &ws, "throttleFetchOrders", &[])?;
            let map = h.member(&prepared, "map")?;
            let live = h.clone();
            let callback = h.function(Rc::new(move |args| {
                let id = live.get(&arg(args, 0), "orderId")?;
                method(live.as_ref(), &id, "toString", &[])
            }))?;
            map.call(&[callback])
        })();
        match after {
            Ok(value) => Ok(Step::Done(value)),
            Err(error) => batch_error(h, b, prepared, error),
        }
    })
}
fn batch_error(
    h: Rc<dyn BatchPlacementHost>,
    b: Rc<Batch>,
    prepared: AccountValue,
    error: TransactionSigningError,
) -> SigningResult<Step> {
    let live = h.clone();
    h.for_each(
        prepared.clone(),
        Rc::new(move |row| {
            let hints = get(live.as_ref(), "placedOrderHints")?;
            let delete = live.member(&hints, "delete")?;
            let id = live.get(&row, "orderId")?;
            let id = method(live.as_ref(), &id, "toString", &[])?;
            delete.call(&[id])?;
            Ok(())
        }),
    )?;
    let first = h.get(&prepared, "0")?;
    let first = optional(h.as_ref(), &first, "sourceOrder")?;
    let first = if first.is_nullish() {
        h.get(&b.orders, "0")?
    } else {
        first
    };
    let error = h.error_value(error);
    let raw = call(h.as_ref(), "extractOrderErrorMessage", &[error.clone()])?;
    let invalid = call(
        h.as_ref(),
        "isInvalidMarginModeOrderError",
        &[error.clone(), raw],
    )?;
    if invalid.truthy() {
        let refresh = h.member(&h.owner(), "refreshMarginModeStateAfterInvalidOrder")?;
        let refreshed = refresh.call(&[h.get(&first, "symbol")?])?;
        Ok(Step::Await(
            refreshed,
            Box::new(move |value| {
                value?;
                finish_error(h, first, error)
            }),
        ))
    } else {
        finish_error(h, first, error)
    }
}
fn finish_error(
    h: Rc<dyn BatchPlacementHost>,
    first: AccountValue,
    error: AccountValue,
) -> SigningResult<Step> {
    let normalized = call(
        h.as_ref(),
        "normalizePlaceOrderErrorMessage",
        &[first, error],
    )?;
    let mut message = "Lighter batch order placement failed: "
        .encode_utf16()
        .collect::<Vec<_>>();
    message.extend(h.template_string(normalized)?);
    message.extend(". Please check open orders before retrying.".encode_utf16());
    let message = AccountValue::String(message);
    let ws = get(h.as_ref(), "privateWebsocket")?;
    method(h.as_ref(), &ws, "throttleFetchOrders", &[])?;
    call(h.as_ref(), "emitAndLogError", &[message.clone()])?;
    let error = h.global("Error")?.construct(&[message])?;
    Err(h.throw_value(error))
}
// ts: LighterExchange.placeOrders
pub fn place_orders(
    h: Rc<dyn BatchPlacementHost>,
    orders: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        call(h.as_ref(), "assertCallerClientOrderIds", &[orders.clone()])?;
        if !call(
            h.as_ref(),
            "shouldUseBatchedCreateOrders",
            &[orders.clone()],
        )?
        .truthy()
        {
            return sequential(h, orders);
        }
        let store = get(h.as_ref(), "store")?;
        let markets = h.get(&store, "marketsBySymbol")?;
        let first = h.get(&orders, "0")?;
        let market = h.index(&markets, h.get(&first, "symbol")?)?;
        if !market.truthy() {
            return sequential(h, orders);
        }
        let limits = h.get(&market, "limits")?;
        let amount = h.get(&limits, "amount")?;
        let max_size = h.get(&amount, "max")?;
        let precision = h.get(&market, "precision")?;
        let p_price = h.get(&precision, "price")?;
        let precision = h.get(&market, "precision")?;
        let p_amount = h.get(&precision, "amount")?;
        let amount_decimals = h
            .global("getLighterAmountDecimals")?
            .call(&[market.clone()])?;
        let price_decimals = h
            .global("getLighterPriceDecimals")?
            .call(&[market.clone()])?;
        let minimums = h
            .global("getLighterOrderMinimums")?
            .call(&[market.clone()])?;
        let min_base = h.get(&minimums, "minBaseAmount")?;
        let min_quote = h.get(&minimums, "minQuoteAmount")?;
        let parse = h.global("parseInt")?;
        let market_id = parse.call(&[h.get(&market, "id")?, AccountValue::Number(10.0)])?;
        if !h
            .global("Number.isFinite")?
            .call(&[market_id.clone()])?
            .truthy()
        {
            return sequential(h, orders);
        }
        let builder = h.global("resolveLighterBuilderFeeConfig")?;
        let config = builder.call(&[get(h.as_ref(), "options")?, get(h.as_ref(), "name")?])?;
        let b = Rc::new(Batch {
            orders,
            max_size,
            p_price,
            p_amount,
            amount_decimals,
            price_decimals,
            min_base,
            min_quote,
            market_id,
            config,
        });
        let tier = call(
            h.as_ref(),
            "ensurePartnerAttributionTierEligible",
            &[b.config.clone()],
        )?;
        Ok(Step::Await(
            tier,
            Box::new(move |value| {
                value?;
                let approved = call(
                    h.as_ref(),
                    "ensurePartnerAttributionApproved",
                    &[b.config.clone()],
                )?;
                Ok(Step::Await(
                    approved,
                    Box::new(move |value| {
                        value?;
                        prepare(h, b)
                    }),
                ))
            }),
        ))
    })
}
