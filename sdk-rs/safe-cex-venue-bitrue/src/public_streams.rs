//! Public message and retained subscription bodies over original runtime bindings.
use crate::collection_reads::{Op, ReadHost};
use std::{cell::RefCell, rc::Rc};
pub trait StreamHost: ReadHost {
    fn global(&self, name: &'static str) -> Result<Self::Value, Self::Error>;
    fn dynamic(&self, value: Self::Value, key: Self::Value) -> Result<Self::Value, Self::Error>;
    fn put(
        &self,
        value: Self::Value,
        key: Self::Value,
        next: Self::Value,
    ) -> Result<(), Self::Error>;
    fn delete(&self, value: Self::Value, key: Self::Value) -> Result<(), Self::Error>;
    fn spread(&self, value: Self::Value) -> Result<Vec<Self::Value>, Self::Error>;
    fn new_set(&self) -> Result<Self::Value, Self::Error>;
    fn is_blob(&self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn pair(&self, value: Self::Value) -> Result<(Self::Value, Self::Value), Self::Error>;
    fn null(&self) -> Self::Value;
}
fn get<H: StreamHost>(h: &H, r: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.get(r, k)
}
fn opt<H: StreamHost>(h: &H, r: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(r) {
        Ok(h.undefined())
    } else {
        get(h, r, k)
    }
}
fn method<H: StreamHost>(h: &H, r: H::Value, k: &str) -> Result<(H::Value, H::Value), H::Error> {
    Ok((get(h, &r, k)?, r))
}
fn invoke<H: StreamHost>(
    h: &H,
    p: (H::Value, H::Value),
    a: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    h.call(p.0, p.1, a)
}
fn call<H: StreamHost>(
    h: &H,
    r: H::Value,
    k: &str,
    a: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    invoke(h, method(h, r, k)?, a)
}
fn dynamic<H: StreamHost>(
    h: &H,
    r: &H::Value,
    field: &str,
    key: H::Value,
) -> Result<H::Value, H::Error> {
    h.dynamic(get(h, r, field)?, key)
}
fn fallback<H: StreamHost>(h: &H, v: H::Value) -> H::Value {
    if h.truthy(&v) { v } else { h.array(vec![]) }
}
fn send<H: StreamHost>(h: &H, r: H::Value, event: &str, topic: H::Value) -> Result<(), H::Error> {
    let p = method(h, r, "send")?;
    let stringify = h.global("JSON.stringify")?;
    let id = h.call(h.global("nanoid")?, h.undefined(), vec![])?;
    let payload = h.object(vec![
        ("event", h.text(event)),
        ("params", h.object(vec![("channel", topic), ("cb_id", id)])),
    ]);
    let encoded = h.call(stringify, h.undefined(), vec![payload])?;
    invoke(h, p, vec![encoded])?;
    Ok(())
}
fn for_each<H: StreamHost>(h: &H, array: H::Value, callback: H::Value) -> Result<(), H::Error> {
    call(h, array, "forEach", vec![callback])?;
    Ok(())
}
pub async fn message<H: StreamHost>(
    h: &H,
    r: H::Value,
    event: H::Value,
) -> Result<H::Value, H::Error> {
    let data = get(h, &event, "data")?;
    if !h.truthy(&get(h, &r, "isDisposed")?) {
        call(h, r.clone(), "trackLastReceived", vec![])?;
        let mut text = data.clone();
        if h.is_blob(&data)? {
            let p = h.call(h.global("readBlob")?, h.undefined(), vec![data])?;
            text = h.await_value(p).await?;
            if h.truthy(&get(h, &r, "isDisposed")?) {
                return Ok(h.undefined());
            }
        }
        let json = h.call(h.global("jsonParse")?, h.undefined(), vec![text.clone()])?;
        if !h.truthy(&json) {
            if h.truthy(&text) {
                h.call(
                    h.global("console.log")?,
                    h.undefined(),
                    vec![h.text("Unknown data"), text],
                )?;
            }
            return Ok(h.undefined());
        }
        if !h.equal(&get(h, &json, "ping")?, &h.undefined())
            || h.equal(&get(h, &json, "event")?, &h.text("ping"))
        {
            call(h, r, "handlePingEvent", vec![json])?;
        } else if h.equal(&get(h, &json, "event_rep")?, &h.text("rep")) {
            if h.truthy(&dynamic(
                h,
                &r,
                "resolveRequests",
                get(h, &json, "channel")?,
            )?) {
                let array = dynamic(h, &r, "resolveRequests", get(h, &json, "channel")?)?;
                let host = h.clone();
                let original = json.clone();
                for_each(
                    h,
                    array,
                    h.function(Rc::new(move |a| {
                        host.call(a[0].clone(), host.undefined(), vec![original.clone()])?;
                        Ok(host.undefined())
                    })),
                )?;
                h.put(
                    get(h, &r, "resolveRequests")?,
                    get(h, &json, "channel")?,
                    h.array(vec![]),
                )?;
            }
        } else {
            let handlers = dynamic(h, &r, "messageHandlers", get(h, &json, "channel")?)?;
            if h.truthy(&handlers) {
                let host = h.clone();
                let original = json.clone();
                for_each(
                    h,
                    handlers,
                    h.function(Rc::new(move |a| {
                        let handler = a[0].clone();
                        let tick = get(&host, &original, "tick")?;
                        let value = if host.truthy(&tick) {
                            tick
                        } else {
                            get(&host, &original, "data")?
                        };
                        host.call(handler, host.undefined(), vec![value])?;
                        Ok(host.undefined())
                    })),
                )?;
            }
        }
    }
    Ok(h.undefined())
}
fn append<H: StreamHost>(
    h: &H,
    r: &H::Value,
    key: H::Value,
    callback: H::Value,
) -> Result<(), H::Error> {
    let target = get(h, r, "messageHandlers")?;
    let current = dynamic(h, r, "messageHandlers", key.clone())?;
    let mut values = h.spread(fallback(h, current))?;
    values.push(callback);
    h.put(target, key, h.array(values))
}
fn increment<H: StreamHost>(h: &H, r: &H::Value, key: H::Value) -> Result<(), H::Error> {
    let target = get(h, r, "topics")?;
    let count = dynamic(h, r, "topics", key.clone())?;
    let count = if h.truthy(&count) {
        count
    } else {
        h.number(0.0)
    };
    h.put(target, key, h.arithmetic(Op::Add, count, h.number(1.0))?)
}
fn remove<H: StreamHost>(
    h: &H,
    r: &H::Value,
    key: H::Value,
    parser: H::Value,
) -> Result<(), H::Error> {
    let target = get(h, r, "messageHandlers")?;
    let array = fallback(h, dynamic(h, r, "messageHandlers", key.clone())?);
    let p = method(h, array, "filter")?;
    let host = h.clone();
    let filtered = invoke(
        h,
        p,
        vec![h.function(Rc::new(move |a| {
            Ok(host.boolean(!host.equal(&a[0], &parser)))
        }))],
    )?;
    let values = h.spread(filtered)?;
    h.put(target, key, h.array(values))
}
pub fn listen_candles<H: StreamHost>(
    h: &H,
    r: H::Value,
    opts: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    let p = method(h, r.clone(), "formatKlineTopic")?;
    let topic = invoke(
        h,
        p,
        vec![get(h, &opts, "symbol")?, get(h, &opts, "interval")?],
    )?;
    let host = h.clone();
    let receiver = r.clone();
    let parser = h.function(Rc::new(move |a| {
        let p = method(&host, receiver.clone(), "mapKline")?;
        let candles = invoke(
            &host,
            p,
            vec![host.array(vec![a.first().cloned().unwrap_or_else(|| host.undefined())])],
        )?;
        for_each(&host, candles, callback.clone())?;
        Ok(host.undefined())
    }));
    let wait: Rc<RefCell<Option<H::Value>>> = Rc::new(RefCell::new(None));
    let weak = Rc::downgrade(&wait);
    let host = h.clone();
    let receiver = r.clone();
    let key = topic.clone();
    let parsed = parser.clone();
    let starter = h.function(Rc::new(move |_| {
        if host.truthy(&get(&host, &receiver, "isConnected")?) {
            append(&host, &receiver, key.clone(), parsed.clone())?;
            increment(&host, &receiver, key.clone())?;
            send(&host, receiver.clone(), "sub", key.clone())?;
        } else {
            let timer = host.global("setTimeout")?;
            let owner = host.clone();
            let retained = weak.upgrade().expect("live wait owner");
            let callback = host.function(Rc::new(move |_| {
                let f = retained.borrow().as_ref().expect("wait callback").clone();
                owner.call(f, owner.undefined(), vec![])?;
                Ok(owner.undefined())
            }));
            host.call(timer, host.undefined(), vec![callback, host.number(100.0)])?;
        }
        Ok(host.undefined())
    }));
    *wait.borrow_mut() = Some(starter.clone());
    h.call(starter, h.undefined(), vec![])?;
    let host = h.clone();
    let receiver = r;
    let key = topic;
    let retained = wait;
    Ok(h.function(Rc::new(move |_| {
        let _owner = &retained;
        remove(&host, &receiver, key.clone(), parser.clone())?;
        let target = get(&host, &receiver, "topics")?;
        let count = host.dynamic(target.clone(), key.clone())?;
        host.put(
            target,
            key.clone(),
            host.arithmetic(Op::Sub, count, host.number(1.0))?,
        )?;
        if host.truthy(&get(&host, &receiver, "isConnected")?) {
            let handlers = dynamic(&host, &receiver, "messageHandlers", key.clone())?;
            if !host.truthy(&get(&host, &handlers, "length")?) {
                call(
                    &host,
                    receiver.clone(),
                    "cleanupMessageHandlerStats",
                    vec![key.clone()],
                )?;
                send(&host, receiver.clone(), "unsub", key.clone())?;
            }
        }
        Ok(host.undefined())
    })))
}
struct BookState<V> {
    timeout: V,
    fallback: V,
    received: bool,
}
fn update_ticker<H: StreamHost>(
    h: &H,
    r: &H::Value,
    symbol: H::Value,
    patch: H::Value,
) -> Result<(), H::Error> {
    let parent = get(h, r, "parent")?;
    call(
        h,
        get(h, &parent, "store")?,
        "updateTicker",
        vec![h.object(vec![("id", symbol)]), patch],
    )?;
    Ok(())
}
pub fn listen_book<H: StreamHost>(
    h: &H,
    r: H::Value,
    symbol: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    let state = Rc::new(RefCell::new(BookState {
        timeout: h.null(),
        fallback: h.null(),
        received: false,
    }));
    let resolved = call(h, r.clone(), "resolveMarketSymbol", vec![symbol])?;
    let primary = h.concat(vec![
        h.text("market_"),
        h.string(call(
            h,
            r.clone(),
            "convertToWsSymbol",
            vec![resolved.clone()],
        )?)?,
        h.text("_simple_depth_step0"),
    ])?;
    let fallback_topic = h.concat(vec![
        h.text("market_"),
        h.string(call(
            h,
            r.clone(),
            "convertToWsSymbol",
            vec![resolved.clone()],
        )?)?,
        h.text("_depth_step0"),
    ])?;
    let subscribed = h.new_set()?;
    let parsers = h.object(vec![]);
    let book = h.object(vec![("bids", h.array(vec![])), ("asks", h.array(vec![]))]);
    let host = h.clone();
    let receiver = r.clone();
    let received = state.clone();
    let retained_book = book.clone();
    let market = resolved.clone();
    let parser = h.function(Rc::new(move |a| {
        received.borrow_mut().received = true;
        let topic = a[0].clone();
        let data = a[1].clone();
        call(
            &host,
            receiver.clone(),
            "recordMessageHandlerStats",
            vec![topic],
        )?;
        let buys = get(&host, &data, "buys")?;
        let buys = if host.truthy(&buys) {
            buys
        } else {
            fallback(&host, get(&host, &data, "bids")?)
        };
        let asks = fallback(&host, get(&host, &data, "asks")?);
        for (key, rows) in [("bids", buys), ("asks", asks)] {
            let mapped = host.map(rows, |row| {
                let (price, amount) = host.pair(row)?;
                Ok(host.object(vec![
                    ("price", price),
                    ("amount", amount),
                    ("total", host.number(0.0)),
                ]))
            })?;
            host.set(&retained_book, key, mapped)?;
        }
        host.call(
            host.global("calcOrderBookTotal")?,
            host.undefined(),
            vec![retained_book.clone()],
        )?;
        let bid = host.dynamic(get(&host, &retained_book, "bids")?, host.number(0.0))?;
        let ask = host.dynamic(get(&host, &retained_book, "asks")?, host.number(0.0))?;
        let patch = host.object(vec![
            ("bid", opt(&host, &bid, "price")?),
            ("ask", opt(&host, &ask, "price")?),
        ]);
        update_ticker(&host, &receiver, market.clone(), patch)?;
        host.call(
            callback.clone(),
            host.undefined(),
            vec![retained_book.clone()],
        )?;
        Ok(host.undefined())
    }));
    let host = h.clone();
    let receiver = r.clone();
    let memberships = subscribed.clone();
    let by_topic = parsers.clone();
    let subscribe = h.function(Rc::new(move |a| {
        let topic = a[0].clone();
        let owner = host.clone();
        let parse = parser.clone();
        let key = topic.clone();
        let handler = host.function(Rc::new(move |a| {
            owner.call(
                parse.clone(),
                owner.undefined(),
                vec![
                    key.clone(),
                    a.first().cloned().unwrap_or_else(|| owner.undefined()),
                ],
            )?;
            Ok(owner.undefined())
        }));
        host.put(by_topic.clone(), topic.clone(), handler.clone())?;
        append(&host, &receiver, topic.clone(), handler)?;
        increment(&host, &receiver, topic.clone())?;
        call(&host, memberships.clone(), "add", vec![topic.clone()])?;
        send(&host, receiver.clone(), "sub", topic)?;
        Ok(host.undefined())
    }));
    let wait: Rc<RefCell<Option<H::Value>>> = Rc::new(RefCell::new(None));
    let weak = Rc::downgrade(&wait);
    let host = h.clone();
    let receiver = r.clone();
    let membership = subscribed.clone();
    let flag = state.clone();
    let primary_key = primary;
    let fallback_key = fallback_topic;
    let subscribe_fn = subscribe;
    let wait_fn = h.function(Rc::new(move |_| {
        if host.truthy(&get(&host, &receiver, "isConnected")?) {
            host.call(
                subscribe_fn.clone(),
                host.undefined(),
                vec![primary_key.clone()],
            )?;
            let timer = host.global("setTimeout")?;
            let owner = host.clone();
            let receiver2 = receiver.clone();
            let flag2 = flag.clone();
            let memberships = membership.clone();
            let key = fallback_key.clone();
            let subscribing = subscribe_fn.clone();
            let cb = host.function(Rc::new(move |_| {
                let got = flag2.borrow().received;
                if !got && owner.truthy(&get(&owner, &receiver2, "isConnected")?) {
                    let has = call(&owner, memberships.clone(), "has", vec![key.clone()])?;
                    if !owner.truthy(&has) {
                        owner.call(subscribing.clone(), owner.undefined(), vec![key.clone()])?;
                    }
                }
                Ok(owner.undefined())
            }));
            let id = host.call(timer, host.undefined(), vec![cb, host.number(3000.0)])?;
            flag.borrow_mut().fallback = id;
            update_ticker(
                &host,
                &receiver,
                resolved.clone(),
                host.object(vec![("bid", host.number(0.0)), ("ask", host.number(0.0))]),
            )?;
        } else {
            let timer = host.global("setTimeout")?;
            let owner = host.clone();
            let retained = weak.upgrade().expect("live book wait owner");
            let cb = host.function(Rc::new(move |_| {
                let f = retained.borrow().as_ref().expect("book wait").clone();
                owner.call(f, owner.undefined(), vec![])?;
                Ok(owner.undefined())
            }));
            let id = host.call(timer, host.undefined(), vec![cb, host.number(100.0)])?;
            flag.borrow_mut().timeout = id;
        }
        Ok(host.undefined())
    }));
    *wait.borrow_mut() = Some(wait_fn.clone());
    h.call(wait_fn, h.undefined(), vec![])?;
    let host = h.clone();
    let receiver = r;
    let flag = state;
    let retained_wait = wait;
    Ok(h.function(Rc::new(move |_| {
        let _owner = &retained_wait;
        let owner = host.clone();
        let receiver2 = receiver.clone();
        let by_topic = parsers.clone();
        for_each(
            &host,
            subscribed.clone(),
            host.function(Rc::new(move |a| {
                let topic = a[0].clone();
                let parser = owner.dynamic(by_topic.clone(), topic.clone())?;
                remove(&owner, &receiver2, topic.clone(), parser)?;
                owner.delete(by_topic.clone(), topic)?;
                Ok(owner.undefined())
            })),
        )?;
        host.set(&book, "asks", host.array(vec![]))?;
        host.set(&book, "bids", host.array(vec![]))?;
        for which in [true, false] {
            let id = {
                let s = flag.borrow();
                if which {
                    s.timeout.clone()
                } else {
                    s.fallback.clone()
                }
            };
            if host.truthy(&id) {
                let clear = host.global("clearTimeout")?;
                let id = {
                    let s = flag.borrow();
                    if which {
                        s.timeout.clone()
                    } else {
                        s.fallback.clone()
                    }
                };
                host.call(clear, host.undefined(), vec![id])?;
                let mut s = flag.borrow_mut();
                if which {
                    s.timeout = host.null()
                } else {
                    s.fallback = host.null()
                }
            }
        }
        let owner = host.clone();
        let receiver2 = receiver.clone();
        for_each(
            &host,
            subscribed.clone(),
            host.function(Rc::new(move |a| {
                let topic = a[0].clone();
                let target = get(&owner, &receiver2, "topics")?;
                let count = dynamic(&owner, &receiver2, "topics", topic.clone())?;
                let count = if owner.truthy(&count) {
                    count
                } else {
                    owner.number(1.0)
                };
                let next = owner.arithmetic(
                    Op::Max,
                    owner.arithmetic(Op::Sub, count, owner.number(1.0))?,
                    owner.number(0.0),
                )?;
                owner.put(target, topic.clone(), next)?;
                if owner.truthy(&get(&owner, &receiver2, "isConnected")?) {
                    let list = dynamic(&owner, &receiver2, "messageHandlers", topic.clone())?;
                    if !owner.truthy(&opt(&owner, &list, "length")?) {
                        call(
                            &owner,
                            receiver2.clone(),
                            "cleanupMessageHandlerStats",
                            vec![topic.clone()],
                        )?;
                        send(&owner, receiver2.clone(), "unsub", topic)?;
                    }
                }
                Ok(owner.undefined())
            })),
        )?;
        Ok(host.undefined())
    })))
}
