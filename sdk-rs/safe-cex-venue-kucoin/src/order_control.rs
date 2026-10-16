use crate::public_streams::StreamOps;
use std::rc::Rc;

pub struct OrderControlOps<V, E> {
    pub value: Rc<StreamOps<V, E>>,
    pub pair: Rc<dyn Fn(V) -> Result<(V, V), E>>,
    pub close_throw: Rc<dyn Fn(&V, E) -> E>,
    pub has: Rc<dyn Fn(&str, &V) -> Result<bool, E>>,
    pub boolean: Rc<dyn Fn(bool) -> V>,
    pub new_error: Rc<dyn Fn(V, V) -> Result<E, E>>,
    pub iterator: Rc<dyn Fn(V) -> Result<V, E>>,
    pub next: Rc<dyn Fn(&V) -> Result<Option<V>, E>>,
    pub some: Rc<dyn Fn(V, Rc<dyn Fn(V) -> Result<bool, E>>) -> Result<bool, E>>,
}
fn call<V, E>(
    h: &OrderControlOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.value.capture)(name)?;
    (h.value.invoke)(callee, args()?)
}
fn optional<V, E>(h: &OrderControlOps<V, E>, value: &V, key: &str) -> Result<V, E> {
    if (h.value.nullish)(value) {
        Ok((h.value.undefined)())
    } else {
        (h.value.get)(value, key)
    }
}
fn fallback<V, E>(
    h: &OrderControlOps<V, E>,
    value: V,
    next: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    if (h.value.truthy)(&value) {
        Ok(value)
    } else {
        next()
    }
}
fn message<V, E>(h: &OrderControlOps<V, E>, error: &E) -> Result<V, E> {
    let value = (h.value.error_value)(error);
    let response = optional(h, &value, "response")?;
    let data = optional(h, &response, "data")?;
    fallback(h, optional(h, &data, "msg")?, || {
        optional(h, &value, "message")
    })
}
fn emit<V, E>(
    h: &OrderControlOps<V, E>,
    error: Option<&E>,
    msg: impl FnOnce() -> Result<V, E>,
) -> Result<(), E> {
    call(h, "this.emitAndLogError", || {
        let mut args = Vec::new();
        if let Some(error) = error {
            args.push((h.value.error_value)(error));
        }
        args.push(msg()?);
        Ok(args)
    })?;
    Ok(())
}
fn error<V, E>(h: &OrderControlOps<V, E>, msg: impl FnOnce() -> Result<V, E>) -> Result<E, E> {
    let ctor = (h.value.capture)("Error")?;
    (h.new_error)(ctor, msg()?)
}
fn parent_id<V, E>(h: &OrderControlOps<V, E>, order: &V) -> Result<V, E> {
    (h.value.intrinsic)(
        "split-double-underscore-first",
        vec![(h.value.get)(order, "id")?],
    )
}
fn market<V, E>(h: &OrderControlOps<V, E>, symbol: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    let markets = (h.value.capture)("this.store.marketsBySymbol")?;
    (h.value.intrinsic)("get-index", vec![markets, symbol()?])
}
fn missing_market<V, E>(
    h: &OrderControlOps<V, E>,
    symbol: impl FnOnce() -> Result<V, E>,
) -> Result<E, E> {
    error(h, || {
        (h.value.intrinsic)("market-not-found-message", vec![symbol()?])
    })
}
fn bad_code<V, E>(h: &OrderControlOps<V, E>, data: &V) -> Result<bool, E> {
    Ok(!(h.value.equal)(
        &(h.value.get)(data, "code")?,
        &(h.value.literal)("200000"),
    ))
}
fn code_error<V, E>(h: &OrderControlOps<V, E>, data: &V, default: &str) -> Result<(), E> {
    if bad_code(h, data)? {
        return Err(error(h, || {
            fallback(h, (h.value.get)(data, "msg")?, || {
                Ok((h.value.literal)(default))
            })
        })?);
    }
    Ok(())
}
pub async fn cancel_all_orders<V: Clone, E>(h: &OrderControlOps<V, E>) -> Result<V, E> {
    let result = async {
        for (endpoint, default) in [
            ("ENDPOINTS.CANCEL_ALL_ORDERS", "Failed to cancel all orders"),
            (
                "ENDPOINTS.CANCEL_ALL_STOP_ORDERS",
                "Failed to cancel all stop orders",
            ),
        ] {
            let response = call(h, "this.unlimitedXHR.delete", || {
                Ok(vec![(h.value.capture)(endpoint)?])
            })?;
            let response = (h.value.await_value)(response).await?;
            let data = (h.value.get)(&response, "data")?;
            if bad_code(h, &data)? {
                emit(h, None, || {
                    fallback(h, (h.value.get)(&data, "msg")?, || {
                        Ok((h.value.literal)(default))
                    })
                })?;
            }
        }
        Ok((h.value.undefined)())
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            emit(h, None, || message(h, &e))?;
            Ok((h.value.undefined)())
        }
    }
}
pub async fn cancel_orders<V: Clone, E>(h: &OrderControlOps<V, E>, orders: V) -> Result<V, E> {
    let groups = call(h, "partition", || {
        Ok(vec![orders, (h.value.capture)("this.isVirtualTPSLOrder")?])
    })?;
    let (virtual_orders, other) = (h.pair)(groups)?;
    let returned = call(h, "this.cancelNormalOrders", || {
        Ok(vec![(h.value.intrinsic)(
            "spread-arrays",
            vec![other, virtual_orders],
        )?])
    })?;
    (h.value.await_value)(returned).await?;
    Ok((h.value.undefined)())
}
pub async fn cancel_normal_orders<V: Clone + 'static, E: 'static>(
    h: Rc<OrderControlOps<V, E>>,
    orders: V,
) -> Result<V, E> {
    let ops = h.clone();
    let orders = (h.value.map)(
        orders,
        Rc::new(move |order| {
            let object = (ops.value.intrinsic)("spread-object", vec![order.clone()])?;
            let id = parent_id(&ops, &order)?;
            (ops.value.intrinsic)(
                "define-field",
                vec![object.clone(), (ops.value.literal)("id"), id],
            )?;
            Ok(object)
        }),
    )?;
    let orders = call(&h, "uniqBy", || Ok(vec![orders, (h.value.literal)("id")]))?;
    let iterator = (h.iterator)(orders)?;
    while let Some(order) = (h.next)(&iterator)? {
        if let Err(e) = cancel_one(&h, order).await {
            return Err((h.close_throw)(&iterator, e));
        }
    }
    Ok((h.value.undefined)())
}

