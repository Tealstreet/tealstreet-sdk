//! Placement/routing and strategy-cancel bodies over supplied native operations.
//! Captured values retain aliases and method owners; the runtime owns started work
//! and native promise/array/imported-operation behavior. Assembly remains separate.
use crate::rest_actions::RestActionsHost;
pub trait OrderPlacementHost: RestActionsHost {
    type Iterator;
    /// Array-literal spread: native iterator and own element definitions, no push lookup.
    fn spread_into(&self, array: &Self::Value, elements: Self::Value) -> Result<(), Self::Error>;
    fn iterator(&self, value: &Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next(&self, iterator: &mut Self::Iterator) -> Result<Option<Self::Value>, Self::Error>;
    /// Native two-element iterator destructuring, including IteratorClose.
    fn pair(&self, value: Self::Value) -> Result<(Self::Value, Self::Value), Self::Error>;
    fn map<F>(
        &self,
        member: Self::Value,
        array: Self::Value,
        callback: F,
    ) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value) -> Result<Self::Value, Self::Error>;
    fn reduce<F>(
        &self,
        member: Self::Value,
        array: Self::Value,
        initial: Self::Value,
        callback: F,
    ) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value, Self::Value) -> Result<Self::Value, Self::Error>;
    fn for_each<F>(
        &self,
        member: Self::Value,
        array: Self::Value,
        callback: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(Self::Value) -> Result<(), Self::Error>;
    fn assign_key(
        &self,
        value: &Self::Value,
        key: &Self::Value,
        field: Self::Value,
    ) -> Result<(), Self::Error>;
}
fn s<H: OrderPlacementHost>(h: &H, v: &str) -> H::Value {
    h.string(&v.encode_utf16().collect::<Vec<_>>())
}
fn p<H: OrderPlacementHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.property(v, k)
}
fn opt<H: OrderPlacementHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        p(h, v, k)
    }
}
fn call<H: OrderPlacementHost>(
    h: &H,
    path: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let f = h.capture(path)?;
    h.invoke(f, args)
}
fn push<H: OrderPlacementHost>(
    h: &H,
    array: &H::Value,
    args: Vec<H::Value>,
) -> Result<(), H::Error> {
    let f = h.capture_member(array, "push")?;
    h.invoke(f, args)?;
    Ok(())
}
fn append<H: OrderPlacementHost>(h: &H, array: &H::Value, value: H::Value) -> Result<(), H::Error> {
    let f = h.capture_member(array, "push")?;
    let mut args = vec![];
    let mut i = h.iterator(&value)?;
    while let Some(v) = h.next(&mut i)? {
        args.push(v)
    }
    h.invoke(f, args)?;
    Ok(())
}
fn positive<H: OrderPlacementHost>(h: &H, v: H::Value) -> Result<bool, H::Error> {
    Ok(h.truthy(&h.operation("greater", vec![v, h.number(0.)])?))
}
fn error_message<H: OrderPlacementHost>(h: &H, e: &H::Error) -> Result<H::Value, H::Error> {
    let e = h.error_value(e);
    let msg = opt(h, &opt(h, &opt(h, &e, "response")?, "data")?, "msg")?;
    if h.truthy(&msg) {
        Ok(msg)
    } else {
        opt(h, &e, "message")
    }
}
fn emit_error<H: OrderPlacementHost>(h: &H, e: &H::Error, both: bool) -> Result<(), H::Error> {
    let f = h.capture("emitAndLogError")?;
    let mut args = vec![];
    if both {
        args.push(h.error_value(e))
    }
    args.push(error_message(h, e)?);
    h.invoke(f, args)?;
    Ok(())
}
fn reject_trailing<H: OrderPlacementHost>(h: &H) -> Result<H::Error, H::Error> {
    let msg = s(h, "Bitget UTA trailing-stop orders are not supported yet.");
    call(h, "emitAndLogError", vec![msg.clone()])?;
    let constructor = h.capture("Error")?;
    h.new_error(constructor, msg)
}
fn schedule<H: OrderPlacementHost>(h: &H, ids: &H::Value) -> Result<(), H::Error> {
    if positive(h, p(h, ids, "length")?)? {
        call(h, "schedulePrivateStreamWriteCheck", vec![])?;
    }
    Ok(())
}
// ts: BitgetExchange.placeOrder
pub async fn place_order<H: OrderPlacementHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    call(h, "resolveCallerClientOrderId", vec![opts.clone()])?;
    let f = h.capture("ensureAccountTypeDetected")?;
    h.start(f, vec![])?.await?;
    if h.truthy(&h.receiver("usesUtaRouting")?) {
        if h.truthy(&call(h, "isAlgoOrder", vec![opts.clone()])?) {
            if h.equal(
                &p(h, &opts, "type")?,
                &h.receiver("OrderType.TrailingStopLoss")?,
            ) {
                return Err(reject_trailing(h)?);
            }
            let place = h.capture("placeUtaStrategyOrders")?;
            let format = h.capture("formatUtaStrategyOrder")?;
            let payload = h.start(format, vec![opts])?.await?;
            return h.start(place, vec![h.array(vec![payload])])?.await;
        }
        let format = h.capture("formatUtaCreateOrder")?;
        let payloads = h.start(format, vec![opts])?.await?;
        let place = h.capture("placeUtaOrders")?;
        return h.start(place, vec![payloads])?.await;
    }
    if h.truthy(&call(h, "isAlgoOrder", vec![opts.clone()])?) {
        let f = h.capture("formatAlgoOrder")?;
        let payload = h.start(f, vec![opts])?.await?;
        let f = h.capture("placeAlgoOrders")?;
        return h.start(f, vec![h.array(vec![payload])])?.await;
    }
    let f = h.capture("formatCreateOrder")?;
    let payloads = h.start(f, vec![opts])?.await?;
    let f = h.capture("placeOrderBatch")?;
    h.start(f, vec![payloads])?.await
}
async fn format_many<H: OrderPlacementHost>(
    h: &H,
    orders: H::Value,
    method: &str,
) -> Result<H::Value, H::Error> {
    let all = h.capture("Promise.all")?;
    let map = h.capture_member(&orders, "map")?;
    let mapped = h.map(map, orders, |o| {
        let f = h.capture(method)?;
        h.invoke(f, vec![o])
    })?;
    h.start(all, vec![mapped])?.await
}
// ts: BitgetExchange.placeOrders
pub async fn place_orders<H: OrderPlacementHost>(
    h: &H,
    orders: H::Value,
) -> Result<H::Value, H::Error> {
    call(h, "assertCallerClientOrderIds", vec![orders.clone()])?;
    let f = h.capture("ensureAccountTypeDetected")?;
    h.start(f, vec![])?.await?;
    let partition = h.capture("partition")?;
    let predicate = h.receiver("isAlgoOrder")?;
    let partition = h.invoke(partition, vec![orders, predicate])?;
    let (algos, normals) = h.pair(partition)?;
    let uta = h.truthy(&h.receiver("usesUtaRouting")?);
    if uta {
        let f = h.capture_member(&algos, "find")?;
        let trailing = h.find(f, algos.clone(), |o| {
            Ok(h.equal(
                &p(h, &o, "type")?,
                &h.receiver("OrderType.TrailingStopLoss")?,
            ))
        })?;
        if h.truthy(&trailing) {
            return Err(reject_trailing(h)?);
        }
    }
    let formatted = format_many(
        h,
        normals,
        if uta {
            "formatUtaCreateOrder"
        } else {
            "formatCreateOrder"
        },
    )
    .await?;
    let flat = h.capture_member(&formatted, "flat")?;
    let normal = h.invoke(flat, vec![])?;
    let filter = h.capture_member(&normal, "filter")?;
    let normal = h.filter(filter, normal, |o| {
        let size = if uta {
            let f = h.capture("toFiniteNumber")?;
            h.invoke(f, vec![p(h, &o, "qty")?])?
        } else {
            p(h, &o, "size")?
        };
        positive(h, size)
    })?;
    let algo = format_many(
        h,
        algos,
        if uta {
            "formatUtaStrategyOrder"
        } else {
            "formatAlgoOrder"
        },
    )
    .await?;
    let out = h.array(vec![]);
    let f = h.capture(if uta {
        "placeUtaOrders"
    } else {
        "placeOrderBatch"
    })?;
    let normal = h.start(f, vec![normal])?.await?;
    h.spread_into(&out, normal)?;
    let f = h.capture(if uta {
        "placeUtaStrategyOrders"
    } else {
        "placeAlgoOrders"
    })?;
    let algo = h.start(f, vec![algo])?.await?;
    h.spread_into(&out, algo)?;
    Ok(out)
}
async fn batch<H: OrderPlacementHost>(
    h: &H,
    payloads: H::Value,
    inverse: bool,
) -> Result<H::Value, H::Error> {
    let ids = h.array(vec![]);
    let grouped = if inverse {
        let f = h.capture_member(&payloads, "reduce")?;
        let initial = h.object(vec![]);
        h.reduce(f, payloads, initial, |acc, payload| {
            let key = p(h, &payload, "symbol")?;
            if !h.truthy(&h.key_get(&acc, &key)?) {
                h.assign_key(&acc, &key, h.array(vec![]))?
            }
            let array = h.key_get(&acc, &key)?;
            let push = h.capture_member(&array, "push")?;
            h.invoke(push, vec![payload])?;
            Ok(acc)
        })?
    } else {
        call(h, "groupBy", vec![payloads, s(h, "symbol")])?
    };
    let entries = call(h, "Object.entries", vec![grouped])?;
    let mut groups = h.iterator(&entries)?;
    while let Some(entry) = h.next(&mut groups)? {
        let (symbol, orders) = h.pair(entry)?;
        let coin = if inverse {
            Some(call(h, "getMarginCoinForSymbol", vec![symbol.clone()])?)
        } else {
            None
        };
        let f = h.capture("getOrFetchMarginType")?;
        let margin = h.start(f, vec![symbol.clone()])?.await?;
        let lots = if inverse {
            let lots = h.array(vec![]);
            let mut i = h.number(0.);
            while h.truthy(&h.operation("less", vec![i.clone(), p(h, &orders, "length")?])?) {
                let push = h.capture_member(&lots, "push")?;
                let slice = h.capture_member(&orders, "slice")?;
                let end = h.operation("add", vec![i.clone(), h.number(50.)])?;
                let lot = h.invoke(slice, vec![i.clone(), end])?;
                h.invoke(push, vec![lot])?;
                i = h.operation("add", vec![i, h.number(50.)])?;
            }
            lots
        } else {
            call(h, "chunk", vec![orders, h.number(50.)])?
        };
        let mut iter = h.iterator(&lots)?;
        while let Some(lot) = h.next(&mut iter)? {
            let result: Result<(), H::Error> = async {
                let post = h.capture("unlimitedXHR.post")?;
                let endpoint = h.receiver("ENDPOINTS.BATCH_ORDERS")?;
                // Object literal fields are evaluated before orderList's callback/map.
                let symbol_value = symbol.clone();
                let product = h.receiver("apiProductType")?;
                let coin_value = if let Some(c) = &coin {
                    c.clone()
                } else {
                    h.receiver("apiMarginCoin")?
                };
                let margin_value = margin.clone();
                let map = h.capture_member(&lot, "map")?;
                let list = h.map(map, lot, |o| {
                    if inverse {
                        h.operation("restOmitSymbol", vec![o])
                    } else {
                        let f = h.capture("omit")?;
                        h.invoke(f, vec![o, s(h, "symbol")])
                    }
                })?;
                let payload = h.object(vec![
                    ("symbol", symbol_value),
                    ("productType", product),
                    ("marginCoin", coin_value),
                    ("marginMode", margin_value),
                    ("orderList", list),
                ]);
                let response = h.start(post, vec![endpoint, payload])?.await?;
                let data = p(h, &p(h, &response, "data")?, "data")?;
                let successes = opt(h, &data, "successList")?;
                let map = opt(h, &successes, "map")?;
                let result = if h.nullish(&map) {
                    h.undefined()
                } else {
                    h.map(map, successes, |obj| p(h, &obj, "orderId"))?
                };
                if h.truthy(&result) {
                    append(h, &ids, result)?;
                }
                let failures = opt(h, &data, "failureList")?;
                let each = opt(h, &failures, "forEach")?;
                if !h.nullish(&each) {
                    h.for_each(each, failures, |obj| {
                        let emit = h.capture("emitAndLogError")?;
                        h.invoke(emit, vec![p(h, &obj, "errorMsg")?])?;
                        let constructor = h.capture("Error")?;
                        Err(h.new_error(constructor, p(h, &obj, "errorMsg")?)?)
                    })?;
                }
                Ok(())
            }
            .await;
            if let Err(e) = result {
                emit_error(h, &e, false)?;
                return Err(e);
            }
        }
    }
    if !inverse {
        schedule(h, &ids)?
    }
    Ok(ids)
}
// ts: BitgetExchange.placeOrderBatch
pub async fn place_order_batch<H: OrderPlacementHost>(
    h: &H,
    payloads: H::Value,
) -> Result<H::Value, H::Error> {
    batch(h, payloads, false).await
}
// ts: BitgetInverseExchange.placeOrderBatch
pub async fn inverse_place_order_batch<H: OrderPlacementHost>(
    h: &H,
    payloads: H::Value,
) -> Result<H::Value, H::Error> {
    batch(h, payloads, true).await
}
async fn uta_place<H: OrderPlacementHost>(
    h: &H,
    payloads: H::Value,
    strategy: bool,
) -> Result<H::Value, H::Error> {
    let ids = h.array(vec![]);
    let mut i = h.iterator(&payloads)?;
    while let Some(payload) = h.next(&mut i)? {
        let result: Result<(), H::Error> = async {
            let f = h.capture("unlimitedXHR.post")?;
            let endpoint = h.receiver(if strategy {
                "ENDPOINTS.UTA_PLACE_STRATEGY_ORDER"
            } else {
                "ENDPOINTS.UTA_PLACE_ORDER"
            })?;
            let response = h.start(f, vec![endpoint, payload.clone()])?.await?;
            let data = p(h, &response, "data")?;
            let f = h.capture("isBitgetSuccessCode")?;
            if !h.truthy(&h.invoke(f, vec![opt(h, &data, "code")?])?) {
                let error = h.capture("Error")?;
                let msg = opt(h, &data, "msg")?;
                let msg = if h.truthy(&msg) {
                    msg
                } else {
                    s(
                        h,
                        if strategy {
                            "Bitget UTA place strategy order request failed"
                        } else {
                            "Bitget UTA place order request failed"
                        },
                    )
                };
                return Err(h.new_error(error, msg)?);
            }
            let id = opt(h, &opt(h, &data, "data")?, "orderId")?;
            let id = if h.truthy(&id) {
                id
            } else {
                let client = opt(h, &opt(h, &data, "data")?, "clientOid")?;
                if h.truthy(&client) {
                    client
                } else {
                    p(h, &payload, "clientOid")?
                }
            };
            if h.truthy(&id) {
                push(h, &ids, vec![id])?;
            }
            Ok(())
        }
        .await;
        if let Err(e) = result {
            emit_error(h, &e, false)?;
            return Err(e);
        }
    }
    schedule(h, &ids)?;
    Ok(ids)
}
// ts: BitgetExchange#placeUtaOrders
pub async fn place_uta_orders<H: OrderPlacementHost>(
    h: &H,
    payloads: H::Value,
) -> Result<H::Value, H::Error> {
    uta_place(h, payloads, false).await
}
// ts: BitgetExchange#placeUtaStrategyOrders
pub async fn place_uta_strategy_orders<H: OrderPlacementHost>(
    h: &H,
    payloads: H::Value,
) -> Result<H::Value, H::Error> {
    uta_place(h, payloads, true).await
}
// ts: BitgetExchange.placeAlgoOrders
pub async fn place_algo_orders<H: OrderPlacementHost>(
    h: &H,
    payloads: H::Value,
) -> Result<H::Value, H::Error> {
    let ids = h.array(vec![]);
    let mut i = h.iterator(&payloads)?;
    while let Some(payload) = h.next(&mut i)? {
        let result: Result<(), H::Error> = async {
            let endpoint = h.receiver(
                if h.equal(&p(h, &payload, "planType")?, &s(h, "track_plan")) {
                    "ENDPOINTS.PLACE_TRAILING_STOP_ORDER"
                } else {
                    "ENDPOINTS.PLACE_ALGO_ORDER"
                },
            )?;
            let f = h.capture("unlimitedXHR.post")?;
            let response = h.start(f, vec![endpoint, payload])?.await?;
            let data = p(h, &response, "data")?;
            let f = h.capture("parseInt")?;
            let code = p(h, &data, "code")?;
            let code = if h.truthy(&code) { code } else { s(h, "0") };
            if !h.equal(&h.invoke(f, vec![code])?, &h.number(0.)) {
                let f = h.capture("Error")?;
                return Err(h.new_error(f, p(h, &data, "msg")?)?);
            }
            if h.truthy(&opt(h, &opt(h, &data, "data")?, "orderId")?) {
                let f = h.capture_member(&ids, "push")?;
                let id = p(h, &p(h, &data, "data")?, "orderId")?;
                h.invoke(f, vec![id])?;
            }
            Ok(())
        }
        .await;
        if let Err(e) = result {
            emit_error(h, &e, true)?;
            return Err(e);
        }
    }
    schedule(h, &ids)?;
    Ok(ids)
}
fn not_found<H: OrderPlacementHost>(h: &H, msg: H::Value) -> Result<bool, H::Error> {
    Ok(h.truthy(&call(h, "isBitgetUtaStrategyOrderNotFound", vec![msg])?))
}
fn already_absent<H: OrderPlacementHost>(h: &H, order: &H::Value) -> Result<(), H::Error> {
    let f = h.capture("log")?;
    let msg = h.operation("absentMessage", vec![p(h, order, "id")?])?;
    h.invoke(f, vec![msg, h.receiver("LogSeverity.Debug")?])?;
    Ok(())
}
// ts: BitgetExchange#cancelUtaStrategyOrders
pub async fn cancel_uta_strategy_orders<H: OrderPlacementHost>(
    h: &H,
    orders: H::Value,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    let opts = if h.equal(&opts, &h.undefined()) {
        h.object(vec![])
    } else {
        opts
    };
    let treat = p(h, &opts, "treatNotFoundAsSuccess")?;
    let treat = if h.nullish(&treat) {
        h.boolean(true)
    } else {
        treat
    };
    let normalized = call(h, "uniqBy", vec![orders, s(h, "id")])?;
    let mut i = h.iterator(&normalized)?;
    while let Some(order) = h.next(&mut i)? {
        let result: Result<bool, H::Error> = async {
            let id = opt(h, &p(h, &order, "info")?, "orderId")?;
            let id = if h.truthy(&id) {
                id
            } else {
                if h.equal(
                    &opt(h, &p(h, &order, "info")?, "clientOid")?,
                    &p(h, &order, "id")?,
                ) {
                    h.undefined()
                } else {
                    p(h, &order, "id")?
                }
            };
            let client = opt(h, &p(h, &order, "info")?, "clientOid")?;
            let client = if h.truthy(&client) {
                client
            } else {
                if !h.truthy(&id) {
                    p(h, &order, "id")?
                } else {
                    h.undefined()
                }
            };
            let f = h.capture("unlimitedXHR.post")?;
            let endpoint = h.receiver("ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER")?;
            let omit = h.capture("omitUndefined")?;
            let payload = h.invoke(
                omit,
                vec![h.object(vec![("orderId", id), ("clientOid", client)])],
            )?;
            let response = h.start(f, vec![endpoint, payload])?.await?;
            let data = p(h, &response, "data")?;
            let success = h.capture("isBitgetSuccessCode")?;
            if !h.truthy(&h.invoke(success, vec![opt(h, &data, "code")?])?) {
                let f = h.capture("getBitgetApiErrorMessage")?;
                let msg = h.invoke(
                    f,
                    vec![
                        data,
                        s(h, "Bitget UTA cancel strategy order request failed"),
                    ],
                )?;
                if h.truthy(&treat) && not_found(h, msg.clone())? {
                    already_absent(h, &order)?;
                    return Ok(true);
                }
                let error = h.capture("Error")?;
                return Err(h.new_error(error, msg)?);
            }
            Ok(false)
        }
        .await;
        match result {
            Ok(true) => continue,
            Ok(false) => (),
            Err(e) => {
                let msg = error_message(h, &e)?;
                if h.truthy(&treat) && not_found(h, msg.clone())? {
                    already_absent(h, &order)?;
                    continue;
                }
                call(h, "emitAndLogError", vec![msg])?;
                return Err(e);
            }
        }
    }
    schedule(h, &normalized)?;
    Ok(h.undefined())
}
