use crate::order_control::OrderControlOps;
use std::rc::Rc;
pub struct OrderLifecycleOps<V, E> {
    pub base: Rc<OrderControlOps<V, E>>,
    pub member: Rc<dyn Fn(V, &str) -> Result<V, E>>,
    pub mapper: Rc<dyn Fn(Rc<dyn Fn(V) -> Result<V, E>>) -> V>,
}
fn call<V, E>(
    h: &OrderLifecycleOps<V, E>,
    path: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.base.value.capture)(path)?;
    (h.base.value.invoke)(callee, args()?)
}
fn member<V, E>(
    h: &OrderLifecycleOps<V, E>,
    value: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.member)(value, key)?;
    (h.base.value.invoke)(callee, args()?)
}
fn optional<V, E>(h: &OrderLifecycleOps<V, E>, value: &V, key: &str) -> Result<V, E> {
    if (h.base.value.nullish)(value) {
        Ok((h.base.value.undefined)())
    } else {
        (h.base.value.get)(value, key)
    }
}
fn or<V, E>(
    h: &OrderLifecycleOps<V, E>,
    value: V,
    next: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    if (h.base.value.truthy)(&value) {
        Ok(value)
    } else {
        next()
    }
}
fn intrinsic<V, E>(h: &OrderLifecycleOps<V, E>, name: &str, args: Vec<V>) -> Result<V, E> {
    (h.base.value.intrinsic)(name, args)
}
fn message<V, E>(h: &OrderLifecycleOps<V, E>, error: &E) -> Result<V, E> {
    let value = (h.base.value.error_value)(error);
    let response = optional(h, &value, "response")?;
    let data = optional(h, &response, "data")?;
    or(h, optional(h, &data, "msg")?, || {
        optional(h, &value, "message")
    })
}
fn error<V, E>(h: &OrderLifecycleOps<V, E>, msg: impl FnOnce() -> Result<V, E>) -> Result<E, E> {
    let ctor = (h.base.value.capture)("Error")?;
    (h.base.new_error)(ctor, msg()?)
}
fn row_float<V, E>(h: &OrderLifecycleOps<V, E>, row: &V, keys: &[&str]) -> Result<V, E> {
    call(h, "parseFloat", || {
        for key in keys {
            let value = (h.base.value.get)(row, key)?;
            if (h.base.value.truthy)(&value) {
                return Ok(vec![value]);
            }
        }
        Ok(vec![(h.base.value.literal)("0")])
    })
}
pub async fn place_order<V: Clone, E>(h: &OrderLifecycleOps<V, E>, opts: V) -> Result<V, E> {
    let v = &h.base.value;
    call(h, "this.resolveCallerClientOrderId", || {
        Ok(vec![opts.clone()])
    })?;
    if (v.truthy)(&call(h, "this.isAlgoOrder", || Ok(vec![opts.clone()]))?) {
        let payload = call(h, "this.formatAlgoOrder", || Ok(vec![opts]))?;
        return (v.await_value)(call(h, "this.placeAlgoOrders", || {
            Ok(vec![(v.array)(vec![payload])])
        })?)
        .await;
    }
    let payloads = (v.await_value)(call(h, "this.formatCreateOrder", || {
        Ok(vec![opts.clone()])
    })?)
    .await?;
    (v.await_value)(call(h, "this.placeOrderBatch", || {
        Ok(vec![
            payloads,
            (v.capture)(
                if (v.truthy)(&(v.get)(&opts, "stopLoss")?)
                    || (v.truthy)(&(v.get)(&opts, "takeProfit")?)
                {
                    "ENDPOINTS.PLACE_ALGO_ORDER"
                } else {
                    "ENDPOINTS.PLACE_ORDER"
                },
            )?,
        ])
    })?)
    .await
}
pub async fn place_order_batch<V: Clone, E>(
    h: &OrderLifecycleOps<V, E>,
    payloads: V,
    endpoint: V,
) -> Result<V, E> {
    let endpoint = if (h.base.value.equal)(&endpoint, &(h.base.value.undefined)()) {
        (h.base.value.capture)("ENDPOINTS.PLACE_ORDER")?
    } else {
        endpoint
    };
    batch(h, payloads, Some(endpoint)).await
}
pub async fn place_algo_orders<V: Clone, E>(
    h: &OrderLifecycleOps<V, E>,
    payloads: V,
) -> Result<V, E> {
    batch(h, payloads, None).await
}
async fn batch<V: Clone, E>(
    h: &OrderLifecycleOps<V, E>,
    payloads: V,
    endpoint: Option<V>,
) -> Result<V, E> {
    let v = &h.base.value;
    let ids = (v.array)(vec![]);
    let iterator = (h.base.iterator)(payloads)?;
    while let Some(payload) = (h.base.next)(&iterator)? {
        let result = async {
            let selected = match &endpoint {
                Some(endpoint) => endpoint.clone(),
                None => (v.capture)(if (v.truthy)(&(v.get)(&payload, "stop")?) {
                    "ENDPOINTS.PLACE_ORDER"
                } else {
                    "ENDPOINTS.PLACE_ALGO_ORDER"
                })?,
            };
            let body = (v.get)(
                &(v.await_value)(call(h, "this.unlimitedXHR.post", || {
                    Ok(vec![selected, payload])
                })?)
                .await?,
                "data",
            )?;
            if (v.truthy)(&optional(h, &body, "code")?)
                && !(v.equal)(&(v.get)(&body, "code")?, &(v.literal)("200000"))
            {
                return Err(error(h, || {
                    or(h, (v.get)(&body, "msg")?, || {
                        intrinsic(h, "rejected-message", vec![(v.get)(&body, "code")?])
                    })
                })?);
            }
            if (v.truthy)(&optional(h, &optional(h, &body, "data")?, "orderId")?) {
                member(h, ids.clone(), "push", || {
                    Ok(vec![(v.get)(&(v.get)(&body, "data")?, "orderId")?])
                })?;
            }
            Ok(())
        }
        .await;
        if let Err(original) = result {
            let emitted = call(h, "this.emitter.emit", || {
                Ok(vec![(v.literal)("error"), message(h, &original)?])
            });
            let error = match emitted {
                Ok(_) => original,
                Err(listener) => listener,
            };
            return Err((h.base.close_throw)(&iterator, error));
        }
    }
    Ok(ids)
}
pub async fn update_virtual_tp_sl_order<V: Clone + 'static, E: 'static>(
    h: Rc<OrderLifecycleOps<V, E>>,
    opts: V,
    related: V,
) -> Result<V, E> {
    let v = &h.base.value;
    let order = (v.get)(&opts, "order")?;
    let update = (v.get)(&opts, "update")?;
    let market = intrinsic(
        &h,
        "get-index",
        vec![
            (v.capture)("this.store.marketsBySymbol")?,
            (v.get)(&order, "symbol")?,
        ],
    )?;
    if !(v.truthy)(&market) {
        return Err(error(&h, || {
            intrinsic(
                &h,
                "market-not-found-message",
                vec![(v.get)(&order, "symbol")?],
            )
        })?);
    }
    let ops = h.clone();
    let parent = member(&h, related.clone(), "find", || {
        Ok(vec![(h.mapper)(Rc::new(move |o| {
            let id = (ops.base.value.get)(&o, "id")?;
            let included = member(&ops, id, "includes", || {
                Ok(vec![(ops.base.value.literal)("__")])
            })?;
            Ok((ops.base.boolean)(!(ops.base.value.truthy)(&included)))
        }))])
    })?;
    if !(v.truthy)(&parent) {
        call(&h, "this.emitAndLogError", || {
            Ok(vec![intrinsic(
                &h,
                "parent-not-found-message",
                vec![(v.get)(&order, "id")?],
            )?])
        })?;
        return Err(error(&h, || {
            intrinsic(&h, "parent-not-found-message", vec![(v.get)(&order, "id")?])
        })?);
    }
    let mut siblings = vec![];
    for suffix in ["__stop_loss", "__take_profit"] {
        let ops = h.clone();
        siblings.push(member(&h, related.clone(), "find", || {
            Ok(vec![(h.mapper)(Rc::new(move |o| {
                let id = (ops.base.value.get)(&o, "id")?;
                member(&ops, id, "endsWith", || {
                    Ok(vec![(ops.base.value.literal)(suffix)])
                })
            }))])
        })?);
    }
    let result = async {
        (v.await_value)(call(&h, "this.cancelOrders", || {
            Ok(vec![(v.array)(vec![parent.clone()])])
        })?)
        .await?;
        let mut price = (v.get)(&parent, "price")?;
        let mut amount = call(&h, "this.nativeAmountForCancelReplace", || {
            Ok(vec![parent.clone()])
        })?;
        let mut sl = optional(&h, &siblings[0], "price")?;
        let mut tp = optional(&h, &siblings[1], "price")?;
        if (v.equal)(&(v.get)(&order, "id")?, &(v.get)(&parent, "id")?) {
            if (h.base.has)("price", &update)? {
                price = (v.get)(&update, "price")?;
            }
            if (h.base.has)("amount", &update)? {
                amount = (v.get)(&update, "amount")?;
            }
        } else if (v.truthy)(&member(&h, (v.get)(&order, "id")?, "endsWith", || {
            Ok(vec![(v.literal)("__stop_loss")])
        })?) && (h.base.has)("price", &update)?
        {
            sl = (v.get)(&update, "price")?;
        } else if (v.truthy)(&member(&h, (v.get)(&order, "id")?, "endsWith", || {
            Ok(vec![(v.literal)("__take_profit")])
        })?) && (h.base.has)("price", &update)?
        {
            tp = (v.get)(&update, "price")?;
        }
        let payloads = (v.await_value)(call(&h, "this.formatCreateOrder", || {
            Ok(vec![(v.object)(vec![
                ("symbol", (v.get)(&parent, "symbol")?),
                ("side", (v.get)(&parent, "side")?),
                ("type", (v.get)(&parent, "type")?),
                ("price", price),
                ("amount", amount),
                ("timeInForce", (v.get)(&parent, "timeInForce")?),
                ("reduceOnly", (v.get)(&parent, "reduceOnly")?),
                ("stopLoss", sl),
                ("takeProfit", tp),
            ])])
        })?)
        .await?;
        (v.await_value)(call(&h, "this.placeOrderBatch", || {
            Ok(vec![payloads, (v.capture)("ENDPOINTS.PLACE_ALGO_ORDER")?])
        })?)
        .await
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            call(&h, "this.emitAndLogError", || {
                Ok(vec![(v.error_value)(&e), message(&h, &e)?])
            })?;
            Err(e)
        }
    }
}
pub fn handle_futures_order_topic<V: Clone + 'static, E: 'static>(
    h: Rc<OrderLifecycleOps<V, E>>,
    data: V,
) -> Result<V, E> {
    let v = &h.base.value;
    if !(v.truthy)(&(v.get)(&data, "data")?) {
        return Ok((v.undefined)());
    }
    let raw = (v.get)(&data, "data")?;
    let orders = call(&h, "this.parent.mapOrder", || Ok(vec![raw.clone()]))?;
    let parent = (v.get)(&orders, "0")?;
    if !(v.truthy)(&parent) {
        return Ok((v.undefined)());
    }
    let event = (v.get)(&raw, "type")?;
    let status = (v.get)(&raw, "status")?;
    let fill = row_float(&h, &raw, &["matchSize"])?;
    let market = intrinsic(
        &h,
        "get-index",
        vec![
            (v.capture)("this.parent.store.marketsBySymbol")?,
            (v.get)(&parent, "symbol")?,
        ],
    )?;
    let size = or(&h, optional(&h, &market, "contractSize")?, || {
        Ok((v.number)(1.))
    })?;
    if (v.truthy)(&intrinsic(&h, "greater-zero", vec![fill.clone()])?) {
        call(&h, "this.emitter.emit", || {
            Ok(vec![
                (v.literal)("fill"),
                (v.object)(vec![
                    ("id", (v.get)(&parent, "id")?),
                    ("side", (v.get)(&parent, "side")?),
                    ("symbol", (v.get)(&parent, "symbol")?),
                    ("price", row_float(&h, &raw, &["matchPrice", "price"])?),
                    ("amount", intrinsic(&h, "multiply", vec![fill, size])?),
                ]),
            ])
        })?;
    }
    let canceled = (v.equal)(&event, &(v.literal)("canceled"))
        || (v.equal)(&event, &(v.literal)("cancel"))
        || (v.equal)(&status, &(v.literal)("canceled"))
        || (v.equal)(&status, &(v.literal)("cancelled"));
    let terminal = if canceled {
        Some("OrderStatus.Canceled")
    } else if (v.equal)(&event, &(v.literal)("filled"))
        || ((v.equal)(&status, &(v.literal)("done"))
            && (v.equal)(&row_float(&h, &raw, &["remainSize"])?, &(v.number)(0.)))
    {
        Some("OrderStatus.Closed")
    } else {
        None
    };
    let add = terminal.is_none()
        && ((v.equal)(&status, &(v.literal)("open"))
            || (v.equal)(&status, &(v.literal)("match"))
            || (v.equal)(&event, &(v.literal)("open"))
            || (v.equal)(&event, &(v.literal)("update"))
            || (v.equal)(&event, &(v.literal)("match")));
    if terminal.is_some() || add {
        let ops = h.clone();
        member(&h, orders, "forEach", || {
            Ok(vec![(h.mapper)(Rc::new(move |order| {
                if let Some(status) = terminal {
                    call(&ops, "this.store.updateOrderStatus", || {
                        Ok(vec![order, (ops.base.value.capture)(status)?])
                    })
                } else {
                    call(&ops, "this.store.addOrUpdateOrder", || Ok(vec![order]))
                }
            }))])
        })?;
    }
    Ok((v.undefined)())
}
pub fn handle_spot_order_topic<V: Clone, E>(
    h: &OrderLifecycleOps<V, E>,
    message: V,
) -> Result<V, E> {
    let v = &h.base.value;
    let raw = (v.get)(&message, "data")?;
    if !(v.truthy)(&optional(h, &raw, "orderId")?) {
        call(h, "this.refreshOrdersSoon", || Ok(vec![]))?;
        return Ok((v.undefined)());
    }
    let event = (v.get)(&raw, "type")?;
    let order = call(h, "this.parent.mapOrder", || {
        let mapped = (v.object)(vec![]);
        intrinsic(h, "spread-into", vec![mapped.clone(), raw.clone()])?;
        for (key, value) in [
            ("id", (v.get)(&raw, "orderId")?),
            ("orderId", (v.get)(&raw, "orderId")?),
            ("status", (v.get)(&raw, "status")?),
            (
                "type",
                or(h, (v.get)(&raw, "orderType")?, || {
                    (v.get)(&raw, "orderTypeV2")
                })?,
            ),
            (
                "price",
                or(h, (v.get)(&raw, "price")?, || {
                    or(h, (v.get)(&raw, "orderPrice")?, || {
                        (v.get)(&raw, "matchPrice")
                    })
                })?,
            ),
            (
                "size",
                or(h, (v.get)(&raw, "size")?, || (v.get)(&raw, "orderSize"))?,
            ),
            (
                "dealSize",
                or(h, (v.get)(&raw, "dealSize")?, || {
                    or(h, (v.get)(&raw, "filledSize")?, || {
                        (v.get)(&raw, "matchSize")
                    })
                })?,
            ),
            (
                "lastUpdatedAt",
                call(h, "normalizeMillis", || {
                    Ok(vec![or(h, (v.get)(&raw, "ts")?, || {
                        or(h, (v.get)(&raw, "orderTime")?, || {
                            (v.get)(&raw, "createdAt")
                        })
                    })?])
                })?,
            ),
        ] {
            intrinsic(
                h,
                "define-field",
                vec![mapped.clone(), (v.literal)(key), value],
            )?;
        }
        Ok(vec![mapped])
    })?;
    if (v.equal)(&event, &(v.literal)("match")) {
        call(h, "this.emitter.emit", || {
            Ok(vec![
                (v.literal)("fill"),
                (v.object)(vec![
                    ("id", (v.get)(&order, "id")?),
                    ("side", (v.get)(&order, "side")?),
                    ("symbol", (v.get)(&order, "symbol")?),
                    ("price", row_float(h, &raw, &["matchPrice", "price"])?),
                    ("amount", row_float(h, &raw, &["matchSize", "filledSize"])?),
                ]),
            ])
        })?;
    }
    if (v.equal)(&event, &(v.literal)("canceled")) || (v.equal)(&event, &(v.literal)("cancelled")) {
        call(h, "this.store.updateOrderStatus", || {
            Ok(vec![order, (v.capture)("OrderStatus.Canceled")?])
        })?;
    } else if (v.equal)(&event, &(v.literal)("filled")) {
        call(h, "this.store.updateOrderStatus", || {
            Ok(vec![order, (v.capture)("OrderStatus.Closed")?])
        })?;
    } else {
        call(h, "this.store.addOrUpdateOrder", || Ok(vec![order]))?;
    }
    call(h, "this.refreshOrdersSoon", || Ok(vec![]))?;
    Ok((v.undefined)())
}
