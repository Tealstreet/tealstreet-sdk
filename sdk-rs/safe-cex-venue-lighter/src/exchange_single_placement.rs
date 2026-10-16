//! Single-order preparation, lot submission and protective sibling composition.
//! Business branches execute in Rust; required live realm/import operations retain
//! source coercion, callable/receiver capture, object spread and iterator close.
use crate::{
    exchange_account_access::AccountAccessCall,
    exchange_trading_preflight::TradingHost,
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, PromiseResolver},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy)]
pub enum PlacementBinary {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    LessThan,
    LessOrEqual,
    GreaterThan,
}
pub type PlacementVisitor = Rc<dyn Fn(AccountValue) -> SigningResult<()>>;
pub trait PlacementIterator {
    /// Full IteratorStepValue using the captured next method. Abrupt step/done/
    /// value completion is propagated without a spurious body IteratorClose.
    fn next(&mut self) -> SigningResult<Option<AccountValue>>;
    /// Full synchronous IteratorClose with Throw completion precedence.
    fn close_throw(&mut self, error: TransactionSigningError) -> TransactionSigningError;
}
pub trait SinglePlacementHost: TradingHost {
    fn binary(
        &self,
        op: PlacementBinary,
        left: AccountValue,
        right: AccountValue,
    ) -> SigningResult<AccountValue>;
    /// Fresh ordinary CopyDataProperties, including getters and symbol keys.
    fn spread_object(&self, source: &AccountValue) -> SigningResult<AccountValue>;
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()>;
    /// Strict PutValue; ToPropertyKey happens here, after the RHS is evaluated.
    fn set_index(
        &self,
        object: &AccountValue,
        key: AccountValue,
        value: AccountValue,
    ) -> SigningResult<()>;
    fn for_each(&self, value: AccountValue, visitor: PlacementVisitor) -> SigningResult<()>;
    fn iterator(&self, value: AccountValue) -> SigningResult<Box<dyn PlacementIterator>>;
    /// Spread ArgumentListEvaluation followed by Call of an already captured callee.
    fn call_spread(
        &self,
        callee: Rc<dyn AccountAccessCall>,
        value: AccountValue,
    ) -> SigningResult<AccountValue>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn bigint(s: &str) -> AccountValue {
    AccountValue::BigInt(s.parse().expect("internal decimal BigInt literal"))
}
fn arg(args: &[AccountValue], index: usize) -> AccountValue {
    args.get(index).cloned().unwrap_or(AccountValue::Undefined)
}
fn get(h: &dyn SinglePlacementHost, key: &str) -> SigningResult<AccountValue> {
    h.get(&h.owner(), key)
}
fn call(
    h: &dyn SinglePlacementHost,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(&h.owner(), key)?.call(args)
}
fn method(
    h: &dyn SinglePlacementHost,
    value: &AccountValue,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(value, key)?.call(args)
}
fn eq(a: &AccountValue, b: &AccountValue) -> bool {
    match (a, b) {
        (AccountValue::Undefined, AccountValue::Undefined)
        | (AccountValue::Null, AccountValue::Null) => true,
        (AccountValue::Boolean(a), AccountValue::Boolean(b)) => a == b,
        (AccountValue::Number(a), AccountValue::Number(b)) => a == b,
        (AccountValue::BigInt(a), AccountValue::BigInt(b)) => a == b,
        (AccountValue::String(a), AccountValue::String(b)) => a == b,
        (AccountValue::Object(a), AccountValue::Object(b)) => Rc::ptr_eq(a, b),
        (AccountValue::Function(a), AccountValue::Function(b))
        | (AccountValue::Symbol(a), AccountValue::Symbol(b)) => a.ptr_eq(b),
        _ => false,
    }
}
fn append(
    h: &dyn SinglePlacementHost,
    buf: &mut Vec<u16>,
    value: AccountValue,
) -> SigningResult<()> {
    buf.extend(h.template_string(value)?);
    Ok(())
}
fn suffix(buf: &mut Vec<u16>, s: &str) {
    buf.extend(s.encode_utf16())
}
fn fail(
    h: &dyn SinglePlacementHost,
    prefix: &str,
    opts: &AccountValue,
    tail: &str,
) -> SigningResult<Step> {
    let constructor = h.global("Error")?;
    let mut message = prefix.encode_utf16().collect::<Vec<_>>();
    append(h, &mut message, h.get(opts, "symbol")?)?;
    suffix(&mut message, tail);
    Err(h.throw_value(constructor.construct(&[AccountValue::String(message)])?))
}
type Next = Box<dyn FnOnce(SigningResult<AccountValue>) -> SigningResult<Step>>;
enum Step {
    Done(AccountValue),
    Await(AccountValue, Next),
}
fn drive(
    h: Rc<dyn SinglePlacementHost>,
    r: PromiseResolver<AccountValue>,
    step: SigningResult<Step>,
) {
    match step {
        Err(error) => r.settle(Err(error)),
        Ok(Step::Done(value)) => h.resolve_return(value, r),
        Ok(Step::Await(value, next)) => match h.await_value(value) {
            Err(error) => drive(h, r, next(Err(error))),
            Ok(p) => p.subscribe(move |value| drive(h, r, next(value.map(|v| (*v).clone())))),
        },
    }
}
struct Order {
    opts: AccountValue,
    signer: AccountValue,
    max_size: AccountValue,
    p_amount: AccountValue,
    amount_decimals: AccountValue,
    price_decimals: AccountValue,
    min_base: AccountValue,
    min_quote: AccountValue,
    amount: AccountValue,
    execution: AccountValue,
    conditional: bool,
    market: bool,
    trigger: AccountValue,
    normalized_tif: AccountValue,
    market_id: AccountValue,
    order_id: AccountValue,
    order_type: AccountValue,
    config: AccountValue,
}
struct Lots {
    order: Rc<Order>,
    params: AccountValue,
    amounts: AccountValue,
    ids: AccountValue,
}
fn minimum_error(
    h: &dyn SinglePlacementHost,
    opts: &AccountValue,
    reasons: AccountValue,
    lot: bool,
) -> SigningResult<Step> {
    let constructor = h.global("Error")?;
    let mut message = if lot {
        "Lighter order lot is below exchange minimums for "
    } else {
        "Lighter order amount is below exchange minimums for "
    }
    .encode_utf16()
    .collect::<Vec<_>>();
    append(h, &mut message, h.get(opts, "symbol")?)?;
    suffix(&mut message, " (");
    append(h, &mut message, reasons)?;
    suffix(
        &mut message,
        if lot {
            ")."
        } else {
            "). Increase order size or place closer to mark price."
        },
    );
    Err(h.throw_value(constructor.construct(&[AccountValue::String(message)])?))
}
fn prepare(
    h: Rc<dyn SinglePlacementHost>,
    opts: AccountValue,
    signer: AccountValue,
    market: AccountValue,
) -> SigningResult<Step> {
    let limits = h.get(&market, "limits")?;
    let amount_limits = h.get(&limits, "amount")?;
    let max_size = h.get(&amount_limits, "max")?;
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
    // The maintained imported helper returns its ordinary minimums record.
    let minimums = h
        .global("getLighterOrderMinimums")?
        .call(&[market.clone()])?;
    let min_base = h.get(&minimums, "minBaseAmount")?;
    let min_quote = h.get(&minimums, "minQuoteAmount")?;
    let adjust = h.global("adjust")?;
    let amount = adjust.call(&[h.get(&opts, "amount")?, p_amount.clone()])?;
    let price = if h.get(&opts, "price")?.truthy() {
        let adjust = h.global("adjust")?;
        adjust.call(&[h.get(&opts, "price")?, p_price.clone()])?
    } else {
        AccountValue::Null
    };
    let conditional = eq(&h.get(&opts, "type")?, &h.constant("OrderType.StopLoss")?)
        || eq(&h.get(&opts, "type")?, &h.constant("OrderType.TakeProfit")?);
    let is_market = eq(&h.get(&opts, "type")?, &h.constant("OrderType.Market")?)
        || eq(
            &h.get(&opts, "type")?,
            &h.constant("OrderType.TrailingStopLoss")?,
        );
    let trigger = if conditional {
        let adjust = h.global("adjust")?;
        let mut value = h.get(&opts, "triggerPrice")?;
        if value.is_nullish() {
            value = h.get(&opts, "price")?;
        }
        if value.is_nullish() {
            value = AccountValue::Number(0.0);
        }
        adjust.call(&[value, p_price.clone()])?
    } else {
        AccountValue::Null
    };
    if conditional
        && (!trigger.truthy()
            || h.binary(
                PlacementBinary::LessOrEqual,
                trigger.clone(),
                AccountValue::Number(0.0),
            )?
            .truthy())
    {
        return fail(
            h.as_ref(),
            "Trigger price is required for conditional Lighter orders (",
            &opts,
            ")",
        );
    }
    let normalized_tif = if conditional {
        h.constant("OrderTimeInForce.ImmediateOrCancel")?
    } else {
        h.get(&opts, "timeInForce")?
    };
    let execution = if is_market {
        let adjust = h.global("adjust")?;
        let reference = h.member(&h.owner(), "resolveMarketOrderReferencePrice")?;
        let reference = reference.call(&[h.get(&opts, "symbol")?, h.get(&opts, "side")?])?;
        let multiplier = if eq(&h.get(&opts, "side")?, &h.constant("OrderSide.Buy")?) {
            1.02
        } else {
            0.98
        };
        let price = h.binary(
            PlacementBinary::Multiply,
            reference,
            AccountValue::Number(multiplier),
        )?;
        adjust.call(&[price, p_price])?
    } else if conditional {
        // The live method is captured before the side getter is evaluated.
        let resolve = h.member(&h.owner(), "resolveConditionalExecutionPrice")?;
        resolve.call(&[trigger.clone(), h.get(&opts, "side")?, opts.clone()])?
    } else {
        price
    };
    if !is_market {
        let reasons = call(
            h.as_ref(),
            "getOrderMinimumViolationReasons",
            &[
                amount.clone(),
                execution.clone(),
                min_base.clone(),
                min_quote.clone(),
            ],
        )?;
        if reasons.truthy() {
            return minimum_error(h.as_ref(), &opts, reasons, false);
        }
    }
    let parse = h.global("parseInt")?;
    let market_id = parse.call(&[h.get(&market, "id")?, AccountValue::Number(10.0)])?;
    let max = bigint("281474976710655");
    let integer = h.global("BigInt")?;
    let now = h.global("Date.now")?.call(&[])?;
    let left = h.binary(
        PlacementBinary::Multiply,
        integer.call(&[now])?,
        bigint("100"),
    )?;
    let integer = h.global("BigInt")?;
    let floor = h.global("Math.floor")?;
    let random = h.global("Math.random")?.call(&[])?;
    let random = h.binary(
        PlacementBinary::Multiply,
        random,
        AccountValue::Number(100.0),
    )?;
    let right = integer.call(&[floor.call(&[random])?])?;
    let combined = h.binary(PlacementBinary::Add, left, right)?;
    let order_id = h.binary(PlacementBinary::Remainder, combined, max)?;
    let order_type = if conditional {
        if eq(&h.get(&opts, "type")?, &h.constant("OrderType.StopLoss")?) {
            text("stop_loss")
        } else {
            text("take_profit")
        }
    } else if is_market {
        text("market")
    } else {
        text("limit")
    };
    let builder = h.global("resolveLighterBuilderFeeConfig")?;
    let config = builder.call(&[get(h.as_ref(), "options")?, get(h.as_ref(), "name")?])?;
    let order = Rc::new(Order {
        opts,
        signer,
        max_size,
        p_amount,
        amount_decimals,
        price_decimals,
        min_base,
        min_quote,
        amount,
        execution,
        conditional,
        market: is_market,
        trigger,
        normalized_tif,
        market_id,
        order_id,
        order_type,
        config,
    });
    let tier = call(
        h.as_ref(),
        "ensurePartnerAttributionTierEligible",
        &[order.config.clone()],
    )?;
    Ok(Step::Await(
        tier,
        Box::new(move |value| {
            value?;
            let approved = call(
                h.as_ref(),
                "ensurePartnerAttributionApproved",
                &[order.config.clone()],
            )?;
            Ok(Step::Await(
                approved,
                Box::new(move |value| {
                    value?;
                    parameters(h, order)
                }),
            ))
        }),
    ))
}
fn parameters(h: Rc<dyn SinglePlacementHost>, order: Rc<Order>) -> SigningResult<Step> {
    let mut fields = vec![
        ("orderId", order.order_id.clone()),
        ("marketId", order.market_id.clone()),
        (
            "isBuy",
            AccountValue::Boolean(eq(
                &h.get(&order.opts, "side")?,
                &h.constant("OrderSide.Buy")?,
            )),
        ),
        (
            "price",
            h.global("toLighterScaledInt")?
                .call(&[order.execution.clone(), order.price_decimals.clone()])?,
        ),
        (
            "amount",
            h.global("toLighterScaledInt")?
                .call(&[order.amount.clone(), order.amount_decimals.clone()])?,
        ),
        (
            "postOnly",
            AccountValue::Boolean(
                !order.conditional
                    && eq(
                        &h.get(&order.opts, "timeInForce")?,
                        &h.constant("OrderTimeInForce.PostOnly")?,
                    ),
            ),
        ),
        ("reduceOnly", {
            let value = h.get(&order.opts, "reduceOnly")?;
            if value.truthy() {
                value
            } else {
                AccountValue::Boolean(false)
            }
        }),
        ("orderType", order.order_type.clone()),
        (
            "triggerPrice",
            if order.conditional {
                h.global("toLighterScaledInt")?
                    .call(&[order.trigger.clone(), order.price_decimals.clone()])?
            } else {
                AccountValue::Undefined
            },
        ),
        ("timeInForce", order.normalized_tif.clone()),
    ];
    if order.config.truthy() {
        fields.push((
            "integratorAccountIndex",
            h.get(&order.config, "integratorAccountIndex")?,
        ));
        fields.push((
            "integratorMakerFee",
            h.get(&order.config, "integratorMakerFee")?,
        ));
        fields.push((
            "integratorTakerFee",
            h.get(&order.config, "integratorTakerFee")?,
        ));
    }
    let params = h.literal(&fields)?;
    let log = h.member(&h.owner(), "log")?;
    let stringify = h.global("JSON.stringify")?;
    let live = h.clone();
    let replacer = h.function(Rc::new(move |args| {
        let value = arg(args, 1);
        if matches!(value, AccountValue::BigInt(_)) {
            method(live.as_ref(), &value, "toString", &[])
        } else {
            Ok(value)
        }
    }))?;
    let json = stringify.call(&[params.clone(), replacer])?;
    let mut message = "Order request: ".encode_utf16().collect::<Vec<_>>();
    append(h.as_ref(), &mut message, json)?;
    let severity = h.constant("LogSeverity.Info")?;
    log.call(&[AccountValue::String(message), severity])?;
    let lots = if h
        .binary(
            PlacementBinary::GreaterThan,
            order.amount.clone(),
            order.max_size.clone(),
        )?
        .truthy()
    {
        let ceil = h.global("Math.ceil")?;
        let divided = h.binary(
            PlacementBinary::Divide,
            order.amount.clone(),
            order.max_size.clone(),
        )?;
        ceil.call(&[divided])?
    } else {
        AccountValue::Number(1.0)
    };
    let rest = if h
        .binary(
            PlacementBinary::GreaterThan,
            order.amount.clone(),
            order.max_size.clone(),
        )?
        .truthy()
    {
        let adjust = h.global("adjust")?;
        let remainder = h.binary(
            PlacementBinary::Remainder,
            order.amount.clone(),
            order.max_size.clone(),
        )?;
        adjust.call(&[remainder, order.p_amount.clone()])?
    } else {
        AccountValue::Number(0.0)
    };
    let adjust = h.global("adjust")?;
    let reduced = h.binary(
        PlacementBinary::Subtract,
        order.amount.clone(),
        rest.clone(),
    )?;
    let divided = h.binary(PlacementBinary::Divide, reduced, lots.clone())?;
    let lot_size = adjust.call(&[divided, order.p_amount.clone()])?;
    let from = h.global("Array.from")?;
    let length = h.literal(&[("length", lots)])?;
    let callback = h.function(Rc::new(move |_| Ok(lot_size.clone())))?;
    let amounts = from.call(&[length, callback])?;
    if h.binary(
        PlacementBinary::GreaterThan,
        rest.clone(),
        AccountValue::Number(0.0),
    )?
    .truthy()
    {
        method(h.as_ref(), &amounts, "push", &[rest])?;
    }
    if !order.market {
        if h.binary(
            PlacementBinary::GreaterThan,
            h.get(&amounts, "length")?,
            AccountValue::Number(1.0),
        )?
        .truthy()
            && h.binary(
                PlacementBinary::GreaterThan,
                order.min_base.clone(),
                AccountValue::Number(0.0),
            )?
            .truthy()
        {
            let index = h.binary(
                PlacementBinary::Subtract,
                h.get(&amounts, "length")?,
                AccountValue::Number(1.0),
            )?;
            let last = h.index(&amounts, index)?;
            if h.binary(
                PlacementBinary::LessThan,
                last.clone(),
                order.min_base.clone(),
            )?
            .truthy()
            {
                let key = h.binary(
                    PlacementBinary::Subtract,
                    h.get(&amounts, "length")?,
                    AccountValue::Number(2.0),
                )?;
                let adjust = h.global("adjust")?;
                let index = h.binary(
                    PlacementBinary::Subtract,
                    h.get(&amounts, "length")?,
                    AccountValue::Number(2.0),
                )?;
                let previous = h.index(&amounts, index)?;
                let combined = h.binary(PlacementBinary::Add, previous, last)?;
                let value = adjust.call(&[combined, order.p_amount.clone()])?;
                h.set_index(&amounts, key, value)?;
                method(h.as_ref(), &amounts, "pop", &[])?;
            }
        }
        let live = h.clone();
        let context = order.clone();
        h.for_each(
            amounts.clone(),
            Rc::new(move |amount| {
                let reasons = call(
                    live.as_ref(),
                    "getOrderMinimumViolationReasons",
                    &[
                        amount,
                        context.execution.clone(),
                        context.min_base.clone(),
                        context.min_quote.clone(),
                    ],
                )?;
                if reasons.truthy() {
                    minimum_error(live.as_ref(), &context.opts, reasons, true)?;
                }
                Ok(())
            }),
        )?;
    }
    let ids = h.array(&[])?;
    submit_next(
        h,
        Rc::new(Lots {
            order,
            params,
            amounts,
            ids,
        }),
        0.0,
    )
}
fn submit_next(h: Rc<dyn SinglePlacementHost>, lots: Rc<Lots>, index: f64) -> SigningResult<Step> {
    if !h
        .binary(
            PlacementBinary::LessThan,
            AccountValue::Number(index),
            h.get(&lots.amounts, "length")?,
        )?
        .truthy()
    {
        return siblings(h, lots);
    }
    let integer = h.global("BigInt")?;
    let lot_id = h.binary(
        PlacementBinary::Add,
        lots.order.order_id.clone(),
        integer.call(&[AccountValue::Number(index)])?,
    )?;
    let scaled = h.global("toLighterScaledInt")?;
    let amount = scaled.call(&[
        h.index(&lots.amounts, AccountValue::Number(index))?,
        lots.order.amount_decimals.clone(),
    ])?;
    let remember = h.member(&h.owner(), "rememberPlacedOrderHint")?;
    let hint = h.literal(&[
        ("type", h.get(&lots.order.opts, "type")?),
        ("side", h.get(&lots.order.opts, "side")?),
        (
            "reduceOnly",
            h.global("Boolean")?
                .call(&[h.get(&lots.order.opts, "reduceOnly")?])?,
        ),
        ("timeInForce", lots.order.normalized_tif.clone()),
        (
            "price",
            if lots.order.conditional {
                lots.order.trigger.clone()
            } else {
                lots.order.execution.clone()
            },
        ),
        ("symbol", h.get(&lots.order.opts, "symbol")?),
    ])?;
    remember.call(&[lot_id.clone(), hint])?;
    let created = (|| {
        let create = h.member(&lots.order.signer, "createOrder")?;
        let params = h.spread_object(&lots.params)?;
        h.define_data(&params, "orderId", lot_id.clone())?;
        h.define_data(&params, "amount", amount)?;
        create.call(&[params])
    })();
    let created = match created {
        Ok(value) => value,
        Err(error) => return primary_error(h, lots, lot_id, error),
    };
    Ok(Step::Await(
        created,
        Box::new(move |value| {
            let pushed = (|| {
                value?;
                let push = h.member(&lots.ids, "push")?;
                let id = method(h.as_ref(), &lot_id, "toString", &[])?;
                push.call(&[id])?;
                Ok(())
            })();
            if let Err(error) = pushed {
                return primary_error(h, lots, lot_id, error);
            }
            let end = h.binary(
                PlacementBinary::Subtract,
                h.get(&lots.amounts, "length")?,
                AccountValue::Number(1.0),
            )?;
            if h.binary(PlacementBinary::LessThan, AccountValue::Number(index), end)?
                .truthy()
            {
                let sleep = h.global("sleep")?.call(&[AccountValue::Number(100.0)])?;
                Ok(Step::Await(
                    sleep,
                    Box::new(move |value| {
                        value?;
                        submit_next(h, lots, index + 1.0)
                    }),
                ))
            } else {
                submit_next(h, lots, index + 1.0)
            }
        }),
    ))
}
fn primary_error(
    h: Rc<dyn SinglePlacementHost>,
    lots: Rc<Lots>,
    id: AccountValue,
    error: TransactionSigningError,
) -> SigningResult<Step> {
    let error = h.error_value(error);
    let raw = call(h.as_ref(), "extractOrderErrorMessage", &[error.clone()])?;
    let invalid = call(
        h.as_ref(),
        "isInvalidMarginModeOrderError",
        &[error.clone(), raw],
    )?;
    if invalid.truthy() {
        let refresh = h.member(&h.owner(), "refreshMarginModeStateAfterInvalidOrder")?;
        let refreshed = refresh.call(&[h.get(&lots.order.opts, "symbol")?])?;
        Ok(Step::Await(
            refreshed,
            Box::new(move |value| {
                value?;
                finish_primary_error(h, lots, id, error)
            }),
        ))
    } else {
        finish_primary_error(h, lots, id, error)
    }
}
fn finish_primary_error(
    h: Rc<dyn SinglePlacementHost>,
    lots: Rc<Lots>,
    id: AccountValue,
    error: AccountValue,
) -> SigningResult<Step> {
    let normalized = call(
        h.as_ref(),
        "normalizePlaceOrderErrorMessage",
        &[lots.order.opts.clone(), error],
    )?;
    let hints = get(h.as_ref(), "placedOrderHints")?;
    let delete = h.member(&hints, "delete")?;
    let id = method(h.as_ref(), &id, "toString", &[])?;
    delete.call(&[id])?;
    call(h.as_ref(), "emitAndLogError", &[normalized.clone()])?;
    let error = h.global("Error")?.construct(&[normalized])?;
    Err(h.throw_value(error))
}
fn siblings(h: Rc<dyn SinglePlacementHost>, lots: Rc<Lots>) -> SigningResult<Step> {
    let opts = &lots.order.opts;
    let attach = (eq(&h.get(opts, "type")?, &h.constant("OrderType.Limit")?)
        || eq(&h.get(opts, "type")?, &h.constant("OrderType.Market")?))
        && !h.get(opts, "reduceOnly")?.truthy()
        && (!matches!(h.get(opts, "stopLoss")?, AccountValue::Undefined)
            || !matches!(h.get(opts, "takeProfit")?, AccountValue::Undefined));
    if !attach {
        return finish(h, lots);
    }
    let derive = h.member(&h.owner(), "deriveAlgoOrdersFromNormalOrdersOpts")?;
    let source = derive.call(&[h.array(&[opts.clone()])?])?;
    let map = h.member(&source, "map")?;
    let live = h.clone();
    let options = opts.clone();
    let callback = h.function(Rc::new(move |args| {
        let sibling = live.spread_object(&arg(args, 0))?;
        live.define_data(&sibling, "symbol", live.get(&options, "symbol")?)?;
        live.define_data(&sibling, "reduceOnly", AccountValue::Boolean(true))?;
        live.define_data(
            &sibling,
            "timeInForce",
            live.constant("OrderTimeInForce.ImmediateOrCancel")?,
        )?;
        Ok(sibling)
    }))?;
    let siblings = map.call(&[callback])?;
    let iterator = Rc::new(RefCell::new(h.iterator(siblings)?));
    sibling_next(h, lots, iterator)
}
type IteratorRef = Rc<RefCell<Box<dyn PlacementIterator>>>;
fn sibling_next(
    h: Rc<dyn SinglePlacementHost>,
    lots: Rc<Lots>,
    iterator: IteratorRef,
) -> SigningResult<Step> {
    let next = iterator.borrow_mut().next()?;
    let Some(sibling) = next else {
        return finish(h, lots);
    };
    let result = call(h.as_ref(), "placeOrder", &[sibling.clone()]);
    let result = match result {
        Ok(value) => value,
        Err(error) => return sibling_error(h, sibling, error, &iterator),
    };
    Ok(Step::Await(
        result,
        Box::new(move |value| {
            let pushed = (|| {
                let ids = value?;
                let push = h.member(&lots.ids, "push")?;
                h.call_spread(push, ids)?;
                Ok(())
            })();
            if let Err(error) = pushed {
                return sibling_error(h, sibling, error, &iterator);
            }
            sibling_next(h, lots, iterator)
        }),
    ))
}
fn sibling_error(
    h: Rc<dyn SinglePlacementHost>,
    sibling: AccountValue,
    error: TransactionSigningError,
    iterator: &IteratorRef,
) -> SigningResult<Step> {
    let result = (|| {
        let sibling_type = if eq(
            &h.get(&sibling, "type")?,
            &h.constant("OrderType.StopLoss")?,
        ) {
            "stop-loss"
        } else {
            "take-profit"
        };
        let constructor = h.global("Error")?;
        let mut message = "Primary order was placed but failed to attach "
            .encode_utf16()
            .collect::<Vec<_>>();
        suffix(&mut message, sibling_type);
        suffix(&mut message, " sibling on Lighter: ");
        let error = h.error_value(error);
        let mut detail = if error.is_nullish() {
            AccountValue::Undefined
        } else {
            h.get(&error, "message")?
        };
        if !detail.truthy() {
            detail = h.global("String")?.call(&[error])?;
        }
        append(h.as_ref(), &mut message, detail)?;
        Err(h.throw_value(constructor.construct(&[AccountValue::String(message)])?))
    })();
    match result {
        Err(error) => Err(iterator.borrow_mut().close_throw(error)),
        Ok(step) => Ok(step),
    }
}
fn finish(h: Rc<dyn SinglePlacementHost>, lots: Rc<Lots>) -> SigningResult<Step> {
    let websocket = get(h.as_ref(), "privateWebsocket")?;
    method(h.as_ref(), &websocket, "throttleFetchOrders", &[])?;
    Ok(Step::Done(lots.ids.clone()))
}
// ts: LighterExchange.placeOrder
pub fn place_order(
    h: Rc<dyn SinglePlacementHost>,
    opts: AccountValue,
) -> NativePromise<AccountValue> {
    let (p, r) = NativePromise::pending(h.runtime());
    let step = (|| {
        call(h.as_ref(), "resolveCallerClientOrderId", &[opts.clone()])?;
        let signer = call(h.as_ref(), "ensureSignerAdapter", &[])?;
        let live = h.clone();
        Ok(Step::Await(
            signer,
            Box::new(move |signer| {
                let signer = signer?;
                let store = get(live.as_ref(), "store")?;
                let markets = live.get(&store, "marketsBySymbol")?;
                let market = live.index(&markets, live.get(&opts, "symbol")?)?;
                if !market.truthy() {
                    return fail(live.as_ref(), "Market ", &opts, " not found");
                }
                let preflight = call(
                    live.as_ref(),
                    "assertReduceOnlyLimitHasPosition",
                    &[opts.clone()],
                )?;
                Ok(Step::Await(
                    preflight,
                    Box::new(move |value| {
                        value?;
                        prepare(live, opts, signer, market)
                    }),
                ))
            }),
        ))
    })();
    drive(h, r, step);
    p
}
