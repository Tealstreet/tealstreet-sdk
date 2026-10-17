//! Source stream/report bodies over original runtime values, mappers and stores.
use crate::ws_controls::{Result, Value, WsHost};
pub trait ReportHost: WsHost {
    fn ticker_patch_refusal(
        &self,
        symbol: &[u16],
        fields: &[(&str, Value<Self>)],
    ) -> Result<Self, Option<String>>;
    fn ticker_refusal_error(&self, messages: Vec<String>) -> Self::Error;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn undefined(&self) -> Value<Self>;
    fn text_eq(&self, value: &Self::Value, expected: &str) -> bool;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn array(&self, values: Vec<Value<Self>>) -> Value<Self>;
    fn for_each(
        &self,
        value: Value<Self>,
        callback: impl FnMut(Value<Self>) -> Result<Self, ()>,
    ) -> Result<Self, ()>;
    fn starts_with(&self, value: Value<Self>, prefix: &str) -> Result<Self, bool>;
    fn normalize_symbol(&self, value: Value<Self>) -> Result<Self, Vec<u16>>;
    fn get_key(&self, value: Value<Self>, key: &str) -> Result<Self, Value<Self>>;
    fn parse_float(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn order_side(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn get<H: ReportHost>(h: &H, value: &Value<H>, key: &str) -> Result<H, Value<H>> {
    h.get(value, &units(key))
}
fn optional<H: ReportHost>(h: &H, value: &Value<H>, key: &str) -> Result<H, Value<H>> {
    if h.nullish(value) {
        Ok(h.undefined())
    } else {
        get(h, value, key)
    }
}
fn text<H: ReportHost>(h: &H, s: &str) -> Value<H> {
    h.text(&units(s))
}
fn invoke<H: ReportHost>(
    h: &H,
    receiver: Value<H>,
    method: &str,
    args: Vec<Value<H>>,
) -> Result<H, Value<H>> {
    let function = get(h, &receiver, method)?;
    h.call(function, Some(receiver), args)
}
fn partial<H: ReportHost>(h: &H, status: &Value<H>) -> bool {
    h.text_eq(status, "PARTIAL_FILLED") || h.text_eq(status, "PARTIALLY_FILLED")
}
fn update_status<H: ReportHost>(
    h: &H,
    receiver: &Value<H>,
    id: impl FnOnce() -> Result<H, Value<H>>,
    canceled: bool,
) -> Result<H, ()> {
    let store = get(h, receiver, "store")?;
    let function = get(h, &store, "updateOrderStatus")?;
    let selector = h.object(vec![("id", h.text(&h.string(&*id()?)?))])?;
    h.call(
        function,
        Some(store),
        vec![
            selector,
            text(h, if canceled { "canceled" } else { "closed" }),
        ],
    )?;
    Ok(())
}
fn public_stream<H: ReportHost>(
    h: &H,
    receiver: Value<H>,
    data: Value<H>,
    mode: u8,
) -> Result<H, ()> {
    let mut refusals = Vec::new();
    h.for_each(data, |row| {
        if !h.starts_with(get(h, &row, "symbol")?, "PERP_")? {
            return Ok(());
        }
        let symbol = h.normalize_symbol(get(h, &row, "symbol")?)?;
        let parent = get(h, &receiver, "parent")?;
        let tickers = get(h, &get(h, &parent, "store")?, "tickersBySymbol")?;
        let ticker = h.get(&tickers, &symbol)?;
        if h.truthy(&ticker) {
            let store = get(h, &receiver, "store")?;
            let function = get(h, &store, "updateTicker")?;
            let fields = match mode {
                0 => vec![
                    ("last", get(h, &row, "close")?),
                    ("quoteVolume", get(h, &row, "amount")?),
                    ("volume", get(h, &row, "volume")?),
                ],
                1 => vec![("bid", get(h, &row, "bid")?), ("ask", get(h, &row, "ask")?)],
                2 => vec![("mark", get(h, &row, "price")?)],
                _ => vec![
                    (
                        "fundingRate",
                        h.parse_float(h.get_key(row.clone(), "fundingRate")?)?,
                    ),
                    (
                        "fundingTime",
                        h.parse_float(h.get_key(row.clone(), "fundingTs")?)?,
                    ),
                ],
            };
            if let Some(message) = h.ticker_patch_refusal(&symbol, &fields)? {
                refusals.push(message);
                return Ok(());
            }
            let patch = h.object(fields)?;
            h.call(function, Some(store), vec![ticker, patch])?;
        }
        Ok(())
    })?;
    if refusals.is_empty() {
        Ok(())
    } else {
        Err(h.ticker_refusal_error(refusals))
    }
}
pub fn tickers<H: ReportHost>(h: &H, receiver: Value<H>, data: Value<H>) -> Result<H, ()> {
    public_stream(h, receiver, data, 0)
}
pub fn bbo<H: ReportHost>(h: &H, receiver: Value<H>, data: Value<H>) -> Result<H, ()> {
    public_stream(h, receiver, data, 1)
}
pub fn mark_prices<H: ReportHost>(h: &H, receiver: Value<H>, data: Value<H>) -> Result<H, ()> {
    public_stream(h, receiver, data, 2)
}
pub fn funding_rates<H: ReportHost>(h: &H, receiver: Value<H>, data: Value<H>) -> Result<H, ()> {
    public_stream(h, receiver, data, 3)
}
pub fn private_message<H: ReportHost>(h: &H, receiver: Value<H>, event: Value<H>) -> Result<H, ()> {
    let data = get(h, &event, "data")?;
    if h.truthy(&*get(h, &receiver, "isDisposed")?) {
        return Ok(());
    }
    invoke(h, receiver.clone(), "trackLastReceived", vec![])?;
    invoke(
        h,
        receiver.clone(),
        "emitWsDiagnosticReceive",
        vec![data.clone()],
    )?;
    let json = h.parse(data)?;
    if h.text_eq(&*optional(h, &json, "event")?, "ping") {
        let function = get(h, &receiver, "sendWithLog")?;
        let payload = h.object(vec![("event", text(h, "pong"))])?;
        let encoded = h.stringify(payload)?;
        h.call(function, Some(receiver), vec![encoded])?;
        return Ok(());
    }
    if h.text_eq(&*optional(h, &json, "event")?, "pong") {
        invoke(h, receiver, "handlePongEvent", vec![])?;
        return Ok(());
    }
    if h.text_eq(&*optional(h, &json, "event")?, "auth")
        && h.truthy(&*optional(h, &json, "success")?)
    {
        invoke(h, receiver, "subscribe", vec![])?;
        return Ok(());
    }
    for (topic, method) in [
        ("executionreport", "handleExecutionReport"),
        ("algoexecutionreportv2", "handleAlgoExecutionReport"),
        ("position", "handlePositionReport"),
    ] {
        if h.text_eq(&*optional(h, &json, "topic")?, topic) {
            let function = get(h, &receiver, method)?;
            let arg = optional(h, &json, "data")?;
            h.call(function, Some(receiver), vec![arg])?;
            return Ok(());
        }
    }
    Ok(())
}
pub fn positions<H: ReportHost>(h: &H, receiver: Value<H>, data: Value<H>) -> Result<H, ()> {
    let positions = optional(h, &data, "positions")?;
    let positions = if h.is_array(&positions) {
        get(h, &data, "positions")?
    } else {
        h.array(vec![])
    };
    let mut iterator = h.iterator(positions)?;
    while let Some(raw) = h.next(&mut iterator)? {
        let parent = get(h, &receiver, "parent")?;
        let position = invoke(h, parent, "mapPosition", vec![raw])?;
        if h.truthy(&position) {
            invoke(
                h,
                get(h, &receiver, "store")?,
                "addOrUpdatePosition",
                vec![position],
            )?;
        }
    }
    Ok(())
}
pub fn execution<H: ReportHost>(h: &H, receiver: Value<H>, data: Value<H>) -> Result<H, ()> {
    let updated = invoke(
        h,
        get(h, &receiver, "parent")?,
        "mapLimitOrder",
        vec![data.clone()],
    )?;
    let status = get(h, &data, "status")?;
    let is_partial = partial(h, &status);
    let price = h.get_key(data.clone(), "executedPrice")?;
    let amount = h.get_key(data.clone(), "executedQuantity")?;
    if (h.text_eq(&status, "REPLACED") || h.text_eq(&status, "NEW") || is_partial)
        && h.truthy(&updated)
    {
        invoke(
            h,
            get(h, &receiver, "store")?,
            "addOrUpdateOrder",
            vec![updated.clone()],
        )?;
    }
    if h.text_eq(&status, "CANCELLED") || h.text_eq(&status, "FILLED") {
        update_status(
            h,
            &receiver,
            || get(h, &data, "orderId"),
            h.text_eq(&status, "CANCELLED"),
        )?;
    }
    if h.truthy(&updated) && (h.text_eq(&status, "FILLED") || is_partial) {
        let emitter = get(h, &get(h, &receiver, "parent")?, "emitter")?;
        let function = get(h, &emitter, "emit")?;
        let payload = h.object(vec![
            ("id", get(h, &data, "orderId")?),
            ("side", get(h, &updated, "side")?),
            ("symbol", get(h, &updated, "symbol")?),
            ("price", price),
            ("amount", amount),
        ])?;
        h.call(function, Some(emitter), vec![text(h, "fill"), payload])?;
    }
    Ok(())
}
pub fn algo_execution<H: ReportHost>(h: &H, receiver: Value<H>, data: Value<H>) -> Result<H, ()> {
    let rows = if h.is_array(&data) {
        data
    } else {
        h.array(vec![data])
    };
    h.for_each(rows, |row| {
        let status = h.get_key(row.clone(), "algoStatus")?;
        let is_partial = partial(h, &status);
        if h.text_eq(&status, "NEW") || h.text_eq(&status, "REPLACED") || is_partial {
            let updated = invoke(
                h,
                get(h, &receiver, "parent")?,
                "mapAlgoOrder",
                vec![row.clone()],
            )?;
            let mut orders = h.iterator(updated)?;
            while let Some(order) = h.next(&mut orders)? {
                invoke(
                    h,
                    get(h, &receiver, "store")?,
                    "addOrUpdateOrder",
                    vec![order],
                )?;
            }
        }
        if h.text_eq(&status, "FILLED") {
            let emitter = get(h, &get(h, &receiver, "parent")?, "emitter")?;
            let function = get(h, &emitter, "emit")?;
            let mut id = h.get_key(row.clone(), "orderId")?;
            if !h.truthy(&id) {
                id = h.get_key(row.clone(), "algoOrderId")?;
            }
            if !h.truthy(&id) {
                id = h.get_key(row.clone(), "id")?;
            }
            let side = h.order_side(get(h, &row, "side")?)?;
            let symbol = h.text(&h.normalize_symbol(get(h, &row, "symbol")?)?);
            let payload = h.object(vec![
                ("id", id),
                ("side", side),
                ("symbol", symbol),
                ("price", get(h, &row, "averageExecutedPrice")?),
                ("amount", get(h, &row, "totalExecutedQuantity")?),
            ])?;
            h.call(function, Some(emitter), vec![text(h, "fill"), payload])?;
        }
        if h.text_eq(&status, "CANCELLED") || h.text_eq(&status, "FILLED") {
            update_status(
                h,
                &receiver,
                || h.get_key(row.clone(), "algoOrderId"),
                h.text_eq(&status, "CANCELLED"),
            )?;
            let children = get(h, &row, "childOrders")?;
            let children = if h.truthy(&children) {
                children
            } else {
                h.array(vec![])
            };
            let mut children = h.iterator(children)?;
            while let Some(child) = h.next(&mut children)? {
                update_status(
                    h,
                    &receiver,
                    || h.get_key(child, "algoOrderId"),
                    h.text_eq(&status, "CANCELLED"),
                )?;
            }
        }
        Ok(())
    })
}
