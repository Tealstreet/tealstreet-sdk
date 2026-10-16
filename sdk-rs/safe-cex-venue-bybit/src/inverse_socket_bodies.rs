//! Inverse socket handler bodies over a retained, identity-preserving realm.
//!
//! `SessionHost` provides language primitives and imported dependencies. These
//! functions own dispatch, iteration and mutation order; class assembly is separate.
use crate::hyro_private_session::SessionHost;

macro_rules! call {
    ($h:expr,$owner:expr,$key:expr,$args:expr) => {{
        let receiver = $owner;
        let function = $h.get(&receiver, $key)?;
        $h.receiver_call(receiver, function, $args)?
    }};
}
macro_rules! imported {
    ($h:expr,$key:expr,$args:expr) => {{
        let function = $h.get(&$h.global(), $key)?;
        $h.receiver_call($h.undefined(), function, $args)?
    }};
}
fn computed<H: SessionHost>(h: &H, owner: H::Value, key: H::Value) -> Result<H::Value, H::Error> {
    h.intrinsic("computed get", vec![owner, key])
}
fn relation<H: SessionHost>(h: &H, op: &str, a: H::Value, b: H::Value) -> Result<bool, H::Error> {
    Ok(h.truthy(&h.intrinsic(op, vec![a, b])?))
}
fn status<H: SessionHost>(h: &H, order: &H::Value, expected: &str) -> Result<bool, H::Error> {
    Ok(h.equal(&h.get(order, "orderStatus")?, &h.literal(expected)))
}
fn parsed<H: SessionHost>(h: &H, order: H::Value, field: &str) -> Result<H::Value, H::Error> {
    let parse = h.get(&h.global(), "parseFloat")?;
    let value = imported!(h, "v", vec![order, h.literal(field)]);
    h.receiver_call(h.undefined(), parse, vec![value])
}
fn for_of<H: SessionHost>(
    h: &H,
    value: H::Value,
    mut body: impl FnMut(H::Value) -> Result<(), H::Error>,
) -> Result<(), H::Error> {
    let iterator = h.intrinsic("iterator", vec![value])?;
    let next = h.get(&iterator, "next")?;
    loop {
        let result = h.receiver_call(iterator.clone(), next.clone(), vec![])?;
        h.intrinsic("require iterator result object", vec![result.clone()])?;
        if h.truthy(&h.get(&result, "done")?) {
            return Ok(());
        }
        // IteratorStep/IteratorValue failures do not run IteratorClose. An abrupt
        // body does; the original throw wins over a failing return method.
        let value = h.get(&result, "value")?;
        if let Err(error) = body(value) {
            let _ = h.intrinsic("iterator close after throw", vec![iterator]);
            return Err(error);
        }
    }
}
/// The field initializer retains the imported factory's exact fresh function.
pub fn incremental_fills<H: SessionHost>(h: &H) -> Result<H::Value, H::Error> {
    Ok(imported!(h, "createIncrementalFills", vec![]))
}
pub fn private_on_message<H: SessionHost>(
    h: &H,
    owner: &H::Value,
    event: H::Value,
) -> Result<H::Value, H::Error> {
    let data = h.get(&event, "data")?;
    if !h.truthy(&h.get(owner, "isDisposed")?) {
        call!(h, owner.clone(), "trackLastReceived", vec![]);
        call!(h, h.get(owner, "directFallback")?, "onMessage", vec![]);
        call!(
            h,
            owner.clone(),
            "emitWsDiagnosticReceive",
            vec![data.clone()]
        );
        let json = imported!(h, "jsonParse", vec![data]);
        if h.equal(&h.optional(&json, "topic")?, &h.literal("order")) {
            call!(
                h,
                owner.clone(),
                "recordMessageHandlerStats",
                vec![h.literal("order")]
            );
            call!(
                h,
                owner.clone(),
                "handleOrderTopic",
                vec![h.get(&json, "data")?]
            );
        }
        if h.equal(&h.optional(&json, "topic")?, &h.literal("position")) {
            call!(
                h,
                owner.clone(),
                "recordMessageHandlerStats",
                vec![h.literal("position")]
            );
            call!(
                h,
                owner.clone(),
                "handlePositionTopic",
                vec![h.get(&json, "data")?]
            );
        }
        if h.equal(&h.optional(&json, "op")?, &h.literal("pong")) {
            call!(h, owner.clone(), "handlePongEvent", vec![]);
        }
        if h.equal(&h.optional(&json, "success")?, &h.boolean(false)) {
            let msg = h.array(vec![h.literal("[WS]")]);
            if h.truthy(&h.optional(&json, "op")?) {
                let push = h.get(&msg, "push")?;
                let mut units = h.units(&h.optional(&json, "op")?)?;
                units.push(u16::from(b':'));
                h.receiver_call(msg.clone(), push, vec![h.text(units)])?;
            }
            if h.truthy(&h.optional(&json, "ret_msg")?) {
                call!(h, msg.clone(), "push", vec![h.optional(&json, "ret_msg")?]);
            }
            let emit = h.get(owner, "emitAndLogError")?;
            let message = call!(h, msg, "join", vec![h.literal("")]);
            h.receiver_call(owner.clone(), emit, vec![message])?;
            if h.equal(&h.optional(&json, "op")?, &h.literal("auth"))
                && relation(h, "less equal", h.get(owner, "authRetries")?, h.number(3.0))?
            {
                call!(
                    h,
                    owner.clone(),
                    "emitAndLogError",
                    vec![h.literal("[WS] Syncing time with BybitInverse...")]
                );
                let retries = h.intrinsic("increment", vec![h.get(owner, "authRetries")?])?;
                h.set(owner, "authRetries", retries)?;
                // The realm invokes the live function immediately. A cold Rust
                // Future must not be substituted for this unawaited call.
                call!(
                    h,
                    owner.clone(),
                    "sendReconnectSequence",
                    vec![h.boolean(true)]
                );
            }
        }
    }
    Ok(h.undefined())
}
pub fn handle_order_topic<H: SessionHost>(
    h: &H,
    owner: &H::Value,
    data: H::Value,
) -> Result<H::Value, H::Error> {
    call!(
        h,
        data,
        "forEach",
        vec![h.callback(owner.clone(), "inverse-order-row", vec![])?]
    );
    Ok(h.undefined())
}
fn order_row<H: SessionHost>(
    h: &H,
    owner: &H::Value,
    order: H::Value,
) -> Result<H::Value, H::Error> {
    let orders = call!(h, h.get(owner, "parent")?, "mapOrder", vec![order.clone()]);
    for_of(h, orders.clone(), |mapped| {
        call!(
            h,
            h.get(owner, "parent")?,
            "markOrderSeen",
            vec![h.get(&mapped, "id")?]
        );
        Ok(())
    })?;
    let price = parsed(h, order.clone(), "price")?;
    let amount = parsed(h, order.clone(), "qty")?;
    if status(h, &order, "PartiallyFilled")?
        && relation(h, "less equal", price, h.number(0.0))?
        && relation(h, "less equal", amount, h.number(0.0))?
    {
        return Ok(h.undefined());
    }
    if status(h, &order, "Filled")? || status(h, &order, "PartiallyFilled")? {
        let executed = imported!(h, "resolveBybitExecution", vec![order.clone()]);
        let cumulative = parsed(h, order.clone(), "cumExecValue")?;
        let function = h.get(owner, "incrementalFills")?;
        let id = h.get(&h.get(&orders, "0")?, "id")?;
        let amount = h.get(&executed, "amount")?;
        let value = if h.truthy(&call!(
            h,
            h.get(&h.global(), "Number")?,
            "isFinite",
            vec![cumulative.clone()]
        )) {
            cumulative
        } else {
            h.undefined()
        };
        let fill = h.receiver_call(owner.clone(), function, vec![id, amount, value])?;
        if relation(h, "greater", h.get(&fill, "qty")?, h.number(0.0))? {
            let emitter = h.get(owner, "emitter")?;
            let emit = h.get(&emitter, "emit")?;
            let id = h.get(&h.get(&orders, "0")?, "id")?;
            let side = h.get(&h.get(&orders, "0")?, "side")?;
            let symbol = h.get(&h.get(&orders, "0")?, "symbol")?;
            let price = if h.truthy(&h.get(&fill, "value")?) {
                h.intrinsic(
                    "division",
                    vec![h.get(&fill, "qty")?, h.get(&fill, "value")?],
                )?
            } else {
                h.get(&executed, "price")?
            };
            let amount = h.get(&fill, "qty")?;
            h.receiver_call(
                emitter,
                emit,
                vec![
                    h.literal("fill"),
                    h.object(vec![
                        ("id", id),
                        ("side", side),
                        ("symbol", symbol),
                        ("price", price),
                        ("amount", amount),
                    ]),
                ],
            )?;
        }
    }
    if status(h, &order, "Cancelled")?
        || status(h, &order, "Filled")?
        || status(h, &order, "Deactivated")?
        || status(h, &order, "Rejected")?
    {
        if !status(h, &order, "Filled")?
            && h.truthy(&h.get(&order, "rejectReason")?)
            && !h.equal(
                &h.get(&order, "rejectReason")?,
                &h.literal("EC_PerCancelRequest"),
            )
            && !h.equal(&h.get(&order, "rejectReason")?, &h.literal("EC_NoError"))
        {
            call!(
                h,
                owner.clone(),
                "emitAndLogError",
                vec![h.get(&order, "rejectReason")?]
            );
        }
        let filled = status(h, &order, "Filled")?;
        let statuses = h.get(&h.global(), "OrderStatus")?;
        let next = h.get(&statuses, if filled { "Closed" } else { "Canceled" })?;
        let mut i = 0.0;
        while relation(h, "less", h.number(i), h.get(&orders, "length")?)? {
            let mapped = computed(h, orders.clone(), h.number(i))?;
            h.set(&mapped, "status", next.clone())?;
            i += 1.0;
        }
        call!(
            h,
            h.get(owner, "store")?,
            "addOrUpdateOrders",
            vec![orders.clone()]
        );
    }
    if status(h, &order, "New")?
        || status(h, &order, "Untriggered")?
        || status(h, &order, "PartiallyFilled")?
    {
        call!(h, h.get(owner, "store")?, "addOrUpdateOrders", vec![orders]);
    }
    Ok(h.undefined())
}
pub fn handle_position_topic<H: SessionHost>(
    h: &H,
    owner: &H::Value,
    data: H::Value,
) -> Result<H::Value, H::Error> {
    let positions = call!(
        h,
        data,
        "map",
        vec![h.callback(owner.clone(), "inverse-position-map", vec![])?]
    );
    let updates = call!(
        h,
        positions,
        "map",
        vec![h.callback(owner.clone(), "inverse-position-pair", vec![])?]
    );
    call!(h, h.get(owner, "store")?, "updatePositions", vec![updates]);
    let parent = h.get(owner, "parent")?;
    let now = call!(
        h,
        h.get(&h.global(), "virtualClock")?,
        "getCurrentTimeMs",
        vec![]
    );
    h.set(&parent, "lastWebsocketPositionsUpdate", now)?;
    Ok(h.undefined())
}
fn invoke_handlers<H: SessionHost>(
    h: &H,
    handlers: H::Value,
    json: Option<H::Value>,
) -> Result<(), H::Error> {
    let mut i = 0.0;
    while relation(h, "less", h.number(i), h.get(&handlers, "length")?)? {
        let handler = computed(h, handlers.clone(), h.number(i))?;
        h.receiver_call(
            handlers.clone(),
            handler,
            vec![json.clone().unwrap_or_else(|| h.object(vec![]))],
        )?;
        i += 1.0;
    }
    Ok(())
}
pub fn public_on_message<H: SessionHost>(
    h: &H,
    owner: &H::Value,
    event: H::Value,
) -> Result<H::Value, H::Error> {
    let data = h.get(&event, "data")?;
    if h.truthy(&h.get(owner, "isDisposed")?) {
        return Ok(h.undefined());
    }
    call!(h, owner.clone(), "trackLastReceived", vec![]);
    call!(h, h.get(owner, "directFallback")?, "onMessage", vec![]);
    let index = call!(h, data.clone(), "indexOf", vec![h.literal("\"topic\":\"")]);
    if relation(h, "greater equal", index.clone(), h.number(0.0))? {
        let start = h.intrinsic("addition", vec![index, h.number(9.0)])?;
        let end = call!(
            h,
            data.clone(),
            "indexOf",
            vec![h.literal("\""), start.clone()]
        );
        if relation(h, "less equal", end.clone(), start.clone())? {
            return Ok(h.undefined());
        }
        let topic = call!(h, data.clone(), "slice", vec![start, end]);
        let exact = computed(h, h.get(owner, "messageHandlers")?, topic.clone())?;
        let mut prefix = h.undefined();
        let dot = call!(h, topic.clone(), "indexOf", vec![h.literal(".")]);
        if relation(h, "greater", dot.clone(), h.number(0.0))? {
            let handlers = h.get(owner, "messageHandlers")?;
            let key = call!(h, topic.clone(), "slice", vec![h.number(0.0), dot.clone()]);
            prefix = computed(h, handlers, key)?;
        }
        let has_exact =
            h.truthy(&exact) && relation(h, "greater", h.get(&exact, "length")?, h.number(0.0))?;
        let has_prefix =
            h.truthy(&prefix) && relation(h, "greater", h.get(&prefix, "length")?, h.number(0.0))?;
        if !has_exact && !has_prefix {
            return Ok(h.undefined());
        }
        let stats = h.get(owner, "recordMessageHandlerStats")?;
        let key = if has_exact {
            topic
        } else {
            call!(h, topic, "slice", vec![h.number(0.0), dot])
        };
        h.receiver_call(owner.clone(), stats, vec![key])?;
        let json = imported!(h, "jsonParse", vec![data]);
        if !h.truthy(&json) {
            return Ok(h.undefined());
        }
        if has_exact {
            invoke_handlers(h, exact, Some(json.clone()))?;
        }
        if has_prefix {
            invoke_handlers(h, prefix, Some(json))?;
        }
        return Ok(h.undefined());
    }
    if h.truthy(&call!(h, data, "includes", vec![h.literal("pong")])) {
        let handlers = h.get(&h.get(owner, "messageHandlers")?, "pong")?;
        if h.truthy(&handlers)
            && relation(h, "greater", h.get(&handlers, "length")?, h.number(0.0))?
        {
            call!(
                h,
                owner.clone(),
                "recordMessageHandlerStats",
                vec![h.literal("pong")]
            );
            invoke_handlers(h, handlers, None)?;
        }
    }
    Ok(h.undefined())
}
/// Routes only closures created by these bodies. Imported methods remain live.
pub fn dispatch<H: SessionHost>(
    h: &H,
    owner: &H::Value,
    code: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    match code {
        "inverse-order-row" => order_row(h, owner, first),
        "inverse-position-map" => Ok(call!(
            h,
            h.get(owner, "parent")?,
            "mapPosition",
            vec![first]
        )),
        "inverse-position-pair" => Ok(h.array(vec![first.clone(), first])),
        _ => h.intrinsic("unknown callback", vec![h.literal(code)]),
    }
}
