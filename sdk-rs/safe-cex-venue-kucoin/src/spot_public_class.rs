use crate::{
    public_streams,
    spot_private_class::{PrivateClassOps, call, fallback, optional},
};
use std::rc::Rc;
async fn connect<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    receiver: V,
) -> Result<V, E> {
    let h = (ops.frame)(&receiver);
    crate::socket_setup::connect_spot_public(
        || Ok((h.truthy)(&(h.get)(&receiver, "isDisposed")?)),
        |key| {
            let response = call(&h, "this.parent.xhr.post", || {
                let table = (h.capture)("KUCOIN_SPOT_ENDPOINTS")?;
                Ok(vec![(h.get)(&table, key)?])
            })?;
            Ok((h.await_value)(response))
        },
        |value, key| match key {
            "instanceServers?.0" => {
                let servers = (h.get)(value, "instanceServers")?;
                optional(&h, &servers, "0")
            }
            "?.endpoint" => optional(&h, value, "endpoint"),
            _ => (h.get)(value, key),
        },
        |code| (h.equal)(code, &(h.literal)("200000")),
        |value| (h.truthy)(value),
        |key| (h.get)(&receiver, key),
        |effect, arg, text| {
            match effect {
                "set:token" | "set:pingInterval" | "set:socketEndpoint" | "set:ws" => {
                    let key = effect.strip_prefix("set:").unwrap();
                    (h.set)(&receiver, key, arg.unwrap().clone())?;
                }
                "emit-token-error" => {
                    call(&h, "this.parent.emitAndLogError", || {
                        Ok(vec![fallback(&h, (h.get)(arg.unwrap(), "msg")?, || {
                            Ok((h.literal)(text.unwrap()))
                        })?])
                    })?;
                }
                "emit-error" => {
                    call(&h, "this.parent.emitAndLogError", || {
                        Ok(vec![(h.literal)(text.unwrap())])
                    })?;
                }
                "create-current" => {
                    return call(&h, "this.createWebsocket", || {
                        Ok(vec![(h.get)(&receiver, "socketEndpoint")?])
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
fn on_open<V: Clone, E>(ops: &PrivateClassOps<V, E>, receiver: &V) -> Result<V, E> {
    let h = (ops.frame)(receiver);
    if (h.truthy)(&(h.get)(receiver, "isDisposed")?) {
        return Ok((h.undefined)());
    }
    call(&h, "this.subscribe", || Ok(vec![]))?;
    call(&h, "this.ping", || Ok(vec![]))?;
    if (h.truthy)(&(h.capture)("ENABLE_RECONNECTING_WEBSOCKET")?) {
        call(&h, "this.emitWsReconnected", || Ok(vec![]))?;
    }
    Ok((h.undefined)())
}
fn ping<V: Clone, E>(ops: &PrivateClassOps<V, E>, receiver: &V) -> Result<V, E> {
    let h = (ops.frame)(receiver);
    if (h.truthy)(&(h.get)(receiver, "isDisposed")?) {
        return Ok((h.undefined)());
    }
    let now = call(&h, "performance.now", || Ok(vec![]))?;
    (h.set)(receiver, "pingAt", now)?;
    call(&h, "this.send", || {
        Ok(vec![call(&h, "JSON.stringify", || {
            Ok(vec![(h.object)(vec![
                ("id", call(&h, "Date.now", || Ok(vec![]))?),
                ("type", (h.literal)("ping")),
            ])])
        })?])
    })?;
    Ok((h.undefined)())
}
fn member_call<V: Clone, E>(
    h: &public_streams::StreamOps<V, E>,
    value: V,
    key: &str,
    args: Vec<V>,
) -> Result<V, E> {
    let mut input = vec![value, (h.literal)(key)];
    input.extend(args);
    (h.intrinsic)("call-method", input)
}
fn pair<V: Clone, E>(h: &public_streams::StreamOps<V, E>, value: V) -> Result<(V, V), E> {
    let values = (h.intrinsic)("destructure-two", vec![value])?;
    Ok(((h.get)(&values, "0")?, (h.get)(&values, "1")?))
}
fn candle_topic<V: Clone, E>(ops: &PrivateClassOps<V, E>, receiver: &V, topic: V) -> Result<V, E> {
    let h = (ops.frame)(receiver);
    let split = member_call(&h, topic, "split", vec![(h.literal)(":")])?;
    let market = (h.intrinsic)("destructure-skip-first-take-second", vec![split])?;
    let split = member_call(&h, market, "split", vec![(h.literal)("_")])?;
    let (symbol, interval) = pair(&h, split)?;
    let mut key = "candle_".encode_utf16().collect::<Vec<_>>();
    key.extend((ops.template)(&symbol)?);
    key.push(95);
    key.extend((ops.template)(&interval)?);
    Ok((ops.string_value)(key))
}
fn topic<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    receiver: &V,
    value: V,
    operation: &str,
) -> Result<V, E> {
    let h = (ops.frame)(receiver);
    if !(h.truthy)(&(h.get)(receiver, "isConnected")?)
        || (h.truthy)(&(h.get)(receiver, "isDisposed")?)
    {
        return Ok((h.undefined)());
    }
    call(&h, "this.send", || {
        Ok(vec![call(&h, "JSON.stringify", || {
            Ok(vec![(h.object)(vec![
                ("id", call(&h, "Date.now", || Ok(vec![]))?),
                ("type", (h.literal)(operation)),
                ("topic", value),
                ("privateChannel", (ops.boolean)(false)),
                ("response", (ops.boolean)(true)),
            ])])
        })?])
    })?;
    Ok((h.undefined)())
}
fn subscribe<V: Clone + 'static, E: 'static>(
    ops: &PrivateClassOps<V, E>,
    receiver: &V,
) -> Result<V, E> {
    let h = (ops.frame)(receiver);
    call(&h, "this.subscribeTopic", || {
        Ok(vec![(h.literal)("/market/ticker:all")])
    })?;
    let keys = call(&h, "Object.keys", || {
        Ok(vec![(h.get)(receiver, "messageHandlers")?])
    })?;
    (h.for_of)(
        keys,
        Rc::new({
            let h = h.clone();
            let ops_template = ops.template.clone();
            let string_value = ops.string_value.clone();
            move |key| {
                for (prefix, head, candle) in [
                    ("candle_", "/market/candles:", true),
                    ("orderBook_", "/spotMarket/level2Depth50:", false),
                ] {
                    if (h.truthy)(&member_call(
                        &h,
                        key.clone(),
                        "startsWith",
                        vec![(h.literal)(prefix)],
                    )?) {
                        let replaced = member_call(
                            &h,
                            key.clone(),
                            "replace",
                            vec![(h.literal)(prefix), (h.literal)("")],
                        )?;
                        if candle {
                            let split = member_call(&h, replaced, "split", vec![(h.literal)("_")])?;
                            let (symbol, interval) = pair(&h, split)?;
                            call(&h, "this.subscribeTopic", || {
                                let mut text = head.encode_utf16().collect::<Vec<_>>();
                                text.extend(ops_template(&symbol)?);
                                text.push(95);
                                text.extend(ops_template(&interval)?);
                                Ok(vec![string_value(text)])
                            })?;
                        } else {
                            call(&h, "this.subscribeTopic", || {
                                let mut text = head.encode_utf16().collect::<Vec<_>>();
                                text.extend(ops_template(&replaced)?);
                                Ok(vec![string_value(text)])
                            })?;
                        }
                    }
                }
                Ok(())
            }
        }),
    )?;
    Ok((h.undefined)())
}
fn on_message<V: Clone + 'static, E: 'static>(
    ops: &PrivateClassOps<V, E>,
    receiver: &V,
    input: V,
) -> Result<V, E> {
    let h = (ops.frame)(receiver);
    let data = (h.get)(&input, "data")?;
    if (h.truthy)(&(h.get)(receiver, "isDisposed")?) {
        return Ok((h.undefined)());
    }
    call(&h, "this.trackLastReceived", || Ok(vec![]))?;
    let json = call(&h, "jsonParse", || Ok(vec![data]))?;
    if !(h.truthy)(&json) {
        return Ok((h.undefined)());
    }
    if (h.equal)(&(h.get)(&json, "type")?, &(h.literal)("pong")) {
        call(&h, "this.handlePongEvent", || {
            Ok(vec![(h.get)(receiver, "pingInterval")?])
        })?;
        return Ok((h.undefined)());
    }
    for (prefix, route) in [
        ("/market/ticker", "ticker"),
        ("/market/candles", "candle"),
        ("/spotMarket/level2Depth50", "book"),
    ] {
        let topic = (h.get)(&json, "topic")?;
        if (h.nullish)(&topic)
            || !(h.truthy)(&member_call(
                &h,
                topic,
                "startsWith",
                vec![(h.literal)(prefix)],
            )?)
        {
            continue;
        }
        if route == "ticker" {
            call(&h, "this.recordMessageHandlerStats", || {
                Ok(vec![(h.literal)("ticker")])
            })?;
            call(&h, "this.handleTickerSnapshot", || Ok(vec![json]))?;
            return Ok((h.undefined)());
        }
        let key = if route == "candle" {
            call(&h, "this.getCandleHandlerTopic", || {
                Ok(vec![(h.get)(&json, "topic")?])
            })?
        } else {
            let text = (ops.string_value)((ops.template)(&(h.get)(&json, "topic")?)?);
            let split = member_call(&h, text, "split", vec![(h.literal)(":")])?;
            let symbol = (h.get)(&split, "1")?;
            let mut text = "orderBook_".encode_utf16().collect::<Vec<_>>();
            text.extend((ops.template)(&symbol)?);
            (ops.string_value)(text)
        };
        call(&h, "this.recordMessageHandlerStats", || {
            Ok(vec![key.clone()])
        })?;
        let handlers = (h.get)(receiver, "messageHandlers")?;
        let callbacks = (h.intrinsic)("get-index", vec![handlers, key])?;
        if !(h.nullish)(&callbacks) {
            let method = (h.get)(&callbacks, "forEach")?;
            let invoke = h.invoke.clone();
            let undefined = h.undefined.clone();
            let arrow = (ops.arrow)(
                "",
                1,
                Rc::new(move |args| {
                    invoke(
                        args.first().cloned().unwrap_or_else(|| undefined()),
                        vec![json.clone()],
                    )
                }),
            );
            (h.intrinsic)(
                "call-with-this",
                vec![method, callbacks, (h.array)(vec![arrow])],
            )?;
        }
        return Ok((h.undefined)());
    }
    Ok((h.undefined)())
}
fn initialize<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    receiver: V,
) -> Result<(), E> {
    for key in ["token", "socketEndpoint"] {
        (ops.define)(&receiver, key, (ops.null)())?;
    }
    (ops.define)(&receiver, "pingInterval", (ops.value.number)(5000.))?;
    let owner = ops.clone();
    let instance = receiver.clone();
    let body = (ops.async_arrow)(
        "connectAndSubscribe",
        0,
        Rc::new(move |_| Box::pin(connect(owner.clone(), instance.clone()))),
    );
    (ops.define)(&receiver, "connectAndSubscribe", body)?;
    for (name, arity) in [
        ("onOpen", 0),
        ("ping", 0),
        ("subscribe", 0),
        ("subscribeTopic", 1),
        ("unsubscribeTopic", 1),
        ("onMessage", 1),
        ("handleTickerSnapshot", 1),
        ("listenOHLCV", 2),
        ("listenOrderBook", 2),
        ("getCandleHandlerTopic", 1),
    ] {
        let owner = ops.clone();
        let instance = receiver.clone();
        let body = (ops.arrow)(
            name,
            arity,
            Rc::new(move |args| {
                let arg = |index| {
                    args.get(index)
                        .cloned()
                        .unwrap_or_else(|| (owner.value.undefined)())
                };
                match name {
                    "onOpen" => on_open(&owner, &instance),
                    "ping" => ping(&owner, &instance),
                    "subscribe" => subscribe(&owner, &instance),
                    "subscribeTopic" => topic(&owner, &instance, arg(0), "subscribe"),
                    "unsubscribeTopic" => topic(&owner, &instance, arg(0), "unsubscribe"),
                    "onMessage" => on_message(&owner, &instance, arg(0)),
                    "handleTickerSnapshot" => {
                        let h = (owner.frame)(&instance);
                        public_streams::handle_spot_ticker_snapshot(&h, arg(0))?;
                        Ok((h.undefined)())
                    }
                    "listenOHLCV" => {
                        public_streams::listen_spot_ohlcv((owner.frame)(&instance), arg(0), arg(1))
                    }
                    "listenOrderBook" => public_streams::listen_spot_order_book(
                        (owner.frame)(&instance),
                        arg(0),
                        arg(1),
                    ),
                    "getCandleHandlerTopic" => candle_topic(&owner, &instance, arg(0)),
                    _ => Err((owner.error)("unknown spot public native member")),
                }
            }),
        );
        (ops.define)(&receiver, name, body)?;
    }
    Ok(())
}
pub fn define_spot_public_class<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
) -> Result<V, E> {
    let base = (ops.value.capture)("BaseWebSocket")?;
    let owner = ops.clone();
    (ops.derive)(
        "KucoinSpotPublicWebsocket",
        base,
        Rc::new(move |receiver| initialize(owner.clone(), receiver)),
    )
}
