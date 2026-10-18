use crate::market_account_reads::{MarketAccountOps, ReadAsyncMapper, ReadAwait, ReadMapper};
use std::rc::Rc;

pub struct OrderMutationOps<V, E> {
    pub value: Rc<MarketAccountOps<V, E>>,
    pub is_terminal_order: Rc<dyn Fn(&V) -> Result<bool, E>>,
    pub has: Rc<dyn Fn(&str, &V) -> Result<bool, E>>,
    pub filter: Rc<dyn Fn(V, ReadMapper<V, E>) -> Result<V, E>>,
    pub find: Rc<dyn Fn(V, ReadMapper<V, E>) -> Result<V, E>>,
    pub optional_find: Rc<dyn Fn(V, ReadMapper<V, E>) -> Result<V, E>>,
    pub partition: Rc<dyn Fn(V, V, ReadMapper<V, E>) -> Result<V, E>>,
    pub for_each: Rc<dyn Fn(V, Rc<dyn Fn(V) -> Result<(), E>>) -> Result<(), E>>,
    pub series: Rc<dyn Fn(V, V, ReadAsyncMapper<V, E>) -> ReadAwait<V, E>>,
    pub adopt_return: Rc<dyn Fn(V) -> ReadAwait<V, E>>,
}
fn optional<V, E>(h: &OrderMutationOps<V, E>, value: &V, key: &str) -> Result<V, E> {
    if (h.value.nullish)(value) {
        Ok((h.value.undefined)())
    } else {
        (h.value.get)(value, key)
    }
}
fn or<V, E>(
    h: &OrderMutationOps<V, E>,
    value: V,
    next: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    if (h.value.truthy)(&value) {
        Ok(value)
    } else {
        next()
    }
}
fn call<V, E>(
    h: &OrderMutationOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.value.capture)(name)?;
    (h.value.invoke)(callee, args()?)
}
fn new_error<V, E>(
    h: &OrderMutationOps<V, E>,
    message: impl FnOnce() -> Result<V, E>,
) -> Result<E, E> {
    let ctor = (h.value.capture)("Error")?;
    (h.value.new_error)(ctor, message()?)
}
fn caught_message<V, E>(h: &OrderMutationOps<V, E>, error: &E) -> Result<V, E> {
    let value = (h.value.error_value)(error);
    let response = optional(h, &value, "response")?;
    let data = optional(h, &response, "data")?;
    or(h, optional(h, &data, "msg")?, || {
        optional(h, &value, "message")
    })
}
fn emit<V, E>(h: &OrderMutationOps<V, E>, message: impl FnOnce() -> Result<V, E>) -> Result<(), E> {
    call(h, "this.emitAndLogError", || Ok(vec![message()?]))?;
    Ok(())
}
fn index<V, E>(h: &OrderMutationOps<V, E>, object: V, key: V) -> Result<V, E> {
    (h.value.intrinsic)("get-index", vec![object, key])
}
fn stringify<V, E>(h: &OrderMutationOps<V, E>, value: V) -> Result<V, E> {
    (h.value.intrinsic)("template-string", vec![value])
}
fn precision<V, E>(h: &OrderMutationOps<V, E>, market: &V, key: &str) -> Result<V, E> {
    let p = (h.value.get)(market, "precision")?;
    (h.value.get)(&p, key)
}
fn define_field<V: Clone, E>(
    h: &OrderMutationOps<V, E>,
    object: &V,
    key: &str,
    value: V,
) -> Result<(), E> {
    (h.value.intrinsic)(
        "define-literal-field",
        vec![object.clone(), (h.value.literal)(key), value],
    )?;
    Ok(())
}
fn suffix<V, E>(h: &OrderMutationOps<V, E>, id: V) -> Result<V, E> {
    (h.value.intrinsic)("replace-order-id-suffix", vec![id])
}
fn market<V, E>(h: &OrderMutationOps<V, E>, order: &V) -> Result<V, E> {
    let markets = (h.value.capture)("this.store.marketsBySymbol")?;
    let symbol = (h.value.get)(order, "symbol")?;
    let m = index(h, markets, symbol)?;
    if !(h.value.truthy)(&m) {
        return Err(new_error(h, || {
            (h.value.intrinsic)(
                "market-not-found-message",
                vec![(h.value.get)(order, "symbol")?],
            )
        })?);
    }
    Ok(m)
}
pub async fn update_order<V: Clone + 'static, E: 'static>(
    h: Rc<OrderMutationOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    let order = (h.value.get)(&opts, "order")?;
    let update = (h.value.get)(&opts, "update")?;
    if !(h.value.equal)(
        &(h.value.get)(&order, "type")?,
        &(h.value.capture)("OrderType.Limit")?,
    ) {
        let value = call(&h, "this.updateAlgoOrder", || {
            Ok(vec![(h.value.object)(vec![
                ("order", order),
                ("update", update),
            ])])
        })?;
        return (h.adopt_return)(value).await;
    }
    let m = market(&h, &order)?;
    let payload = (h.value.object)(vec![
        ("instId", (h.value.get)(&m, "id")?),
        ("ordId", (h.value.get)(&order, "id")?),
    ]);
    if (h.has)("price", &update)? {
        (h.value.set)(
            &payload,
            "newPx",
            stringify(&h, (h.value.get)(&update, "price")?)?,
        )?;
    }
    if (h.has)("amount", &update)? {
        let amount = (h.value.intrinsic)("unary-plus", vec![(h.value.get)(&order, "amount")?])?;
        if !(h.value.equal)(&amount, &(h.value.number)(0.)) {
            let step = precision(&h, &m, "amount")?;
            let amount = call(&h, "adjust", || {
                Ok(vec![(h.value.get)(&update, "amount")?, step])
            })?;
            if (h.value.equal)(&amount, &(h.value.number)(0.)) {
                emit(&h, || Ok((h.value.literal)("Order amount is too small")))?;
                return Ok((h.value.array)(vec![]));
            }
            (h.value.set)(&payload, "newSz", stringify(&h, amount)?)?;
        }
    }
    let posted = async {
        let response = call(&h, "this.xhr.post:ENDPOINTS.UPDATE_ORDER", || {
            Ok(vec![payload])
        })?;
        (h.value.await_value)(response).await?;
        Ok((h.value.array)(vec![(h.value.get)(&order, "id")?]))
    }
    .await;
    match posted {
        Ok(value) => Ok(value),
        Err(error) => {
            emit(&h, || caught_message(&h, &error))?;
            Ok((h.value.array)(vec![]))
        }
    }
}
pub async fn update_algo_order<V: Clone + 'static, E: 'static>(
    h: Rc<OrderMutationOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    let order = (h.value.get)(&opts, "order")?;
    let update = (h.value.get)(&opts, "update")?;
    if (h.value.equal)(
        &(h.value.get)(&order, "type")?,
        &(h.value.capture)("OrderType.Conditional")?,
    ) {
        let value = call(&h, "this.updateTriggerAlgoOrder", || {
            Ok(vec![(h.value.object)(vec![
                ("order", order),
                ("update", update),
            ])])
        })?;
        return (h.adopt_return)(value).await;
    }
    let orders = (h.value.capture)("this.store.orders")?;
    let ops = h.clone();
    let original = order.clone();
    let orders = (h.filter)(
        orders,
        Rc::new(move |o| {
            let id = (ops.value.get)(&o, "id")?;
            let starts = (ops.value.member)(id, "startsWith")?;
            let prefix = suffix(&ops, (ops.value.get)(&original, "id")?)?;
            (ops.value.invoke)(starts, vec![prefix])
        }),
    )?;
    let amount = if (h.has)("amount", &update)? {
        (h.value.get)(&update, "amount")?
    } else {
        (h.value.get)(&order, "amount")?
    };
    let price = if (h.has)("price", &update)? {
        (h.value.get)(&update, "price")?
    } else {
        (h.value.get)(&order, "price")?
    };
    let base = (h.value.intrinsic)("object-spread", vec![order.clone()])?;
    for key in ["symbol", "type", "side"] {
        define_field(&h, &base, key, (h.value.get)(&order, key)?)?;
    }
    define_field(&h, &base, "amount", amount)?;
    let reduce = or(&h, (h.value.get)(&order, "reduceOnly")?, || {
        Ok((h.value.boolean)(false))
    })?;
    define_field(&h, &base, "reduceOnly", reduce)?;
    let new = (h.value.intrinsic)("object-spread", vec![base])?;
    define_field(&h, &new, "price", price)?;
    let cancel = call(&h, "this.cancelAlgoOrders", || Ok(vec![orders]))?;
    (h.value.await_value)(cancel).await?;
    let placed = call(&h, "this.placeOrder", || Ok(vec![new]))?;
    (h.value.await_value)(placed).await
}
fn trigger_failure<V: Clone, E>(h: &OrderMutationOps<V, E>, message: V) -> Result<E, E> {
    emit(h, || Ok(message.clone()))?;
    new_error(h, || Ok(message))
}
pub async fn update_trigger_algo_order<V: Clone + 'static, E: 'static>(
    h: Rc<OrderMutationOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    let order = (h.value.get)(&opts, "order")?;
    let update = (h.value.get)(&opts, "update")?;
    let m = market(&h, &order)?;
    let payload = (h.value.object)(vec![
        ("instId", (h.value.get)(&m, "id")?),
        ("algoId", suffix(&h, (h.value.get)(&order, "id")?)?),
    ]);
    let price = call(&h, "adjust", || {
        Ok(vec![
            if (h.has)("price", &update)? {
                (h.value.get)(&update, "price")?
            } else {
                (h.value.get)(&order, "price")?
            },
            precision(&h, &m, "price")?,
        ])
    })?;
    let finite = call(&h, "Number.isFinite", || Ok(vec![price.clone()]))?;
    if !(h.value.truthy)(&finite)
        || (h.value.truthy)(&(h.value.intrinsic)(
            "less-or-equal",
            vec![price.clone(), (h.value.number)(0.)],
        )?)
    {
        return Err(trigger_failure(
            &h,
            (h.value.literal)("Trigger price is required for OKX opening conditional orders"),
        )?);
    }
    let limit = call(&h, "this.toFiniteNumber", || {
        let extra = (h.value.get)(&order, "extra")?;
        Ok(vec![optional(&h, &extra, "conditionalOrderPrice")?])
    })?;
    (h.value.set)(&payload, "newTriggerPx", stringify(&h, price)?)?;
    let order_px = if (h.value.truthy)(&(h.value.intrinsic)(
        "greater-than",
        vec![limit.clone(), (h.value.number)(0.)],
    )?) {
        let adjusted = call(&h, "adjust", || {
            Ok(vec![limit, precision(&h, &m, "price")?])
        })?;
        stringify(&h, adjusted)?
    } else {
        (h.value.literal)("-1")
    };
    (h.value.set)(&payload, "newOrdPx", order_px)?;
    let trigger_type = call(&h, "getOKXTriggerPxType", || {
        Ok(vec![(h.value.get)(&order, "triggerType")?])
    })?;
    (h.value.set)(&payload, "newTriggerPxType", trigger_type)?;
    if (h.has)("amount", &update)? {
        let original = (h.value.intrinsic)("unary-plus", vec![(h.value.get)(&order, "amount")?])?;
        if !(h.value.equal)(&original, &(h.value.number)(0.)) {
            let amount = call(&h, "adjust", || {
                Ok(vec![
                    (h.value.get)(&update, "amount")?,
                    precision(&h, &m, "amount")?,
                ])
            })?;
            if (h.value.equal)(&amount, &(h.value.number)(0.)) {
                return Err(trigger_failure(
                    &h,
                    (h.value.literal)("Order amount is too small"),
                )?);
            }
            (h.value.set)(&payload, "newSz", stringify(&h, amount)?)?;
        }
    }
    let posted = async {
        let value = call(&h, "this.xhr.post:ENDPOINTS.UPDATE_ALGO_ORDER", || {
            Ok(vec![payload])
        })?;
        let response = (h.value.await_value)(value).await?;
        (h.value.get)(&response, "data")
    }
    .await;
    let body = match posted {
        Ok(body) => body,
        Err(error) => {
            let message = or(&h, caught_message(&h, &error)?, || {
                Ok((h.value.literal)("OKX trigger order amend failed"))
            })?;
            return Err(trigger_failure(&h, message)?);
        }
    };
    if (h.value.truthy)(&optional(&h, &body, "code")?)
        && !(h.value.equal)(&(h.value.get)(&body, "code")?, &(h.value.literal)("0"))
    {
        let message = or(&h, (h.value.get)(&body, "msg")?, || {
            or(&h, (h.value.get)(&body, "code")?, || {
                Ok((h.value.literal)("OKX trigger order amend failed"))
            })
        })?;
        return Err(trigger_failure(&h, message)?);
    }
    let data = optional(&h, &body, "data")?;
    let ops = h.clone();
    let failed = (h.optional_find)(
        data,
        Rc::new(move |d| {
            let code = (ops.value.get)(&d, "sCode")?;
            let mismatch = if (ops.value.truthy)(&code) {
                (ops.value.boolean)(!(ops.value.equal)(
                    &(ops.value.get)(&d, "sCode")?,
                    &(ops.value.literal)("0"),
                ))
            } else {
                code
            };
            or(&ops, mismatch, || (ops.value.get)(&d, "sMsg"))
        }),
    )?;
    if (h.value.truthy)(&failed) {
        let message = or(&h, (h.value.get)(&failed, "sMsg")?, || {
            or(&h, (h.value.get)(&failed, "sCode")?, || {
                Ok((h.value.literal)("OKX trigger order amend failed"))
            })
        })?;
        return Err(trigger_failure(&h, message)?);
    }
    Ok((h.value.array)(vec![(h.value.get)(&order, "id")?]))
}
pub async fn cancel_orders<V: Clone + 'static, E: 'static>(
    h: Rc<OrderMutationOps<V, E>>,
    orders: V,
) -> Result<V, E> {
    let partition = (h.value.capture)("partition")?;
    let ops = h.clone();
    let groups = (h.partition)(
        partition,
        orders,
        Rc::new(move |o| {
            call(&ops, "this.isAlgoOrder", || {
                Ok(vec![(ops.value.get)(&o, "type")?])
            })
        }),
    )?;
    let pair = (h.value.intrinsic)("destructure-two", vec![groups])?;
    let algo = (h.value.get)(&pair, "0")?;
    let normal = (h.value.get)(&pair, "1")?;
    if (h.value.truthy)(&(h.value.get)(&normal, "length")?) {
        let value = call(&h, "this.cancelNormalOrders", || Ok(vec![normal]))?;
        (h.value.await_value)(value).await?;
    }
    if (h.value.truthy)(&(h.value.get)(&algo, "length")?) {
        let value = call(&h, "this.cancelAlgoOrders", || Ok(vec![algo]))?;
        (h.value.await_value)(value).await?;
    }
    Ok((h.value.undefined)())
}
pub async fn cancel_symbol_orders<V: Clone + 'static, E: 'static>(
    h: Rc<OrderMutationOps<V, E>>,
    symbol: V,
) -> Result<V, E> {
    let orders = (h.value.capture)("this.store.orders")?;
    let ops = h.clone();
    let orders = (h.filter)(
        orders,
        Rc::new(move |o| {
            let selected = (ops.value.equal)(&(ops.value.get)(&o, "symbol")?, &symbol)
                && !(ops.is_terminal_order)(&o)?;
            Ok((ops.value.boolean)(selected))
        }),
    )?;
    let value = call(&h, "this.cancelOrders", || Ok(vec![orders]))?;
    (h.value.await_value)(value).await?;
    Ok((h.value.undefined)())
}
pub async fn cancel_batch<V: Clone + 'static, E: 'static>(
    h: Rc<OrderMutationOps<V, E>>,
    batch: V,
    algo: bool,
) -> Result<V, E> {
    let value = call(
        &h,
        if algo {
            "this.xhr.post:ENDPOINTS.CANCEL_ALGO_ORDERS"
        } else {
            "this.xhr.post:ENDPOINTS.CANCEL_ORDERS"
        },
        || Ok(vec![batch]),
    )?;
    let response = (h.value.await_value)(value).await?;
    let data = (h.value.get)(&response, "data")?;
    let data = (h.value.get)(&data, "data")?;
    let ops = h.clone();
    (h.for_each)(
        data,
        Rc::new(move |d| {
            if (ops.value.truthy)(&(ops.value.get)(&d, "sMsg")?) {
                emit(&ops, || (ops.value.get)(&d, "sMsg"))?;
            }
            Ok(())
        }),
    )?;
    Ok((h.value.undefined)())
}
pub async fn cancel_kind<V: Clone + 'static, E: 'static>(
    h: Rc<OrderMutationOps<V, E>>,
    orders: V,
    algo: bool,
) -> Result<V, E> {
    let ops = h.clone();
    let orders = (h.filter)(
        orders,
        Rc::new(move |o| {
            let yes = call(&ops, "this.isAlgoOrder", || {
                Ok(vec![(ops.value.get)(&o, "type")?])
            })?;
            Ok(if algo {
                yes
            } else {
                (ops.value.boolean)(!(ops.value.truthy)(&yes))
            })
        }),
    )?;
    let ops = h.clone();
    let batches = (h.value.reduce)(
        orders,
        (h.value.array)(vec![]),
        Rc::new(move |acc, o| {
            let markets = (ops.value.capture)("this.store.marketsBySymbol")?;
            let market = index(&ops, markets, (ops.value.get)(&o, "symbol")?)?;
            if !(ops.value.truthy)(&market) {
                return Ok(acc);
            }
            let prefix = (ops.value.intrinsic)("array-spread-prefix", vec![acc])?;
            let inst_id = (ops.value.get)(&market, "id")?;
            let id = (ops.value.get)(&o, "id")?;
            let id = if algo { suffix(&ops, id)? } else { id };
            let row = (ops.value.object)(vec![
                ("instId", inst_id),
                (if algo { "algoId" } else { "ordId" }, id),
            ]);
            (ops.value.intrinsic)("array-prefix-append", vec![prefix, row])
        }),
    )?;
    let series = (h.value.capture)("forEachSeries")?;
    let chunk = call(&h, "chunk", || {
        Ok(vec![
            batches,
            (h.value.number)(if algo { 10. } else { 20. }),
        ])
    })?;
    let ops = h.clone();
    let callback = Rc::new(move |batch| {
        let ops = ops.clone();
        Box::pin(async move { cancel_batch(ops, batch, algo).await }) as ReadAwait<V, E>
    });
    (h.series)(series, chunk, callback).await?;
    Ok((h.value.undefined)())
}
pub async fn cancel_normal_orders<V: Clone + 'static, E: 'static>(
    h: Rc<OrderMutationOps<V, E>>,
    orders: V,
) -> Result<V, E> {
    cancel_kind(h, orders, false).await
}
pub async fn cancel_algo_orders<V: Clone + 'static, E: 'static>(
    h: Rc<OrderMutationOps<V, E>>,
    orders: V,
) -> Result<V, E> {
    cancel_kind(h, orders, true).await
}
async fn recover_positions<V: Clone + 'static, E: 'static>(
    h: Rc<OrderMutationOps<V, E>>,
    error: &E,
) -> Result<E, E> {
    emit(&h, || caught_message(&h, error))?;
    let value = call(&h, "this.fetchPositions", || Ok(vec![]))?;
    let positions = (h.value.await_value)(value).await?;
    call(&h, "this.store.addOrUpdatePositions", || {
        Ok(vec![positions])
    })?;
    new_error(&h, || caught_message(&h, error))
}
pub async fn set_leverage<V: Clone + 'static, E: 'static>(
    h: Rc<OrderMutationOps<V, E>>,
    symbol: V,
    input: V,
) -> Result<V, E> {
    let changed = async {
        let markets = (h.value.capture)("this.store.marketsBySymbol")?;
        let market = index(&h, markets, symbol.clone())?;
        let positions = (h.value.capture)("this.store.positions")?;
        let ops = h.clone();
        let sym = symbol.clone();
        let position = (h.find)(
            positions,
            Rc::new(move |p| {
                Ok((ops.value.boolean)((ops.value.equal)(
                    &(ops.value.get)(&p, "symbol")?,
                    &sym,
                )))
            }),
        )?;
        if !(h.value.truthy)(&market) {
            return Err(new_error(&h, || {
                (h.value.intrinsic)("market-not-found-message", vec![symbol.clone()])
            })?);
        }
        if !(h.value.truthy)(&position) {
            return Err(new_error(&h, || {
                (h.value.intrinsic)("position-not-found-message", vec![symbol])
            })?);
        }
        let min = (h.value.capture)("Math.min")?;
        let max = (h.value.capture)("Math.max")?;
        let limits = (h.value.get)(&market, "limits")?;
        let leverage = (h.value.get)(&limits, "leverage")?;
        let lower = (h.value.get)(&leverage, "min")?;
        let clamped = (h.value.invoke)(max, vec![input, lower])?;
        let limits = (h.value.get)(&market, "limits")?;
        let leverage = (h.value.get)(&limits, "leverage")?;
        let upper = (h.value.get)(&leverage, "max")?;
        let leverage = (h.value.invoke)(min, vec![clamped, upper])?;
        if !(h.value.equal)(&(h.value.get)(&position, "leverage")?, &leverage) {
            let value = call(&h, "this.xhr.post:ENDPOINTS.SET_LEVERAGE", || {
                Ok(vec![(h.value.object)(vec![
                    ("instId", (h.value.get)(&market, "id")?),
                    ("lever", stringify(&h, leverage)?),
                    ("mgnMode", (h.value.literal)("cross")),
                ])])
            })?;
            (h.value.await_value)(value).await?;
            let value = call(&h, "this.fetchPositions", || Ok(vec![]))?;
            let positions = (h.value.await_value)(value).await?;
            call(&h, "this.store.update", || {
                Ok(vec![(h.value.object)(vec![("positions", positions)])])
            })?;
        }
        Ok((h.value.undefined)())
    }
    .await;
    match changed {
        Ok(value) => Ok(value),
        Err(error) => Err(recover_positions(h, &error).await?),
    }
}
pub async fn change_position_mode<V: Clone + 'static, E: 'static>(
    h: Rc<OrderMutationOps<V, E>>,
    hedged: V,
) -> Result<V, E> {
    let positions = (h.value.capture)("this.store.positions")?;
    let ops = h.clone();
    let open = (h.filter)(
        positions,
        Rc::new(move |p| {
            (ops.value.intrinsic)(
                "greater-than",
                vec![(ops.value.get)(&p, "contracts")?, (ops.value.number)(0.)],
            )
        }),
    )?;
    if (h.value.truthy)(&(h.value.intrinsic)(
        "greater-than",
        vec![(h.value.get)(&open, "length")?, (h.value.number)(0.)],
    )?) {
        emit(&h, || {
            Ok((h.value.literal)(
                "Please close all positions before switching position mode",
            ))
        })?;
        return Ok((h.value.undefined)());
    }
    let changed = async {
        let value = call(&h, "this.xhr.post:ENDPOINTS.SET_POSITION_MODE", || {
            Ok(vec![(h.value.object)(vec![(
                "posMode",
                (h.value.literal)(if (h.value.truthy)(&hedged) {
                    "long_short_mode"
                } else {
                    "net_mode"
                }),
            )])])
        })?;
        let response = (h.value.await_value)(value).await?;
        let data = (h.value.get)(&response, "data")?;
        if (h.value.equal)(&(h.value.get)(&data, "msg")?, &(h.value.literal)("")) {
            let receiver = (h.value.capture)("this")?;
            let mode = (h.value.capture)(if (h.value.truthy)(&hedged) {
                "PositionMode.Hedged"
            } else {
                "PositionMode.OneWay"
            })?;
            (h.value.set)(&receiver, "_accountPositionMode", mode)?;
            let value = call(&h, "this.fetchPositions", || Ok(vec![]))?;
            let positions = (h.value.await_value)(value).await?;
            call(&h, "this.store.update", || {
                Ok(vec![(h.value.object)(vec![("positions", positions)])])
            })?;
        } else {
            return Err(new_error(&h, || (h.value.get)(&data, "msg"))?);
        }
        Ok((h.value.undefined)())
    }
    .await;
    match changed {
        Ok(value) => Ok(value),
        Err(error) => Err(recover_positions(h, &error).await?),
    }
}
