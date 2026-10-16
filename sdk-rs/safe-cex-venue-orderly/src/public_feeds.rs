//! Candle/book subscription closures retaining source receiver and shared state.
use crate::{
    classes::{BodyFuture, BoundMember, Return},
    data_mappers::{DataHost, Op},
    ws_controls::{Result, Value},
};
use std::{cell::RefCell, rc::Rc};
pub trait FeedHost: DataHost + Clone + 'static {
    fn function(&self, name: &str, body: BoundMember<Self>) -> Value<Self>;
    fn timeout(&self, callback: Value<Self>, delay: f64) -> Result<Self, Value<Self>>;
    fn clear_timeout(&self, id: Value<Self>) -> Result<Self, ()>;
    // Poll the source async prefix eagerly and observe detached failures out of band.
    fn spawn(&self, future: BodyFuture<Self>);
    fn strict_eq(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn delete(&self, object: Value<Self>, key: &[u16]) -> Result<Self, ()>;
    fn for_each(
        &self,
        rows: Value<Self>,
        body: impl FnMut(Value<Self>) -> Result<Self, ()>,
    ) -> Result<Self, ()>;
    fn pair(&self, value: Value<Self>) -> Result<Self, (Value<Self>, Value<Self>)>;
    fn update_level(&self, book: Value<Self>, side: &str, update: Value<Self>) -> Result<Self, ()>;
    fn calc_total(&self, book: Value<Self>) -> Result<Self, ()>;
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text<H: FeedHost>(h: &H, s: &str) -> Value<H> {
    h.text(&u(s))
}
fn get<H: FeedHost>(h: &H, v: &Value<H>, key: &str) -> Result<H, Value<H>> {
    h.get(v, &u(key))
}
fn call<H: FeedHost>(h: &H, r: Value<H>, name: &str, args: Vec<Value<H>>) -> Result<H, Value<H>> {
    let f = get(h, &r, name)?;
    h.call(f, Some(r), args)
}
fn opt<H: FeedHost>(h: &H, v: &Value<H>, key: &str) -> Result<H, Value<H>> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        get(h, v, key)
    }
}
fn frame<H: FeedHost>(h: &H, r: &Value<H>, event: &str, topic: Value<H>) -> Result<H, ()> {
    let f = get(h, r, "send")?;
    let payload = h.object(vec![("event", text(h, event)), ("topic", topic)])?;
    let encoded = h.stringify(payload)?;
    h.call(f, Some(r.clone()), vec![encoded])?;
    Ok(())
}
fn handlers<H: FeedHost>(h: &H, r: &Value<H>, topic: &Value<H>) -> Result<H, Value<H>> {
    let map = get(h, r, "messageHandlers")?;
    let old = h.get(&map, &h.string(topic)?)?;
    Ok(if h.truthy(&old) { old } else { h.array(vec![]) })
}
fn empty<H: FeedHost>(h: &H, r: &Value<H>, topic: &Value<H>) -> Result<H, bool> {
    let map = get(h, r, "messageHandlers")?;
    let rows = h.get(&map, &h.string(topic)?)?;
    Ok(!h.truthy(&*get(h, &rows, "length")?))
}
struct Waiting<H: FeedHost> {
    h: H,
    r: Value<H>,
    topic: Value<H>,
    parser: Value<H>,
    timer: Option<Rc<RefCell<Value<H>>>>,
}
impl<H: FeedHost> Clone for Waiting<H> {
    fn clone(&self) -> Self {
        Self {
            h: self.h.clone(),
            r: self.r.clone(),
            topic: self.topic.clone(),
            parser: self.parser.clone(),
            timer: self.timer.clone(),
        }
    }
}
impl<H: FeedHost> Waiting<H> {
    fn invoke(&self) -> Result<H, ()> {
        let h = &self.h;
        if h.truthy(&*get(h, &self.r, "isConnected")?) {
            if !h.truthy(&*get(h, &self.r, "isDisposed")?) {
                let map = get(h, &self.r, "messageHandlers")?;
                let old = handlers(h, &self.r, &self.topic)?;
                let mut values = h.spread(old)?;
                values.push(self.parser.clone());
                h.set(&map, &h.string(&self.topic)?, h.array(values))?;
                let topics = get(h, &self.r, "topics")?;
                h.set(&topics, &h.string(&self.topic)?, self.topic.clone())?;
                frame(h, &self.r, "subscribe", self.topic.clone())?;
            }
        } else {
            let waiting = self.clone();
            let cb = h.function(
                "",
                Rc::new(move |_| {
                    waiting.invoke()?;
                    Ok(Return::Value(waiting.h.undefined()))
                }),
            );
            let id = h.timeout(cb, 100.0)?;
            if let Some(slot) = &self.timer {
                *slot.borrow_mut() = id;
            }
        }
        Ok(())
    }
}
fn unregister<H: FeedHost>(
    h: &H,
    r: &Value<H>,
    topic: &Value<H>,
    parser: &Value<H>,
) -> Result<H, ()> {
    let map = get(h, r, "messageHandlers")?;
    let old = handlers(h, r, topic)?;
    let rows = h.filter(old, |f| Ok(!h.strict_eq(&f, parser)))?;
    let rows = h.array(h.spread(rows)?);
    h.set(&map, &h.string(topic)?, rows)?;
    if empty(h, r, topic)? {
        h.delete(get(h, r, "topics")?, &h.string(topic)?)?;
        call(
            h,
            r.clone(),
            "cleanupMessageHandlerStats",
            vec![topic.clone()],
        )?;
    }
    Ok(())
}
fn unsubscribe<H: FeedHost>(h: &H, r: &Value<H>, topic: &Value<H>) -> Result<H, ()> {
    if h.truthy(&*get(h, r, "isConnected")?) && empty(h, r, topic)? {
        frame(h, r, "unsubscribe", topic.clone())?;
    }
    Ok(())
}
pub fn listen_candles<H: FeedHost>(
    h: &H,
    r: Value<H>,
    opts: Value<H>,
    callback: Value<H>,
) -> Result<H, Value<H>> {
    let topic = h.text(
        &[
            h.string(&*h.reverse_symbol(get(h, &opts, "symbol")?)?)?,
            u("@kline_"),
            h.string(&*get(h, &opts, "interval")?)?,
        ]
        .concat(),
    );
    let clone = h.clone();
    let parser = h.function(
        "parser",
        Rc::new(move |args| {
            let json = args.first().cloned().unwrap_or_else(|| clone.undefined());
            let h = &clone;
            let candle = h.object(vec![
                (
                    "timestamp",
                    h.arithmetic(
                        Op::Div,
                        get(h, &get(h, &json, "data")?, "startTime")?,
                        h.number(1000.0),
                    )?,
                ),
                ("open", get(h, &get(h, &json, "data")?, "open")?),
                ("high", get(h, &get(h, &json, "data")?, "high")?),
                ("low", get(h, &get(h, &json, "data")?, "low")?),
                ("close", get(h, &get(h, &json, "data")?, "close")?),
                ("volume", get(h, &get(h, &json, "data")?, "volume")?),
            ])?;
            h.call(callback.clone(), None, vec![candle])?;
            Ok(Return::Value(h.undefined()))
        }),
    );
    Waiting {
        h: h.clone(),
        r: r.clone(),
        topic: topic.clone(),
        parser: parser.clone(),
        timer: None,
    }
    .invoke()?;
    let clone = h.clone();
    Ok(h.function(
        "",
        Rc::new(move |_| {
            unregister(&clone, &r, &topic, &parser)?;
            unsubscribe(&clone, &r, &topic)?;
            Ok(Return::Value(clone.undefined()))
        }),
    ))
}
struct BookState<H: FeedHost> {
    h: H,
    r: Value<H>,
    symbol: Value<H>,
    callback: Value<H>,
    book: Value<H>,
    inner: Value<H>,
}
impl<H: FeedHost> Clone for BookState<H> {
    fn clone(&self) -> Self {
        Self {
            h: self.h.clone(),
            r: self.r.clone(),
            symbol: self.symbol.clone(),
            callback: self.callback.clone(),
            book: self.book.clone(),
            inner: self.inner.clone(),
        }
    }
}
impl<H: FeedHost> BookState<H> {
    fn clear_updates(&self) -> Result<H, ()> {
        self.h.set(&self.inner, &u("updates"), self.h.array(vec![]))
    }
    async fn snapshot(self) -> Result<H, Value<H>> {
        let h = &self.h;
        let result = async {
            let parent = get(h, &self.r, "parent")?;
            let xhr = get(h, &parent, "xhr")?;
            let f = get(h, &xhr, "get")?;
            let path = crate::types::ENDPOINTS
                .iter()
                .find(|(k, _)| *k == "ORDERBOOK")
                .expect("endpoint")
                .1;
            let url = h.text(
                &[
                    u(path),
                    u("/"),
                    h.string(&*h.reverse_symbol(self.symbol.clone())?)?,
                ]
                .concat(),
            );
            let p = h.call(f, Some(xhr), vec![url])?;
            let response = h.await_value(p).await?;
            let data = get(h, &get(h, &response, "data")?, "data")?;
            if !h.truthy(&*get(h, &self.r, "isDisposed")?) {
                for side in ["bids", "asks"] {
                    let rows = get(h, &data, side)?;
                    let mapped = h.map(rows, |row| {
                        h.object(vec![
                            ("price", get(h, &row, "price")?),
                            ("amount", get(h, &row, "quantity")?),
                            ("total", h.number(0.0)),
                        ])
                    })?;
                    h.set(&self.book, &u(side), mapped)?;
                }
                let rows = get(h, &self.inner, "updates")?;
                let filtered = h.filter(rows, |update| {
                    h.compare(get(h, &update, "ts")?, get(h, &data, "timestamp")?, false)
                })?;
                h.set(&self.inner, &u("updates"), filtered)?;
                let rows = get(h, &self.inner, "updates")?;
                h.for_each(rows, |update| {
                    call(
                        h,
                        self.r.clone(),
                        "processOrderBookUpdate",
                        vec![self.book.clone(), update],
                    )?;
                    Ok(())
                })?;
                h.calc_total(self.book.clone())?;
                h.set(&self.inner, &u("isSnapshotLoaded"), h.boolean(true))?;
                self.clear_updates()?;
                h.call(self.callback.clone(), None, vec![self.book.clone()])?;
            }
            Ok(h.undefined())
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                self.clear_updates()?;
                let f = get(h, &self.r, "emitAndLogError")?;
                let err = h.error_value(&e);
                let data = opt(h, &opt(h, &err, "response")?, "data")?;
                let msg = opt(h, &data, "message")?;
                let msg = if h.truthy(&msg) {
                    msg
                } else {
                    opt(h, &err, "message")?
                };
                let msg = if h.truthy(&msg) {
                    msg
                } else {
                    text(h, "Orderly orderbook snapshot failed")
                };
                h.call(f, Some(self.r.clone()), vec![msg])?;
                Ok(h.undefined())
            }
        }
    }
    fn parse(&self, json: Value<H>) -> Result<H, ()> {
        let h = &self.h;
        if !h.truthy(&*get(h, &self.inner, "isSnapshotLoaded")?)
            && h.strict_eq(
                &*get(h, &get(h, &self.inner, "updates")?, "length")?,
                &*h.number(0.0),
            )
        {
            let state = self.clone();
            h.spawn(Box::pin(async move { state.snapshot().await }));
            h.set(&self.inner, &u("updates"), h.array(vec![json]))?;
            return Ok(());
        }
        if !h.truthy(&*get(h, &self.inner, "isSnapshotLoaded")?) {
            h.push(get(h, &self.inner, "updates")?, vec![json])?;
            return Ok(());
        }
        call(
            h,
            self.r.clone(),
            "processOrderBookUpdate",
            vec![self.book.clone(), json],
        )?;
        h.calc_total(self.book.clone())?;
        h.call(self.callback.clone(), None, vec![self.book.clone()])?;
        Ok(())
    }
}
pub fn listen_book<H: FeedHost>(
    h: &H,
    r: Value<H>,
    symbol: Value<H>,
    callback: Value<H>,
) -> Result<H, Value<H>> {
    let timer = Rc::new(RefCell::new(h.null()));
    let topic = h.text(
        &[
            h.string(&*h.reverse_symbol(symbol.clone())?)?,
            u("@orderbookupdate"),
        ]
        .concat(),
    );
    let book = h.object(vec![("bids", h.array(vec![])), ("asks", h.array(vec![]))])?;
    let inner = h.object(vec![
        ("updates", h.array(vec![])),
        ("isSnapshotLoaded", h.boolean(false)),
    ])?;
    let state = BookState {
        h: h.clone(),
        r: r.clone(),
        symbol,
        callback,
        book: book.clone(),
        inner: inner.clone(),
    };
    let parser = h.function(
        "parser",
        Rc::new(move |args| {
            state.parse(args.first().cloned().unwrap_or_else(|| state.h.undefined()))?;
            Ok(Return::Value(state.h.undefined()))
        }),
    );
    Waiting {
        h: h.clone(),
        r: r.clone(),
        topic: topic.clone(),
        parser: parser.clone(),
        timer: Some(timer.clone()),
    }
    .invoke()?;
    let clone = h.clone();
    Ok(h.function(
        "",
        Rc::new(move |_| {
            let h = &clone;
            unregister(h, &r, &topic, &parser)?;
            h.set(&book, &u("asks"), h.array(vec![]))?;
            h.set(&book, &u("bids"), h.array(vec![]))?;
            h.set(&inner, &u("updates"), h.array(vec![]))?;
            h.set(&inner, &u("isSnapshotLoaded"), h.boolean(false))?;
            let id = timer.borrow().clone();
            if h.truthy(&id) {
                h.clear_timeout(id)?;
                *timer.borrow_mut() = h.null();
            }
            unsubscribe(h, &r, &topic)?;
            Ok(Return::Value(h.undefined()))
        }),
    ))
}
pub fn process_book<H: FeedHost>(h: &H, book: Value<H>, update: Value<H>) -> Result<H, Value<H>> {
    let sides = h.object(vec![
        ("bids", get(h, &get(h, &update, "data")?, "bids")?),
        ("asks", get(h, &get(h, &update, "data")?, "asks")?),
    ])?;
    let entries = h.entries(sides)?;
    for (side, rows) in entries {
        let side = String::from_utf16(&side).expect("source fixed side keys");
        if side != "bids" && side != "asks" {
            continue;
        }
        h.for_each(rows, |row| {
            let (price, amount) = h.pair(row)?;
            let level = h.object(vec![
                ("price", price),
                ("amount", amount),
                ("total", h.number(0.0)),
            ])?;
            h.update_level(book.clone(), &side, level)
        })?;
    }
    Ok(h.undefined())
}
