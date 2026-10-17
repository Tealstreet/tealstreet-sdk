//! Order workflow bodies over the existing bound exchange and task owners.
use crate::{
    exchange_runtime::ExchangeRuntimeHost,
    method_tasks::{MethodObserver, observe, start},
};
use std::rc::Rc;
pub type Arguments<V, E> = Rc<dyn Fn(Vec<V>) -> Result<V, E>>;
/// Imports are the existing source helpers/tables/constants, not method bodies.
/// Calls, array builtins, promise reactions and timers preserve native ownership.
pub trait OrderWorkflowHost: ExchangeRuntimeHost {
    fn import(&self, name: &str, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn constant(&self, name: &str) -> Self::Value;
    fn function_args(&self, callback: Arguments<Self::Value, Self::Error>) -> Self::Value;
    fn promise_value(&self, observer: MethodObserver<Self::Value, Self::Error>) -> Self::Value;
    fn add(&self, a: Self::Value, b: Self::Value) -> Result<Self::Value, Self::Error>;
    fn subtract(&self, a: Self::Value, b: Self::Value) -> Result<Self::Value, Self::Error>;
    fn less(&self, a: Self::Value, b: Self::Value) -> Result<bool, Self::Error>;
    fn is_object(&self, value: &Self::Value) -> bool;
    fn property_key(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn define_key(
        &self,
        object: &Self::Value,
        key: Self::Value,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn set_timeout(
        &self,
        callback: Rc<dyn Fn() -> Result<Self::Value, Self::Error>>,
        delay: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
}
pub(crate) fn t<H: OrderWorkflowHost>(h: &H, s: &str) -> H::Value {
    h.text(s.encode_utf16().collect())
}
pub(crate) fn f<H: OrderWorkflowHost>(h: &H, key: &str) -> Result<H::Value, H::Error> {
    h.field(&h.exchange(), key)
}
pub(crate) fn invoke<H: OrderWorkflowHost>(
    h: &H,
    receiver: H::Value,
    key: &str,
    args: impl FnOnce() -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let callee = h.field(&receiver, key)?;
    h.call(callee, receiver, args()?)
}
pub(crate) fn own<H: OrderWorkflowHost>(
    h: &H,
    key: &str,
    args: impl FnOnce() -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    invoke(h, h.exchange(), key, args)
}
pub(crate) fn opt<H: OrderWorkflowHost>(
    h: &H,
    v: H::Value,
    key: &str,
) -> Result<H::Value, H::Error> {
    if h.nullish(&v) {
        Ok(h.undefined())
    } else {
        h.field(&v, key)
    }
}
pub(crate) fn or<H: OrderWorkflowHost>(
    h: &H,
    a: H::Value,
    b: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if h.truthy(&a) { Ok(a) } else { b() }
}
pub(crate) fn nullish<H: OrderWorkflowHost>(
    h: &H,
    a: H::Value,
    b: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if h.nullish(&a) { b() } else { Ok(a) }
}
pub(crate) fn template<H: OrderWorkflowHost>(
    h: &H,
    prefix: &str,
    v: H::Value,
    suffix: &str,
) -> Result<H::Value, H::Error> {
    let mut s = prefix.encode_utf16().collect::<Vec<_>>();
    s.extend(h.abstract_string(&v)?);
    s.extend(suffix.encode_utf16());
    Ok(h.text(s))
}
pub(crate) async fn await_value<H: OrderWorkflowHost>(
    h: &H,
    v: Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    observe(h.resolve_return(v?)).await
}
pub(crate) fn market<H: OrderWorkflowHost>(h: &H, opts: &H::Value) -> Result<H::Value, H::Error> {
    let table = h.field(&f(h, "store")?, "marketsBySymbol")?;
    h.indexed(&table, &h.field(opts, "symbol")?)
}
fn volume<H: OrderWorkflowHost>(
    h: &H,
    opts: &H::Value,
    market: &H::Value,
) -> Result<H::Value, H::Error> {
    let step = h.import(
        "toNumber",
        vec![opt(h, h.field(market, "info")?, "volUnit")?, h.number(1.)],
    )?;
    let value = h.import("adjust", vec![h.field(opts, "amount")?, step.clone()])?;
    h.maximum(h.import("toNumber", vec![value, step.clone()])?, step)
}
fn post_only<H: OrderWorkflowHost>(h: &H, opts: &H::Value, id: H::Value) -> Result<(), H::Error> {
    if h.is_text(&h.field(opts, "timeInForce")?, "PostOnly") {
        invoke(h, f(h, "store")?, "updateOrder", || {
            Ok(vec![
                h.object(vec![("id", id)]),
                h.object(vec![("timeInForce", t(h, "PostOnly"))]),
            ])
        })?;
    }
    Ok(())
}
/// ts: MexcExchange#formatRegularOrderPayload.
pub fn format_regular_order_payload<H: OrderWorkflowHost>(
    h: Rc<H>,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    let market = market(h.as_ref(), &opts)?;
    if !h.truthy(&market) {
        let message = template(
            h.as_ref(),
            "Market ",
            h.field(&opts, "symbol")?,
            " not found",
        )?;
        own(h.as_ref(), "emitAndLogError", || Ok(vec![message.clone()]))?;
        return Err(h.new_error(h.abstract_string(&message)?));
    }
    let ty = if h.is_text(&h.field(&opts, "type")?, "limit")
        && h.truthy(&h.field(&opts, "timeInForce")?)
    {
        or(
            h.as_ref(),
            h.indexed(
                &h.constant("REVERSE_TIME_IN_FORCE"),
                &h.field(&opts, "timeInForce")?,
            )?,
            || h.indexed(&h.constant("REVERSE_ORDER_TYPE"), &h.field(&opts, "type")?),
        )?
    } else {
        h.indexed(&h.constant("REVERSE_ORDER_TYPE"), &h.field(&opts, "type")?)?
    };
    let vol = volume(h.as_ref(), &opts, &market)?;
    let side = if h.truthy(&h.field(&opts, "reduceOnly")?) {
        h.field(
            &h.indexed(&h.constant("REVERSE_ORDER_SIDE"), &h.field(&opts, "side")?)?,
            "close",
        )?
    } else {
        h.field(
            &h.indexed(&h.constant("REVERSE_ORDER_SIDE"), &h.field(&opts, "side")?)?,
            "open",
        )?
    };
    let leverage = own(h.as_ref(), "getLeverageForOrder", || Ok(vec![opts.clone()]))?;
    let symbol = h.field(&market, "id")?;
    let price = if h.is_text(&h.field(&opts, "type")?, "market") {
        h.undefined()
    } else {
        let price = or(h.as_ref(), h.field(&opts, "price")?, || Ok(h.number(0.)))?;
        template(
            h.as_ref(),
            "",
            h.import(
                "adjust",
                vec![price, h.field(&h.field(&market, "precision")?, "price")?],
            )?,
            "",
        )?
    };
    let settings = invoke(h.as_ref(), f(h.as_ref(), "symbolSettings")?, "get", || {
        Ok(vec![h.field(&opts, "symbol")?])
    })?;
    let margin = or(h.as_ref(), opt(h.as_ref(), settings, "marginType")?, || {
        Ok(t(h.as_ref(), "cross"))
    })?;
    let open_type = h.indexed(&h.constant("REVERSE_MARGIN_TYPE"), &margin)?;
    let leverage = h.maximum(
        leverage,
        h.field(&h.field(&h.field(&market, "limits")?, "leverage")?, "min")?,
    )?;
    let leverage = h.minimum(
        leverage,
        h.field(&h.field(&h.field(&market, "limits")?, "leverage")?, "max")?,
    )?;
    let external = template(h.as_ref(), "ts_", h.import("uuid", vec![])?, "")?;
    let reduce = or(h.as_ref(), h.field(&opts, "reduceOnly")?, || {
        Ok(h.undefined())
    })?;
    h.omit_undefined(h.object(vec![
        ("symbol", symbol),
        ("price", price),
        ("vol", vol),
        ("side", side),
        ("type", ty),
        ("openType", open_type),
        ("leverage", leverage),
        ("externalOid", external),
        ("reduceOnly", reduce),
    ]))
}
/// ts: MexcExchange#waitForRegularOrderSubmitSlot.
pub fn wait_for_regular_order_submit_slot<H: OrderWorkflowHost>(
    host: Rc<H>,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let now = h.import("now", vec![])?;
        let earliest = h.maximum(
            h.add(
                f(h.as_ref(), "lastRegularOrderSubmitAt")?,
                h.constant("MEXC_REGULAR_ORDER_SUBMIT_GAP_MS"),
            )?,
            f(h.as_ref(), "regularOrderSubmitCooldownUntil")?,
        )?;
        let delay = h.subtract(earliest, now)?;
        if h.less(h.number(0.), delay.clone())? {
            await_value(h.as_ref(), h.import("sleep", vec![delay])).await?;
        }
        h.set_field(
            &h.exchange(),
            "lastRegularOrderSubmitAt",
            h.import("now", vec![])?,
        )?;
        Ok(h.undefined())
    })
}
/// ts: MexcExchange#enqueueRegularOrderSubmit.
pub fn enqueue_regular_order_submit<H: OrderWorkflowHost>(
    host: Rc<H>,
    task: H::Value,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let undefined_host = h.clone();
        let clear = h.function(Rc::new(move |_| Ok(undefined_host.undefined())));
        let predecessor = invoke(
            h.as_ref(),
            f(h.as_ref(), "regularOrderSubmitQueue")?,
            "catch",
            || Ok(vec![clear]),
        )?;
        let run_host = h.clone();
        let callback = h.function(Rc::new(move |_| {
            let h = run_host.clone();
            let owner = h.clone();
            let task = task.clone();
            let observer = start(owner, async move {
                await_value(
                    h.as_ref(),
                    own(h.as_ref(), "waitForRegularOrderSubmitSlot", || Ok(vec![])),
                )
                .await?;
                match await_value(h.as_ref(), h.call(task, h.undefined(), vec![])).await {
                    Ok(value) => Ok(value),
                    Err(error) => {
                        own(h.as_ref(), "noteRegularOrderSubmitError", || {
                            Ok(vec![h.error_value(&error)])
                        })?;
                        Err(error)
                    }
                }
            });
            Ok(run_host.promise_value(observer))
        }));
        let run = invoke(h.as_ref(), predecessor, "then", || Ok(vec![callback]))?;
        let a = h.clone();
        let b = h.clone();
        let sentinel = invoke(h.as_ref(), run.clone(), "then", || {
            Ok(vec![
                h.function(Rc::new(move |_| Ok(a.undefined()))),
                h.function(Rc::new(move |_| Ok(b.undefined()))),
            ])
        })?;
        h.set_field(&h.exchange(), "regularOrderSubmitQueue", sentinel)?;
        Ok(run)
    })
}
/// ts: MexcExchange#addOptimisticRegularOrder.
pub fn add_optimistic_regular_order<H: OrderWorkflowHost>(
    h: Rc<H>,
    opts: H::Value,
    id: H::Value,
) -> Result<H::Value, H::Error> {
    if h.is_text(&h.field(&opts, "type")?, "market") {
        return Ok(h.undefined());
    }
    if h.truthy(&own(h.as_ref(), "isRecentlyRejectedRegularOrder", || {
        Ok(vec![id.clone()])
    })?) {
        return Ok(h.undefined());
    }
    let cb = h.clone();
    let order_id = id.clone();
    let predicate = h.function(Rc::new(move |order| {
        Ok(cb.boolean(cb.equal(&cb.field(&order, "id")?, &order_id)))
    }));
    let existing = invoke(
        h.as_ref(),
        h.field(&f(h.as_ref(), "store")?, "orders")?,
        "find",
        || Ok(vec![predicate]),
    )?;
    if h.truthy(&existing)
        && (h.is_text(&h.field(&existing, "status")?, "canceled")
            || h.is_text(&h.field(&existing, "status")?, "closed"))
    {
        return Ok(h.undefined());
    }
    let market = market(h.as_ref(), &opts)?;
    if !h.truthy(&market) {
        return Ok(h.undefined());
    }
    let vol = volume(h.as_ref(), &opts, &market)?;
    let amount = h.import("nativeVolumeToAmount", vec![vol.clone(), market.clone()])?;
    let price = or(h.as_ref(), h.field(&opts, "price")?, || Ok(h.number(0.)))?;
    let price = h.import(
        "toNumber",
        vec![h.import(
            "adjust",
            vec![price, h.field(&h.field(&market, "precision")?, "price")?],
        )?],
    )?;
    let store = f(h.as_ref(), "store")?;
    invoke(h.as_ref(), store, "addOrUpdateOrder", || {
        let symbol = h.field(&opts, "symbol")?;
        let ty = h.field(&opts, "type")?;
        let side = h.field(&opts, "side")?;
        let reduce = h.boolean(h.truthy(&h.field(&opts, "reduceOnly")?));
        let tif = or(h.as_ref(), h.field(&opts, "timeInForce")?, || {
            Ok(t(h.as_ref(), "GoodTillCancel"))
        })?;
        let now = h.import("now", vec![])?;
        let info = h.object(vec![
            ("__mexcOptimisticRegular", h.boolean(true)),
            ("symbol", h.field(&market, "id")?),
            ("vol", vol.clone()),
            ("price", template(h.as_ref(), "", price.clone(), "")?),
        ]);
        Ok(vec![h.object(vec![
            ("id", id),
            ("status", t(h.as_ref(), "open")),
            ("symbol", symbol),
            ("type", ty),
            ("side", side),
            ("price", price),
            ("amount", amount.clone()),
            ("filled", h.number(0.)),
            ("remaining", amount),
            ("reduceOnly", reduce),
            ("timeInForce", tif),
            ("lastUpdate", now),
            ("info", info),
            ("extra", h.object(vec![("remainingNative", vol)])),
        ])])
    })?;
    Ok(h.undefined())
}
/// ts: MexcExchange#addOptimisticStopOrder.
pub fn add_optimistic_stop_order<H: OrderWorkflowHost>(
    h: Rc<H>,
    opts: H::Value,
    market: H::Value,
    id: H::Value,
    trigger: H::Value,
    marker: H::Value,
) -> Result<H::Value, H::Error> {
    let native = h.import("toNumber", vec![h.field(&opts, "amount")?, h.number(0.)])?;
    let amount = if h.equal(&native, &h.number(0.)) {
        h.number(0.)
    } else {
        h.import("nativeVolumeToAmount", vec![native.clone(), market.clone()])?
    };
    invoke(
        h.as_ref(),
        f(h.as_ref(), "store")?,
        "addOrUpdateOrder",
        || {
            let symbol = h.field(&opts, "symbol")?;
            let ty = h.field(&opts, "type")?;
            let side = h.field(&opts, "side")?;
            let price = h.import(
                "toNumber",
                vec![h.import(
                    "adjust",
                    vec![trigger, h.field(&h.field(&market, "precision")?, "price")?],
                )?],
            )?;
            let now = h.import("now", vec![])?;
            let key = h.property_key(marker)?;
            let info = h.object(vec![]);
            h.define_key(&info, key, h.boolean(true))?;
            h.define_field(&info, "symbol", h.field(&market, "id")?)?;
            h.define_field(&info, "vol", native.clone())?;
            Ok(vec![h.object(vec![
                ("id", id.clone()),
                ("status", t(h.as_ref(), "open")),
                ("symbol", symbol),
                ("type", ty),
                ("side", side),
                ("price", price),
                ("amount", amount.clone()),
                ("filled", h.number(0.)),
                ("remaining", amount),
                ("reduceOnly", h.boolean(true)),
                ("timeInForce", t(h.as_ref(), "GoodTillCancel")),
                ("lastUpdate", now),
                ("info", info),
                ("extra", h.object(vec![("remainingNative", native)])),
            ])])
        },
    )?;
    own(h.as_ref(), "notePlacedStopLikeOrder", || Ok(vec![id]))?;
    Ok(h.undefined())
}
/// ts: MexcExchange#scheduleStopOrderSnapshotRefresh.
pub fn schedule_stop_order_snapshot_refresh<H: OrderWorkflowHost>(
    h: Rc<H>,
    symbol: H::Value,
) -> Result<H::Value, H::Error> {
    let existing = invoke(
        h.as_ref(),
        f(h.as_ref(), "pendingStopSnapshotRefreshTimers")?,
        "get",
        || Ok(vec![symbol.clone()]),
    )?;
    if h.truthy(&existing) {
        h.clear_timeout(existing)?;
    }
    let owner = h.clone();
    let key = symbol.clone();
    let timer = h.set_timeout(
        Rc::new(move || {
            invoke(
                owner.as_ref(),
                f(owner.as_ref(), "pendingStopSnapshotRefreshTimers")?,
                "delete",
                || Ok(vec![key.clone()]),
            )?;
            if owner.truthy(&f(owner.as_ref(), "isDisposed")?) {
                return Ok(owner.undefined());
            }
            let promise = own(owner.as_ref(), "fetchAndSetOrders", || {
                Ok(vec![owner.array(vec![key.clone()])])
            })?;
            let cb = owner.clone();
            let callback = owner.function(Rc::new(move |err| {
                own(cb.as_ref(), "log", || {
                    let response = opt(cb.as_ref(), err.clone(), "response")?;
                    let data = opt(cb.as_ref(), response, "data")?;
                    let message = or(cb.as_ref(), opt(cb.as_ref(), data, "message")?, || {
                        opt(cb.as_ref(), err, "message")
                    })?;
                    Ok(vec![
                        template(
                            cb.as_ref(),
                            "Failed to refresh MEXC TP/SL orders: ",
                            message,
                            "",
                        )?,
                        t(cb.as_ref(), "warning"),
                    ])
                })
            }));
            invoke(owner.as_ref(), promise, "catch", || Ok(vec![callback]))?;
            Ok(owner.undefined())
        }),
        h.constant("MEXC_STOP_SNAPSHOT_REFRESH_DELAY_MS"),
    )?;
    let unref = h.field(&timer, "unref")?;
    if !h.nullish(&unref) {
        h.call(unref, timer.clone(), vec![])?;
    }
    invoke(
        h.as_ref(),
        f(h.as_ref(), "pendingStopSnapshotRefreshTimers")?,
        "set",
        || Ok(vec![symbol, timer]),
    )?;
    Ok(h.undefined())
}
/// ts: MexcExchange.placeOrder.
pub fn place_order<H: OrderWorkflowHost>(
    host: Rc<H>,
    opts: H::Value,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        own(h.as_ref(), "resolveCallerClientOrderId", || {
            Ok(vec![opts.clone()])
        })?;
        if h.is_text(&h.field(&opts, "type")?, "trailing_stop_market") {
            let message = t(
                h.as_ref(),
                "MEXC trailing-stop order placement is not implemented yet",
            );
            own(h.as_ref(), "emitAndLogError", || Ok(vec![message.clone()]))?;
            return Err(h.new_error(h.abstract_string(&message)?));
        }
        if h.is_text(&h.field(&opts, "type")?, "stop_market")
            || h.is_text(&h.field(&opts, "type")?, "take_profit_market")
        {
            let id = await_value(
                h.as_ref(),
                own(h.as_ref(), "placePlanOrder", || Ok(vec![opts.clone()])),
            )
            .await?;
            return Ok(h.array(vec![id]));
        }
        let payload = own(h.as_ref(), "formatRegularOrderPayload", || {
            Ok(vec![opts.clone()])
        })?;
        let result: Result<H::Value, H::Error> = async {
            let cb = h.clone();
            let callback = h.function(Rc::new(move |_| {
                invoke(cb.as_ref(), f(cb.as_ref(), "xhr")?, "post", || {
                    Ok(vec![cb.endpoint("ORDER_SUBMIT"), payload.clone()])
                })
            }));
            let response = await_value(
                h.as_ref(),
                own(h.as_ref(), "enqueueRegularOrderSubmit", || {
                    Ok(vec![callback])
                }),
            )
            .await?;
            let data = h.field(&response, "data")?;
            own(h.as_ref(), "assertRegularOrderSubmitSucceeded", || {
                Ok(vec![data.clone()])
            })?;
            let inner = h.import("extractData", vec![data.clone()])?;
            let id = h.import(
                "toIntegerString",
                vec![nullish(
                    h.as_ref(),
                    opt(h.as_ref(), inner, "orderId")?,
                    || h.import("extractData", vec![data]),
                )?],
            )?;
            if !h.truthy(&id) {
                return Err(h.new_error(
                    "MEXC order placement returned no order id"
                        .encode_utf16()
                        .collect(),
                ));
            }
            own(h.as_ref(), "addOptimisticRegularOrder", || {
                Ok(vec![opts.clone(), id.clone()])
            })?;
            post_only(h.as_ref(), &opts, id.clone())?;
            Ok(h.array(vec![id]))
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(err) => {
                let value = h.error_value(&err);
                let message = or(
                    h.as_ref(),
                    opt(
                        h.as_ref(),
                        opt(
                            h.as_ref(),
                            opt(h.as_ref(), value.clone(), "response")?,
                            "data",
                        )?,
                        "message",
                    )?,
                    || {
                        or(
                            h.as_ref(),
                            opt(
                                h.as_ref(),
                                opt(
                                    h.as_ref(),
                                    opt(h.as_ref(), value.clone(), "response")?,
                                    "data",
                                )?,
                                "msg",
                            )?,
                            || {
                                or(h.as_ref(), opt(h.as_ref(), value, "message")?, || {
                                    Ok(t(h.as_ref(), "MEXC order placement failed"))
                                })
                            },
                        )
                    },
                )?;
                own(h.as_ref(), "emitAndLogError", || Ok(vec![message]))?;
                Err(err)
            }
        }
    })
}
/// ts: MexcExchange.placeOrders.
pub fn place_orders<H: OrderWorkflowHost>(
    host: Rc<H>,
    orders: H::Value,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        own(h.as_ref(), "assertCallerClientOrderIds", || {
            Ok(vec![orders.clone()])
        })?;
        if !h.truthy(&h.field(&orders, "length")?) {
            return Ok(h.array(vec![]));
        }
        let regular = invoke(h.as_ref(), orders.clone(), "every", || {
            Ok(vec![f(h.as_ref(), "isRegularPlaceOrder")?])
        })?;
        if h.truthy(&regular) {
            if !h.equal(
                &f(h.as_ref(), "nativeBatchOrdersAvailable")?,
                &h.boolean(false),
            ) {
                match await_value(
                    h.as_ref(),
                    own(h.as_ref(), "placeRegularOrdersBatch", || {
                        Ok(vec![orders.clone()])
                    }),
                )
                .await
                {
                    Ok(ids) => {
                        h.set_field(&h.exchange(), "nativeBatchOrdersAvailable", h.boolean(true))?;
                        return Ok(ids);
                    }
                    Err(err) => {
                        if !h.truthy(&own(h.as_ref(), "isNativeBatchAccessDeniedError", || {
                            Ok(vec![h.error_value(&err)])
                        })?) {
                            return Err(err);
                        }
                        h.set_field(
                            &h.exchange(),
                            "nativeBatchOrdersAvailable",
                            h.boolean(false),
                        )?;
                        own(h.as_ref(), "log", || {
                            Ok(vec![t(
                                h.as_ref(),
                                "MEXC native batch order placement is unavailable for this account; falling back to paced sequential placement.",
                            )])
                        })?;
                    }
                }
            }
            return await_value(
                h.as_ref(),
                own(h.as_ref(), "placeOrdersSequentially", || {
                    Ok(vec![orders, h.constant("MEXC_SCALED_ORDER_SUBMIT_GAP_MS")])
                }),
            )
            .await;
        }
        await_value(
            h.as_ref(),
            own(h.as_ref(), "placeOrdersSequentially", || Ok(vec![orders])),
        )
        .await
    })
}
/// ts: MexcExchange#placeOrdersSequentially.
pub fn place_orders_sequentially<H: OrderWorkflowHost>(
    host: Rc<H>,
    orders: H::Value,
    gap: H::Value,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let gap = if h.equal(&gap, &h.undefined()) {
            h.number(0.)
        } else {
            gap
        };
        let ids = h.array(vec![]);
        let mut index = 0usize;
        while h.less(h.number(index as f64), h.field(&orders, "length")?)? {
            let result: Result<(), H::Error> = async {
                let push = h.field(&ids, "push")?;
                let returned = await_value(
                    h.as_ref(),
                    own(h.as_ref(), "placeOrder", || {
                        Ok(vec![h.indexed(&orders, &h.number(index as f64))?])
                    }),
                )
                .await?;
                let mut values = vec![];
                h.for_of(returned, &mut |v| {
                    values.push(v);
                    Ok(false)
                })?;
                h.call(push, ids.clone(), values)?;
                if h.less(h.number(0.), gap.clone())?
                    && h.less(
                        h.number(index as f64),
                        h.subtract(h.field(&orders, "length")?, h.number(1.))?,
                    )?
                {
                    await_value(h.as_ref(), h.import("sleep", vec![gap.clone()])).await?;
                }
                Ok(())
            }
            .await;
            if let Err(err) = result {
                if h.less(h.number(0.), h.field(&ids, "length")?)? {
                    let value = h.error_value(&err);
                    if h.truthy(&value) && h.is_object(&value) {
                        let mut copy = vec![];
                        h.for_of(ids.clone(), &mut |v| {
                            copy.push(v);
                            Ok(false)
                        })?;
                        h.set_field(&value, "partialOrderIds", h.array(copy))?;
                        h.set_field(&value, "partialOrderCount", h.field(&ids, "length")?)?;
                        h.set_field(&value, "totalOrderCount", h.field(&orders, "length")?)?;
                        h.set_field(&value, "failedOrderIndex", h.number(index as f64))?;
                    }
                }
                return Err(err);
            }
            index += 1;
        }
        Ok(ids)
    })
}
/// ts: MexcExchange#placeRegularOrdersBatch.
pub fn place_regular_orders_batch<H: OrderWorkflowHost>(
    host: Rc<H>,
    orders: H::Value,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let cb = h.clone();
        let callback = h.function(Rc::new(move |order| {
            let result = own(cb.as_ref(), "formatRegularOrderPayload", || Ok(vec![order]))?;
            let result = cb.spread(&result)?;
            cb.define_field(&result, "stpMode", cb.number(0.))?;
            Ok(result)
        }));
        let payload = invoke(h.as_ref(), orders.clone(), "map", || Ok(vec![callback]))?;
        let response = await_value(
            h.as_ref(),
            invoke(h.as_ref(), f(h.as_ref(), "xhr")?, "post", || {
                Ok(vec![h.endpoint("ORDER_SUBMIT_BATCH"), payload])
            }),
        )
        .await?;
        let data = h.field(&response, "data")?;
        own(h.as_ref(), "assertRegularOrderBatchSucceeded", || {
            Ok(vec![data.clone(), h.field(&orders, "length")?])
        })?;
        let cb = h.clone();
        let rows = h.import("extractRows", vec![data])?;
        let ids = invoke(h.as_ref(), rows, "map", || {
            Ok(vec![h.function(Rc::new(move |row| {
                cb.import(
                    "toIntegerString",
                    vec![nullish(cb.as_ref(), cb.field(&row, "orderId")?, || {
                        cb.field(&row, "id")
                    })?],
                )
            }))])
        })?;
        let cb = h.clone();
        let ids = invoke(h.as_ref(), ids, "filter", || {
            Ok(vec![
                h.function(Rc::new(move |v| Ok(cb.boolean(cb.truthy(&v))))),
            ])
        })?;
        if !h.truthy(&h.field(&ids, "length")?) {
            return Err(h.new_error(
                "MEXC batch order placement returned no order ids"
                    .encode_utf16()
                    .collect(),
            ));
        }
        let cb = h.clone();
        let order_ids = ids.clone();
        let callback = h.function_args(Rc::new(move |args| {
            let order = args.first().cloned().unwrap_or_else(|| cb.undefined());
            let index = args.get(1).cloned().unwrap_or_else(|| cb.undefined());
            let id = cb.indexed(&order_ids, &index)?;
            if !cb.truthy(&id) {
                return Ok(cb.undefined());
            }
            own(cb.as_ref(), "addOptimisticRegularOrder", || {
                Ok(vec![order.clone(), id.clone()])
            })?;
            post_only(cb.as_ref(), &order, id)?;
            Ok(cb.undefined())
        }));
        invoke(h.as_ref(), orders, "forEach", || Ok(vec![callback]))?;
        Ok(ids)
    })
}
