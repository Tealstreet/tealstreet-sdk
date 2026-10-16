//! Socket dispatch/authentication and reconciliation bodies. Native operations
//! and independently owned started work are supplied by the runtime; async bodies
//! are polled at the original execution boundary. Values retain native aliases.
use std::{future::Future, pin::Pin};
pub type SocketFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
pub trait SocketRuntimeHost {
    type Value: Clone;
    type Error;
    type Iterator;
    fn receiver(&self, path: &str) -> Result<Self::Value, Self::Error>;
    fn set_receiver(&self, path: &str, value: Self::Value) -> Result<(), Self::Error>;
    fn capture(&self, path: &str) -> Result<Self::Value, Self::Error>;
    fn capture_member(&self, value: &Self::Value, name: &str) -> Result<Self::Value, Self::Error>;
    fn invoke(
        &self,
        method: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn invoke_bare(&self, callback: Self::Value, args: Vec<Self::Value>)
    -> Result<(), Self::Error>;
    fn start(
        &self,
        method: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<SocketFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn property(&self, value: &Self::Value, name: &str) -> Result<Self::Value, Self::Error>;
    fn key_get(&self, value: &Self::Value, key: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn string(&self, value: &[u16]) -> Self::Value;
    fn undefined(&self) -> Self::Value;
    fn array(&self, value: Vec<Self::Value>) -> Self::Value;
    fn object(&self, fields: Vec<(&str, Self::Value)>) -> Self::Value;
    fn spread(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn define(&self, value: &Self::Value, key: &str, field: Self::Value)
    -> Result<(), Self::Error>;
    fn iterator(&self, value: &Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next(&self, iter: &mut Self::Iterator) -> Result<Option<Self::Value>, Self::Error>;
    fn filter<F>(&self, value: Self::Value, callback: F) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value, usize) -> Result<bool, Self::Error>;
    fn map<F>(&self, value: Self::Value, callback: F) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value, usize) -> Result<Self::Value, Self::Error>;
    /// Original imported/native operation, including coercion, map/set construction,
    /// template interpolation, JSON and crypto, preserving errors and native values.
    fn operation(&self, name: &str, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
}
fn s<H: SocketRuntimeHost>(h: &H, v: &str) -> H::Value {
    h.string(&v.encode_utf16().collect::<Vec<_>>())
}
fn p<H: SocketRuntimeHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.property(v, k)
}
fn opt<H: SocketRuntimeHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        p(h, v, k)
    }
}
fn call<H: SocketRuntimeHost>(
    h: &H,
    path: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let f = h.capture(path)?;
    h.invoke(f, args)
}
fn contains<H: SocketRuntimeHost>(h: &H, data: &H::Value, needle: &str) -> Result<bool, H::Error> {
    let f = h.capture_member(data, "includes")?;
    Ok(h.truthy(&h.invoke(f, vec![s(h, needle)])?))
}
fn increment<H: SocketRuntimeHost>(h: &H, path: &str) -> Result<H::Value, H::Error> {
    let old = h.receiver(path)?;
    let next = h.operation("increment", vec![old])?;
    h.set_receiver(path, next.clone())?;
    Ok(next)
}
fn append<H: SocketRuntimeHost>(
    h: &H,
    out: &mut Vec<H::Value>,
    v: H::Value,
) -> Result<(), H::Error> {
    let mut i = h.iterator(&v)?;
    while let Some(v) = h.next(&mut i)? {
        out.push(v)
    }
    Ok(())
}
fn dispatch<H: SocketRuntimeHost>(
    h: &H,
    stats: &str,
    handler: &str,
    json: H::Value,
) -> Result<(), H::Error> {
    call(h, "recordMessageHandlerStats", vec![s(h, stats)])?;
    call(h, handler, vec![json])?;
    Ok(())
}
// ts: BitgetPrivateWebsocket.connectAndSubscribe
pub fn connect_and_subscribe<H: SocketRuntimeHost>(h: &H) -> Result<(), H::Error> {
    if h.truthy(&h.receiver("isDisposed")?) {
        return Ok(());
    }
    if h.equal(&h.receiver("lastMessageCount")?, &h.number(0.))
        && h.equal(&h.receiver("messageCount")?, &h.number(0.))
    {
        increment(h, "connectionAttempts")?;
        if h.truthy(&h.operation(
            "greaterEqual",
            vec![
                h.receiver("connectionAttempts")?,
                h.receiver("MAX_ATTEMPTS")?,
            ],
        )?) {
            h.set_receiver("useFallbackUrl", h.boolean(true))?;
        }
    }
    let count = h.receiver("messageCount")?;
    h.set_receiver("lastMessageCount", count)?;
    h.set_receiver("messageCount", h.number(0.))?;
    let uta = h.receiver("parent.usesUtaRouting")?;
    let routing = h.operation(
        "resolveTransportRouting",
        vec![h.receiver("parent.name")?, h.receiver("parent.options")?],
    )?;
    let fallback = p(h, &p(h, &routing, "wsPrivate")?, "fallback")?;
    let use_fallback =
        !h.truthy(&uta) && h.truthy(&h.receiver("useFallbackUrl")?) && h.truthy(&fallback);
    let url = if use_fallback {
        fallback
    } else {
        h.operation(
            if h.truthy(&uta) {
                "BASE_WS_PRIVATE_UTA_URL"
            } else {
                "BASE_WS_PRIVATE_URL"
            },
            vec![],
        )?
    };
    if use_fallback {
        let _ = call(
            h,
            "parent.emitter.emit",
            vec![
                s(h, "grafana-info"),
                s(h, "Using tealstreet bitget proxy (private)"),
            ],
        );
    }
    let create = h.capture("createWebsocket")?;
    let ws = h.invoke(create, vec![url])?;
    h.set_receiver("ws", ws)?;
    for (event, handler) in [
        ("open", "onOpen"),
        ("message", "onMessage"),
        ("close", "onClose"),
    ] {
        let add = h.capture("ws.addEventListener")?;
        let cb = h.receiver(handler)?;
        h.invoke(add, vec![s(h, event), cb])?;
    }
    Ok(())
}
// ts: BitgetPrivateWebsocket.onMessage; argument prologue supplies data.
pub fn on_private_message<H: SocketRuntimeHost>(h: &H, data: H::Value) -> Result<(), H::Error> {
    if !h.truthy(&h.receiver("isDisposed")?) {
        call(h, "trackLastReceived", vec![])?;
        call(h, "emitWsDiagnosticReceive", vec![data.clone()])?;
        if h.equal(&data, &s(h, "pong")) {
            call(h, "handlePongEvent", vec![])?;
            return Ok(());
        }
        increment(h, "messageCount")?;
    }
    // Source parses and dispatches even when disposed.
    let json = h.operation("jsonParse", vec![data])?;
    if !h.truthy(&json) {
        return Ok(());
    }
    let normalize = h.capture("normalizeChannel")?;
    let channel = h.invoke(normalize, vec![opt(h, &opt(h, &json, "arg")?, "channel")?])?;
    if h.equal(&opt(h, &json, "event")?, &s(h, "login")) {
        let code = h.operation("template", vec![p(h, &json, "code")?])?;
        if h.equal(&code, &s(h, "0")) {
            call(h, "subscribe", vec![])?;
        } else {
            let msg = p(h, &json, "msg")?;
            let has = if h.truthy(&msg) {
                msg
            } else {
                p(h, &json, "code")?
            };
            if h.truthy(&has) {
                let emit = h.capture("emitAndLogError")?;
                let msg = p(h, &json, "msg")?;
                let value = if h.truthy(&msg) {
                    msg
                } else {
                    p(h, &json, "code")?
                };
                let text = h.operation(
                    "template",
                    vec![s(h, "Bitget private WS login failed: "), value],
                )?;
                h.invoke(emit, vec![text])?;
            }
        }
        return Ok(());
    }
    for (topic, handler) in [
        ("order", "handleUtaOrderTopic"),
        ("position", "handleUtaPositionsTopic"),
        ("account", "handleUtaAccountTopic"),
    ] {
        if h.equal(&opt(h, &opt(h, &json, "arg")?, "topic")?, &s(h, topic))
            && !h.equal(&opt(h, &json, "event")?, &s(h, "subscribe"))
        {
            return dispatch(h, topic, handler, json);
        }
    }
    for (expected, stats, handler) in [
        ("orders", "order", "handleOrderTopic"),
        ("ordersalgo", "order-algo", "handleAlgoOrdersTopic"),
        ("positions", "position", "handlePositionsTopic"),
        ("account", "account", "handleAccountTopic"),
    ] {
        if h.equal(&channel, &s(h, expected))
            && !h.equal(&opt(h, &json, "event")?, &s(h, "subscribe"))
        {
            return dispatch(h, stats, handler, json);
        }
    }
    Ok(())
}
fn callbacks<H: SocketRuntimeHost>(h: &H, topic: H::Value, json: H::Value) -> Result<(), H::Error> {
    let stats = h.capture("recordMessageHandlerStats")?;
    h.invoke(stats, vec![topic.clone()])?;
    let handlers = h.receiver("messageHandlers")?;
    if h.truthy(&h.key_get(&handlers, &topic)?) {
        let handlers = h.receiver("messageHandlers")?;
        let values = h.key_get(&handlers, &topic)?;
        let mut i = h.iterator(&values)?;
        while let Some(cb) = h.next(&mut i)? {
            h.invoke_bare(cb, vec![json.clone()])?;
        }
    }
    Ok(())
}
// ts: BitgetPublicWebsocket.onMessage; argument prologue supplies data.
pub fn on_public_message<H: SocketRuntimeHost>(h: &H, data: H::Value) -> Result<(), H::Error> {
    if h.truthy(&h.receiver("isDisposed")?) {
        return Ok(());
    }
    call(h, "trackLastReceived", vec![])?;
    increment(h, "messageCount")?;
    if h.equal(&data, &s(h, "pong")) {
        call(h, "handlePongEvent", vec![])?;
        return Ok(());
    }
    if contains(h, &data, "\"channel\":\"ticker\"")? {
        let json = h.operation("jsonParse", vec![data.clone()])?;
        if h.equal(&opt(h, &opt(h, &json, "arg")?, "channel")?, &s(h, "ticker"))
            && !h.equal(&opt(h, &json, "event")?, &s(h, "subscribe"))
        {
            dispatch(h, "ticker", "handleTickerTopic", json)?;
        }
        return Ok(());
    }
    if contains(h, &data, "\"action\":\"update\"")? && contains(h, &data, "\"channel\":\"candle")? {
        let json = h.operation("jsonParse", vec![data.clone()])?;
        if h.truthy(&json) {
            let channel = p(h, &p(h, &json, "arg")?, "channel")?;
            let replace = h.capture_member(&channel, "replace")?;
            let interval = h.invoke(replace, vec![s(h, "candle"), s(h, "")])?;
            let topic = h.operation(
                "template",
                vec![
                    s(h, "candle_"),
                    p(h, &p(h, &json, "arg")?, "instId")?,
                    s(h, "_"),
                    interval,
                ],
            )?;
            callbacks(h, topic, json)?;
        }
    }
    if contains(h, &data, "\"channel\":\"books\"")? {
        let json = h.operation("jsonParse", vec![data])?;
        if h.truthy(&json) {
            let topic = h.operation(
                "template",
                vec![s(h, "orderBook_"), p(h, &p(h, &json, "arg")?, "instId")?],
            )?;
            callbacks(h, topic, json)?;
        }
    }
    Ok(())
}
// ts: BitgetPrivateWebsocket#executePositionFetch
pub async fn execute_position_fetch<H: SocketRuntimeHost>(h: &H) -> Result<(), H::Error> {
    let _result: Result<(), H::Error> = async {
        let now = h.operation("Date.now", vec![])?;
        h.set_receiver("lastPositionFetchTime", now)?;
        let start = h.operation("clock.ms", vec![])?;
        h.set_receiver("hasPendingPositionEvents", h.boolean(false))?;
        let fetch = h.capture("parent.fetchPositions")?;
        let positions = h.start(fetch, vec![])?.await?;
        if !h.truthy(&h.receiver("isDisposed")?) {
            let update = h.capture("store.addOrUpdatePositionsFull")?;
            let merge = h.capture("mergeRestPositionsWithFreshWsState")?;
            let merged = h.invoke(merge, vec![positions, start])?;
            h.invoke(update, vec![merged])?;
        }
        Ok(())
    }
    .await;
    Ok(())
}
// ts: BitgetPrivateWebsocket#executeBalanceFetch
pub async fn execute_balance_fetch<H: SocketRuntimeHost>(h: &H) -> Result<(), H::Error> {
    let _result: Result<(), H::Error> = async {
        let now = h.operation("Date.now", vec![])?;
        h.set_receiver("lastBalanceFetchTime", now)?;
        h.set_receiver("hasPendingBalanceEvents", h.boolean(false))?;
        let fetch = h.capture("parent.fetchBalance")?;
        let balance = h
            .start(fetch, vec![h.object(vec![("preferFast", h.boolean(true))])])?
            .await?;
        if !h.truthy(&h.receiver("isDisposed")?) {
            let update = h.capture("store.update")?;
            let loaded = h.spread(&h.receiver("store.loaded")?)?;
            h.define(&loaded, "balance", h.boolean(true))?;
            let patch = h.object(vec![("balance", balance), ("loaded", loaded)]);
            h.invoke(update, vec![patch])?;
        }
        Ok(())
    }
    .await;
    Ok(())
}
// ts: BitgetPrivateWebsocket#executeOrdersFetch
pub async fn execute_orders_fetch<H: SocketRuntimeHost>(h: &H) -> Result<(), H::Error> {
    let _result: Result<(), H::Error> = async {
        let seq = increment(h, "ordersFetchRequestSeq")?;
        let now = h.operation("Date.now", vec![])?;
        h.set_receiver("lastOrdersFetchTime", now)?;
        h.set_receiver("hasPendingOrderEvents", h.boolean(false))?;
        let fetch = h.capture("parent.fetchOrders")?;
        let fetched = h.start(fetch, vec![])?.await?;
        if h.truthy(&h.receiver("isDisposed")?)
            || !h.equal(&seq, &h.receiver("ordersFetchRequestSeq")?)
        {
            return Ok(());
        }
        let now = h.operation("Date.now", vec![])?;
        call(h, "pruneOrderLifecycleCaches", vec![now.clone()])?;
        let current = h.map(h.receiver("store.orders")?, |order, _| {
            Ok(h.array(vec![p(h, &order, "id")?, order]))
        })?;
        let by_id = h.operation("newMap", vec![current])?;
        let reconciled = h.filter(fetched, |order, _| {
            let open = h.equal(
                &p(h, &order, "status")?,
                &h.operation("OrderStatus.Open", vec![])?,
            );
            if open {
                let get = h.capture("terminalOrderTombstones.get")?;
                let tomb = h.invoke(get, vec![p(h, &order, "id")?])?;
                if h.truthy(&tomb) && h.truthy(&h.operation("greater", vec![tomb, now.clone()])?) {
                    return Ok(false);
                }
            }
            let get = h.capture_member(&by_id, "get")?;
            let current = h.invoke(get, vec![p(h, &order, "id")?])?;
            if !h.truthy(&current) {
                return Ok(true);
            }
            let terminal = h.equal(
                &p(h, &current, "status")?,
                &h.operation("OrderStatus.Canceled", vec![])?,
            ) || h.equal(
                &p(h, &current, "status")?,
                &h.operation("OrderStatus.Closed", vec![])?,
            ) || h.equal(
                &p(h, &current, "status")?,
                &h.operation("OrderStatus.Canceling", vec![])?,
            );
            if terminal && open {
                return Ok(false);
            }
            Ok(true)
        })?;
        let ids = h.map(reconciled.clone(), |order, _| p(h, &order, "id"))?;
        let fetched_ids = h.operation("newSet", vec![ids])?;
        let preserved = h.filter(h.receiver("store.orders")?, |order, _| {
            let has = h.capture_member(&fetched_ids, "has")?;
            if h.truthy(&h.invoke(has, vec![p(h, &order, "id")?])?) {
                return Ok(false);
            }
            if !h.equal(
                &p(h, &order, "status")?,
                &h.operation("OrderStatus.Open", vec![])?,
            ) {
                return Ok(false);
            }
            if h.equal(
                &p(h, &order, "type")?,
                &h.operation("OrderType.StopLoss", vec![])?,
            ) || h.equal(
                &p(h, &order, "type")?,
                &h.operation("OrderType.TakeProfit", vec![])?,
            ) || h.equal(
                &p(h, &order, "type")?,
                &h.operation("OrderType.TrailingStopLoss", vec![])?,
            ) {
                return Ok(true);
            }
            let get = h.capture("recentWsOpenOrders.get")?;
            let expiry = h.invoke(get, vec![p(h, &order, "id")?])?;
            let expiry = if h.nullish(&expiry) {
                h.number(0.)
            } else {
                expiry
            };
            Ok(h.truthy(&h.operation("greater", vec![expiry, now.clone()])?))
        })?;
        let update = h.capture("store.addOrUpdateOrdersFull")?;
        let mut orders = vec![];
        append(h, &mut orders, reconciled)?;
        append(h, &mut orders, preserved)?;
        h.invoke(update, vec![h.array(orders)])?;
        Ok(())
    }
    .await;
    Ok(())
}
fn login<H: SocketRuntimeHost>(
    h: &H,
    timestamp: H::Value,
    signature: H::Value,
) -> Result<H::Value, H::Error> {
    let key = h.receiver("parent.options.key")?;
    let pass = h.receiver("parent.options.passphrase")?;
    Ok(h.object(vec![
        ("op", s(h, "login")),
        (
            "args",
            h.array(vec![h.object(vec![
                ("apiKey", key),
                ("passphrase", pass),
                ("timestamp", timestamp),
                ("sign", signature),
            ])]),
        ),
    ]))
}
// ts: BitgetPrivateWebsocket#auth
pub async fn auth<H: SocketRuntimeHost>(h: &H) -> Result<(), H::Error> {
    let timestamp = h.operation("clock.unix", vec![])?;
    if h.truthy(&h.receiver("parent.options.remoteSigner")?) {
        let signer = h.capture("parent.options.remoteSigner.sign")?;
        let id = h.operation("nanoid", vec![])?;
        let account = h.receiver("parent.options.remoteSigner.accountName")?;
        let to_sign = h.operation("template", vec![timestamp.clone(), s(h, "GET/user/verify")])?;
        let request = h.object(vec![
            ("id", id),
            ("type", s(h, "ws-auth")),
            ("exchangeName", s(h, "bitget")),
            ("accountName", account),
            (
                "wsAuth",
                h.object(vec![
                    ("toSign", to_sign),
                    ("outputFormat", s(h, "base64")),
                    (
                        "extraData",
                        h.object(vec![("timestamp", timestamp.clone())]),
                    ),
                ]),
            ),
        ]);
        let result = h.start(signer, vec![request])?.await?;
        if h.truthy(&p(h, &result, "error")?) {
            let emit = h.capture("emitAndLogError")?;
            let error = p(h, &result, "error")?;
            let text = h.operation(
                "template",
                vec![s(h, "Remote WS signing unavailable: "), error],
            )?;
            h.invoke(emit, vec![text])?;
            return Ok(());
        }
        let send = h.capture("sendWithLog")?;
        // Source signature getter is after key/passphrase getters in the literal.
        let key = h.receiver("parent.options.key")?;
        let pass = h.receiver("parent.options.passphrase")?;
        let signature = p(h, &result, "wsAuthSignature")?;
        let payload = h.object(vec![
            ("op", s(h, "login")),
            (
                "args",
                h.array(vec![h.object(vec![
                    ("apiKey", key),
                    ("passphrase", pass),
                    ("timestamp", timestamp),
                    ("sign", signature),
                ])]),
            ),
        ]);
        let text = h.operation("JSON.stringify", vec![payload])?;
        h.invoke(send, vec![text])?;
        return Ok(());
    }
    let secret = h.receiver("parent.options.secret")?;
    let mac = h.operation("createHmac", vec![s(h, "sha256"), secret])?;
    let update = h.capture_member(&mac, "update")?;
    let to_sign = h.operation("template", vec![timestamp.clone(), s(h, "GET/user/verify")])?;
    let mac = h.invoke(update, vec![to_sign])?;
    let digest = h.capture_member(&mac, "digest")?;
    let sign = h.invoke(digest, vec![s(h, "base64")])?;
    let payload = login(h, timestamp, sign)?;
    let send = h.capture("sendWithLog")?;
    let text = h.operation("JSON.stringify", vec![payload])?;
    h.invoke(send, vec![text])?;
    Ok(())
}
// ts: BitgetPrivateWebsocket#subscribe
pub fn subscribe<H: SocketRuntimeHost>(h: &H) -> Result<(), H::Error> {
    if h.truthy(&h.receiver("parent.usesUtaRouting")?) {
        let send = h.capture("sendWithLog")?;
        let args = ["account", "position", "order"]
            .into_iter()
            .map(|topic| h.object(vec![("instType", s(h, "UTA")), ("topic", s(h, topic))]))
            .collect();
        let payload = h.object(vec![("op", s(h, "subscribe")), ("args", h.array(args))]);
        let text = h.operation("JSON.stringify", vec![payload])?;
        h.invoke(send, vec![text])?;
        return Ok(());
    }
    let mut args = vec![];
    for (channel, key) in [
        ("account", "coin"),
        ("positions", "instId"),
        ("orders", "instId"),
        ("orders-algo", "instId"),
    ] {
        let product = h.receiver("parent.apiProductType")?;
        let upper = h.operation("toUpperCase", vec![product])?;
        args.push(h.object(vec![
            ("instType", upper),
            ("channel", s(h, channel)),
            (key, s(h, "default")),
        ]));
    }
    let payload = h.object(vec![("op", s(h, "subscribe")), ("args", h.array(args))]);
    let send = h.capture("sendWithLog")?;
    let text = h.operation("JSON.stringify", vec![payload])?;
    h.invoke(send, vec![text])?;
    Ok(())
}
