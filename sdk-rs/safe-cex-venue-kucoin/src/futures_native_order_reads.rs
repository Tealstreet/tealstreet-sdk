use crate::{
    public_streams::StreamOps,
    spot_private_class::{PrivateClassOps, call, fallback, optional},
};
use std::rc::Rc;
fn member<V: Clone, E>(
    h: &StreamOps<V, E>,
    value: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let f = (h.get)(&value, key)?;
    (h.intrinsic)("call-with-this", vec![f, value, (h.array)(args()?)])
}
fn null<V, E>(h: &StreamOps<V, E>) -> Result<V, E> {
    (h.intrinsic)("null", vec![])
}
fn emit<V, E>(h: &StreamOps<V, E>, message: impl FnOnce() -> Result<V, E>) -> Result<(), E> {
    call(h, "this.emitAndLogError", || Ok(vec![message()?]))?;
    Ok(())
}
fn error_message<V, E>(h: &StreamOps<V, E>, error: &E) -> Result<V, E> {
    let error = (h.error_value)(error);
    let response = optional(h, &error, "response")?;
    let data = optional(h, &response, "data")?;
    fallback(h, optional(h, &data, "msg")?, || {
        optional(h, &error, "message")
    })
}
async fn fetch_book<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    algo: bool,
) -> Result<V, E> {
    let result = async {
        let response = call(&h, "this.xhr.get", || {
            Ok(vec![
                (h.capture)(if algo {
                    "ENDPOINTS.ALGO_ORDERS"
                } else {
                    "ENDPOINTS.ORDERS"
                })?,
                (h.object)(vec![(
                    "params",
                    (h.object)(vec![("status", (h.literal)("active"))]),
                )]),
            ])
        })?;
        let response = (h.await_value)(response).await?;
        let data = (h.intrinsic)("destructure-data", vec![response])?;
        if !(h.equal)(&(h.get)(&data, "code")?, &(h.literal)("200000")) {
            emit(&h, || {
                fallback(&h, (h.get)(&data, "msg")?, || {
                    Ok((h.literal)(if algo {
                        "Failed to fetch algo orders"
                    } else {
                        "Failed to fetch orders"
                    }))
                })
            })?;
            return null(&h);
        }
        let is_array = call(&h, "Array.isArray", || {
            Ok(vec![(h.get)(&(h.get)(&data, "data")?, "items")?])
        })?;
        if !(h.truthy)(&is_array) {
            return Ok((h.array)(vec![]));
        }
        let items = (h.get)(&(h.get)(&data, "data")?, "items")?;
        member(&h, items, "flatMap", || {
            Ok(vec![(h.get)(&(h.capture)("this")?, "mapOrder")?])
        })
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            emit(&h, || error_message(&h, &error))?;
            null(&h)
        }
    }
}
pub async fn fetch_normal_orders<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
) -> Result<V, E> {
    fetch_book(h, false).await
}
pub async fn fetch_algo_orders<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
) -> Result<V, E> {
    fetch_book(h, true).await
}
pub fn listen_ohlcv<V, E>(h: &StreamOps<V, E>, opts: V, callback: V) -> Result<V, E> {
    call(h, "this.publicWebsocket.listenOHLCV", || {
        Ok(vec![opts, callback])
    })
}
pub fn listen_order_book<V, E>(h: &StreamOps<V, E>, symbol: V, callback: V) -> Result<V, E> {
    call(h, "this.publicWebsocket.listenOrderBook", || {
        Ok(vec![symbol, callback])
    })
}
pub async fn fetch_orders<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    h: Rc<StreamOps<V, E>>,
) -> Result<V, E> {
    let normal = (h.await_value)(call(&h, "this.fetchNormalOrders", || Ok(vec![]))?).await?;
    let stop = (h.await_value)(call(&h, "this.fetchAlgoOrders", || Ok(vec![]))?).await?;
    let language = h.clone();
    let boolean = ops.boolean.clone();
    let predicate = (ops.arrow)(
        "isStopType",
        1,
        Rc::new(move |args| {
            let row = args
                .first()
                .cloned()
                .unwrap_or_else(|| (language.undefined)());
            for name in [
                "OrderType.StopLoss",
                "OrderType.TakeProfit",
                "OrderType.TrailingStopLoss",
            ] {
                if (language.equal)(&(language.get)(&row, "type")?, &(language.capture)(name)?) {
                    return Ok(boolean(true));
                }
            }
            Ok(boolean(false))
        }),
    );
    let normal = if (h.nullish)(&normal) {
        let orders = (h.capture)("this.store.orders")?;
        let language = h.clone();
        let inner = predicate.clone();
        let boolean = ops.boolean.clone();
        member(&h, orders, "filter", || {
            Ok(vec![(ops.arrow)(
                "",
                1,
                Rc::new(move |args| {
                    Ok(boolean(!(language.truthy)(&(language.invoke)(
                        inner.clone(),
                        vec![
                            args.first()
                                .cloned()
                                .unwrap_or_else(|| (language.undefined)()),
                        ],
                    )?)))
                }),
            )])
        })?
    } else {
        normal
    };
    let stop = if (h.nullish)(&stop) {
        let orders = (h.capture)("this.store.orders")?;
        member(&h, orders, "filter", || Ok(vec![predicate]))?
    } else {
        stop
    };
    call(&h, "this.reconcileFetchedOrderRemainders", || {
        let out = (h.array)(vec![]);
        (h.intrinsic)("spread-array-into", vec![out.clone(), normal])?;
        (h.intrinsic)("spread-array-into", vec![out.clone(), stop])?;
        Ok(vec![out])
    })
}
pub fn reconcile_fetched_order_remainders<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    construction: &crate::futures_public_class::ConstructionOps<V, E>,
    h: Rc<StreamOps<V, E>>,
    fetched: V,
) -> Result<V, E> {
    let map_constructor = (h.capture)("Map")?;
    let orders = (h.capture)("this.store.orders")?;
    let entries = member(&h, orders, "map", || {
        let language = h.clone();
        let to_entry = (ops.arrow)(
            "",
            1,
            Rc::new(move |args| {
                let order = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| (language.undefined)());
                Ok((language.array)(vec![(language.get)(&order, "id")?, order]))
            }),
        );
        Ok(vec![to_entry])
    })?;
    let existing = (construction.construct)(map_constructor, vec![entries])?;
    member(&h, fetched, "map", || {
        let language = h.clone();
        let reconcile = (ops.arrow)(
            "",
            1,
            Rc::new(move |args| {
                let order = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| (language.undefined)());
                let previous = member(&language, existing.clone(), "get", || {
                    Ok(vec![(language.get)(&order, "id")?])
                })?;
                let previous = optional(&language, &previous, "extra")?;
                let previous = optional(&language, &previous, "remainingNative")?;
                let current = optional(
                    &language,
                    &(language.get)(&order, "extra")?,
                    "remainingNative",
                )?;
                let finite = call(&language, "Number.isFinite", || Ok(vec![previous.clone()]))?;
                if !(language.truthy)(&finite)
                    || (language.truthy)(&(language.intrinsic)(
                        "lt",
                        vec![previous.clone(), (language.number)(0.)],
                    )?)
                {
                    return Ok(order);
                }
                let finite = call(&language, "Number.isFinite", || Ok(vec![current.clone()]))?;
                if !(language.truthy)(&finite)
                    || (language.truthy)(&(language.intrinsic)(
                        "lt",
                        vec![current.clone(), (language.number)(0.)],
                    )?)
                {
                    return Ok(order);
                }
                let remaining = call(&language, "Math.min", || {
                    Ok(vec![previous, current.clone()])
                })?;
                if (language.equal)(&remaining, &current) {
                    return Ok(order);
                }
                let out = (language.object)(vec![]);
                (language.intrinsic)("spread-into", vec![out.clone(), order.clone()])?;
                let base = call(&language, "multiply", || {
                    Ok(vec![
                        remaining.clone(),
                        call(&language, "this.contractSizeFor", || {
                            Ok(vec![(language.get)(&order, "symbol")?])
                        })?,
                    ])
                })?;
                (language.intrinsic)(
                    "define-field",
                    vec![out.clone(), (language.literal)("remaining"), base],
                )?;
                let extra = (language.object)(vec![]);
                (language.intrinsic)(
                    "spread-into",
                    vec![extra.clone(), (language.get)(&order, "extra")?],
                )?;
                (language.intrinsic)(
                    "define-field",
                    vec![
                        extra.clone(),
                        (language.literal)("remainingNative"),
                        remaining,
                    ],
                )?;
                (language.intrinsic)(
                    "define-field",
                    vec![out.clone(), (language.literal)("extra"), extra],
                )?;
                Ok(out)
            }),
        );
        Ok(vec![reconcile])
    })
}
