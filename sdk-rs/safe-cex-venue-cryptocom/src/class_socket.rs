//! Native bindings for older socket members. All application logic lives in Rust.
use crate::native_bindings::{BindingOps, BoundHost};
use std::rc::Rc;
fn arg<V: Clone, E>(h: &BindingOps<V, E>, args: &[V], i: usize) -> V {
    args.get(i)
        .cloned()
        .unwrap_or_else(|| (h.value.undefined)())
}
fn call<V: Clone + 'static, E: 'static>(
    h: &BoundHost<V, E>,
    receiver: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let f = h.prepare(receiver, key)?;
    h.call(f, args()?)
}
fn own<V: Clone + 'static, E: 'static>(
    h: &BoundHost<V, E>,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    call(h, h.receiver.clone(), key, args)
}
fn undefined<V, E>(h: &BindingOps<V, E>) -> V {
    (h.value.undefined)()
}
fn lit<V, E>(h: &BindingOps<V, E>, s: &str) -> V {
    (h.value.literal)(s)
}
fn disposed<V: Clone + 'static, E: 'static>(h: &BoundHost<V, E>) -> Result<bool, E> {
    Ok((h.ops.value.truthy)(&h.get(&h.receiver, "isDisposed")?))
}
fn concat<V, E>(h: &BindingOps<V, E>, parts: Vec<V>) -> Result<V, E> {
    let mut text = vec![];
    for p in parts {
        text.extend((h.value.text)(&p)?)
    }
    Ok((h.value.utf16)(text))
}
fn increment<V: Clone + 'static, E: 'static>(h: &BoundHost<V, E>) -> Result<V, E> {
    let value = h.get(&h.receiver, "messageId")?;
    let old = (h.ops.binary)("ToNumeric", value, undefined(&h.ops))?;
    let next = (h.ops.binary)("++", old.clone(), undefined(&h.ops))?;
    (h.ops.value.set)(&h.receiver, "messageId", next)?;
    Ok(old)
}
fn send<V: Clone + 'static, E: 'static>(
    h: &BoundHost<V, E>,
    frame: impl FnOnce() -> Result<V, E>,
) -> Result<(), E> {
    own(h, "send", || {
        let json = (h.ops.value.global)("JSON")?;
        let text = call(h, json, "stringify", || Ok(vec![frame()?]))?;
        Ok(vec![text])
    })?;
    Ok(())
}
fn frame<V: Clone + 'static, E: 'static>(
    h: &BoundHost<V, E>,
    method: &str,
    channels: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    let id = increment(h)?;
    let method = lit(&h.ops, method);
    let params = (h.ops.value.object)(vec![("channels", channels()?)]);
    let clock = (h.ops.value.global)("virtualClock")?;
    let nonce = call(h, clock, "getCurrentTimeMs", || Ok(vec![]))?;
    Ok((h.ops.value.object)(vec![
        ("id", id),
        ("method", method),
        ("params", params),
        ("nonce", nonce),
    ]))
}
fn has<V: Clone + 'static, E: 'static>(h: &BoundHost<V, E>, channel: V) -> Result<bool, E> {
    let set = h.get(&h.receiver, "subscriptions")?;
    Ok((h.ops.value.truthy)(&call(h, set, "has", || {
        Ok(vec![channel])
    })?))
}
fn add<V: Clone + 'static, E: 'static>(h: &BoundHost<V, E>, channel: V) -> Result<(), E> {
    let set = h.get(&h.receiver, "subscriptions")?;
    call(h, set, "add", || Ok(vec![channel]))?;
    Ok(())
}
fn connect<V: Clone + 'static, E: 'static>(h: &BoundHost<V, E>, private: bool) -> Result<V, E> {
    if !disposed(h)? {
        let urls = (h.ops.value.global)("WSS_URLS")?;
        let parent = h.get(&h.receiver, "parent")?;
        let opts = h.get(&parent, "options")?;
        let testnet = h.get(&opts, "testnet")?;
        let urls = (h.ops.value.index)(
            &urls,
            &lit(
                &h.ops,
                if (h.ops.value.truthy)(&testnet) {
                    "TESTNET"
                } else {
                    "PRODUCTION"
                },
            ),
        )?;
        let socket = own(h, "createWebsocket", || {
            Ok(vec![h.get(&urls, if private { "USER" } else { "MARKET" })?])
        })?;
        (h.ops.value.set)(&h.receiver, "ws", socket)?;
        for (event, key) in [
            ("open", "onOpen"),
            ("message", "onMessage"),
            ("close", "onClose"),
        ] {
            let ws = h.get(&h.receiver, "ws")?;
            call(h, ws, "addEventListener", || {
                Ok(vec![lit(&h.ops, event), h.get(&h.receiver, key)?])
            })?;
        }
    }
    Ok(undefined(&h.ops))
}
fn on_open<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<BoundHost<V, E>>,
    private: bool,
) -> Result<V, E> {
    if !disposed(&h)? {
        own(&h, "trackLastReceived", || Ok(vec![]))?;
        if private {
            (h.ops.value.set)(&h.receiver, "isAuthenticated", (h.ops.value.boolean)(false))?;
        }
        let timer = if private {
            "reconnectAuthTimer"
        } else {
            "reconnectSubscriptionTimer"
        };
        let clear = (h.ops.value.global)("clearTimeout")?;
        let value = h.get(&h.receiver, timer)?;
        (h.ops.value.invoke)(clear, undefined(&h.ops), vec![value])?;
        let schedule = if private {
            true
        } else {
            let set = h.get(&h.receiver, "subscriptions")?;
            let size = h.get(&set, "size")?;
            (h.ops.value.truthy)(&(h.ops.binary)(">", size, (h.ops.value.number)(0.))?)
        };
        if schedule {
            let timeout = (h.ops.value.global)("setTimeout")?;
            let callback_h = h.clone();
            let callback = (h.ops.value.function)(
                "",
                0,
                Rc::new(move |_| {
                    if !disposed(&callback_h)? {
                        own(
                            &callback_h,
                            if private {
                                "authenticate"
                            } else {
                                "resubscribe"
                            },
                            || Ok(vec![]),
                        )?;
                    }
                    Ok(undefined(&callback_h.ops))
                }),
            );
            let value = (h.ops.value.invoke)(
                timeout,
                undefined(&h.ops),
                vec![callback, (h.ops.value.number)(1000.)],
            )?;
            (h.ops.value.set)(&h.receiver, timer, value)?;
        }
    }
    Ok(undefined(&h.ops))
}
fn authenticate<V: Clone + 'static, E: Clone + 'static>(h: &BoundHost<V, E>) -> Result<V, E> {
    let result = (|| {
        let f = (h.ops.value.global)("generateWsAuthMessage")?;
        let parent = h.get(&h.receiver, "parent")?;
        let opts = h.get(&parent, "options")?;
        let key = h.get(&opts, "key")?;
        let parent = h.get(&h.receiver, "parent")?;
        let opts = h.get(&parent, "options")?;
        let secret = h.get(&opts, "secret")?;
        let auth = (h.ops.value.invoke)(f, undefined(&h.ops), vec![key, secret])?;
        send(h, || Ok(auth))?;
        Ok(())
    })();
    if let Err(error) = result {
        let parent = h.get(&h.receiver, "parent")?;
        call(h, parent, "emitAndLogError", || {
            let value = (h.ops.value.error_value)(error);
            Ok(vec![concat(
                &h.ops,
                vec![lit(&h.ops, "Failed to authenticate WebSocket: "), value],
            )?])
        })?;
    }
    Ok(undefined(&h.ops))
}
fn resubscribe<V: Clone + 'static, E: 'static>(h: &BoundHost<V, E>, private: bool) -> Result<V, E> {
    if !private || (h.ops.value.truthy)(&h.get(&h.receiver, "isAuthenticated")?) {
        let set = h.get(&h.receiver, "subscriptions")?;
        let size = h.get(&set, "size")?;
        if (h.ops.value.truthy)(&(h.ops.binary)(">", size, (h.ops.value.number)(0.))?) {
            if private {
                send(h, || {
                    frame(h, "subscribe", || {
                        let array = (h.ops.value.global)("Array")?;
                        call(h, array, "from", || {
                            Ok(vec![h.get(&h.receiver, "subscriptions")?])
                        })
                    })
                })?;
            } else {
                own(h, "sendSubscriptionFrames", || {
                    let array = (h.ops.value.global)("Array")?;
                    Ok(vec![call(h, array, "from", || {
                        Ok(vec![h.get(&h.receiver, "subscriptions")?])
                    })?])
                })?;
            }
        }
    }
    Ok(undefined(&h.ops))
}
fn send_frames<V: Clone + 'static, E: 'static>(h: &BoundHost<V, E>, channels: V) -> Result<V, E> {
    let mut i = 0.;
    while (h.ops.value.truthy)(&(h.ops.binary)(
        "<",
        (h.ops.value.number)(i),
        h.get(&channels, "length")?,
    )?) {
        send(h, || {
            frame(h, "subscribe", || {
                call(h, channels.clone(), "slice", || {
                    Ok(vec![
                        (h.ops.value.number)(i),
                        (h.ops.value.number)(i + 100.),
                    ])
                })
            })
        })?;
        i += 100.;
    }
    Ok(undefined(&h.ops))
}
fn register<V: Clone + 'static, E: 'static>(h: &BoundHost<V, E>, channels: V) -> Result<V, E> {
    let new = (h.ops.value.array)(vec![]);
    h.each(channels, |channel| {
        if !has(h, channel.clone())? {
            add(h, channel.clone())?;
            call(h, new.clone(), "push", || Ok(vec![channel]))?;
        }
        Ok(())
    })?;
    own(h, "sendSubscriptionFrames", || Ok(vec![new]))?;
    Ok(undefined(&h.ops))
}
fn subscribe_one<V: Clone + 'static, E: 'static>(
    h: &BoundHost<V, E>,
    channel: V,
    private: bool,
) -> Result<V, E> {
    if !has(h, channel.clone())? {
        add(h, channel.clone())?;
        if !private || (h.ops.value.truthy)(&h.get(&h.receiver, "isAuthenticated")?) {
            send(h, || {
                frame(h, "subscribe", || Ok((h.ops.value.array)(vec![channel])))
            })?;
        }
    }
    Ok(undefined(&h.ops))
}
fn order_updates<V: Clone + 'static, E: 'static>(h: Rc<BoundHost<V, E>>) -> Result<V, E> {
    let channels = (h.ops.value.array)(vec![
        lit(&h.ops, "user.order"),
        lit(&h.ops, "user.advance.order"),
    ]);
    let callback_h = h.clone();
    let filter = h.prepare(channels, "filter")?;
    let callback = (h.ops.value.function)(
        "",
        1,
        Rc::new(move |args| {
            Ok((callback_h.ops.value.boolean)(!has(
                &callback_h,
                arg(&callback_h.ops, &args, 0),
            )?))
        }),
    );
    let new = h.call(filter, vec![callback])?;
    h.each(new.clone(), |channel| add(&h, channel))?;
    if (h.ops.value.truthy)(&h.get(&h.receiver, "isAuthenticated")?)
        && (h.ops.value.truthy)(&(h.ops.binary)(
            ">",
            h.get(&new, "length")?,
            (h.ops.value.number)(0.),
        )?)
    {
        send(&h, || frame(&h, "subscribe", || Ok(new)))?;
    }
    Ok(undefined(&h.ops))
}
fn unsubscribe<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<BoundHost<V, E>>,
    channels: V,
) -> Result<V, E> {
    if (h.ops.value.truthy)(&(h.ops.binary)(
        ">",
        h.get(&channels, "length")?,
        (h.ops.value.number)(0.),
    )?) {
        send(&h, || frame(&h, "unsubscribe", || Ok(channels.clone())))?;
        let method = h.prepare(channels, "forEach")?;
        let callback_h = h.clone();
        let callback = (h.ops.value.function)(
            "",
            1,
            Rc::new(move |args| {
                let set = callback_h.get(&callback_h.receiver, "subscriptions")?;
                call(&callback_h, set, "delete", || {
                    Ok(vec![arg(&callback_h.ops, &args, 0)])
                })
            }),
        );
        h.call(method, vec![callback])?;
    }
    Ok(undefined(&h.ops))
}
fn dispose<V: Clone + 'static, E: 'static>(h: &BoundHost<V, E>, private: bool) -> Result<V, E> {
    let clear = (h.ops.value.global)("clearTimeout")?;
    let timer = h.get(
        &h.receiver,
        if private {
            "reconnectAuthTimer"
        } else {
            "reconnectSubscriptionTimer"
        },
    )?;
    (h.ops.value.invoke)(clear, undefined(&h.ops), vec![timer])?;
    let set = h.get(&h.receiver, "subscriptions")?;
    call(h, set, "clear", || Ok(vec![]))?;
    if private {
        (h.ops.value.set)(&h.receiver, "isAuthenticated", (h.ops.value.boolean)(false))?;
    } else {
        let map = h.get(&h.receiver, "marketIdsByIndexInstrument")?;
        call(h, map, "clear", || Ok(vec![]))?;
    }
    if (h.ops.value.truthy)(&h.get(&h.receiver, "ws")?) {
        let ws = h.get(&h.receiver, "ws")?;
        call(h, ws, "close", || Ok(vec![]))?;
    }
    Ok(undefined(&h.ops))
}
fn index_markets<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<BoundHost<V, E>>,
    name: V,
) -> Result<V, E> {
    let map = h.get(&h.receiver, "marketIdsByIndexInstrument")?;
    let cached = call(&h, map, "get", || Ok(vec![name.clone()]))?;
    if (h.ops.value.truthy)(&cached) {
        return Ok(cached);
    }
    let constructor = (h.ops.value.global)("Set")?;
    let store = h.get(&h.receiver, "store")?;
    let markets = h.get(&store, "markets")?;
    let filter = h.prepare(markets, "filter")?;
    let callback_h = h.clone();
    let index = name.clone();
    let predicate = (h.ops.value.function)(
        "",
        1,
        Rc::new(move |args| {
            let market = arg(&callback_h.ops, &args, 0);
            let info = callback_h.get(&market, "info")?;
            let symbol = if (callback_h.ops.value.nullish)(&info) {
                undefined(&callback_h.ops)
            } else {
                callback_h.get(&info, "underlyingSymbol")?
            };
            Ok((callback_h.ops.value.boolean)(
                (callback_h.ops.value.equal)(&symbol, &index),
            ))
        }),
    );
    let filtered = h.call(filter, vec![predicate])?;
    let mapped = h.prepare(filtered, "map")?;
    let callback_h = h.clone();
    let project = (h.ops.value.function)(
        "",
        1,
        Rc::new(move |args| callback_h.get(&arg(&callback_h.ops, &args, 0), "id")),
    );
    let rows = h.call(mapped, vec![project])?;
    let result = (h.ops.construct)(constructor, vec![rows])?;
    if (h.ops.value.truthy)(&(h.ops.binary)(
        ">",
        h.get(&result, "size")?,
        (h.ops.value.number)(0.),
    )?) {
        let map = h.get(&h.receiver, "marketIdsByIndexInstrument")?;
        call(&h, map, "set", || Ok(vec![name, result.clone()]))?;
    }
    Ok(result)
}
fn valuation<V: Clone + 'static, E: Clone + 'static>(
    h: &BoundHost<V, E>,
    update: V,
) -> Result<V, E> {
    let data = h.get(&update, "data")?;
    let first = (h.ops.value.index)(&data, &(h.ops.value.number)(0.))?;
    let raw = if (h.ops.value.nullish)(&first) {
        undefined(&h.ops)
    } else {
        h.get(&first, "v")?
    };
    if !(h.ops.is_string)(&raw) {
        return Ok(undefined(&h.ops));
    }
    let trimmed = call(h, raw.clone(), "trim", || Ok(vec![]))?;
    if (h.ops.value.equal)(&trimmed, &lit(&h.ops, "")) {
        return Ok(undefined(&h.ops));
    }
    let value = h.global("Number", vec![raw])?;
    let number = (h.ops.value.global)("Number")?;
    let finite = call(h, number, "isFinite", || Ok(vec![value.clone()]))?;
    if !(h.ops.value.truthy)(&finite) {
        return Ok(undefined(&h.ops));
    }
    if ((h.ops.value.equal)(&h.get(&update, "channel")?, &lit(&h.ops, "mark"))
        || (h.ops.value.equal)(&h.get(&update, "channel")?, &lit(&h.ops, "index")))
        && (h.ops.value.truthy)(&(h.ops.binary)(
            "<=",
            value.clone(),
            (h.ops.value.number)(0.),
        )?)
    {
        return Ok(undefined(&h.ops));
    }
    if (h.ops.value.equal)(&h.get(&update, "channel")?, &lit(&h.ops, "index")) {
        let ids = own(h, "getMarketIdsByIndexInstrument", || {
            Ok(vec![h.get(&update, "instrument_name")?])
        })?;
        h.each(ids, |id| {
            let store = h.get(&h.receiver, "store")?;
            let markets = h.get(&store, "marketsById")?;
            let market = (h.ops.value.index)(&markets, &id)?;
            if !(h.ops.value.truthy)(&market) {
                return Ok(());
            }
            let store = h.get(&h.receiver, "store")?;
            let tickers = h.get(&store, "tickersBySymbol")?;
            let ticker = (h.ops.value.index)(&tickers, &h.get(&market, "symbol")?)?;
            if (h.ops.value.truthy)(&ticker) {
                let store = h.get(&h.receiver, "store")?;
                call(h, store, "updateTicker", || {
                    Ok(vec![
                        ticker,
                        (h.ops.value.object)(vec![("index", value.clone())]),
                    ])
                })?;
            }
            Ok(())
        })?;
        return Ok(undefined(&h.ops));
    }
    let store = h.get(&h.receiver, "store")?;
    let markets = h.get(&store, "marketsById")?;
    let market = (h.ops.value.index)(&markets, &h.get(&update, "instrument_name")?)?;
    if !(h.ops.value.truthy)(&market) {
        return Ok(undefined(&h.ops));
    }
    let store = h.get(&h.receiver, "store")?;
    let tickers = h.get(&store, "tickersBySymbol")?;
    let ticker = (h.ops.value.index)(&tickers, &h.get(&market, "symbol")?)?;
    if !(h.ops.value.truthy)(&ticker) {
        return Ok(undefined(&h.ops));
    }
    let field = if (h.ops.value.equal)(&h.get(&update, "channel")?, &lit(&h.ops, "mark")) {
        Some("mark")
    } else if (h.ops.value.equal)(&h.get(&update, "channel")?, &lit(&h.ops, "funding")) {
        Some("fundingRate")
    } else {
        None
    };
    if let Some(field) = field {
        let store = h.get(&h.receiver, "store")?;
        call(h, store, "updateTicker", || {
            Ok(vec![ticker, (h.ops.value.object)(vec![(field, value)])])
        })?;
    }
    Ok(undefined(&h.ops))
}
fn market_data<V: Clone + 'static, E: Clone + 'static>(
    h: &BoundHost<V, E>,
    markets: V,
) -> Result<V, E> {
    let channels = (h.ops.value.array)(vec![]);
    let mut ticker = 0f64;
    let mut valuation = 0f64;
    let mut invalid = 0f64;
    let mut queue = |channel: V, optional: bool| -> Result<bool, E> {
        if has(h, channel.clone())?
            || (h.ops.value.truthy)(&call(h, channels.clone(), "includes", || {
                Ok(vec![channel.clone()])
            })?)
        {
            return Ok(true);
        }
        let set = h.get(&h.receiver, "subscriptions")?;
        let sum = (h.ops.binary)("+", h.get(&set, "size")?, h.get(&channels, "length")?)?;
        if (h.ops.value.truthy)(&(h.ops.binary)(">=", sum, (h.ops.value.number)(350.))?) {
            if optional {
                valuation += 1.;
            } else {
                ticker += 1.;
            }
            return Ok(false);
        }
        call(h, channels.clone(), "push", || Ok(vec![channel]))?;
        Ok(true)
    };
    h.each(markets.clone(), |market| {
        let instrument = h.get(&market, "instrumentName")?;
        queue(
            concat(&h.ops, vec![lit(&h.ops, "ticker."), instrument])?,
            false,
        )?;
        Ok(())
    })?;
    h.each(markets, |market| {
        let instrument = h.get(&market, "instrumentName")?;
        let index = h.get(&market, "indexInstrumentName")?;
        queue(
            concat(&h.ops, vec![lit(&h.ops, "mark."), instrument.clone()])?,
            true,
        )?;
        let valid = if (h.ops.value.nullish)(&index) {
            false
        } else {
            (h.ops.value.truthy)(&call(h, index.clone(), "endsWith", || {
                Ok(vec![lit(&h.ops, "-INDEX")])
            })?)
        };
        if valid {
            if queue(
                concat(&h.ops, vec![lit(&h.ops, "index."), index.clone()])?,
                true,
            )? {
                let map = h.get(&h.receiver, "marketIdsByIndexInstrument")?;
                let cached = call(h, map, "get", || Ok(vec![index.clone()]))?;
                let ids = if (h.ops.value.truthy)(&cached) {
                    cached
                } else {
                    (h.ops.construct)((h.ops.value.global)("Set")?, vec![])?
                };
                call(h, ids.clone(), "add", || Ok(vec![instrument.clone()]))?;
                let map = h.get(&h.receiver, "marketIdsByIndexInstrument")?;
                call(h, map, "set", || Ok(vec![index, ids]))?;
            }
        } else if (h.ops.value.truthy)(&index) {
            invalid += 1.;
        }
        queue(
            concat(&h.ops, vec![lit(&h.ops, "funding."), instrument])?,
            true,
        )?;
        Ok(())
    })?;
    if ticker > 0. || valuation > 0. || invalid > 0. {
        let summary = concat(
            &h.ops,
            vec![
                (h.ops.value.number)(ticker),
                lit(&h.ops, " ticker / "),
                (h.ops.value.number)(valuation),
                lit(&h.ops, " valuation / "),
                (h.ops.value.number)(invalid),
                lit(&h.ops, " invalid index"),
            ],
        )?;
        let parent = h.get(&h.receiver, "parent")?;
        call(h, parent, "log", || {
            let message = concat(
                &h.ops,
                vec![
                    lit(&h.ops, "Crypto.com public WS cap 350/400; skipped "),
                    summary,
                    lit(&h.ops, " channels"),
                ],
            )?;
            let severity = (h.ops.value.global)("LogSeverity")?;
            Ok(vec![message, h.get(&severity, "Warning")?])
        })?;
    }
    own(h, "registerAndSubscribe", || Ok(vec![channels]))?;
    Ok(undefined(&h.ops))
}
fn symbol_channel<V: Clone + 'static, E: 'static>(
    h: &BoundHost<V, E>,
    name: &str,
    args: &[V],
) -> Result<V, E> {
    let symbol = arg(&h.ops, args, 0);
    let parts = match name {
        "subscribeToOrderBook" => {
            let depth = arg(&h.ops, args, 1);
            let depth = if (h.ops.value.equal)(&depth, &undefined(&h.ops)) {
                (h.ops.value.number)(50.)
            } else {
                depth
            };
            vec![lit(&h.ops, "book."), symbol, lit(&h.ops, "."), depth]
        }
        "subscribeToTicker" => vec![lit(&h.ops, "ticker."), symbol],
        "subscribeToTrades" => vec![lit(&h.ops, "trade."), symbol],
        "subscribeToCandlestick" => vec![
            lit(&h.ops, "candlestick."),
            arg(&h.ops, args, 1),
            lit(&h.ops, "."),
            symbol,
        ],
        _ => unreachable!(),
    };
    subscribe_one(h, concat(&h.ops, parts)?, false)
}
fn candle<V: Clone + 'static, E: 'static>(h: Rc<BoundHost<V, E>>, update: V) -> Result<V, E> {
    let handler = h.get(&update, "subscription")?;
    let handlers = h.get(&h.receiver, "messageHandlers")?;
    let value = (h.ops.value.index)(&handlers, &handler)?;
    let callbacks = if (h.ops.value.truthy)(&value) {
        value
    } else {
        (h.ops.value.array)(vec![])
    };
    let method = h.prepare(callbacks, "forEach")?;
    let callback_h = h.clone();
    let callback = (h.ops.value.function)(
        "",
        1,
        Rc::new(move |args| {
            (callback_h.ops.value.invoke)(
                arg(&callback_h.ops, &args, 0),
                undefined(&callback_h.ops),
                vec![update.clone()],
            )
        }),
    );
    h.call(method, vec![callback])?;
    Ok(undefined(&h.ops))
}
pub fn member<V: Clone + 'static, E: Clone + 'static>(
    ops: Rc<BindingOps<V, E>>,
    receiver: V,
    name: &'static str,
    private: bool,
) -> V {
    let h = Rc::new(BoundHost {
        ops: ops.clone(),
        receiver,
    });
    let length = match name {
        "subscribeToCandlestick" => 2,
        "sendSubscriptionFrames"
        | "registerAndSubscribe"
        | "subscribeToOrderBook"
        | "subscribeToTicker"
        | "subscribeToTickers"
        | "subscribeToMarketData"
        | "subscribeToTrades"
        | "handleCandlestickUpdate"
        | "handleValuationUpdate"
        | "getMarketIdsByIndexInstrument"
        | "unsubscribe"
        | "mapOrderType"
        | "mapOrderStatus"
        | "mapTimeInForce" => 1,
        _ => 0,
    };
    (ops.value.function)(
        name,
        length,
        Rc::new(move |args| {
            let result = match name {
                "connectAndSubscribe" => connect(&h, private),
                "onOpen" => on_open(h.clone(), private),
                "shouldEnforcePongMissReconnect" => Ok((h.ops.value.boolean)(false)),
                "getWatchdogStaleThresholdMs" => {
                    (h.ops.value.global)("CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS")
                }
                "getWatchdogPingThresholdMs" => {
                    (h.ops.value.global)("CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS")
                }
                "ping" => Ok(undefined(&h.ops)),
                "authenticate" => authenticate(&h),
                "resubscribe" => resubscribe(&h, private),
                "sendSubscriptionFrames" => send_frames(&h, arg(&h.ops, &args, 0)),
                "registerAndSubscribe" => register(&h, arg(&h.ops, &args, 0)),
                "subscribeToOrderBook"
                | "subscribeToTicker"
                | "subscribeToTrades"
                | "subscribeToCandlestick" => symbol_channel(&h, name, &args),
                "subscribeToTickers" => own(&h, "registerAndSubscribe", || {
                    let symbols = arg(&h.ops, &args, 0);
                    let callback_h = h.clone();
                    let map = h.prepare(symbols, "map")?;
                    let callback = (h.ops.value.function)(
                        "",
                        1,
                        Rc::new(move |args| {
                            concat(
                                &callback_h.ops,
                                vec![
                                    lit(&callback_h.ops, "ticker."),
                                    arg(&callback_h.ops, &args, 0),
                                ],
                            )
                        }),
                    );
                    Ok(vec![h.call(map, vec![callback])?])
                })
                .map(|_| undefined(&h.ops)),
                "subscribeToMarketData" => market_data(&h, arg(&h.ops, &args, 0)),
                "subscribeToOrderUpdates" => order_updates(h.clone()),
                "subscribeToPositionUpdates" => {
                    subscribe_one(&h, lit(&h.ops, "user.positions"), true)
                }
                "subscribeToBalanceUpdates" => subscribe_one(&h, lit(&h.ops, "user.balance"), true),
                "handleCandlestickUpdate" => candle(h.clone(), arg(&h.ops, &args, 0)),
                "handleValuationUpdate" => valuation(&h, arg(&h.ops, &args, 0)),
                "getMarketIdsByIndexInstrument" => index_markets(h.clone(), arg(&h.ops, &args, 0)),
                "unsubscribe" => unsubscribe(h.clone(), arg(&h.ops, &args, 0)),
                "dispose" => dispose(&h, private),
                "mapOrderType" | "mapOrderStatus" | "mapTimeInForce" => {
                    crate::class_mappings::member(&h, name, arg(&h.ops, &args, 0), true)
                }
                _ => unreachable!("socket binding must enumerate every member"),
            };
            if private && matches!(name, "connectAndSubscribe" | "authenticate" | "resubscribe") {
                Ok((h.ops.promise)(Box::pin(async move { result })))
            } else {
                result
            }
        }),
    )
}
