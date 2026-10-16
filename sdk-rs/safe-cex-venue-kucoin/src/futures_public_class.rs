use crate::{
    public_streams,
    spot_private_class::{PrivateClassOps, call, fallback, optional},
};
use std::rc::Rc;
pub type Constructor<V, E> = Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>;
pub struct ConstructionOps<V, E> {
    pub derive: Rc<dyn Fn(&str, V, usize, Constructor<V, E>) -> Result<V, E>>,
    pub superclass: Rc<dyn Fn(&V) -> Result<V, E>>,
    pub super_construct: Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>,
    pub construct: Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>,
}
async fn connect<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    receiver: V,
) -> Result<V, E> {
    let h = (ops.frame)(&receiver);
    crate::socket_setup::connect_futures_public(
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
    let task = call(&h, "this.connectAndSubscribe", || Ok(vec![]))?;
    (h.await_value)(task).await?;
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
fn on_open<V: Clone, E>(ops: &PrivateClassOps<V, E>, r: &V) -> Result<V, E> {
    let h = (ops.frame)(r);
    if !(h.truthy)(&(h.get)(r, "isDisposed")?) {
        call(&h, "this.subscribe", || Ok(vec![]))?;
        call(&h, "this.ping", || Ok(vec![]))?;
        for name in ["this.parent.emitter.off", "this.parent.emitter.on"] {
            call(&h, name, || {
                Ok(vec![
                    (h.literal)("markets:changed"),
                    (h.get)(r, "onMarketsChanged")?,
                ])
            })?;
        }
        if (h.truthy)(&(h.capture)("ENABLE_RECONNECTING_WEBSOCKET")?) {
            call(&h, "this.emitWsReconnected", || Ok(vec![]))?;
        }
    }
    Ok((h.undefined)())
}
fn subscribe<V: Clone, E>(ops: &PrivateClassOps<V, E>, r: &V) -> Result<V, E> {
    let h = (ops.frame)(r);
    if (h.equal)(&(h.get)(r, "role")?, &(h.literal)("snapshot")) {
        call(&h, "this.sendChannelOp", || {
            Ok(vec![
                (h.literal)("subscribe"),
                (h.array)(vec![(h.capture)("SNAPSHOT_CHANNEL")?]),
                call(&h, "this.snapshotShardSymbols", || Ok(vec![]))?,
            ])
        })?;
    }
    Ok((h.undefined)())
}
fn active_ticker<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    r: &V,
    symbol: V,
    operation: &str,
) -> Result<V, E> {
    let h = (ops.frame)(r);
    if (h.truthy)(&symbol) && !(h.truthy)(&(h.get)(r, "isDisposed")?) {
        call(&h, "this.sendChannelOp", || {
            Ok(vec![
                (h.literal)(operation),
                (h.capture)("ACTIVE_DETAIL_TICKER_CHANNELS")?,
                (h.array)(vec![symbol]),
            ])
        })?;
    }
    Ok((h.undefined)())
}
fn snapshot_symbols<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    r: V,
) -> Result<V, E> {
    let h = (ops.frame)(&r);
    let input = (h.capture)("this.parent.store.markets")?;
    let map = (h.get)(&input, "map")?;
    let frame = h.clone();
    let mapper = (ops.arrow)(
        "",
        1,
        Rc::new(move |args| {
            let row = args.first().cloned().unwrap_or_else(|| (frame.undefined)());
            fallback(&frame, (frame.get)(&row, "id")?, || {
                call(&frame, "this.parent.toApiSymbol", || {
                    Ok(vec![(frame.get)(&row, "symbol")?])
                })
            })
        }),
    );
    let mapped = (h.intrinsic)("call-with-this", vec![map, input, (h.array)(vec![mapper])])?;
    let method = (h.get)(&mapped, "filter")?;
    let frame = h.clone();
    let predicate = (ops.arrow)(
        "",
        1,
        Rc::new(move |args| {
            call(&frame, "Boolean", || {
                Ok(vec![
                    args.first().cloned().unwrap_or_else(|| (frame.undefined)()),
                ])
            })
        }),
    );
    let ids = (h.intrinsic)(
        "call-with-this",
        vec![method, mapped, (h.array)(vec![predicate])],
    )?;
    let method = (h.get)(&ids, "filter")?;
    let frame = h.clone();
    let receiver = r.clone();
    let owner = ops.clone();
    let predicate = (ops.arrow)(
        "",
        2,
        Rc::new(move |args| {
            let i = args.get(1).cloned().unwrap_or_else(|| (frame.undefined)());
            let rem =
                (frame.intrinsic)("remainder", vec![i, (frame.get)(&receiver, "shardCount")?])?;
            Ok((owner.boolean)((frame.equal)(
                &rem,
                &(frame.get)(&receiver, "shardIndex")?,
            )))
        }),
    );
    (h.intrinsic)(
        "call-with-this",
        vec![method, ids, (h.array)(vec![predicate])],
    )
}
fn markets_changed<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    construction: &ConstructionOps<V, E>,
    r: V,
    event: V,
) -> Result<V, E> {
    let h = (ops.frame)(&r);
    let added = (h.get)(&event, "added")?;
    if (h.equal)(&(h.get)(&added, "length")?, &(h.number)(0.))
        || !(h.truthy)(&(h.get)(&r, "ws")?)
        || (h.truthy)(&(h.get)(&r, "isDisposed")?)
        || !(h.truthy)(&(h.get)(&r, "isConnected")?)
    {
        return Ok((h.undefined)());
    }
    if !(h.equal)(&(h.get)(&r, "role")?, &(h.literal)("snapshot")) {
        return Ok((h.undefined)());
    }
    let constructor = (h.capture)("Set")?;
    let input = call(&h, "this.snapshotShardSymbols", || Ok(vec![]))?;
    let owned = (construction.construct)(constructor, vec![input])?;
    let method = (h.get)(&added, "map")?;
    let frame = h.clone();
    let mapper = (ops.arrow)(
        "",
        1,
        Rc::new(move |args| {
            let row = args.first().cloned().unwrap_or_else(|| (frame.undefined)());
            fallback(&frame, (frame.get)(&row, "id")?, || {
                call(&frame, "this.parent.toApiSymbol", || {
                    Ok(vec![(frame.get)(&row, "symbol")?])
                })
            })
        }),
    );
    let mapped = (h.intrinsic)(
        "call-with-this",
        vec![method, added, (h.array)(vec![mapper])],
    )?;
    let method = (h.get)(&mapped, "filter")?;
    let frame = h.clone();
    let predicate = (ops.arrow)(
        "",
        1,
        Rc::new(move |args| {
            let symbol = args.first().cloned().unwrap_or_else(|| (frame.undefined)());
            let boolean = call(&frame, "Boolean", || Ok(vec![symbol.clone()]))?;
            if !(frame.truthy)(&boolean) {
                return Ok(boolean);
            }
            member_call(&frame, owned.clone(), "has", vec![symbol])
        }),
    );
    let symbols = (h.intrinsic)(
        "call-with-this",
        vec![method, mapped, (h.array)(vec![predicate])],
    )?;
    if (h.truthy)(&(h.get)(&symbols, "length")?) {
        call(&h, "this.sendChannelOp", || {
            Ok(vec![
                (h.literal)("subscribe"),
                (h.array)(vec![(h.capture)("SNAPSHOT_CHANNEL")?]),
                symbols,
            ])
        })?;
    }
    Ok((h.undefined)())
}
fn send_channels<V: Clone + 'static, E: 'static>(
    ops: &PrivateClassOps<V, E>,
    r: &V,
    operation: V,
    channels: V,
    symbols: V,
) -> Result<V, E> {
    let h = (ops.frame)(r);
    let mut i = 0.;
    while (h.truthy)(&(h.intrinsic)(
        "less-than",
        vec![(h.number)(i), (h.get)(&symbols, "length")?],
    )?) {
        let sliced = member_call(
            &h,
            symbols.clone(),
            "slice",
            vec![(h.number)(i), (h.number)(i + 100.)],
        )?;
        let chunk = member_call(&h, sliced, "join", vec![(h.literal)(",")])?;
        if (h.truthy)(&chunk) {
            let frame = h.clone();
            let kind = operation.clone();
            let template = ops.template.clone();
            let string_value = ops.string_value.clone();
            (h.for_of)(
                channels.clone(),
                Rc::new(move |channel| {
                    call(&frame, "this.send", || {
                        Ok(vec![call(&frame, "JSON.stringify", || {
                            let id = call(&frame, "Date.now", || Ok(vec![]))?;
                            let mut topic = template(&channel)?;
                            topic.push(58);
                            topic.extend(template(&chunk)?);
                            Ok(vec![(frame.object)(vec![
                                ("id", id),
                                ("type", kind.clone()),
                                ("topic", string_value(topic)),
                                ("privateChannel", (frame.intrinsic)("false", vec![])?),
                                ("response", (frame.intrinsic)("false", vec![])?),
                            ])])
                        })?])
                    })?;
                    Ok(())
                }),
            )?;
        }
        i += 100.;
    }
    Ok((h.undefined)())
}
fn ticker_for_symbol<V: Clone, E>(ops: &PrivateClassOps<V, E>, r: &V, symbol: V) -> Result<V, E> {
    let h = (ops.frame)(r);
    if !(h.truthy)(&symbol) {
        return Ok((h.undefined)());
    }
    let tickers = (h.capture)("this.parent.store.tickersBySymbol")?;
    let key = call(&h, "this.parent.toInternalSymbol", || Ok(vec![symbol]))?;
    (h.intrinsic)("get-index", vec![tickers, key])
}
fn on_message<V: Clone + 'static, E: 'static>(
    ops: &PrivateClassOps<V, E>,
    r: &V,
    input: V,
) -> Result<V, E> {
    let h = (ops.frame)(r);
    let data = (h.get)(&input, "data")?;
    if (h.truthy)(&(h.get)(r, "isDisposed")?) {
        return Ok((h.undefined)());
    }
    call(&h, "this.trackLastReceived", || Ok(vec![]))?;
    let json = call(&h, "jsonParse", || Ok(vec![data]))?;
    if !(h.truthy)(&json) {
        return Ok((h.undefined)());
    }
    if (h.equal)(&(h.get)(&json, "type")?, &(h.literal)("pong")) {
        call(&h, "this.handlePongEvent", || Ok(vec![]))?;
        return Ok((h.undefined)());
    }
    for (prefix, stats, method) in [
        ("/contractMarket/tickerV2", "ticker", "this.handleTickerV2"),
        (
            "/contractMarket/snapshot",
            "snapshot",
            "this.handleSnapshot",
        ),
        (
            "/contract/instrument",
            "instrument",
            "this.handleInstrument",
        ),
        ("/contractMarket/limitCandle", "", ""),
        ("/contractMarket/level2", "", ""),
    ] {
        if !(h.truthy)(&(h.get)(&json, "topic")?) {
            continue;
        }
        let topic = (h.get)(&json, "topic")?;
        if !(h.truthy)(&member_call(
            &h,
            topic,
            "startsWith",
            vec![(h.literal)(prefix)],
        )?) {
            continue;
        }
        if !method.is_empty() {
            call(&h, "this.recordMessageHandlerStats", || {
                Ok(vec![(h.literal)(stats)])
            })?;
            call(&h, method, || Ok(vec![json]))?;
            return Ok((h.undefined)());
        }
        let topic = (h.get)(&json, "topic")?;
        let split = member_call(&h, topic, "split", vec![(h.literal)(":")])?;
        let symbol = (h.get)(&split, "1")?;
        let (symbol, interval) = if prefix == "/contractMarket/limitCandle" {
            let argument = fallback(&h, symbol, || Ok((h.literal)("")))?;
            let split = member_call(&h, argument, "split", vec![(h.literal)("_")])?;
            pair(&h, split)?
        } else {
            (symbol, (h.undefined)())
        };
        let internal = fallback(
            &h,
            call(&h, "this.parent.toInternalSymbol", || {
                Ok(vec![symbol.clone()])
            })?,
            || Ok(symbol),
        )?;
        let mut key = if prefix == "/contractMarket/limitCandle" {
            "candle_"
        } else {
            "orderBook_"
        }
        .encode_utf16()
        .collect::<Vec<_>>();
        key.extend((ops.template)(&internal)?);
        if prefix == "/contractMarket/limitCandle" {
            key.push(95);
            key.extend((ops.template)(&interval)?);
        }
        let key = (ops.string_value)(key);
        call(&h, "this.recordMessageHandlerStats", || {
            Ok(vec![key.clone()])
        })?;
        let handlers = (h.get)(r, "messageHandlers")?;
        let values = (h.intrinsic)("get-index", vec![handlers, key.clone()])?;
        if (h.truthy)(&values) {
            let handlers = (h.get)(r, "messageHandlers")?;
            let values = (h.intrinsic)("get-index", vec![handlers, key])?;
            let invoke = h.invoke.clone();
            (h.for_of)(
                values,
                Rc::new(move |callback| {
                    invoke(callback, vec![json.clone()])?;
                    Ok(())
                }),
            )?;
        }
        return Ok((h.undefined)());
    }
    Ok((h.undefined)())
}
fn initialize<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    construction: Rc<ConstructionOps<V, E>>,
    r: V,
) -> Result<(), E> {
    (ops.define)(&r, "messageHandlers", (ops.value.object)(vec![]))?;
    for key in ["token", "socketEndpoint"] {
        (ops.define)(&r, key, (ops.null)())?;
    }
    for key in ["role", "shardIndex", "shardCount"] {
        (ops.define)(&r, key, (ops.value.undefined)())?;
    }
    for (name, arity, asynchronous) in [
        ("connectAndSubscribe", 0, true),
        ("reconnect", 0, true),
        ("onOpen", 0, false),
        ("onMarketsChanged", 1, false),
        ("ping", 0, false),
        ("subscribe", 0, false),
        ("snapshotShardSymbols", 0, false),
        ("subscribeActiveTicker", 1, false),
        ("unsubscribeActiveTicker", 1, false),
        ("sendChannelOp", 3, false),
        ("onMessage", 1, false),
        ("tickerForApiSymbol", 1, false),
        ("handleTickerV2", 1, false),
        ("handleSnapshot", 1, false),
        ("handleInstrument", 1, false),
        ("listenOHLCV", 2, false),
        ("listenOrderBook", 2, false),
    ] {
        let owner = ops.clone();
        let receiver = r.clone();
        let constructors = construction.clone();
        let body = if asynchronous {
            (ops.async_arrow)(
                name,
                arity,
                Rc::new(move |_| {
                    if name == "connectAndSubscribe" {
                        Box::pin(connect(owner.clone(), receiver.clone()))
                    } else {
                        Box::pin(reconnect(owner.clone(), receiver.clone()))
                    }
                }),
            )
        } else {
            (ops.arrow)(
                name,
                arity,
                Rc::new(move |args| {
                    let arg = |index| {
                        args.get(index)
                            .cloned()
                            .unwrap_or_else(|| (owner.value.undefined)())
                    };
                    match name {
                        "onOpen" => on_open(&owner, &receiver),
                        "onMarketsChanged" => {
                            markets_changed(owner.clone(), &constructors, receiver.clone(), arg(0))
                        }
                        "ping" => ping(&owner, &receiver),
                        "subscribe" => subscribe(&owner, &receiver),
                        "snapshotShardSymbols" => snapshot_symbols(owner.clone(), receiver.clone()),
                        "subscribeActiveTicker" => {
                            active_ticker(&owner, &receiver, arg(0), "subscribe")
                        }
                        "unsubscribeActiveTicker" => {
                            active_ticker(&owner, &receiver, arg(0), "unsubscribe")
                        }
                        "sendChannelOp" => send_channels(&owner, &receiver, arg(0), arg(1), arg(2)),
                        "onMessage" => on_message(&owner, &receiver, arg(0)),
                        "tickerForApiSymbol" => ticker_for_symbol(&owner, &receiver, arg(0)),
                        "handleTickerV2" | "handleSnapshot" | "handleInstrument" => {
                            let h = (owner.frame)(&receiver);
                            match name {
                                "handleTickerV2" => public_streams::handle_ticker_v2(&h, arg(0))?,
                                "handleSnapshot" => public_streams::handle_snapshot(&h, arg(0))?,
                                _ => public_streams::handle_instrument(&h, arg(0))?,
                            };
                            Ok((h.undefined)())
                        }
                        "listenOHLCV" => {
                            public_streams::listen_ohlcv((owner.frame)(&receiver), arg(0), arg(1))
                        }
                        "listenOrderBook" => public_streams::listen_order_book(
                            (owner.frame)(&receiver),
                            arg(0),
                            arg(1),
                        ),
                        _ => Err((owner.error)("unknown futures public native member")),
                    }
                }),
            )
        };
        (ops.define)(&r, name, body)?;
    }
    Ok(())
}
fn construct<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    construction: Rc<ConstructionOps<V, E>>,
    active: V,
    target: V,
    args: Vec<V>,
) -> Result<V, E> {
    let h = &ops.value;
    let parent = args.first().cloned().unwrap_or_else(|| (h.undefined)());
    let opts = args.get(1).cloned().unwrap_or_else(|| (h.undefined)());
    let receiver = std::cell::RefCell::new(None::<V>);
    crate::socket_setup::public_constructor(
        parent,
        |parent| {
            *receiver.borrow_mut() = Some((construction.super_construct)(
                (construction.superclass)(&active)?,
                target,
                vec![parent],
            )?);
            Ok(())
        },
        || {
            initialize(
                ops.clone(),
                construction.clone(),
                receiver.borrow().as_ref().unwrap().clone(),
            )
        },
        |key| {
            if (h.nullish)(&opts) {
                return Ok(None);
            }
            let value = (h.get)(&opts, key)?;
            Ok(if (h.nullish)(&value) {
                None
            } else {
                Some(value)
            })
        },
        |key| match key {
            "detail" => (h.literal)("detail"),
            "0" => (h.number)(0.),
            _ => (h.number)(1.),
        },
        |key, value| (h.set)(receiver.borrow().as_ref().unwrap(), key, value),
    )?;
    Ok(receiver.into_inner().unwrap())
}
pub fn define_futures_public_class<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    construction: Rc<ConstructionOps<V, E>>,
) -> Result<V, E> {
    let base = (ops.value.capture)("BaseWebSocket")?;
    let owner = ops.clone();
    let constructors = construction.clone();
    (construction.derive)(
        "KucoinPublicWebsocket",
        base,
        2,
        Rc::new(move |active, target, args| {
            construct(owner.clone(), constructors.clone(), active, target, args)
        }),
    )
}
