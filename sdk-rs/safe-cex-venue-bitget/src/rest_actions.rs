//! REST amendment and account-setting bodies over supplied native operations.
//! Values retain aliases; started async work belongs to the runtime. Concrete
//! HTTP/Exchange assembly and JS intrinsic/receiver bindings remain separate.
use std::{future::Future, pin::Pin};
pub type RestActionFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
pub trait RestActionsHost {
    type Value: Clone;
    type Error;
    fn receiver(&self, path: &str) -> Result<Self::Value, Self::Error>;
    fn set_receiver(&self, path: &str, value: Self::Value) -> Result<(), Self::Error>;
    /// Capture a callable together with its native member owner before arguments.
    fn capture(&self, path: &str) -> Result<Self::Value, Self::Error>;
    fn capture_member(&self, value: &Self::Value, name: &str) -> Result<Self::Value, Self::Error>;
    fn invoke(
        &self,
        method: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn start(
        &self,
        method: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<RestActionFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn property(&self, value: &Self::Value, name: &str) -> Result<Self::Value, Self::Error>;
    fn key_get(&self, value: &Self::Value, key: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn has(&self, value: &Self::Value, key: &str) -> Result<bool, Self::Error>;
    fn assign(&self, value: &Self::Value, key: &str, field: Self::Value)
    -> Result<(), Self::Error>;
    fn define(&self, value: &Self::Value, key: &str, field: Self::Value)
    -> Result<(), Self::Error>;
    fn delete(&self, value: &Self::Value, key: &Self::Value) -> Result<(), Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn string(&self, value: &[u16]) -> Self::Value;
    fn undefined(&self) -> Self::Value;
    fn null(&self) -> Self::Value;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn object(&self, fields: Vec<(&str, Self::Value)>) -> Self::Value;
    fn filter<F>(
        &self,
        member: Self::Value,
        array: Self::Value,
        predicate: F,
    ) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value) -> Result<bool, Self::Error>;
    fn find<F>(
        &self,
        member: Self::Value,
        array: Self::Value,
        predicate: F,
    ) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value) -> Result<bool, Self::Error>;
    fn some<F>(
        &self,
        member: Self::Value,
        array: Self::Value,
        predicate: F,
    ) -> Result<bool, Self::Error>
    where
        F: FnMut(Self::Value) -> Result<bool, Self::Error>;
    /// Original operators / template interpolation, preserving coercions/errors.
    fn operation(&self, name: &str, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn new_error(
        &self,
        constructor: Self::Value,
        message: Self::Value,
    ) -> Result<Self::Error, Self::Error>;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
}
fn s<H: RestActionsHost>(h: &H, v: &str) -> H::Value {
    h.string(&v.encode_utf16().collect::<Vec<_>>())
}
fn p<H: RestActionsHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.property(v, k)
}
fn opt<H: RestActionsHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        p(h, v, k)
    }
}
fn call<H: RestActionsHost>(h: &H, path: &str, args: Vec<H::Value>) -> Result<H::Value, H::Error> {
    let f = h.capture(path)?;
    h.invoke(f, args)
}
fn member<H: RestActionsHost>(
    h: &H,
    v: &H::Value,
    name: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let f = h.capture_member(v, name)?;
    h.invoke(f, args)
}
fn fmt<H: RestActionsHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    h.operation("template", vec![v])
}
fn split_id<H: RestActionsHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    let parts = member(h, &v, "split", vec![s(h, "__")])?;
    h.key_get(&parts, &h.number(0.))
}
fn market<H: RestActionsHost>(
    h: &H,
    symbol: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    let markets = h.receiver("store.marketsBySymbol")?;
    h.key_get(&markets, &symbol()?)
}
fn missing<H: RestActionsHost>(
    h: &H,
    kind: &str,
    symbol: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Error, H::Error> {
    let constructor = h.capture("Error")?;
    let msg = h.operation("missing", vec![s(h, kind), symbol()?])?;
    h.new_error(constructor, msg)
}
fn adjusted<H: RestActionsHost>(
    h: &H,
    value: impl FnOnce() -> Result<H::Value, H::Error>,
    market: &H::Value,
    key: &str,
) -> Result<H::Value, H::Error> {
    let f = h.capture("adjust")?;
    let value = value()?;
    let precision = p(h, &p(h, market, "precision")?, key)?;
    fmt(h, h.invoke(f, vec![value, precision])?)
}
fn message<H: RestActionsHost>(h: &H, error: &H::Error) -> Result<H::Value, H::Error> {
    let v = h.error_value(error);
    let msg = opt(h, &opt(h, &opt(h, &v, "response")?, "data")?, "msg")?;
    if h.truthy(&msg) {
        Ok(msg)
    } else {
        opt(h, &v, "message")
    }
}
fn code_failed<H: RestActionsHost>(
    h: &H,
    data: &H::Value,
    optional: bool,
) -> Result<bool, H::Error> {
    let f = h.capture("parseInt")?;
    let code = if optional {
        opt(h, data, "code")?
    } else {
        p(h, data, "code")?
    };
    let code = if h.truthy(&code) { code } else { s(h, "0") };
    Ok(!h.equal(&h.invoke(f, vec![code])?, &h.number(0.)))
}
async fn post<H: RestActionsHost>(
    h: &H,
    path: &str,
    endpoint: &str,
    payload: H::Value,
) -> Result<H::Value, H::Error> {
    let f = h.capture(path)?;
    let endpoint = h.receiver(endpoint)?;
    let response = h.start(f, vec![endpoint, payload])?.await?;
    p(h, &response, "data")
}
fn greater_zero<H: RestActionsHost>(h: &H, v: H::Value) -> Result<bool, H::Error> {
    Ok(h.truthy(&h.operation("greater", vec![v, h.number(0.)])?))
}
// ts: BitgetExchange#tryAmendLimitOrder
pub async fn try_amend_limit_order<H: RestActionsHost>(
    h: &H,
    order: H::Value,
    update: H::Value,
) -> Result<H::Value, H::Error> {
    let m = market(h, || p(h, &order, "symbol"))?;
    if !h.truthy(&m) {
        return Ok(h.null());
    }
    let orders = h.receiver("store.orders")?;
    let some = h.capture_member(&orders, "some")?;
    if h.some(some, orders, |o| {
        let id = p(h, &o, "id")?;
        let f = h.capture_member(&id, "startsWith")?;
        let prefix = h.operation("suffix", vec![p(h, &order, "id")?, s(h, "__")])?;
        Ok(h.truthy(&h.invoke(f, vec![prefix])?))
    })? {
        return Ok(h.null());
    }
    let payload = h.object(vec![
        ("orderId", p(h, &order, "id")?),
        ("symbol", p(h, &order, "symbol")?),
        ("productType", h.receiver("apiProductType")?),
        ("newClientOid", call(h, "uuid", vec![])?),
    ]);
    let price = if h.has(&update, "price")? {
        p(h, &update, "price")?
    } else {
        p(h, &order, "price")?
    };
    let size = if h.has(&update, "amount")? {
        p(h, &update, "amount")?
    } else {
        p(h, &order, "amount")?
    };
    if !greater_zero(h, price.clone())? || !greater_zero(h, size.clone())? {
        return Ok(h.array(vec![]));
    }
    h.assign(
        &payload,
        "newPrice",
        adjusted(h, || Ok(price), &m, "price")?,
    )?;
    h.assign(&payload, "newSize", adjusted(h, || Ok(size), &m, "amount")?)?;
    let result: Result<H::Value, H::Error> = async {
        let data = post(h, "unlimitedXHR.post", "ENDPOINTS.UPDATE_ORDER", payload).await?;
        if code_failed(h, &data, true)? {
            return Ok(h.null());
        }
        call(h, "schedulePrivateStreamWriteCheck", vec![])?;
        let id = opt(h, &opt(h, &data, "data")?, "orderId")?;
        let id = if h.truthy(&id) {
            id
        } else {
            p(h, &order, "id")?
        };
        Ok(h.array(vec![id]))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(_) => Ok(h.null()),
    }
}
// ts: BitgetExchange.updateOrder
pub async fn update_order<H: RestActionsHost>(
    h: &H,
    order: H::Value,
    update: H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&call(h, "isVirtualTPSLOrder", vec![order.clone()])?) {
        let parent = split_id(h, p(h, &order, "id")?)?;
        let orders = h.receiver("store.orders")?;
        let f = h.capture_member(&orders, "filter")?;
        let related = h.filter(f, orders, |o| {
            Ok(h.equal(&split_id(h, p(h, &o, "id")?)?, &parent))
        })?;
        if h.truthy(&h.operation("greater", vec![p(h, &related, "length")?, h.number(1.)])?) {
            let f = h.capture("updateVirtualTpSlOrder")?;
            return h
                .start(
                    f,
                    vec![
                        h.object(vec![("order", order), ("update", update)]),
                        related,
                    ],
                )?
                .await;
        }
    }
    if h.truthy(&call(h, "isAlgoOrder", vec![order.clone()])?) {
        let f = h.capture("ensureAccountTypeDetected")?;
        h.start(f, vec![])?.await?;
        let method = if h.truthy(&h.receiver("usesUtaRouting")?) {
            "updateUtaStrategyOrder"
        } else {
            "updateAlgoOrder"
        };
        let f = h.capture(method)?;
        return h
            .start(
                f,
                vec![h.object(vec![("order", order), ("update", update)])],
            )?
            .await;
    }
    let f = h.capture("ensureAccountTypeDetected")?;
    h.start(f, vec![])?.await?;
    let f = h.capture("Boolean")?;
    let legacy = h.invoke(
        f,
        vec![opt(
            h,
            &p(h, &order, "info")?,
            "__tealstreetUseLegacyBitgetDragUpdate",
        )?],
    )?;
    if h.equal(&p(h, &order, "type")?, &h.receiver("OrderType.Limit")?)
        && !h.truthy(&legacy)
        && !h.truthy(&h.receiver("usesUtaRouting")?)
    {
        let f = h.capture("tryAmendLimitOrder")?;
        let amended = h.start(f, vec![order.clone(), update.clone()])?.await?;
        if !h.equal(&amended, &h.null()) {
            return Ok(amended);
        }
    }
    let new = h.object(vec![
        ("symbol", p(h, &order, "symbol")?),
        ("type", p(h, &order, "type")?),
        ("side", p(h, &order, "side")?),
        ("price", p(h, &order, "price")?),
        ("amount", p(h, &order, "amount")?),
        ("reduceOnly", {
            let v = p(h, &order, "reduceOnly")?;
            if h.truthy(&v) { v } else { h.boolean(false) }
        }),
        ("timeInForce", p(h, &order, "timeInForce")?),
    ]);
    for key in ["tpTriggerType", "slTriggerType"] {
        if h.has(&order, key)? {
            h.assign(&new, key, p(h, &order, key)?)?
        }
    }
    for key in ["price", "amount"] {
        if h.has(&update, key)? {
            h.assign(&new, key, p(h, &update, key)?)?
        }
    }
    let f = h.capture("cancelOrders")?;
    h.start(f, vec![h.array(vec![order])])?.await?;
    let f = h.capture("placeOrder")?;
    h.start(f, vec![new])?.await
}
async fn update_algo<H: RestActionsHost>(
    h: &H,
    order: H::Value,
    update: H::Value,
    inverse: bool,
) -> Result<H::Value, H::Error> {
    let m = market(h, || p(h, &order, "symbol"))?;
    if !h.truthy(&m) {
        return Err(missing(h, "Market", || p(h, &order, "symbol"))?);
    }
    let (id, coin) = if inverse {
        let f = h.capture("getMarginCoinForSymbol")?;
        let coin = h.invoke(f, vec![p(h, &order, "symbol")?])?;
        (p(h, &order, "id")?, coin)
    } else {
        (
            split_id(h, p(h, &order, "id")?)?,
            h.receiver("apiMarginCoin")?,
        )
    };
    let payload = h.object(vec![
        ("orderId", id),
        ("marginCoin", coin),
        ("symbol", p(h, &m, "id")?),
        ("productType", h.receiver("apiProductType")?),
    ]);
    if h.has(&update, "price")? {
        h.assign(
            &payload,
            "triggerPrice",
            adjusted(h, || p(h, &update, "price"), &m, "price")?,
        )?
    }
    let positive = if h.has(&update, "amount")? {
        let f = h.capture("Boolean")?;
        let b = h.invoke(f, vec![p(h, &update, "amount")?])?;
        h.truthy(&b) && {
            let v = h.operation("unaryPlus", vec![p(h, &update, "amount")?])?;
            greater_zero(h, v)?
        }
    } else {
        false
    };
    if positive {
        h.assign(
            &payload,
            "size",
            adjusted(h, || p(h, &update, "amount"), &m, "amount")?,
        )?
    } else if !inverse {
        let old = p(h, &order, "amount")?;
        if h.truthy(&old) && greater_zero(h, p(h, &order, "amount")?)? {
            h.assign(
                &payload,
                "size",
                adjusted(h, || p(h, &order, "amount"), &m, "amount")?,
            )?
        } else {
            h.assign(&payload, "size", s(h, ""))?
        }
    }
    if inverse && !h.truthy(&p(h, &payload, "newSize")?) {
        h.assign(&payload, "size", s(h, ""))?
    }
    let result: Result<H::Value, H::Error> = async {
        let data = post(
            h,
            "unlimitedXHR.post",
            "ENDPOINTS.UPDATE_ALGO_ORDER",
            payload,
        )
        .await?;
        let data = p(h, &data, "data")?;
        if !inverse && h.truthy(&opt(h, &data, "orderId")?) {
            call(h, "schedulePrivateStreamWriteCheck", vec![])?;
        }
        if h.truthy(&opt(h, &data, "orderId")?) {
            Ok(h.array(vec![p(h, &data, "orderId")?]))
        } else {
            Ok(h.array(vec![]))
        }
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let f = h.capture("emitAndLogError")?;
            h.invoke(f, vec![message(h, &e)?])?;
            Ok(h.array(vec![]))
        }
    }
}
// ts: BitgetExchange.updateAlgoOrder
pub async fn update_algo_order<H: RestActionsHost>(
    h: &H,
    order: H::Value,
    update: H::Value,
) -> Result<H::Value, H::Error> {
    update_algo(h, order, update, false).await
}
// ts: BitgetInverseExchange.updateAlgoOrder
pub async fn inverse_update_algo_order<H: RestActionsHost>(
    h: &H,
    order: H::Value,
    update: H::Value,
) -> Result<H::Value, H::Error> {
    update_algo(h, order, update, true).await
}
fn find_position<H: RestActionsHost>(
    h: &H,
    symbol: &H::Value,
    side: &H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(side) {
        let positions = h.receiver("store.positions")?;
        let f = h.capture_member(&positions, "find")?;
        let found = h.find(f, positions, |p0| {
            Ok(h.equal(&p(h, &p0, "symbol")?, symbol) && h.equal(&p(h, &p0, "side")?, side))
        })?;
        if !h.nullish(&found) {
            return Ok(found);
        }
    }
    let positions = h.receiver("store.positions")?;
    let f = h.capture_member(&positions, "find")?;
    h.find(
        f,
        positions,
        |p0| Ok(h.equal(&p(h, &p0, "symbol")?, symbol)),
    )
}
fn per_side<H: RestActionsHost>(h: &H, position: &H::Value) -> Result<bool, H::Error> {
    Ok(h.equal(
        &p(h, position, "positionMode")?,
        &h.receiver("PositionMode.Hedged")?,
    ) && h.equal(
        &p(h, position, "marginType")?,
        &h.receiver("MarginType.Isolated")?,
    ))
}
async fn set_leverage_body<H: RestActionsHost>(
    h: &H,
    symbol: H::Value,
    input: H::Value,
    side: H::Value,
    inverse: bool,
) -> Result<H::Value, H::Error> {
    let m = market(h, || Ok(symbol.clone()))?;
    let position = find_position(h, &symbol, &side)?;
    if !h.truthy(&m) {
        return Err(missing(h, "Market", || Ok(symbol))?);
    }
    if !h.truthy(&position) {
        return Err(missing(h, "Position", || Ok(symbol))?);
    }
    let coin = if inverse {
        let f = h.capture("getMarginCoinForSymbol")?;
        Some(h.invoke(f, vec![symbol.clone()])?)
    } else {
        None
    };
    let min = h.capture("Math.min")?;
    let max = h.capture("Math.max")?;
    let low = p(h, &p(h, &p(h, &m, "limits")?, "leverage")?, "min")?;
    let bounded = h.invoke(max, vec![input, low])?;
    let high = p(h, &p(h, &p(h, &m, "limits")?, "leverage")?, "max")?;
    let leverage = h.invoke(min, vec![bounded, high])?;
    if h.equal(&p(h, &position, "leverage")?, &leverage) {
        return Ok(h.undefined());
    }
    let result: Result<H::Value, H::Error> = async {
        let (per, use_v3) = if inverse {
            (false, false)
        } else {
            let f = h.capture("ensureAccountTypeDetected")?;
            h.start(f, vec![])?.await?;
            let per = per_side(h, &position)?;
            let uta = h.receiver("usesUtaRouting")?;
            (per, h.truthy(&uta) && per)
        };
        let f = h.capture("xhr.post")?;
        let endpoint = h.receiver(if use_v3 {
            "ENDPOINTS.SET_LEVERAGE_V3"
        } else {
            "ENDPOINTS.SET_LEVERAGE"
        })?;
        let payload = if use_v3 {
            h.object(vec![
                ("symbol", p(h, &m, "id")?),
                ("coin", h.receiver("apiMarginCoin")?),
                ("category", {
                    let v = h.receiver("apiProductType")?;
                    member(h, &v, "toUpperCase", vec![])?
                }),
                ("leverage", call(h, "String", vec![leverage.clone()])?),
                (
                    "posSide",
                    if h.equal(&side, &h.receiver("PositionSide.Short")?) {
                        s(h, "short")
                    } else {
                        s(h, "long")
                    },
                ),
            ])
        } else {
            let payload = h.object(vec![
                ("symbol", p(h, &m, "id")?),
                (
                    "marginCoin",
                    if let Some(v) = &coin {
                        v.clone()
                    } else {
                        h.receiver("apiMarginCoin")?
                    },
                ),
                ("productType", h.receiver("apiProductType")?),
                ("leverage", leverage.clone()),
            ]);
            let per = if inverse {
                h.equal(
                    &p(h, &position, "positionMode")?,
                    &h.receiver("PositionMode.Hedged")?,
                )
            } else {
                per
            };
            if per {
                h.define(
                    &payload,
                    "holdSide",
                    if h.equal(&side, &h.receiver("PositionSide.Short")?) {
                        s(h, "short")
                    } else {
                        s(h, "long")
                    },
                )?
            }
            payload
        };
        let response = h.start(f, vec![endpoint, payload])?.await?;
        let data = p(h, &response, "data")?;
        if code_failed(h, &data, false)? {
            let f = h.capture("Error")?;
            return Err(h.new_error(f, p(h, &data, "msg")?)?);
        }
        let f = h.capture("fetchPositions")?;
        let positions = h.start(f, vec![h.array(vec![symbol])])?.await?;
        call(h, "store.addOrUpdatePositions", vec![positions])?;
        Ok(h.undefined())
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let f = h.capture("emitAndLogError")?;
            h.invoke(f, vec![h.error_value(&e), message(h, &e)?])?;
            Err(e)
        }
    }
}
// ts: BitgetExchange.setLeverage
pub async fn set_leverage<H: RestActionsHost>(
    h: &H,
    symbol: H::Value,
    input: H::Value,
    side: H::Value,
) -> Result<H::Value, H::Error> {
    set_leverage_body(h, symbol, input, side, false).await
}
// ts: BitgetInverseExchange.setLeverage
pub async fn inverse_set_leverage<H: RestActionsHost>(
    h: &H,
    symbol: H::Value,
    input: H::Value,
    side: H::Value,
) -> Result<H::Value, H::Error> {
    set_leverage_body(h, symbol, input, side, true).await
}
fn clear_symbol<H: RestActionsHost>(h: &H, symbol: &H::Value) -> Result<(), H::Error> {
    h.delete(&h.receiver("_positioningCache")?, symbol)?;
    h.delete(&h.receiver("_positioningInflight")?, symbol)
}
async fn change_margin_body<H: RestActionsHost>(
    h: &H,
    isolated: H::Value,
    symbol: H::Value,
    inverse: bool,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&symbol) {
        call(h, "emitAndLogError", vec![s(h, "Symbol is required")])?;
        let f = h.capture("Error")?;
        return Err(h.new_error(f, s(h, "Symbol is required"))?);
    }
    let coin = if inverse {
        let f = h.capture("getMarginCoinForSymbol")?;
        Some(h.invoke(f, vec![symbol.clone()])?)
    } else {
        None
    };
    let result: Result<H::Value, H::Error> = async {
        if !inverse {
            let f = h.capture("ensureAccountTypeDetected")?;
            h.start(f, vec![])?.await?;
            if h.truthy(&h.receiver("usesUtaRouting")?) {
                let m = market(h, || Ok(symbol.clone()))?;
                if !h.truthy(&m) {
                    return Err(missing(h, "Market", || Ok(symbol.clone()))?);
                }
                let f = h.capture("fetchUtaPositionMetadata")?;
                let metadata = h.start(f, vec![h.array(vec![symbol.clone()])])?.await?;
                let metadata = h.operation("destructureFirst", vec![metadata])?;
                let lev = opt(h, &metadata, "leverage")?;
                let lev = if h.truthy(&lev) {
                    lev
                } else {
                    p(h, &p(h, &p(h, &m, "limits")?, "leverage")?, "min")?
                };
                let f = h.capture("xhr.post")?;
                let endpoint = h.receiver("ENDPOINTS.SET_LEVERAGE_V3")?;
                let payload = h.object(vec![
                    ("category", h.receiver("apiUtaCategory")?),
                    ("symbol", p(h, &m, "id")?),
                    ("leverage", call(h, "String", vec![lev])?),
                    (
                        "marginMode",
                        if h.truthy(&isolated) {
                            s(h, "isolated")
                        } else {
                            s(h, "crossed")
                        },
                    ),
                ]);
                let response = h.start(f, vec![endpoint, payload])?.await?;
                let data = p(h, &response, "data")?;
                let f = h.capture("isBitgetSuccessCode")?;
                if !h.truthy(&h.invoke(f, vec![p(h, &data, "code")?])?) {
                    let f = h.capture("Error")?;
                    return Err(h.new_error(f, p(h, &data, "msg")?)?);
                }
                clear_symbol(h, &symbol)?;
                let f = h.capture("fetchPositions")?;
                let positions = h.start(f, vec![h.array(vec![symbol.clone()])])?.await?;
                call(h, "store.addOrUpdatePositions", vec![positions])?;
                return Ok(h.undefined());
            }
        }
        let f = h.capture("xhr.post")?;
        let endpoint = h.receiver("ENDPOINTS.SET_MARGIN_MODE")?;
        let payload = h.object(vec![
            ("productType", h.receiver("apiProductType")?),
            (
                "marginCoin",
                if let Some(v) = coin {
                    v
                } else {
                    h.receiver("apiMarginCoin")?
                },
            ),
            (
                "marginMode",
                if h.truthy(&isolated) {
                    s(h, "isolated")
                } else {
                    s(h, "crossed")
                },
            ),
            ("symbol", symbol.clone()),
        ]);
        let response = h.start(f, vec![endpoint, payload])?.await?;
        let data = p(h, &response, "data")?;
        if code_failed(h, &data, false)? {
            let f = h.capture("Error")?;
            return Err(h.new_error(f, p(h, &data, "msg")?)?);
        }
        if !inverse {
            clear_symbol(h, &symbol)?
        }
        let f = h.capture("fetchPositions")?;
        let positions = h.start(f, vec![h.array(vec![symbol])])?.await?;
        call(h, "store.addOrUpdatePositions", vec![positions])?;
        Ok(h.undefined())
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let f = h.capture("emitAndLogError")?;
            h.invoke(f, vec![message(h, &e)?])?;
            Err(e)
        }
    }
}
// ts: BitgetExchange.changeMarginType
pub async fn change_margin_type<H: RestActionsHost>(
    h: &H,
    isolated: H::Value,
    symbol: H::Value,
) -> Result<H::Value, H::Error> {
    change_margin_body(h, isolated, symbol, false).await
}
// ts: BitgetInverseExchange.changeMarginType
pub async fn inverse_change_margin_type<H: RestActionsHost>(
    h: &H,
    isolated: H::Value,
    symbol: H::Value,
) -> Result<H::Value, H::Error> {
    change_margin_body(h, isolated, symbol, true).await
}
// ts: BitgetExchange.changePositionMode
pub async fn change_position_mode<H: RestActionsHost>(
    h: &H,
    hedged: H::Value,
) -> Result<H::Value, H::Error> {
    let result: Result<H::Value, H::Error> = async {
        let f = h.capture("ensureAccountTypeDetected")?;
        h.start(f, vec![])?.await?;
        let uta = h.truthy(&h.receiver("usesUtaRouting")?);
        let f = h.capture("xhr.post")?;
        let endpoint = h.receiver(if uta {
            "ENDPOINTS.UTA_SET_HOLD_MODE"
        } else {
            "ENDPOINTS.SET_POSITION_MODE"
        })?;
        let mode = if h.truthy(&hedged) {
            s(h, "hedge_mode")
        } else {
            s(h, "one_way_mode")
        };
        let payload = if uta {
            h.object(vec![("holdMode", mode)])
        } else {
            h.object(vec![
                ("productType", h.receiver("apiProductType")?),
                ("posMode", mode),
            ])
        };
        let response = h.start(f, vec![endpoint, payload])?.await?;
        let data = p(h, &response, "data")?;
        let failed = if uta {
            let f = h.capture("isBitgetSuccessCode")?;
            !h.truthy(&h.invoke(f, vec![p(h, &data, "code")?])?)
        } else {
            code_failed(h, &data, false)?
        };
        if failed {
            let f = h.capture("Error")?;
            return Err(h.new_error(f, p(h, &data, "msg")?)?);
        }
        h.set_receiver("_positioningCache", h.object(vec![]))?;
        h.set_receiver("_positioningInflight", h.object(vec![]))?;
        let f = h.capture("fetchPositions")?;
        let positions = h.start(f, vec![])?.await?;
        let f = h.capture("store.update")?;
        h.invoke(f, vec![h.object(vec![("positions", positions)])])?;
        Ok(h.undefined())
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let f = h.capture("emitAndLogError")?;
            h.invoke(f, vec![h.error_value(&e), message(h, &e)?])?;
            Err(e)
        }
    }
}