async fn cancel_one<V: Clone, E>(h: &OrderControlOps<V, E>, order: V) -> Result<(), E> {
    let result = async {
        let response = call(&h, "this.unlimitedXHR.delete", || {
            let prefix = (h.value.intrinsic)(
                "template-prefix",
                vec![(h.value.capture)("ENDPOINTS.CANCEL_ORDERS")?],
            )?;
            let path =
                (h.value.intrinsic)("concat-id", vec![prefix, (h.value.get)(&order, "id")?])?;
            let symbol = call(&h, "this.toApiSymbol", || {
                Ok(vec![(h.value.get)(&order, "symbol")?])
            })?;
            Ok(vec![
                path,
                (h.value.object)(vec![("params", (h.value.object)(vec![("symbol", symbol)]))]),
            ])
        })?;
        let response = (h.value.await_value)(response).await?;
        let data = (h.value.get)(&response, "data")?;
        if bad_code(&h, &data)? {
            emit(&h, None, || {
                fallback(&h, (h.value.get)(&data, "msg")?, || {
                    (h.value.intrinsic)("cancel-order-message", vec![(h.value.get)(&order, "id")?])
                })
            })?;
        }
        Ok::<(), E>(())
    }
    .await;
    if let Err(e) = result {
        let value = (h.value.error_value)(&e);
        let response = optional(&h, &value, "response")?;
        if (h.value.equal)(&optional(&h, &response, "status")?, &(h.value.number)(404.)) {
            let retried = async {
                let response = call(&h, "this.unlimitedXHR.delete", || {
                    let replace = (h.value.intrinsic)(
                        "capture-replace",
                        vec![(h.value.capture)("ENDPOINTS.CANCEL_ALGO_ORDER")?],
                    )?;
                    Ok(vec![(h.value.invoke)(
                        replace,
                        vec![(h.value.literal)("{orderId}"), (h.value.get)(&order, "id")?],
                    )?])
                })?;
                let response = (h.value.await_value)(response).await?;
                let data = (h.value.get)(&response, "data")?;
                Ok::<bool, E>(!bad_code(&h, &data)?)
            }
            .await;
            if matches!(retried, Ok(true)) {
                return Ok(());
            }
        }
        emit(&h, None, || message(&h, &e))?;
        return Err(e);
    }

    Ok(())
}
pub async fn cancel_symbol_orders<V: Clone, E>(
    h: &OrderControlOps<V, E>,
    symbol: V,
) -> Result<V, E> {
    let result = async {
        let response = call(h, "this.unlimitedXHR.delete", || {
            let endpoint = (h.value.capture)("ENDPOINTS.CANCEL_SYMBOL_ORDERS")?;
            let api = call(h, "this.toApiSymbol", || Ok(vec![symbol.clone()]))?;
            Ok(vec![
                endpoint,
                (h.value.object)(vec![("params", (h.value.object)(vec![("symbol", api)]))]),
            ])
        })?;
        let response = (h.value.await_value)(response).await?;
        let data = (h.value.get)(&response, "data")?;
        if bad_code(h, &data)? {
            emit(h, None, || {
                fallback(h, (h.value.get)(&data, "msg")?, || {
                    (h.value.intrinsic)("cancel-symbol-message", vec![symbol.clone()])
                })
            })?;
        }
        Ok((h.value.undefined)())
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            emit(h, None, || message(h, &e))?;
            Err(e)
        }
    }
}
pub async fn update_order<V: Clone + 'static, E: 'static>(
    h: Rc<OrderControlOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    let order = (h.value.get)(&opts, "order")?;
    let update = (h.value.get)(&opts, "update")?;
    let parent = parent_id(&h, &order)?;
    let orders = (h.value.capture)("this.store.orders")?;
    let ops = h.clone();
    let related = (h.value.filter)(
        orders,
        Rc::new(move |o| Ok((ops.value.equal)(&parent_id(&ops, &o)?, &parent))),
    )?;
    if (h.value.intrinsic)("greater-than-one", vec![(h.value.get)(&related, "length")?])
        .map(|v| (h.value.truthy)(&v))?
    {
        let returned = call(&h, "this.updateVirtualTpSlOrder", || {
            Ok(vec![
                (h.value.object)(vec![("order", order), ("update", update)]),
                related,
            ])
        })?;
        return (h.value.await_value)(returned).await;
    }
    if (h.value.truthy)(&call(&h, "this.isAlgoOrder", || Ok(vec![order.clone()]))?) {
        let returned = call(&h, "this.updateAlgoOrder", || {
            Ok(vec![(h.value.object)(vec![
                ("order", order),
                ("update", update),
            ])])
        })?;
        return (h.value.await_value)(returned).await;
    }
    replace_order(&h, order, update, false).await
}
pub async fn update_algo_order<V: Clone, E>(h: &OrderControlOps<V, E>, opts: V) -> Result<V, E> {
    let order = (h.value.get)(&opts, "order")?;
    let update = (h.value.get)(&opts, "update")?;
    replace_order(h, order, update, true).await
}
async fn replace_order<V: Clone, E>(
    h: &OrderControlOps<V, E>,
    order: V,
    update: V,
    algo: bool,
) -> Result<V, E> {
    let m = market(h, || (h.value.get)(&order, "symbol"))?;
    if !(h.value.truthy)(&m) {
        return Err(missing_market(h, || (h.value.get)(&order, "symbol"))?);
    }
    let result = async {
        let canceled = call(h, "this.cancelOrders", || {
            Ok(vec![(h.value.array)(vec![order.clone()])])
        })?;
        (h.value.await_value)(canceled).await?;
        let mut fields = vec![
            ("symbol", (h.value.get)(&order, "symbol")?),
            ("side", (h.value.get)(&order, "side")?),
            ("type", (h.value.get)(&order, "type")?),
        ];
        fields.push((
            "price",
            if (h.has)("price", &update)? {
                (h.value.get)(&update, "price")?
            } else {
                (h.value.get)(&order, "price")?
            },
        ));
        fields.push((
            "amount",
            if (h.has)("amount", &update)? {
                (h.value.get)(&update, "amount")?
            } else {
                call(h, "this.nativeAmountForCancelReplace", || {
                    Ok(vec![order.clone()])
                })?
            },
        ));
        if !algo {
            fields.push(("timeInForce", (h.value.get)(&order, "timeInForce")?));
        }
        fields.push((
            "reduceOnly",
            if algo {
                (h.boolean)(true)
            } else {
                (h.value.get)(&order, "reduceOnly")?
            },
        ));
        let placed = call(h, "this.placeOrder", || Ok(vec![(h.value.object)(fields)]))?;
        (h.value.await_value)(placed).await
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            emit(h, if algo { None } else { Some(&e) }, || message(h, &e))?;
            Err(e)
        }
    }
}
async fn refresh<V: Clone, E>(h: &OrderControlOps<V, E>, symbol: V) -> Result<(), E> {
    let value = call(h, "this.fetchPositions", || {
        Ok(vec![(h.value.array)(vec![symbol])])
    })?;
    let positions = (h.value.await_value)(value).await?;
    call(h, "this.store.addOrUpdatePositions", || Ok(vec![positions]))?;
    Ok(())
}
pub async fn set_leverage<V: Clone, E>(
    h: &OrderControlOps<V, E>,
    symbol: V,
    input: V,
) -> Result<V, E> {
    let m = market(h, || Ok(symbol.clone()))?;
    if !(h.value.truthy)(&m) {
        return Err(missing_market(h, || Ok(symbol.clone()))?);
    }
    let min = (h.value.capture)("Math.min")?;
    let lower = call(h, "Math.max", || {
        let limits = (h.value.get)(&m, "limits")?;
        let leverage = (h.value.get)(&limits, "leverage")?;
        Ok(vec![input, (h.value.get)(&leverage, "min")?])
    })?;
    let limits = (h.value.get)(&m, "limits")?;
    let range = (h.value.get)(&limits, "leverage")?;
    let leverage = (h.value.invoke)(min, vec![lower, (h.value.get)(&range, "max")?])?;
    let result = async {
        let margin = call(h,"this.getOrFetchMarginType",|| Ok(vec![symbol.clone()]))?;
        let margin = (h.value.await_value)(margin).await?;
        if !(h.value.equal)(&margin,&(h.value.capture)("MarginType.Cross")?) {
            return Err(error(h,|| Ok((h.value.literal)("KuCoin Futures only exposes cross-margin leverage changes through the current REST API")))?);
        }
        let value = call(h,"this.xhr.post",|| {
            let endpoint = (h.value.capture)("ENDPOINTS.SET_LEVERAGE")?;
            let id = (h.value.get)(&m,"id")?;
            let leverage = (h.value.intrinsic)("toString-method",vec![leverage])?;
            Ok(vec![endpoint,(h.value.object)(vec![("symbol",id),("leverage",leverage)])])
        })?;
        let response = (h.value.await_value)(value).await?;
        code_error(h,&(h.value.get)(&response,"data")?,"Failed to set leverage")?;
        refresh(h,symbol).await?;
        Ok((h.value.undefined)())
    }.await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            emit(h, Some(&e), || message(h, &e))?;
            Err(e)
        }
    }
}
pub async fn change_margin_type<V: Clone, E>(
    h: &OrderControlOps<V, E>,
    isolated: V,
    symbol: V,
) -> Result<V, E> {
    if !(h.value.truthy)(&symbol) {
        emit(h, None, || Ok((h.value.literal)("Symbol is required")))?;
        return Err(error(h, || Ok((h.value.literal)("Symbol is required")))?);
    }
    let result = async {
        let m = market(h, || Ok(symbol.clone()))?;
        if !(h.value.truthy)(&m) {
            return Err(missing_market(h, || Ok(symbol.clone()))?);
        }
        let value = call(h, "this.xhr.post", || {
            Ok(vec![
                (h.value.capture)("ENDPOINTS.SET_MARGIN_MODE")?,
                (h.value.object)(vec![
                    ("symbol", (h.value.get)(&m, "id")?),
                    (
                        "marginMode",
                        (h.value.literal)(if (h.value.truthy)(&isolated) {
                            "ISOLATED"
                        } else {
                            "CROSS"
                        }),
                    ),
                ]),
            ])
        })?;
        let response = (h.value.await_value)(value).await?;
        code_error(
            h,
            &(h.value.get)(&response, "data")?,
            "Failed to change margin type",
        )?;
        refresh(h, symbol).await?;
        Ok((h.value.undefined)())
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            emit(h, None, || message(h, &e))?;
            Err(e)
        }
    }
}
pub async fn change_position_mode<V: Clone + 'static, E: 'static>(
    h: Rc<OrderControlOps<V, E>>,
    hedged: V,
) -> Result<V, E> {
    let result = async {
        let value = call(&h, "this.fetchPositions", || Ok(vec![]))?;
        let positions = (h.value.await_value)(value).await?;
        let ops = h.clone();
        if (h.some)(
            positions,
            Rc::new(move |p| {
                Ok(!(ops.value.equal)(
                    &(ops.value.get)(&p, "contracts")?,
                    &(ops.value.number)(0.),
                ))
            }),
        )? {
            let msg = (h.value.literal)("Cannot update position mode with open positions");
            call(&h, "this.emitAndLogError", || {
                let e = error(&h, || Ok(msg.clone()))?;
                Ok(vec![(h.value.error_value)(&e), msg.clone()])
            })?;
            return Err(error(&h, || Ok(msg))?);
        }
        let mode = (h.value.literal)(if (h.value.truthy)(&hedged) { "1" } else { "0" });
        let value = call(&h, "this.xhr.post", || {
            Ok(vec![
                (h.value.capture)("ENDPOINTS.SET_POSITION_MODE")?,
                (h.value.object)(vec![("positionMode", mode)]),
            ])
        })?;
        let response = (h.value.await_value)(value).await?;
        code_error(
            &h,
            &(h.value.get)(&response, "data")?,
            "Failed to change position mode",
        )?;
        let value = call(&h, "this.fetchPositions", || Ok(vec![]))?;
        let positions = (h.value.await_value)(value).await?;
        call(&h, "this.store.update", || {
            Ok(vec![(h.value.object)(vec![("positions", positions)])])
        })?;
        Ok((h.value.undefined)())
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            emit(&h, Some(&e), || message(&h, &e))?;
            Err(e)
        }
    }
}
