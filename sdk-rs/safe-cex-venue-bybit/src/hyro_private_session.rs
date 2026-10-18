//! HyroTrader session bodies over identity-preserving runtime capabilities.
use std::{future::Future, pin::Pin};
pub type SessionFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
pub trait SessionHost {
    type Value: Clone;
    type Error: Clone;
    fn get(&self, owner: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn optional(&self, owner: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn set(&self, owner: &Self::Value, key: &str, value: Self::Value) -> Result<(), Self::Error>;
    // Standard own data property: writable, enumerable and configurable.
    fn define(&self, owner: &Self::Value, key: &str, value: Self::Value)
    -> Result<(), Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn undefined(&self) -> Self::Value;
    fn literal(&self, value: &str) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn numeric(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn units(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn text(&self, units: Vec<u16>) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn global(&self) -> Self::Value;
    fn select(&self, owner: Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn receiver_call(
        &self,
        owner: Self::Value,
        callable: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn apply(
        &self,
        selected: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn wait(&self, value: Self::Value) -> SessionFuture<'_, Self::Value, Self::Error>;
    fn intrinsic(&self, name: &str, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn allocate(&self, parent: Self::Value) -> Result<Self::Value, Self::Error>;
    // Fresh lexical callback; dispatch routes codes to these owned bodies.
    fn callback(
        &self,
        owner: Self::Value,
        code: &'static str,
        captures: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    // Allocates a pending Promise with its original resolve/reject functions.
    fn promise(&self) -> Result<(Self::Value, Self::Value, Self::Value), Self::Error>;
    fn error(&self, message: Self::Value) -> Result<Self::Error, Self::Error>;
    fn error_value(&self, error: Self::Error) -> Self::Value;
    fn normalize_error(&self, error: Self::Error) -> Result<Self::Value, Self::Error>;
}
macro_rules! call {
    ($h:expr,$owner:expr,$key:expr,$args:expr) => {{
        let selected = $h.select($owner, $key)?;
        $h.apply(selected, $args)?
    }};
}
fn concat<H: SessionHost>(h: &H, parts: Vec<H::Value>) -> Result<H::Value, H::Error> {
    let mut units = vec![];
    for part in parts {
        units.extend(h.units(&part)?);
    }
    Ok(h.text(units))
}
fn increment<H: SessionHost>(h: &H, owner: &H::Value, name: &str) -> Result<(), H::Error> {
    let next = h.numeric(&h.get(owner, name)?)? + 1.0;
    h.set(owner, name, h.number(next))
}
pub fn allocate_session<H: SessionHost>(h: &H, parent: H::Value) -> Result<H::Value, H::Error> {
    let owner = h.allocate(parent)?;
    h.define(&owner, "recoversAfterOffline", h.boolean(false))?;
    h.define(&owner, "ready", h.boolean(false))?;
    h.define(&owner, "started", h.boolean(false))?;
    h.define(&owner, "stateRevision", h.number(0.0))?;
    h.define(&owner, "generation", h.number(0.0))?;
    h.define(&owner, "pendingAcks", h.intrinsic("new Map", vec![])?)?;
    h.define(
        &owner,
        "sendReconnectSequence",
        h.callback(owner.clone(), "reconnect", vec![])?,
    )?;
    Ok(owner)
}
pub fn session_version<H: SessionHost>(h: &H, owner: &H::Value) -> Result<H::Value, H::Error> {
    h.get(owner, "generation")
}
pub fn is_recovery_complete<H: SessionHost>(h: &H, owner: &H::Value) -> Result<H::Value, H::Error> {
    if !h.truthy(&h.get(owner, "started")?) {
        Ok(h.boolean(true))
    } else {
        h.get(owner, "ready")
    }
}
pub fn resyncs_private_state_on_reopen<H: SessionHost>(h: &H) -> H::Value {
    h.boolean(false)
}
pub fn constructor<H: SessionHost>(h: &H, owner: &H::Value) -> Result<(), H::Error> {
    let order = h.get(owner, "handleOrderTopic")?;
    h.set(
        owner,
        "handleOrderTopic",
        h.callback(owner.clone(), "order", vec![order])?,
    )?;
    let message = h.get(owner, "onMessage")?;
    let close = h.get(owner, "onClose")?;
    let connect = h.get(owner, "connectAndSubscribe")?;
    let ping = h.get(owner, "ping")?;
    for (key, code, captured) in [
        ("connectAndSubscribe", "connect", connect),
        ("onClose", "close", close),
        ("ping", "ping", ping),
        ("onMessage", "message", message),
    ] {
        h.set(owner, key, h.callback(owner.clone(), code, vec![captured])?)?;
    }
    Ok(())
}
pub fn reset_session<H: SessionHost>(h: &H, owner: &H::Value) -> Result<(), H::Error> {
    increment(h, owner, "generation")?;
    increment(h, owner, "stateRevision")?;
    h.set(owner, "ready", h.boolean(false))?;
    h.set(owner, "lastReceived", h.number(0.0))?;
    let iterator = call!(h, h.get(owner, "pendingAcks")?, "values", vec![]);
    loop {
        let next = call!(h, iterator.clone(), "next", vec![]);
        if h.truthy(&h.get(&next, "done")?) {
            break;
        }
        let pending = h.get(&next, "value")?;
        let clear = h.select(h.global(), "clearTimeout")?;
        h.apply(clear, vec![h.get(&pending, "timer")?])?;
        let reject = h.select(pending, "reject")?;
        let error = h.error(h.literal("HyroTrader private socket session ended"))?;
        h.apply(reject, vec![h.normalize_error(error)?])?;
    }
    call!(h, h.get(owner, "pendingAcks")?, "clear", vec![]);
    let parent = h.get(owner, "parent")?;
    let sync = h.get(&parent, "syncSocketHealthToStore")?;
    if !h.equal(&sync, &h.undefined()) && !h.equal(&sync, &h.intrinsic("null", vec![])?) {
        h.receiver_call(parent, sync, vec![])?;
    }
    Ok(())
}
/// Closed set of lexical callbacks created by this module and selected by the native host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callback {
    Order,
    Connect,
    Close,
    Ping,
    Message,
    Timeout,
}
impl Callback {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "order" => Some(Self::Order),
            "connect" => Some(Self::Connect),
            "close" => Some(Self::Close),
            "ping" => Some(Self::Ping),
            "message" => Some(Self::Message),
            "timeout" => Some(Self::Timeout),
            _ => None,
        }
    }
}

pub fn dispatch<H: SessionHost>(
    h: &H,
    owner: &H::Value,
    code: Callback,
    captures: &[H::Value],
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    match code {
        Callback::Order => {
            call!(
                h,
                h.get(owner, "parent")?,
                "observePrivateOrders",
                vec![first.clone()]
            );
            h.apply(captures[0].clone(), vec![first])?;
        }
        Callback::Connect => {
            h.set(owner, "started", h.boolean(true))?;
            call!(h, owner.clone(), "resetSession", vec![]);
            h.apply(captures[0].clone(), vec![])?;
        }
        Callback::Close => {
            let target = h.optional(&first, "currentTarget")?;
            if h.truthy(&target) && !h.equal(&h.get(&first, "currentTarget")?, &h.get(owner, "ws")?)
            {
                return Ok(h.undefined());
            }
            call!(h, owner.clone(), "resetSession", vec![]);
            h.apply(captures[0].clone(), vec![first])?;
        }
        Callback::Ping => {
            if h.truthy(&h.get(owner, "ready")?) {
                h.apply(captures[0].clone(), vec![])?;
            }
        }
        Callback::Message => {
            if h.truthy(&h.get(owner, "isDisposed")?) {
                return Ok(h.undefined());
            }
            if h.truthy(&h.get(&first, "currentTarget")?)
                && !h.equal(&h.get(&first, "currentTarget")?, &h.get(owner, "ws")?)
            {
                return Ok(h.undefined());
            }
            let parser = h.select(h.global(), "jsonParse")?;
            let message = h.apply(parser, vec![h.get(&first, "data")?])?;
            if h.truthy(&h.optional(&message, "op")?) && h.truthy(&h.get(&message, "req_id")?) {
                let map = h.get(owner, "pendingAcks")?;
                let pending = call!(h, map, "get", vec![h.get(&message, "req_id")?]);
                if h.truthy(&pending) && h.equal(&h.get(&pending, "op")?, &h.get(&message, "op")?) {
                    let map = h.get(owner, "pendingAcks")?;
                    call!(h, map, "delete", vec![h.get(&message, "req_id")?]);
                    let clear = h.select(h.global(), "clearTimeout")?;
                    h.apply(clear, vec![h.get(&pending, "timer")?])?;
                    if h.equal(&h.get(&message, "success")?, &h.boolean(true)) {
                        call!(h, pending, "resolve", vec![]);
                    } else {
                        let reject = h.select(pending, "reject")?;
                        let op = h.get(&message, "op")?;
                        let reason = h.get(&message, "ret_msg")?;
                        let reason = if h.truthy(&reason) {
                            reason
                        } else {
                            h.literal("request rejected")
                        };
                        let text = concat(
                            h,
                            vec![
                                h.literal("HyroTrader private "),
                                op,
                                h.literal(" rejected: "),
                                reason,
                            ],
                        )?;
                        let error = h.error(text)?;
                        h.apply(reject, vec![h.normalize_error(error)?])?;
                    }
                }
            }
            if h.truthy(&h.optional(&message, "topic")?) {
                increment(h, owner, "stateRevision")?;
            }
            if h.truthy(&h.get(owner, "ready")?) {
                h.apply(captures[0].clone(), vec![first])?;
            }
        }
        Callback::Timeout => {
            call!(
                h,
                h.get(owner, "pendingAcks")?,
                "delete",
                vec![captures[0].clone()]
            );
            let text = concat(
                h,
                vec![
                    h.literal("HyroTrader private "),
                    captures[1].clone(),
                    h.literal(" acknowledgement timed out"),
                ],
            )?;
            let error = h.error(text)?;
            h.apply(captures[2].clone(), vec![h.normalize_error(error)?])?;
        }
    }
    Ok(h.undefined())
}
pub fn request<H: SessionHost>(
    h: &H,
    owner: &H::Value,
    op: H::Value,
    args: H::Value,
) -> Result<H::Value, H::Error> {
    let id = call!(h, h.global(), "nanoid", vec![]);
    let (promise, resolve, reject) = h.promise()?;
    let executor = || -> Result<(), H::Error> {
        let timer_fn = h.select(h.global(), "setTimeout")?;
        let callback = h.callback(
            owner.clone(),
            "timeout",
            vec![id.clone(), op.clone(), reject.clone()],
        )?;
        let timer = h.apply(timer_fn, vec![callback, h.number(10_000.0)])?;
        let map = h.get(owner, "pendingAcks")?;
        call!(
            h,
            map,
            "set",
            vec![
                id.clone(),
                h.object(vec![
                    ("op", op.clone()),
                    ("resolve", resolve),
                    ("reject", reject.clone()),
                    ("timer", timer.clone())
                ])
            ]
        );
        let send = || -> Result<(), H::Error> {
            let selected = h.select(owner.clone(), "sendWithLog")?;
            let stringify = h.select(h.intrinsic("JSON", vec![])?, "stringify")?;
            let payload = h.object(vec![("op", op), ("req_id", id.clone()), ("args", args)]);
            h.apply(selected, vec![h.apply(stringify, vec![payload])?])?;
            Ok(())
        };
        if let Err(error) = send() {
            call!(h, h.global(), "clearTimeout", vec![timer]);
            call!(h, h.get(owner, "pendingAcks")?, "delete", vec![id]);
            h.apply(reject.clone(), vec![h.normalize_error(error)?])?;
        }
        Ok(())
    };
    if let Err(error) = executor() {
        h.apply(reject, vec![h.error_value(error)])?;
    }
    Ok(promise)
}
fn current<H: SessionHost>(
    h: &H,
    owner: &H::Value,
    generation: &H::Value,
    socket: &H::Value,
) -> Result<bool, H::Error> {
    Ok(!h.truthy(&h.get(owner, "isDisposed")?)
        && h.equal(generation, &h.get(owner, "generation")?)
        && h.equal(socket, &h.get(owner, "ws")?))
}
pub async fn send_reconnect_sequence<H: SessionHost>(
    h: &H,
    owner: &H::Value,
) -> Result<(), H::Error> {
    h.set(owner, "started", h.boolean(true))?;
    call!(h, owner.clone(), "resetSession", vec![]);
    let generation = h.get(owner, "generation")?;
    let socket = h.get(owner, "ws")?;
    let run = async {
        let now = call!(
            h,
            h.intrinsic("virtualClock", vec![])?,
            "getCurrentTimeMs",
            vec![]
        );
        let expires = h.number(h.numeric(&now)? + 10_000.0);
        let parent = h.get(owner, "parent")?;
        let options = h.get(&parent, "options")?;
        let signature = if h.truthy(&h.get(&options, "remoteSigner")?) {
            let remote = h.get(&h.get(&h.get(owner, "parent")?, "options")?, "remoteSigner")?;
            let sign = h.select(remote, "sign")?;
            let id = call!(h, h.global(), "nanoid", vec![]);
            let remote = h.get(&h.get(&h.get(owner, "parent")?, "options")?, "remoteSigner")?;
            let account = h.get(&remote, "accountName")?;
            let to_sign = concat(h, vec![h.literal("GET/realtime"), expires.clone()])?;
            let payload = h.object(vec![
                ("id", id),
                ("type", h.literal("ws-auth")),
                ("exchangeName", h.literal("bybit")),
                ("accountName", account),
                (
                    "wsAuth",
                    h.object(vec![
                        ("toSign", to_sign),
                        ("outputFormat", h.literal("hex")),
                        ("extraData", h.object(vec![("expires", expires.clone())])),
                    ]),
                ),
            ]);
            let result = h.wait(h.apply(sign, vec![payload])?).await?;
            if h.truthy(&h.get(&result, "error")?) || !h.truthy(&h.get(&result, "wsAuthSignature")?)
            {
                let reason = h.get(&result, "error")?;
                let reason = if h.truthy(&reason) {
                    reason
                } else {
                    h.literal("missing signature")
                };
                return Err(h.error(concat(
                    h,
                    vec![h.literal("Remote WS signing unavailable: "), reason],
                )?)?);
            }
            h.get(&result, "wsAuthSignature")?
        } else {
            let factory = h.select(h.global(), "createHmac")?;
            let secret = h.get(&h.get(&h.get(owner, "parent")?, "options")?, "secret")?;
            let signer = h.apply(factory, vec![h.literal("sha256"), secret])?;
            let update = h.select(signer, "update")?;
            let text = concat(h, vec![h.literal("GET/realtime"), expires.clone()])?;
            let signer = h.apply(update, vec![text])?;
            call!(h, signer, "digest", vec![h.literal("hex")])
        };
        if !current(h, owner, &generation, &socket)? {
            return Ok(());
        }
        let selected = h.select(owner.clone(), "request")?;
        let key = h.get(&h.get(&h.get(owner, "parent")?, "options")?, "key")?;
        h.wait(h.apply(
            selected,
            vec![h.literal("auth"), h.array(vec![key, expires, signature])],
        )?)
        .await?;
        if !current(h, owner, &generation, &socket)? {
            return Ok(());
        }
        let promise = call!(
            h,
            owner.clone(),
            "request",
            vec![
                h.literal("subscribe"),
                h.array(vec![h.literal("order"), h.literal("position")])
            ]
        );
        h.wait(promise).await?;
        for _ in 0..3 {
            if !current(h, owner, &generation, &socket)? {
                break;
            }
            let parent = h.get(owner, "parent")?;
            let selected = h.select(parent, "reconcilePrivateState")?;
            let guard = h.callback(
                owner.clone(),
                "current",
                vec![generation.clone(), socket.clone()],
            )?;
            let result = h.wait(h.apply(selected, vec![guard])?).await?;
            if h.truthy(&result) {
                if !current(h, owner, &generation, &socket)? {
                    return Ok(());
                }
                h.set(owner, "ready", h.boolean(true))?;
                call!(h, owner.clone(), "trackLastReceived", vec![]);
                call!(h, owner.clone(), "ping", vec![]);
                return Ok(());
            }
        }
        if current(h, owner, &generation, &socket)? {
            return Err(h.error(
                h.literal("HyroTrader private state did not stabilize during reconciliation"),
            )?);
        }
        Ok(())
    }
    .await;
    if let Err(error) = run {
        if !current(h, owner, &generation, &socket)? {
            return Ok(());
        }
        let selected = h.select(owner.clone(), "emitAndLogError")?;
        h.apply(selected, vec![h.normalize_error(error)?])?;
        call!(h, owner.clone(), "resetSession", vec![]);
        call!(h, owner.clone(), "reconnect", vec![]);
    }
    Ok(())
}
pub fn is_current<H: SessionHost>(
    h: &H,
    owner: &H::Value,
    captures: &[H::Value],
) -> Result<H::Value, H::Error> {
    Ok(h.boolean(current(h, owner, &captures[0], &captures[1])?))
}
