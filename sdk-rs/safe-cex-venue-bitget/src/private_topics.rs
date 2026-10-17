//! Eight synchronous private-topic bodies over supplied native receiver operations.
//! Clones retain native identity; primitives bind JS semantics, not JSON values.
//! Callees, array methods/iterators, and concrete WebSocket assembly are separate.
pub trait PrivateTopicsHost {
    type Value: Clone;
    type Error;
    type Iterator;
    fn property(&self, value: &Self::Value, name: &str) -> Result<Self::Value, Self::Error>;
    fn receiver(&self, name: &str) -> Result<Self::Value, Self::Error>;
    fn set_receiver(&self, name: &str, value: Self::Value) -> Result<(), Self::Error>;
    fn call(&self, name: &str, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn equal(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn is_object(&self, value: &Self::Value) -> bool;
    fn is_string(&self, value: &Self::Value) -> bool;
    fn undefined(&self) -> Self::Value;
    fn string(&self, value: &[u16]) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    /// Native array with preserved holes, distinct from present undefined.
    fn array(&self, values: Vec<Option<Self::Value>>) -> Self::Value;
    fn length(&self, array: &Self::Value) -> Result<usize, Self::Error>;
    fn has_index(&self, array: &Self::Value, index: usize) -> Result<bool, Self::Error>;
    fn index(&self, array: &Self::Value, index: usize) -> Result<Self::Value, Self::Error>;
    fn iterator(&self, value: &Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next(&self, iterator: &mut Self::Iterator) -> Result<Option<Self::Value>, Self::Error>;
    /// Create own data properties and copy enumerable own fields; no serialization.
    fn object(&self, fields: Vec<(&str, Self::Value)>) -> Self::Value;
    fn spread(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn define(
        &self,
        object: &Self::Value,
        name: &str,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn finite(&self, value: &Self::Value, fallback: f64) -> Result<f64, Self::Error>;
    fn parse_float(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn status_lookup(&self, normalized: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn account_upnl(&self, fallback: f64) -> Result<f64, Self::Error>;
    fn clock_now(&self) -> Result<f64, Self::Error>;
    fn set_of(&self, values: Self::Value) -> Result<Self::Value, Self::Error>;
    fn map_set(&self, map: Self::Value, key: Self::Value, value: f64) -> Result<(), Self::Error>;
}
fn s<H: PrivateTopicsHost>(h: &H, v: &str) -> H::Value {
    h.string(&v.encode_utf16().collect::<Vec<_>>())
}
fn prop<H: PrivateTopicsHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.property(v, k)
}
fn optional<H: PrivateTopicsHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        prop(h, v, k)
    }
}
fn is<H: PrivateTopicsHost>(h: &H, v: &H::Value, k: &str) -> bool {
    h.equal(v, &s(h, k))
}
fn or<H: PrivateTopicsHost>(
    h: &H,
    v: H::Value,
    next: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if h.truthy(&v) { Ok(v) } else { next() }
}
fn empty<H: PrivateTopicsHost>(h: &H) -> H::Value {
    h.array(vec![])
}
fn map<H: PrivateTopicsHost>(
    h: &H,
    array: &H::Value,
    mut f: impl FnMut(H::Value) -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    let len = h.length(array)?;
    let mut out = vec![None; len];
    for (i, slot) in out.iter_mut().enumerate() {
        if h.has_index(array, i)? {
            *slot = Some(f(h.index(array, i)?)?)
        }
    }
    Ok(h.array(out))
}
fn each<H: PrivateTopicsHost>(
    h: &H,
    array: &H::Value,
    mut f: impl FnMut(H::Value) -> Result<(), H::Error>,
) -> Result<(), H::Error> {
    let len = h.length(array)?;
    for i in 0..len {
        if h.has_index(array, i)? {
            f(h.index(array, i)?)?
        }
    }
    Ok(())
}
fn raw_list<H: PrivateTopicsHost>(h: &H, data: &H::Value) -> Result<H::Value, H::Error> {
    let first = optional(h, data, "data")?;
    if h.is_array(&first) {
        prop(h, data, "data")
    } else {
        Ok(empty(h))
    }
}
// ts: BitgetPrivateWebsocket#markWsPositions
pub fn mark_ws_positions<H: PrivateTopicsHost>(
    h: &H,
    positions: H::Value,
    action: H::Value,
) -> Result<(), H::Error> {
    let now = h.clock_now()?;
    if is(h, &action, "snapshot") {
        h.set_receiver("lastWsPositionSnapshotAt", h.number(now))?;
        let ids = map(h, &positions, |p| prop(h, &p, "id"))?;
        h.set_receiver("lastWsPositionSnapshotIds", h.set_of(ids)?)?;
    }
    let mut iterator = h.iterator(&positions)?;
    while let Some(p) = h.next(&mut iterator)? {
        let map = h.receiver("wsPositionUpdatedAtById")?;
        h.map_set(map, prop(h, &p, "id")?, now)?
    }
    Ok(())
}
// ts: BitgetPrivateWebsocket.handleOrderTopic
pub fn handle_order_topic<H: PrivateTopicsHost>(h: &H, data: H::Value) -> Result<(), H::Error> {
    h.call("parent.markPrivateStreamMessageReceived", vec![])?;
    let raw = prop(h, &data, "data")?;
    let mut iter = h.iterator(&raw)?;
    while let Some(raw) = h.next(&mut iter)? {
        let status = h.call("getRawOrderStatus", vec![raw.clone()])?;
        let mapped = h.status_lookup(&status)?;
        let partial = is(h, &status, "partial_fill") || is(h, &status, "partially_filled");
        let full = h.call("isTerminalFillStatus", vec![status.clone()])?;
        let payload = h.spread(&raw)?;
        h.define(
            &payload,
            "status",
            or(h, status, || prop(h, &raw, "status"))?,
        )?;
        let side = if is(h, &prop(h, &raw, "posMode")?, "hedge_mode")
            && is(h, &prop(h, &raw, "reduceOnly")?, "yes")
        {
            if is(h, &prop(h, &raw, "side")?, "sell") {
                s(h, "buy")
            } else {
                s(h, "sell")
            }
        } else {
            prop(h, &raw, "side")?
        };
        h.define(&payload, "side", side)?;
        let orders = h.call("parent.mapOrder", vec![payload])?;
        let parent = h.index(&orders, 0)?;
        if h.truthy(&full) || partial {
            let fill = h.object(vec![
                ("id", prop(h, &parent, "id")?),
                ("side", prop(h, &parent, "side")?),
                ("symbol", prop(h, &parent, "symbol")?),
                (
                    "price",
                    or(h, prop(h, &raw, "priceAvg")?, || prop(h, &parent, "price"))?,
                ),
                (
                    "amount",
                    h.number(h.parse_float(&or(h, prop(h, &raw, "baseVolume")?, || {
                        or(h, prop(h, &raw, "accBaseVolume")?, || Ok(h.number(0.)))
                    })?)?),
                ),
            ]);
            h.call("emitter.emit", vec![s(h, "fill"), fill])?;
            h.call("throttledFetchBalance", vec![])?;
        }
        if h.truthy(&full) {
            h.call("throttledFetchPositions", vec![])?;
            h.call("throttledFetchBalance", vec![])?;
        }
        // Source has three independent conditions and captures each forEach length.
        for terminal in ["canceled", "closed"] {
            if is(h, &mapped, terminal) {
                each(h, &orders, |o| {
                    h.call("store.updateOrderStatus", vec![o.clone(), s(h, terminal)])?;
                    h.call("markOrderAsTerminal", vec![prop(h, &o, "id")?])?;
                    Ok(())
                })?
            }
        }
        if is(h, &mapped, "open") {
            each(h, &orders, |o| {
                h.call("store.addOrUpdateOrder", vec![o.clone()])?;
                h.call("markOrderAsOpenFromWs", vec![prop(h, &o, "id")?])?;
                Ok(())
            })?
        }
    }
    h.set_receiver("hasPendingOrderEvents", h.boolean(true))?;
    h.call("throttledFetchOrders", vec![])?;
    Ok(())
}
// ts: BitgetPrivateWebsocket.handleUtaOrderTopic
pub fn handle_uta_order_topic<H: PrivateTopicsHost>(h: &H, data: H::Value) -> Result<(), H::Error> {
    let raw = raw_list(h, &data)?;
    let payload = h.spread(&data)?;
    let orders = map(h, &raw, |raw| {
        let payload = h.spread(&raw)?;
        h.define(&payload, "status", prop(h, &raw, "orderStatus")?)?;
        let volume = prop(h, &raw, "baseVolume")?;
        h.define(
            &payload,
            "baseVolume",
            if h.nullish(&volume) {
                prop(h, &raw, "cumExecQty")?
            } else {
                volume
            },
        )?;
        h.define(&payload, "priceAvg", prop(h, &raw, "avgPrice")?)?;
        Ok(payload)
    })?;
    h.define(&payload, "data", orders)?;
    h.call("handleOrderTopic", vec![payload])?;
    Ok(())
}
// ts: BitgetPrivateWebsocket.handleAlgoOrdersTopic
pub fn handle_algo_orders_topic<H: PrivateTopicsHost>(
    h: &H,
    data: H::Value,
) -> Result<(), H::Error> {
    h.call("parent.markPrivateStreamMessageReceived", vec![])?;
    let raw = prop(h, &data, "data")?;
    let mut iter = h.iterator(&raw)?;
    while let Some(raw) = h.next(&mut iter)? {
        let status = h.call("getRawOrderStatus", vec![raw.clone()])?;
        let mapped = h.status_lookup(&status)?;
        // TS reads the mapped order only for these statuses (terminal fills all map to closed),
        // so an unmapped algo status writes nothing and the frame continues to the refetch.
        if !["open", "canceled", "closed"]
            .iter()
            .any(|status| is(h, &mapped, status))
        {
            continue;
        }
        let payload = h.spread(&raw)?;
        h.define(
            &payload,
            "status",
            or(h, status.clone(), || prop(h, &raw, "status"))?,
        )?;
        let side = if is(h, &prop(h, &raw, "posMode")?, "hedge_mode") {
            if is(h, &prop(h, &raw, "side")?, "sell") {
                s(h, "buy")
            } else {
                s(h, "sell")
            }
        } else {
            prop(h, &raw, "side")?
        };
        h.define(&payload, "side", side)?;
        let orders = h.call("parent.mapOrder", vec![payload])?;
        let parent = h.index(&orders, 0)?;
        let full = h.call("isTerminalFillStatus", vec![status])?;
        if is(h, &mapped, "open") {
            h.call("store.addOrUpdateOrder", vec![parent.clone()])?;
            h.call("markOrderAsOpenFromWs", vec![prop(h, &parent, "id")?])?;
        } else if is(h, &mapped, "canceled") {
            h.call(
                "store.updateOrderStatus",
                vec![parent.clone(), s(h, "canceled")],
            )?;
            h.call("markOrderAsTerminal", vec![prop(h, &parent, "id")?])?;
        } else if is(h, &mapped, "closed") {
            h.call(
                "store.updateOrderStatus",
                vec![parent.clone(), s(h, "closed")],
            )?;
            h.call("markOrderAsTerminal", vec![prop(h, &parent, "id")?])?;
        }
        if h.truthy(&full) && h.truthy(&prop(h, &raw, "actualSize")?) {
            let fill = h.object(vec![
                ("id", prop(h, &parent, "id")?),
                ("side", prop(h, &parent, "side")?),
                ("symbol", prop(h, &parent, "symbol")?),
                (
                    "price",
                    h.number(h.parse_float(&prop(h, &raw, "triggerPrice")?)?),
                ),
                (
                    "amount",
                    h.number(h.parse_float(&prop(h, &raw, "actualSize")?)?),
                ),
            ]);
            h.call("emitter.emit", vec![s(h, "fill"), fill])?;
            h.call("throttledFetchBalance", vec![])?;
        }
        if h.truthy(&full) {
            h.call("throttledFetchPositions", vec![])?;
            h.call("throttledFetchBalance", vec![])?;
        }
    }
    h.set_receiver("hasPendingOrderEvents", h.boolean(true))?;
    h.call("throttledFetchOrders", vec![])?;
    Ok(())
}
fn position_topic<H: PrivateTopicsHost>(h: &H, data: H::Value, uta: bool) -> Result<(), H::Error> {
    h.call("parent.markPrivateStreamMessageReceived", vec![])?;
    let first = optional(h, &data, "action")?;
    let action = if h.is_string(&first) {
        prop(h, &data, "action")?
    } else {
        h.undefined()
    };
    let raw = raw_list(h, &data)?;
    let len = h.length(&raw)?;
    let mut valid = vec![];
    for i in 0..len {
        if h.has_index(&raw, i)? {
            let p = h.index(&raw, i)?;
            if h.truthy(&p) && h.is_object(&p) {
                valid.push(Some(p))
            }
        }
    }
    let valid = h.array(valid);
    if h.length(&valid)? == 0 {
        if is(h, &action, "snapshot") {
            h.call("markWsPositions", vec![empty(h), action])?;
            h.call("store.addOrUpdatePositionsFull", vec![empty(h)])?;
        }
        h.set_receiver("hasPendingPositionEvents", h.boolean(true))?;
        h.call("throttledFetchPositions", vec![])?;
        h.call("throttledFetchBalance", vec![])?;
        return Ok(());
    }
    let positions = map(h, &valid, |raw| {
        h.call(
            if uta {
                "parent.mapUtaPosition"
            } else {
                "parent.mapPosition"
            },
            vec![raw],
        )
    })?;
    h.call("markWsPositions", vec![positions.clone(), action.clone()])?;
    if is(h, &action, "snapshot") {
        h.call("store.addOrUpdatePositionsFull", vec![positions])?;
    } else {
        let mut iter = h.iterator(&positions)?;
        while let Some(p) = h.next(&mut iter)? {
            h.call("store.addOrUpdatePosition", vec![p])?;
        }
    }
    h.set_receiver("hasPendingPositionEvents", h.boolean(true))?;
    h.call("throttledFetchPositions", vec![])?;
    h.call("throttledFetchBalance", vec![])?;
    Ok(())
}
// ts: BitgetPrivateWebsocket.handlePositionsTopic
pub fn handle_positions_topic<H: PrivateTopicsHost>(h: &H, data: H::Value) -> Result<(), H::Error> {
    position_topic(h, data, false)
}
// ts: BitgetPrivateWebsocket.handleUtaPositionsTopic
pub fn handle_uta_positions_topic<H: PrivateTopicsHost>(
    h: &H,
    data: H::Value,
) -> Result<(), H::Error> {
    position_topic(h, data, true)
}
// ts: BitgetPrivateWebsocket.handleAccountTopic
pub fn handle_account_topic<H: PrivateTopicsHost>(h: &H, data: H::Value) -> Result<(), H::Error> {
    h.call("parent.markPrivateStreamMessageReceived", vec![])?;
    let raw = raw_list(h, &data)?;
    let len = h.length(&raw)?;
    let mut acct = h.undefined();
    for i in 0..len {
        let b = h.index(&raw, i)?;
        if h.equal(
            &prop(h, &b, "marginCoin")?,
            &h.receiver("parent.apiMarginCoin")?,
        ) {
            acct = b;
            break;
        }
    }
    if !h.truthy(&acct) {
        return Ok(());
    }
    let total = h.finite(&prop(h, &acct, "unionTotalMargin")?, f64::NAN)?;
    let available = h.finite(&prop(h, &acct, "unionAvailable")?, f64::NAN)?;
    let api = h.finite(
        &prop(h, &acct, "unrealizedPL")?,
        h.finite(&prop(h, &acct, "crossedUnrealizedPL")?, 0.)?
            + h.finite(&prop(h, &acct, "isolatedUnrealizedPL")?, 0.)?,
    )?;
    if total.is_nan() || available.is_nan() {
        return Ok(());
    }
    let upnl = h.account_upnl(api)?;
    let total_value = h.number(safe_cex_num::subtract(total, upnl));
    let balance = h.object(vec![
        ("used", h.number(safe_cex_num::subtract(total, available))),
        ("free", h.number(available)),
        ("total", total_value),
        ("upnl", h.number(upnl)),
        ("info", acct),
    ]);
    h.call("store.update", vec![h.object(vec![("balance", balance)])])?;
    Ok(())
}
// ts: BitgetPrivateWebsocket.handleUtaAccountTopic
pub fn handle_uta_account_topic<H: PrivateTopicsHost>(
    h: &H,
    data: H::Value,
) -> Result<(), H::Error> {
    h.call("parent.markPrivateStreamMessageReceived", vec![])?;
    let first = optional(h, &data, "data")?;
    let raw = if h.is_array(&first) {
        h.index(&prop(h, &data, "data")?, 0)?
    } else {
        optional(h, &data, "data")?
    };
    if !h.truthy(&raw) || !h.is_object(&raw) {
        h.call("throttledFetchBalance", vec![])?;
        return Ok(());
    }
    let equity = h.finite(&prop(h, &raw, "accountEquity")?, f64::NAN)?;
    let available = h.finite(&prop(h, &raw, "effEquity")?, f64::NAN)?;
    let api = h.finite(
        &prop(h, &raw, "unrealisedPnl")?,
        h.finite(&prop(h, &raw, "usdtUnrealisedPnl")?, 0.)?,
    )?;
    if equity.is_nan() || available.is_nan() {
        h.call("throttledFetchBalance", vec![])?;
        return Ok(());
    }
    let upnl = h.account_upnl(api)?;
    let total = h.number(safe_cex_num::subtract(equity, upnl));
    let balance = h.object(vec![
        ("used", h.number(safe_cex_num::subtract(equity, available))),
        ("free", h.number(available)),
        ("total", total),
        ("upnl", h.number(upnl)),
        ("info", raw),
    ]);
    h.call("store.update", vec![h.object(vec![("balance", balance)])])?;
    h.set_receiver("hasPendingBalanceEvents", h.boolean(true))?;
    h.call("throttledFetchBalance", vec![])?;
    Ok(())
}
