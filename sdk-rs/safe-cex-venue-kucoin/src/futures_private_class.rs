use crate::{
    order_lifecycle,
    spot_private_class::{PrivateClassOps, call, fallback, optional, ping},
};
use std::{
    cell::{Cell, OnceCell},
    rc::Rc,
};
async fn connect<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    receiver: V,
) -> Result<V, E> {
    let h = (ops.frame)(&receiver);
    crate::socket_setup::connect_futures_private(
        || Ok((h.truthy)(&(h.get)(&receiver, "isDisposed")?)),
        |key| {
            let response = call(&h, "this.parent.xhr.post", || {
                let table = (h.capture)("ENDPOINTS")?;
                Ok(vec![(h.get)(&table, key)?])
            })?;
            Ok((h.await_value)(response))
        },
        |value, key| (h.get)(value, key),
        |code| (h.equal)(code, &(h.literal)("200000")),
        |value| (h.truthy)(value),
        |key| (h.get)(&receiver, key),
        |effect, arg, text| {
            match effect {
                "set:token" | "set:socketEndpoint" | "set:ws" => {
                    let key = effect.strip_prefix("set:").unwrap();
                    (h.set)(&receiver, key, arg.unwrap().clone())?;
                }
                "emit-error" => {
                    call(&h, "this.parent.emitAndLogError", || {
                        Ok(vec![(h.literal)(text.unwrap())])
                    })?;
                }
                "create-captured" => {
                    return call(&h, "this.createWebsocket", || {
                        Ok(vec![arg.unwrap().clone()])
                    });
                }
                "add-listener" => {
                    let event = text.unwrap();
                    let field = match event {
                        "open" => "onOpen",
                        "message" => "onMessage",
                        "close" => "onClose",
                        _ => return Err((ops.error)("unknown websocket event")),
                    };
                    call(&h, "this.ws.addEventListener", || {
                        Ok(vec![(h.literal)(event), (h.get)(&receiver, field)?])
                    })?;
                }
                _ => return Err((ops.error)("unknown socket connection effect")),
            }
            Ok((h.undefined)())
        },
        |value| (ops.template)(value),
        || call(&h, "Date.now", || Ok(vec![])),
        |units| (ops.string_value)(units),
        |error, message| {
            call(&h, "this.retryFailedConnect", || {
                let value = (h.error_value)(error);
                let response = optional(&h, &value, "response")?;
                let data = optional(&h, &response, "data")?;
                let cause = fallback(&h, optional(&h, &data, "msg")?, || {
                    fallback(&h, optional(&h, &value, "message")?, || {
                        Ok((h.literal)(message))
                    })
                })?;
                Ok(vec![cause])
            })?;
            Ok(())
        },
    )
    .await?;
    Ok((h.undefined)())
}
fn on_open<V: Clone, E>(ops: &PrivateClassOps<V, E>, r: &V) -> Result<V, E> {
    let h = (ops.frame)(r);
    if (h.truthy)(&(h.get)(r, "isDisposed")?) {
        return Ok((h.undefined)());
    }
    (h.set)(r, "subscribed", (ops.boolean)(false))?;
    call(&h, "this.subscribe", || Ok(vec![]))?;
    call(&h, "this.ping", || Ok(vec![]))?;
    if (h.truthy)(&(h.capture)("ENABLE_RECONNECTING_WEBSOCKET")?) {
        call(&h, "this.parent.fetchAndSetPrivateSnapshot", || Ok(vec![]))?;
    }
    Ok((h.undefined)())
}
fn on_message<V: Clone + 'static, E: 'static>(
    ops: &PrivateClassOps<V, E>,
    r: &V,
    input: V,
) -> Result<V, E> {
    let h = (ops.frame)(r);
    let data = (h.get)(&input, "data")?;
    let disposed = Cell::new((h.truthy)(&(h.get)(r, "isDisposed")?));
    crate::message_routing::futures_private_message(
        &disposed,
        |effect, json: Option<&Rc<V>>, text| {
            match effect {
                "track" => {
                    call(&h, "this.trackLastReceived", || Ok(vec![]))?;
                }
                "diagnostic" => {
                    call(&h, "this.emitWsDiagnosticReceive", || {
                        Ok(vec![data.clone()])
                    })?;
                }
                "pong" => {
                    call(&h, "this.handlePongEvent", || Ok(vec![]))?;
                }
                "subscribe" => {
                    call(&h, "this.subscribe", || Ok(vec![]))?;
                }
                "stats" => {
                    call(&h, "this.recordMessageHandlerStats", || {
                        Ok(vec![(ops.string_value)(text.unwrap().to_vec())])
                    })?;
                }
                "order" | "algo" => {
                    call(
                        &h,
                        if effect == "order" {
                            "this.handleOrderTopic"
                        } else {
                            "this.handleAlgoOrdersTopic"
                        },
                        || Ok(vec![(**json.unwrap()).clone()]),
                    )?;
                }
                "refresh-snapshot" => {
                    call(&h, "this.refreshSnapshotSoon", || Ok(vec![]))?;
                }
                _ => return Err((ops.error)("unknown futures private routing effect")),
            }
            Ok(())
        },
        || {
            let value = call(&h, "jsonParse", || Ok(vec![data.clone()]))?;
            Ok((h.truthy)(&value).then_some(Rc::new(value)))
        },
        |json, selector| {
            if let Some(kind) = selector.strip_prefix("type:") {
                return Ok((h.equal)(&(h.get)(json, "type")?, &(h.literal)(kind)));
            }
            if let Some(topic) = selector.strip_prefix("equal:") {
                return Ok((h.equal)(&(h.get)(json, "topic")?, &(h.literal)(topic)));
            }
            let optional = selector.strip_prefix("optional:");
            let topic = (h.get)(json, "topic")?;
            let topic = if optional.is_some() {
                if (h.nullish)(&topic) {
                    return Ok(false);
                }
                topic
            } else {
                if !(h.truthy)(&topic) {
                    return Ok(false);
                }
                (h.get)(json, "topic")?
            };
            let result = (h.intrinsic)(
                "call-method",
                vec![
                    topic,
                    (h.literal)("startsWith"),
                    (h.literal)(optional.unwrap_or(selector)),
                ],
            )?;
            Ok((h.truthy)(&result))
        },
    )?;
    Ok((h.undefined)())
}
fn algo_order_topic<V: Clone, E>(ops: &PrivateClassOps<V, E>, r: &V, input: V) -> Result<V, E> {
    let h = (ops.frame)(r);
    crate::message_routing::algo_order_message(
        &input,
        |v| Ok((h.truthy)(&(h.get)(v, "data")?)),
        |v| Ok(Rc::new((h.get)(v, "data")?)),
        |raw| call(&h, "this.parent.mapOrder", || Ok(vec![(**raw).clone()])),
        |orders| {
            let parent = (h.get)(orders, "0")?;
            Ok((h.truthy)(&parent).then_some(Rc::new(parent)))
        },
        |raw| {
            let event = fallback(&h, (h.get)(raw, "type")?, || (h.get)(raw, "status"))?;
            Ok(["cancel", "triggered", "active", "open"]
                .into_iter()
                .find(|kind| (h.equal)(&event, &(h.literal)(kind)))
                .map(|s| s.encode_utf16().collect()))
        },
        |parent, status| {
            call(
                &h,
                if status.is_some() {
                    "this.store.updateOrderStatus"
                } else {
                    "this.store.addOrUpdateOrder"
                },
                || {
                    let mut args = vec![(**parent).clone()];
                    if status.is_some() {
                        args.push((h.capture)("OrderStatus.Canceled")?);
                    }
                    Ok(args)
                },
            )?;
            Ok(())
        },
    )?;
    Ok((h.undefined)())
}
fn refresh_snapshot<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    r: V,
) -> Result<V, E> {
    let h = (ops.frame)(&r);
    if (h.truthy)(&(h.get)(&r, "snapshotRefreshTimer")?) {
        call(&h, "clearTimeout", || {
            Ok(vec![(h.get)(&r, "snapshotRefreshTimer")?])
        })?;
    }
    let set = (h.capture)("setTimeout")?;
    let owner = ops.clone();
    let receiver = r.clone();
    let callback = (ops.arrow)(
        "",
        0,
        Rc::new(move |_| {
            let h = (owner.frame)(&receiver);
            (h.set)(&receiver, "snapshotRefreshTimer", (h.undefined)())?;
            if !(h.truthy)(&(h.get)(&receiver, "isDisposed")?) {
                call(&h, "this.parent.fetchAndSetPrivateSnapshot", || Ok(vec![]))?;
            }
            Ok((h.undefined)())
        }),
    );
    let handle = (h.invoke)(set, vec![callback, (h.number)(500.)])?;
    (h.set)(&r, "snapshotRefreshTimer", handle)?;
    Ok((h.undefined)())
}
fn optional_member<V: Clone, E>(
    h: &crate::public_streams::StreamOps<V, E>,
    value: &V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    if (h.nullish)(value) {
        return Ok((h.undefined)());
    }
    let member = (h.get)(value, key)?;
    if (h.nullish)(&member) {
        return Ok((h.undefined)());
    }
    let args = (h.array)(args()?);
    (h.intrinsic)("call-with-this", vec![member, value.clone(), args])
}
async fn reconnect<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    r: V,
) -> Result<V, E> {
    let h = (ops.frame)(&r);
    if (h.truthy)(&(h.get)(&r, "isDisposed")?) {
        return Ok((h.undefined)());
    }
    let previous = (h.get)(&r, "ws")?;
    for (event, field) in [
        ("open", "onOpen"),
        ("message", "onMessage"),
        ("close", "onClose"),
    ] {
        optional_member(&h, &previous, "removeEventListener", || {
            Ok(vec![(h.literal)(event), (h.get)(&r, field)?])
        })?;
    }
    (h.set)(&r, "ws", (h.undefined)())?;
    optional_member(&h, &previous, "close", || Ok(vec![]))?;
    for field in ["token", "socketEndpoint"] {
        (h.set)(&r, field, (ops.null)())?;
    }
    (h.set)(&r, "subscribed", (ops.boolean)(false))?;
    let task = call(&h, "this.connectAndSubscribe", || Ok(vec![]))?;
    (h.await_value)(task).await?;
    Ok((h.undefined)())
}
fn dispose<V: Clone, E>(ops: &PrivateClassOps<V, E>, home: &V, r: &V) -> Result<V, E> {
    let h = (ops.frame)(r);
    if (h.truthy)(&(h.get)(r, "snapshotRefreshTimer")?) {
        call(&h, "clearTimeout", || {
            Ok(vec![(h.get)(r, "snapshotRefreshTimer")?])
        })?;
        (h.set)(r, "snapshotRefreshTimer", (h.undefined)())?;
    }
    let member = (ops.super_member)(home, r, "dispose")?;
    (h.invoke)(member, vec![])?;
    Ok((h.undefined)())
}
fn subscribe<V: Clone + 'static, E: 'static>(ops: Rc<PrivateClassOps<V, E>>, r: V) -> Result<V, E> {
    let h = (ops.frame)(&r);
    if (h.truthy)(&(h.get)(&r, "subscribed")?) {
        return Ok((h.undefined)());
    }
    (h.set)(&r, "subscribed", (ops.boolean)(true))?;
    let topics = (h.array)(
        [
            "/contractMarket/tradeOrders",
            "/contractMarket/advancedOrders",
            "/contract/positionAll",
            "/contractAccount/wallet",
            "/contract/marginMode",
            "/contract/crossLeverage",
        ]
        .into_iter()
        .map(|s| (h.literal)(s))
        .collect(),
    );
    let send = h.clone();
    let owner = ops.clone();
    (h.for_of)(
        topics,
        Rc::new(move |topic| {
            let payload = (send.object)(vec![
                ("id", call(&send, "Date.now", || Ok(vec![]))?),
                ("type", (send.literal)("subscribe")),
                ("topic", topic),
                ("privateChannel", (owner.boolean)(true)),
                ("response", (owner.boolean)(true)),
            ]);
            call(&send, "this.sendWithLog", || {
                Ok(vec![call(&send, "JSON.stringify", || Ok(vec![payload]))?])
            })?;
            Ok(())
        }),
    )?;
    Ok((h.undefined)())
}
fn initialize<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    r: V,
    home: Rc<OnceCell<V>>,
) -> Result<(), E> {
    for field in ["token", "socketEndpoint"] {
        (ops.define)(&r, field, (ops.null)())?;
    }
    (ops.define)(&r, "subscribed", (ops.boolean)(false))?;
    (ops.define)(&r, "snapshotRefreshTimer", (ops.value.undefined)())?;
    for name in [
        "connectAndSubscribe",
        "onOpen",
        "ping",
        "onMessage",
        "refreshSnapshotSoon",
        "reconnect",
        "dispose",
        "handleOrderTopic",
        "handleAlgoOrdersTopic",
        "subscribe",
    ] {
        let owner = ops.clone();
        let receiver = r.clone();
        let class = home.clone();
        let arity =
            usize::from(["onMessage", "handleOrderTopic", "handleAlgoOrdersTopic"].contains(&name));
        let body = if ["connectAndSubscribe", "reconnect"].contains(&name) {
            (ops.async_arrow)(
                name,
                arity,
                Rc::new(move |_| {
                    let owner = owner.clone();
                    let receiver = receiver.clone();
                    Box::pin(async move {
                        if name == "reconnect" {
                            reconnect(owner, receiver).await
                        } else {
                            connect(owner, receiver).await
                        }
                    })
                }),
            )
        } else {
            (ops.arrow)(
                name,
                arity,
                Rc::new(move |args| {
                    let argument = || {
                        args.first()
                            .cloned()
                            .unwrap_or_else(|| (owner.value.undefined)())
                    };
                    match name {
                        "onOpen" => on_open(&owner, &receiver),
                        "ping" => ping(&owner, &receiver),
                        "onMessage" => on_message(&owner, &receiver, argument()),
                        "refreshSnapshotSoon" => refresh_snapshot(owner.clone(), receiver.clone()),
                        "dispose" => dispose(
                            &owner,
                            class.get().ok_or_else(|| {
                                (owner.error)("private class home is not initialized")
                            })?,
                            &receiver,
                        ),
                        "handleOrderTopic" => order_lifecycle::handle_futures_order_topic(
                            (owner.orders)(&receiver),
                            argument(),
                        ),
                        "handleAlgoOrdersTopic" => algo_order_topic(&owner, &receiver, argument()),
                        "subscribe" => subscribe(owner.clone(), receiver.clone()),
                        _ => Err((owner.error)("unknown futures private native member")),
                    }
                }),
            )
        };
        (ops.define)(&r, name, body)?;
    }
    Ok(())
}
pub fn define_futures_private_class<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
) -> Result<V, E> {
    let base = (ops.value.capture)("BaseWebSocket")?;
    let home = Rc::new(OnceCell::new());
    let initializer_home = home.clone();
    let owner = ops.clone();
    let class = (ops.derive)(
        "KucoinPrivateWebsocket",
        base,
        Rc::new(move |r| initialize(owner.clone(), r, initializer_home.clone())),
    )?;
    if home.set(class.clone()).is_err() {
        return Err((ops.error)("private class home was initialized twice"));
    }
    Ok(class)
}
