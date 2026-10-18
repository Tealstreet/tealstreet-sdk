//! COIN-M private stream bodies over the assembled exchange receiver.
use super::Rt;
use safe_cex_exchange::{
    js_runtime::{
        ops,
        promise::await_js,
        scope::scope_of,
        sockets,
        value::{Js, Thrown, arg, from_retained_input},
    },
    websocket::BaseWebSocket,
};
use std::rc::Rc;

pub(crate) fn new(parent: &Rt) -> Result<(Js, Rc<BaseWebSocket>), Thrown> {
    let socket = sockets::base_websocket(&parent.shared.sockets, parent.this.clone());
    socket.define("baseDispose", socket.get("dispose")?)?;
    for (key, value) in [
        ("lastPositionFetchTime", 0.),
        ("positionFetchThrottleMs", 10_000.),
        ("listenKeyRetryDelayMs", 5_000.),
    ] {
        socket.define(key, Js::Number(value))?;
    }
    for key in [
        "positionFetchScheduled",
        "positionFetchInFlight",
        "hasPendingPositionEvents",
        "connecting",
        "resyncOrdersOnOpen",
        "hasOpened",
        "keepaliveFailing",
    ] {
        socket.define(key, Js::Bool(false))?;
    }
    for key in [
        "ordersThrottleTimeout",
        "listenKeyRefreshTimer",
        "listenKeyRetryTimer",
    ] {
        socket.define(key, Js::Undefined)?;
    }
    let Js::Object(object) = &socket else {
        unreachable!("socket is an object")
    };
    let lexical = Rc::downgrade(object);
    let weak = Rc::downgrade(&parent.shared);
    for name in [
        "dispose",
        "connectAndSubscribe",
        "onOpen",
        "detachSocket",
        "onMessage",
        "ping",
        "getOrderRejectReason",
        "handleOrderEvents",
        "handleAccountEvents",
        "throttledFetchPositions",
        "executePositionFetch",
        "fetchListenKey",
        "scheduleListenKeyRefresh",
        "updateListenKey",
        "throttledFetchOrders",
        "fetchThrottledOrders",
    ] {
        let weak = weak.clone();
        let lexical = lexical.clone();
        socket.define(
            name,
            Js::function(name, move |this, args| {
                let this = if name == "dispose" {
                    this
                } else {
                    Js::Object(
                        lexical
                            .upgrade()
                            .ok_or_else(|| Thrown::error("Error", "socket receiver dropped"))?,
                    )
                };
                let rt = scope_of(&weak)?.with_this(this);
                match name {
                    "connectAndSubscribe"
                    | "executePositionFetch"
                    | "fetchListenKey"
                    | "updateListenKey"
                    | "fetchThrottledOrders" => {
                        let spawn = rt.clone();
                        Ok(spawn.spawn(Box::pin(async move {
                            match name {
                                "connectAndSubscribe" => connect(&rt).await,
                                "executePositionFetch" => execute_positions(&rt).await,
                                "fetchListenKey" => fetch_listen_key(&rt).await,
                                "updateListenKey" => update_listen_key(&rt).await,
                                _ => fetch_orders(&rt).await,
                            }
                        })))
                    }
                    "dispose" => {
                        for name in [
                            "listenKeyRetryTimer",
                            "listenKeyRefreshTimer",
                            "ordersThrottleTimeout",
                        ] {
                            rt.clear_timeout(&rt.this.get(name)?);
                        }
                        rt.call_path("baseDispose", vec![])
                    }
                    "onOpen" => {
                        if !flag(&rt, "isDisposed")? {
                            rt.call_path("ping", vec![])?;
                            if flag(&rt, "resyncOrdersOnOpen")? || flag(&rt, "hasOpened")? {
                                rt.this.set("resyncOrdersOnOpen", Js::Bool(false))?;
                                rt.call_path("throttledFetchOrders", vec![])?;
                            }
                            rt.this.set("hasOpened", Js::Bool(true))?;
                        }
                        Ok(Js::Undefined)
                    }
                    "detachSocket" => {
                        detach(&rt)?;
                        Ok(Js::Undefined)
                    }
                    "onMessage" => {
                        on_message(&rt, arg(&args, 0).get("data")?)?;
                        Ok(Js::Undefined)
                    }
                    "ping" => {
                        if !flag(&rt, "isDisposed")? {
                            rt.this
                                .set("pingAt", global(&rt, "performance.now", vec![])?)?;
                            send(
                                &rt,
                                "sendWithLog",
                                Js::object(vec![
                                    ("id", Js::Number(42.)),
                                    ("method", Js::str("LIST_SUBSCRIPTIONS")),
                                ]),
                            )?;
                        }
                        Ok(Js::Undefined)
                    }
                    "getOrderRejectReason" => reject_reason(&rt, &arg(&args, 0)),
                    "handleOrderEvents" => {
                        orders(&rt, arg(&args, 0))?;
                        Ok(Js::Undefined)
                    }
                    "handleAccountEvents" => {
                        accounts(&rt, arg(&args, 0))?;
                        Ok(Js::Undefined)
                    }
                    "throttledFetchPositions" => {
                        throttle_positions(&rt)?;
                        Ok(Js::Undefined)
                    }
                    "scheduleListenKeyRefresh" => {
                        schedule_refresh(&rt, arg(&args, 0).to_number()?)?;
                        Ok(Js::Undefined)
                    }
                    "throttledFetchOrders" => {
                        rt.clear_timeout(&rt.this.get("ordersThrottleTimeout")?);
                        let handle = timeout(&rt, "fetchThrottledOrders", 10_000., false)?;
                        rt.this.set("ordersThrottleTimeout", handle)?;
                        Ok(Js::Undefined)
                    }
                    _ => unreachable!(),
                }
            }),
        )?;
    }
    let base = sockets::core_of(&parent.shared.sockets, &socket)
        .expect("new socket core")
        .base()?;
    let weak_base = Rc::downgrade(&base);
    socket.define_accessor(
        "isForceClosed",
        Some(Rc::new(move |_| {
            let base = weak_base
                .upgrade()
                .ok_or_else(|| Thrown::error("Error", "private socket core dropped"))?;
            Ok(Js::Bool(base.is_force_closed()))
        })),
        None,
    );
    Ok((socket, base))
}
fn flag(rt: &Rt, key: &str) -> Result<bool, Thrown> {
    Ok(rt.this.get(key)?.truthy())
}
fn call(owner: &Js, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    owner.get(name)?.call(owner.clone(), args)
}
fn global(rt: &Rt, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    rt.global(name)?.call(Js::Undefined, args)
}
fn optional(value: &Js, key: &str) -> Result<Js, Thrown> {
    if value.is_nullish() {
        Ok(Js::Undefined)
    } else {
        value.get(key)
    }
}
fn or(value: Js, fallback: Js) -> Js {
    if value.truthy() { value } else { fallback }
}
fn nullish(value: Js, fallback: Js) -> Js {
    if value.is_nullish() { fallback } else { value }
}
fn send(rt: &Rt, method: &str, value: Js) -> Result<(), Thrown> {
    let text = global(rt, "JSON.stringify", vec![value])?;
    rt.call_path(method, vec![text])?;
    Ok(())
}
fn timeout(rt: &Rt, method: &'static str, delay: f64, guarded: bool) -> Result<Js, Thrown> {
    let weak = Rc::downgrade(&rt.shared);
    let this = rt.this.clone();
    Ok(rt.set_timeout(
        Rc::new(move || {
            let rt = scope_of(&weak)?.with_this(this.clone());
            if !guarded || (!flag(&rt, "isDisposed")? && !flag(&rt, "isForceClosed")?) {
                rt.call_path(method, vec![])?;
            }
            Ok(())
        }),
        delay,
    ))
}
fn error_text(rt: &Rt, error: &Thrown) -> Result<Js, Thrown> {
    if matches!(&error.value, Js::Object(o) if o.class == "Error") {
        Ok(Js::utf16(&error.value.get("message")?.to_js_string()?))
    } else {
        global(rt, "String", vec![error.value.clone()])
    }
}
fn component(value: &Js) -> Result<String, Thrown> {
    let text = String::from_utf16(&value.to_js_string()?)
        .map_err(|_| Thrown::error("URIError", "URI malformed"))?;
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(out, "%{byte:02X}").expect("String write");
        }
    }
    Ok(out)
}
async fn connect(rt: &Rt) -> Result<Js, Thrown> {
    if flag(rt, "isDisposed")? || flag(rt, "connecting")? {
        return Ok(Js::Undefined);
    }
    rt.this.set("connecting", Js::Bool(true))?;
    let listen_key = match async { await_js(rt.call_path("fetchListenKey", vec![])?).await }.await {
        Ok(key) => key,
        Err(error) => {
            let message = ops::add(
                &Js::str("Binance COIN-M user data stream unavailable: "),
                &error_text(rt, &error)?,
            )?;
            let delay = rt.this.get("listenKeyRetryDelayMs")?.to_number()?;
            if delay == 5_000. {
                rt.call_path("emitAndLogError", vec![message])?;
            } else {
                rt.call_path(
                    "parent.log",
                    vec![message, rt.global("LogSeverity.Warning")?],
                )?;
            }
            rt.this.set("resyncOrdersOnOpen", Js::Bool(true))?;
            rt.this.set(
                "listenKeyRetryDelayMs",
                Js::Number((delay * 2.).min(60_000.)),
            )?;
            let handle = timeout(rt, "connectAndSubscribe", delay, true)?;
            rt.this.set("listenKeyRetryTimer", handle)?;
            rt.this.set("connecting", Js::Bool(false))?;
            return Ok(Js::Undefined);
        }
    };
    rt.this.set("connecting", Js::Bool(false))?;
    rt.this.set("listenKeyRetryDelayMs", Js::Number(5_000.))?;
    if flag(rt, "isDisposed")? || flag(rt, "isForceClosed")? {
        return Ok(Js::Undefined);
    }
    let testnet = rt.path("parent.options.testnet")?.truthy();
    let base = rt
        .global("BASE_WS_URL")?
        .get(if testnet { "testnet" } else { "livenet" })?;
    rt.call_path("detachSocket", vec![])?;
    let ws = rt.call_path(
        "createWebsocket",
        vec![Js::str(&format!(
            "{}/{}",
            base.to_display(),
            component(&listen_key)?
        ))],
    )?;
    rt.this.set("ws", ws.clone())?;
    for (event, member) in [
        ("message", "onMessage"),
        ("close", "onClose"),
        ("open", "onOpen"),
    ] {
        call(
            &ws,
            "addEventListener",
            vec![Js::str(event), rt.this.get(member)?],
        )?;
    }
    Ok(Js::Undefined)
}
fn detach(rt: &Rt) -> Result<(), Thrown> {
    let ws = rt.this.get("ws")?;
    if !ws.truthy() {
        return Ok(());
    }
    for (event, member) in [
        ("message", "onMessage"),
        ("close", "onClose"),
        ("open", "onOpen"),
    ] {
        let remove = ws.get("removeEventListener")?;
        if !remove.is_nullish() {
            remove.call(ws.clone(), vec![Js::str(event), rt.this.get(member)?])?;
        }
    }
    let ready = ws.get("readyState")?;
    if ready.strict_equals(&Js::Number(0.)) || ready.strict_equals(&Js::Number(1.)) {
        call(&ws, "close", vec![])?;
    }
    Ok(())
}
fn on_message(rt: &Rt, data: Js) -> Result<(), Thrown> {
    if flag(rt, "isDisposed")? {
        return Ok(());
    }
    rt.call_path("trackLastReceived", vec![])?;
    rt.call_path("emitWsDiagnosticReceive", vec![data.clone()])?;
    let json = if let Js::String(text) = &data {
        from_retained_input(safe_cex_transport::how::big_int_as_string_json_parse_utf16(
            text,
        ))
    } else {
        data
    };
    let event = optional(&json, "e")?;
    for (kind, stats, method) in [
        ("ACCOUNT_UPDATE", "position", "handleAccountEvents"),
        ("ORDER_TRADE_UPDATE", "order", "handleOrderEvents"),
    ] {
        if event.strict_equals(&Js::str(kind)) {
            rt.call_path("recordMessageHandlerStats", vec![Js::str(stats)])?;
            rt.call_path(method, vec![Js::array(vec![json.clone()])])?;
        }
    }
    if optional(&json, "id")?.strict_equals(&Js::Number(42.)) {
        rt.call_path("handlePongEvent", vec![])?;
    }
    Ok(())
}
fn reject_reason(rt: &Rt, data: &Js) -> Result<Js, Thrown> {
    let reject = global(rt, "String", vec![or(optional(data, "r")?, Js::str(""))])?;
    let reject = call(&reject, "toUpperCase", vec![])?;
    for text in ["POST_ONLY", "GTX", "MAKER"] {
        if call(&reject, "includes", vec![Js::str(text)])?.truthy() {
            return rt.global("OrderRejectReason.PostOnly");
        }
    }
    Ok(Js::Undefined)
}
fn orders(rt: &Rt, events: Js) -> Result<(), Thrown> {
    for event in events
        .array_values()
        .ok_or_else(|| Thrown::type_error("events.forEach is not a function"))?
    {
        let data = event.get("o")?;
        let id_value = data.get("i")?;
        let id = if id_value.is_nullish() {
            Js::Undefined
        } else {
            call(&id_value, "toString", vec![])?
        };
        let amount = data.get("q")?.parse_float()?;
        let filled = data.get("z")?.parse_float()?;
        let status = data.get("X")?;
        if status.strict_equals(&Js::str("PARTIALLY_FILLED"))
            || status.strict_equals(&Js::str("FILLED"))
        {
            rt.call_path(
                "parent.emitter.emit",
                vec![
                    Js::str("fill"),
                    Js::object(vec![
                        ("id", id.clone()),
                        (
                            "side",
                            rt.global("ORDER_SIDE")?.get_property(&data.get("S")?)?,
                        ),
                        ("symbol", data.get("s")?),
                        ("price", Js::Number(data.get("ap")?.parse_float()?)),
                        ("amount", Js::Number(data.get("l")?.parse_float()?)),
                    ]),
                ],
            )?;
            rt.call_path(
                "store.updateOrder",
                vec![
                    Js::object(vec![("id", id.clone())]),
                    Js::object(vec![
                        ("filled", Js::Number(filled)),
                        ("remaining", Js::Number(amount - filled)),
                        (
                            "lastUpdate",
                            global(rt, "virtualClock.getCurrentTimeMs", vec![])?,
                        ),
                    ]),
                ],
            )?;
        }
        if status.strict_equals(&Js::str("NEW")) {
            let kind = rt.global("ORDER_TYPE")?.get_property(&data.get("ot")?)?;
            let info = data.spread_object()?;
            info.set("orderId", id.clone())?;
            let price = data.get("p")?.parse_float()?;
            let order = Js::object(vec![
                ("id", id.clone()),
                ("status", rt.global("OrderStatus.Open")?),
                ("symbol", data.get("s")?),
                ("type", kind.clone()),
                (
                    "side",
                    rt.global("ORDER_SIDE")?.get_property(&data.get("S")?)?,
                ),
                (
                    "price",
                    Js::Number(if price != 0. && !price.is_nan() {
                        price
                    } else {
                        data.get("sp")?.parse_float()?
                    }),
                ),
                ("amount", Js::Number(amount)),
                ("filled", Js::Number(filled)),
                (
                    "lastUpdate",
                    global(rt, "virtualClock.getCurrentTimeMs", vec![])?,
                ),
                ("remaining", Js::Number(amount - filled)),
                ("reduceOnly", or(data.get("R")?, Js::Bool(false))),
                ("close", or(data.get("cp")?, Js::Bool(false))),
                ("info", info),
            ]);
            for (key, target) in [
                ("TakeProfit", "tpTriggerType"),
                ("StopLoss", "slTriggerType"),
            ] {
                if kind.strict_equals(&rt.global(&format!("OrderType.{key}"))?) {
                    let wt = global(rt, "v", vec![data.clone(), Js::str("wt")])?;
                    order.set(target, rt.global("ORDER_TRIGGER_TYPE")?.get_property(&wt)?)?;
                }
            }
            rt.call_path("store.addOrUpdateOrder", vec![order])?;
        }
        if ["CANCELED", "EXPIRED", "CANCELLED"]
            .iter()
            .any(|s| status.strict_equals(&Js::str(s)))
        {
            rt.call_path(
                "store.updateOrderStatus",
                vec![
                    Js::object(vec![("id", id.clone())]),
                    rt.global("OrderStatus.Canceled")?,
                ],
            )?;
            let reason = rt.call_path("getOrderRejectReason", vec![data.clone()])?;
            if reason.truthy() {
                let mut existing = Js::Undefined;
                for candidate in rt
                    .path("store.orders")?
                    .array_values()
                    .ok_or_else(|| Thrown::type_error("orders.find is not a function"))?
                {
                    if candidate.get("id")?.strict_equals(&id) {
                        existing = candidate;
                        break;
                    }
                }
                let extra = optional(&existing, "extra")?.spread_object()?;
                extra.set("rejectReason", reason)?;
                rt.call_path(
                    "store.updateOrder",
                    vec![
                        Js::object(vec![("id", id.clone())]),
                        Js::object(vec![("extra", extra)]),
                    ],
                )?;
            }
        }
        if status.strict_equals(&Js::str("FILLED")) {
            rt.call_path(
                "store.updateOrderStatus",
                vec![
                    Js::object(vec![("id", id)]),
                    rt.global("OrderStatus.Closed")?,
                ],
            )?;
        }
    }
    rt.call_path("throttledFetchOrders", vec![])?;
    Ok(())
}
fn accounts(rt: &Rt, events: Js) -> Result<(), Thrown> {
    for event in events
        .array_values()
        .ok_or_else(|| Thrown::type_error("events.forEach is not a function"))?
    {
        let positions = optional(&event.get("a")?, "P")?;
        if optional(&positions, "length")?.truthy() {
            rt.call_path("parent.markPositionStateUpdatedByWebsocket", vec![])?;
            rt.this.set("hasPendingPositionEvents", Js::Bool(true))?;
            rt.call_path("throttledFetchPositions", vec![])?;
        }
        if positions.is_nullish() {
            continue;
        }
        for p in positions
            .array_values()
            .ok_or_else(|| Thrown::type_error("P.forEach is not a function"))?
        {
            let symbol = p.get("s")?;
            let signed = p.get("pa")?.parse_float()?;
            let mode = rt.global(if p.get("ps")?.strict_equals(&Js::str("BOTH")) {
                "PositionMode.OneWay"
            } else {
                "PositionMode.Hedged"
            })?;
            let event_side = or(
                rt.global("POSITION_SIDE")?.get_property(&p.get("ps")?)?,
                rt.global(if signed >= 0. {
                    "PositionSide.Long"
                } else {
                    "PositionSide.Short"
                })?,
            );
            let id = global(
                rt,
                "createPositionId",
                vec![symbol.clone(), event_side.clone(), mode.clone()],
            )?;
            let mut existing = Js::Undefined;
            for candidate in rt
                .path("parent.store.positions")?
                .array_values()
                .ok_or_else(|| Thrown::type_error("positions.find is not a function"))?
            {
                if candidate.get("id")?.strict_equals(&id) {
                    existing = candidate;
                    break;
                }
            }
            if !existing.truthy() && signed == 0. {
                continue;
            }
            let market = rt
                .path("parent.store.marketsBySymbol")?
                .get_property(&symbol)?;
            let ticker = rt
                .path("parent.store.tickersBySymbol")?
                .get_property(&symbol)?;
            let entry = p.get("ep")?.parse_float()?;
            let upnl = p.get("up")?.parse_float()?;
            let reference = or(
                optional(&ticker, "mark")?,
                or(optional(&ticker, "last")?, Js::Number(entry)),
            )
            .to_number()?;
            let native = signed.abs();
            let size = or(optional(&market, "contractSize")?, Js::Number(1.)).to_number()?;
            let base = if reference > 0. {
                native * size / reference
            } else {
                or(optional(&existing, "contracts")?, Js::Number(0.)).to_number()?
            };
            let side = if signed == 0. && existing.truthy() {
                existing.get("side")?
            } else {
                event_side
            };
            let position = existing.spread_object()?;
            let info = optional(&existing, "info")?.spread_object()?;
            for (key, value) in p.own_entries()? {
                info.set_key(&key, value)?;
            }
            info.set("contractsNative", Js::Number(native))?;
            for (key, value) in [
                ("id", id),
                ("symbol", symbol),
                ("side", side),
                ("entryPrice", Js::Number(entry)),
                ("contracts", Js::Number(base)),
                ("notional", Js::Number(native * size)),
                ("unrealizedPnl", Js::Number(upnl)),
                (
                    "leverage",
                    nullish(optional(&existing, "leverage")?, Js::Number(0.)),
                ),
                (
                    "liquidationPrice",
                    nullish(optional(&existing, "liquidationPrice")?, Js::Number(0.)),
                ),
                (
                    "marginType",
                    nullish(
                        optional(&existing, "marginType")?,
                        rt.global("MarginType.Cross")?,
                    ),
                ),
                ("positionMode", mode),
                ("info", info),
            ] {
                position.set(key, value)?;
            }
            rt.call_path("store.addOrUpdatePosition", vec![position])?;
        }
    }
    Ok(())
}
fn throttle_positions(rt: &Rt) -> Result<(), Thrown> {
    let now = global(rt, "Date.now", vec![])?.to_number()?;
    if flag(rt, "isDisposed")?
        || rt
            .call_path("parent.isBackgroundReconciliationPaused", vec![])?
            .truthy()
    {
        rt.this.set("hasPendingPositionEvents", Js::Bool(false))?;
        return Ok(());
    }
    if flag(rt, "positionFetchScheduled")? {
        return Ok(());
    }
    let since = now - rt.this.get("lastPositionFetchTime")?.to_number()?;
    let throttle = rt.this.get("positionFetchThrottleMs")?.to_number()?;
    if since >= throttle {
        rt.call_path("executePositionFetch", vec![])?;
    } else {
        rt.this.set("positionFetchScheduled", Js::Bool(true))?;
        let weak = Rc::downgrade(&rt.shared);
        let this = rt.this.clone();
        rt.set_timeout(
            Rc::new(move || {
                let rt = scope_of(&weak)?.with_this(this.clone());
                rt.this.set("positionFetchScheduled", Js::Bool(false))?;
                if !flag(&rt, "isDisposed")? && flag(&rt, "hasPendingPositionEvents")? {
                    rt.call_path("executePositionFetch", vec![])?;
                }
                Ok(())
            }),
            throttle - since,
        );
    }
    Ok(())
}
// The runtime owns discarded async calls; cancelling that owner must release
// the native in-flight latch as well as the source's normal finally path.
struct PositionFlight {
    socket: Js,
    armed: bool,
}
impl Drop for PositionFlight {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.socket.set("positionFetchInFlight", Js::Bool(false));
        }
    }
}
async fn execute_positions(rt: &Rt) -> Result<Js, Thrown> {
    if flag(rt, "positionFetchInFlight")?
        || flag(rt, "isDisposed")?
        || rt
            .call_path("parent.isBackgroundReconciliationPaused", vec![])?
            .truthy()
    {
        return Ok(Js::Undefined);
    }
    let version = rt.call_path("parent.getPositionStateVersion", vec![])?;
    rt.this.set("positionFetchInFlight", Js::Bool(true))?;
    let mut flight = PositionFlight {
        socket: rt.this.clone(),
        armed: true,
    };
    rt.this
        .set("lastPositionFetchTime", global(rt, "Date.now", vec![])?)?;
    rt.this.set("hasPendingPositionEvents", Js::Bool(false))?;
    let _ = async {
        let positions = await_js(rt.call_path("parent.fetchPositions", vec![])?).await?;
        if !flag(rt, "isDisposed")?
            && version.strict_equals(&rt.call_path("parent.getPositionStateVersion", vec![])?)
        {
            rt.call_path("store.addOrUpdatePositionsFull", vec![positions])?;
        }
        Ok::<_, Thrown>(())
    }
    .await;
    rt.this.set("positionFetchInFlight", Js::Bool(false))?;
    flight.armed = false;
    if !flag(rt, "isDisposed")? && flag(rt, "hasPendingPositionEvents")? {
        rt.call_path("throttledFetchPositions", vec![])?;
    }
    Ok(Js::Undefined)
}
async fn fetch_listen_key(rt: &Rt) -> Result<Js, Thrown> {
    let reply =
        await_js(rt.call_path("parent.xhr.post", vec![rt.global("ENDPOINTS.LISTEN_KEY")?])?)
            .await?;
    let data = reply.get("data")?;
    rt.call_path("scheduleListenKeyRefresh", vec![Js::Number(1_800_000.)])?;
    data.get("listenKey")
}
fn schedule_refresh(rt: &Rt, delay: f64) -> Result<(), Thrown> {
    rt.clear_timeout(&rt.this.get("listenKeyRefreshTimer")?);
    let handle = timeout(rt, "updateListenKey", delay, false)?;
    rt.this.set("listenKeyRefreshTimer", handle)
}
async fn update_listen_key(rt: &Rt) -> Result<Js, Thrown> {
    if flag(rt, "isDisposed")? {
        return Ok(Js::Undefined);
    }
    let result = async {
        await_js(rt.call_path("parent.xhr.put", vec![rt.global("ENDPOINTS.LISTEN_KEY")?])?).await?;
        rt.call_path("scheduleListenKeyRefresh", vec![Js::Number(1_800_000.)])?;
        rt.this.set("keepaliveFailing", Js::Bool(false))
    }
    .await;
    if let Err(error) = result {
        let message = ops::add(
            &Js::str("Binance COIN-M user data stream keepalive failed: "),
            &error_text(rt, &error)?,
        )?;
        if flag(rt, "keepaliveFailing")? {
            rt.call_path(
                "parent.log",
                vec![message, rt.global("LogSeverity.Warning")?],
            )?;
        } else {
            rt.call_path("emitAndLogError", vec![message])?;
        }
        rt.this.set("keepaliveFailing", Js::Bool(true))?;
        rt.call_path("scheduleListenKeyRefresh", vec![Js::Number(60_000.)])?;
    }
    Ok(Js::Undefined)
}
async fn fetch_orders(rt: &Rt) -> Result<Js, Thrown> {
    rt.this.set("ordersThrottleTimeout", Js::Undefined)?;
    if flag(rt, "isDisposed")? {
        return Ok(Js::Undefined);
    }
    let _ = async {
        let orders = await_js(rt.call_path("parent.fetchOrders", vec![])?).await?;
        if flag(rt, "isDisposed")? || rt.path("parent.ordersSnapshotStale")?.truthy() {
            return Ok(());
        }
        let loaded = rt.path("parent.store.loaded")?.spread_object()?;
        loaded.set("orders", Js::Bool(true))?;
        rt.call_path(
            "parent.store.update",
            vec![Js::object(vec![("orders", orders), ("loaded", loaded)])],
        )?;
        Ok::<_, Thrown>(())
    }
    .await;
    Ok(Js::Undefined)
}
