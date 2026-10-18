use safe_cex_venue_bybit::public_data_recovery::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Debug)]
struct Fault {
    origin: String,
    cause: Option<Rc<Fault>>,
    retry: Option<u32>,
}
type Error = Rc<Fault>;
type Task<T> = Pin<Box<dyn Future<Output = Result<T, Error>>>>;
fn undef() -> Value {
    json!({"undefined":true})
}
fn value(v: &Value) -> DataValue {
    match v {
        Value::Null => DataValue::Null,
        Value::Bool(v) => DataValue::Bool(*v),
        Value::Number(v) => DataValue::Number(v.as_f64().unwrap()),
        Value::String(v) => DataValue::String(v.encode_utf16().collect()),
        Value::Array(v) => DataValue::Array(Rc::new(RefCell::new(v.iter().map(value).collect()))),
        Value::Object(v) => {
            if v.get("fixtureUndefined") == Some(&json!(true)) {
                DataValue::Undefined
            } else {
                DataValue::Object(Rc::new(RefCell::new(
                    v.iter()
                        .map(|(k, v)| (k.encode_utf16().collect(), value(v)))
                        .collect(),
                )))
            }
        }
    }
}
fn leaves(v: &DataValue) -> Value {
    match v {
        DataValue::Undefined => undef(),
        DataValue::Null => Value::Null,
        DataValue::Bool(v) => json!(v),
        DataValue::Number(v) => json!(v),
        DataValue::String(v) => json!(String::from_utf16(v).unwrap()),
        DataValue::Array(v) => json!(v.borrow().iter().map(leaves).collect::<Vec<_>>()),
        DataValue::Object(v) => Value::Object(
            v.borrow()
                .iter()
                .map(|(k, v)| (String::from_utf16(k).unwrap(), leaves(v)))
                .collect(),
        ),
    }
}
fn assert_value(v: &DataValue, e: &Value, label: &str) {
    match v {
        DataValue::Array(v) => {
            let e = e.as_array().expect("native array expected");
            assert_eq!(v.borrow().len(), e.len(), "{label}: native array count");
            for (v, e) in v.borrow().iter().zip(e) {
                assert_value(v, e, label)
            }
        }
        DataValue::Object(v) => {
            let e = e.as_object().expect("native object expected");
            assert_eq!(v.borrow().len(), e.len(), "{label}: native object count");
            for (k, v) in v.borrow().iter() {
                assert_value(v, &e[&String::from_utf16(k).unwrap()], label)
            }
        }
        DataValue::Number(v) => assert_eq!(*v, e.as_f64().unwrap(), "{label}: native number"),
        _ => assert_eq!(leaves(v), *e, "{label}: native scalar"),
    }
}
fn assert_json(a: &Value, e: &Value, label: &str) {
    match (a, e) {
        (Value::Number(a), Value::Number(e)) => {
            assert_eq!(a.as_f64(), e.as_f64(), "{label}: native number")
        }
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "{label}: native array count");
            for (a, e) in a.iter().zip(e) {
                assert_json(a, e, label)
            }
        }
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(a.len(), e.len(), "{label}: native object count");
            for (k, a) in a {
                assert_json(a, &e[k], label)
            }
        }
        _ => assert_eq!(a, e, "{label}: native scalar"),
    }
}
fn same(a: &DataValue, b: &DataValue) -> bool {
    match (a, b) {
        (DataValue::Array(a), DataValue::Array(b)) => Rc::ptr_eq(a, b),
        (DataValue::Object(a), DataValue::Object(b)) => Rc::ptr_eq(a, b),
        (DataValue::Undefined, DataValue::Undefined) | (DataValue::Null, DataValue::Null) => true,
        (DataValue::Bool(a), DataValue::Bool(b)) => a == b,
        (DataValue::Number(a), DataValue::Number(b)) => a == b,
        (DataValue::String(a), DataValue::String(b)) => a == b,
        _ => false,
    }
}
async fn pending_once() {
    let mut first = true;
    std::future::poll_fn(move |cx| {
        if first {
            first = false;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await
}
fn drive<T>(f: impl Future<Output = T>) -> T {
    let mut f = Box::pin(f);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..16 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("RECOVERY native future exceeded 16 polls")
}
struct Host {
    input: Value,
    events: RefCell<Vec<Value>>,
    errors: RefCell<BTreeMap<String, Error>>,
    topics: [RefCell<BTreeMap<String, TopicSlot>>; 2],
    topic_index: Cell<usize>,
    timers: [RefCell<BTreeMap<String, DataValue>>; 2],
    timer_index: Cell<usize>,
    last: Cell<f64>,
    handlers: RefCell<BTreeMap<String, usize>>,
    markets: Markets,
    disposed: Cell<bool>,
    emitter: Cell<usize>,
    callback: RefCell<Option<Callback<'static, Error>>>,
    payloads: RefCell<Vec<Payload>>,
    frames: RefCell<Vec<String>>,
    get_index: Cell<usize>,
    old_tickers: TopicSlot,
    fee: DataValue,
    cache: DataValue,
    replacement: DataValue,
    stored: RefCell<DataValue>,
    reply: DataValue,
    symbol: RefCell<String>,
    helper_error: Error,
}
impl Host {
    fn new(i: &Value) -> Rc<Self> {
        let slot = match i.get("tickers") {
            Some(Value::String(s)) => TopicSlot::One(s.clone()),
            _ => TopicSlot::Many(Rc::new(RefCell::new(
                i.get("tickers")
                    .cloned()
                    .unwrap_or(json!(["old"]))
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().into())
                    .collect(),
            ))),
        };
        let cache = value(
            &i.get("cache")
                .cloned()
                .unwrap_or(json!([{"symbol":"CACHE"}])),
        );
        let mut handlers = BTreeMap::from([("k".into(), 1), ("s".into(), 1)]);
        if let Some(o) = i["handlers"].as_object() {
            for (k, v) in o {
                handlers.insert(k.clone(), v.as_u64().unwrap() as usize);
            }
        }
        let markets = i.get("markets").cloned().unwrap_or(json!([
            {"symbol":"BTC","quote":"USDT"}, {"symbol":"ETH","quote":"USDC"}, {"symbol":"INV","quote":"BTC"}
        ]));
        Rc::new(Self {
            input: i.clone(),
            events: RefCell::new(vec![]),
            errors: RefCell::new(BTreeMap::new()),
            topics: [
                RefCell::new(BTreeMap::from([("tickers".into(), slot.clone())])),
                RefCell::new(BTreeMap::new()),
            ],
            topic_index: Cell::new(0),
            timers: [
                RefCell::new(BTreeMap::from([(
                    "k".into(),
                    value(&i.get("timer").cloned().unwrap_or(json!(7))),
                )])),
                RefCell::new(BTreeMap::from([("k".into(), value(&json!("replacement")))])),
            ],
            timer_index: Cell::new(0),
            last: Cell::new(
                i.get("last")
                    .map(|v| {
                        v.as_f64()
                            .unwrap_or_else(|| v.as_str().unwrap().parse().unwrap())
                    })
                    .unwrap_or(0.),
            ),
            handlers: RefCell::new(handlers),
            markets: Rc::new(RefCell::new(
                markets
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| {
                        Rc::new(RefCell::new(Market {
                            symbol: v["symbol"].as_str().unwrap().into(),
                            quote: v["quote"].as_str().unwrap().into(),
                        }))
                    })
                    .collect(),
            )),
            disposed: Cell::new(i["disposed"] == true),
            emitter: Cell::new(0),
            callback: RefCell::new(None),
            payloads: RefCell::new(vec![]),
            frames: RefCell::new(vec![]),
            get_index: Cell::new(0),
            old_tickers: slot,
            fee: value(
                &i.get("data")
                    .cloned()
                    .unwrap_or(json!({"result":{"list":[{"symbol":"BTC","rate":"0.001"}]}})),
            ),
            cache: cache.clone(),
            replacement: value(&json!([{"symbol":"REPLACED"}])),
            stored: RefCell::new(cache),
            reply: value(&json!({"data":{"result":{"list":[{"symbol":"REPLY"}]}}})),
            symbol: RefCell::new("BTC".into()),
            helper_error: Rc::new(Fault {
                origin: "wrapped".into(),
                cause: Some(Rc::new(Fault {
                    origin: "cause".into(),
                    cause: None,
                    retry: None,
                })),
                retry: None,
            }),
        })
    }
    fn error(&self, name: &str) -> Error {
        self.errors
            .borrow_mut()
            .entry(name.into())
            .or_insert_with(|| {
                Rc::new(Fault {
                    origin: name.into(),
                    cause: None,
                    retry: None,
                })
            })
            .clone()
    }
    fn event(&self, name: &str, args: Vec<Value>) -> Result<(), Error> {
        let mut row = vec![json!(name)];
        row.extend(args);
        self.events.borrow_mut().push(json!(row));
        if self.input["fail"] == name {
            Err(self.error(name))
        } else {
            Ok(())
        }
    }
    fn category(&self) -> Result<String, Error> {
        self.event("category", vec![])?;
        if self.input["swapTopics"] == true {
            self.topic_index.set(1)
        }
        if self.input["mutateMarket"] == true {
            let row = self.markets.borrow()[0].clone();
            row.borrow_mut().symbol = "MUTATED".into();
            row.borrow_mut().quote = "USDT".into();
        }
        Ok(self.input["category"].as_str().unwrap_or("linear").into())
    }
    fn topic_put(&self, index: usize, key: &str, slot: TopicSlot) -> Result<(), Error> {
        let args = match &slot {
            TopicSlot::One(v) => json!(v),
            TopicSlot::Many(v) => json!(*v.borrow()),
        };
        self.event("topicPut", vec![json!(index), json!(key), args])?;
        self.topics[index].borrow_mut().insert(key.into(), slot);
        Ok(())
    }
    fn timer_delete(&self, key: &str) -> Result<(), Error> {
        let i = self.timer_index.get();
        self.event("delete", vec![json!(i), json!(key)])?;
        self.timers[i].borrow_mut().remove(key);
        Ok(())
    }
    fn handlers(&self, key: &str) -> Result<usize, Error> {
        Ok(*self.handlers.borrow().get(key).unwrap_or(&0))
    }
    fn recover(&self, t: &str, s: Option<&str>, reason: &str) -> Result<(), Error> {
        self.event("recover", vec![json!(t), json!(s), json!(reason)])
    }
    fn log(&self, m: &str, s: &str) -> Result<(), Error> {
        self.event("log", vec![json!(m), json!(s)])?;
        if self.input["replaceCache"] == true {
            *self.stored.borrow_mut() = self.replacement.clone()
        }
        Ok(())
    }
    fn sleep(self: &Rc<Self>, delay: u64) -> Result<Task<()>, Error> {
        self.event("sleep", vec![json!(delay)])?;
        let h = self.clone();
        Ok(Box::pin(async move {
            pending_once().await;
            if h.input["failAsync"] == "sleep" {
                Err(h.error("sleep"))
            } else {
                Ok(())
            }
        }))
    }
    fn get(
        self: &Rc<Self>,
        endpoint: &str,
        category: &str,
        limit: Option<u64>,
    ) -> Result<Task<DataValue>, Error> {
        let n = self.get_index.get();
        self.get_index.set(n + 1);
        let name = format!("get{n}");
        let params = if let Some(limit) = limit {
            json!({"category":category,"limit":limit})
        } else {
            json!({"category":category})
        };
        self.event(&name, vec![json!(endpoint), params])?;
        let h = self.clone();
        Ok(Box::pin(async move {
            pending_once().await;
            if h.input["failAsync"] == name {
                Err(h.error(&name))
            } else if h.input["op"] == "fetchFees" {
                Ok(h.fee.clone())
            } else {
                Ok(h.reply.clone())
            }
        }))
    }
    fn option_get(
        self: &Rc<Self>,
        endpoint: &str,
        params: Option<(DataValue, DataValue)>,
    ) -> Result<Task<DataValue>, Error> {
        let n = self.get_index.get();
        self.get_index.set(n + 1);
        let name = format!("get{n}");
        let params = params.map_or(Value::Null, |(coin, cursor)| {
            json!({"category":"option","baseCoin":leaves(&coin),"limit":1000,"cursor":leaves(&cursor)})
        });
        self.event(&name, vec![json!(endpoint), params])?;
        let h = self.clone();
        Ok(Box::pin(async move {
            pending_once().await;
            if h.input["failAsync"] == name {
                Err(h.error(&name))
            } else {
                Ok(value(&h.input["optionPages"][n]))
            }
        }))
    }
    fn map_option(&self, row: DataValue) -> Result<DataValue, Error> {
        self.events
            .borrow_mut()
            .push(json!(["mapOption", leaves(&row)]));
        let symbol = match &row {
            DataValue::Object(fields) => fields
                .borrow()
                .get(&"symbol".encode_utf16().collect::<Vec<_>>())
                .cloned(),
            _ => None,
        };
        let symbol = match symbol {
            Some(DataValue::String(symbol)) => symbol,
            _ => return Ok(DataValue::Null),
        };
        if self.input["failMap"].as_str() == Some(String::from_utf16(&symbol).unwrap().as_str()) {
            return Err(self.error("mapOption"));
        }
        let fields = match &row {
            DataValue::Object(fields) => fields.borrow().clone(),
            _ => unreachable!(),
        };
        if !matches!(fields.get(&"status".encode_utf16().collect::<Vec<_>>()), Some(DataValue::String(s)) if s.iter().copied().eq("Trading".encode_utf16()))
        {
            return Ok(DataValue::Null);
        }
        let strike = fields
            .get(&"strike".encode_utf16().collect::<Vec<_>>())
            .cloned()
            .unwrap_or(DataValue::Undefined);
        Ok(value(
            &json!({"symbol":String::from_utf16(&symbol).unwrap(),"option":{"strike":leaves(&strike)}}),
        ))
    }
    fn send(&self, frame: &str) -> Result<(), Error> {
        let decoded: Value = serde_json::from_str(frame).unwrap();
        self.event("send", vec![decoded["op"].clone(), decoded["args"].clone()])?;
        self.frames.borrow_mut().push(frame.into());
        if self.input["disposeOnSend"] == true {
            self.disposed.set(true)
        }
        Ok(())
    }
}
fn run(h: Rc<Host>) -> (Result<DataValue, Error>, Vec<Value>) {
    let op = h.input["op"].as_str().unwrap();
    let category = h.input["category"].as_str().unwrap_or("linear");
    let mut before = None;
    let result: Result<DataValue, Error> = (|| {
        match op {
            "onOpen" => {
                on_open(
                    || Ok(h.disposed.get()),
                    || Ok(h.markets.clone()),
                    || h.category(),
                    || {
                        let i = h.topic_index.get();
                        let h = h.clone();
                        Ok(move |t| h.topic_put(i, "tickers", TopicSlot::Many(t)))
                    },
                    || h.event("subscribe", vec![]),
                    || h.event("ping", vec![]),
                    || h.event("existing", vec![]),
                    || {
                        h.event("handler", vec![])?;
                        if h.input["swapEmitter"] == true {
                            h.emitter.set(h.emitter.get() + 1)
                        }
                        Ok(format!("handler{}", h.emitter.get()))
                    },
                    || {
                        let id = h.emitter.get();
                        let h = h.clone();
                        Ok(move |name: &str, handler: String| {
                            h.event("off", vec![json!(id), json!(name), json!(handler)])
                        })
                    },
                    || {
                        let id = h.emitter.get();
                        let h = h.clone();
                        Ok(move |name: &str, handler: String| {
                            h.event("on", vec![json!(id), json!(name), json!(handler)])
                        })
                    },
                    || h.input["reconnecting"] == true,
                    || h.event("reconnected", vec![]),
                )?;
            }
            "clear" => clear_ohlcv_no_data_timeout(
                "k",
                |key| {
                    Ok(h.timers[h.timer_index.get()]
                        .borrow()
                        .get(key)
                        .cloned()
                        .unwrap_or(DataValue::Undefined))
                },
                |id| h.event("clear", vec![leaves(&id)]),
                |key| h.timer_delete(key),
            )?,
            "arm" => {
                let h1 = h.clone();
                let h2 = h.clone();
                let h3 = h.clone();
                let h4 = h.clone();
                let h5 = h.clone();
                arm_ohlcv_no_data_timeout(
                    "k".into(),
                    if h.input["secondary"] == false {
                        None
                    } else {
                        Some("s".into())
                    },
                    |t| h.event("clearMethod", vec![json!(t)]),
                    Rc::new(move |t| h1.handlers(t)),
                    |cb, delay| {
                        h.event("set", vec![json!(delay)])?;
                        *h.callback.borrow_mut() = Some(cb);
                        if h.input["swapTimer"] == true {
                            h.timer_index.set(1)
                        }
                        Ok(DataValue::String("t0".encode_utf16().collect()))
                    },
                    || {
                        let i = h.timer_index.get();
                        let h = h.clone();
                        Ok(move |key: &str, id: DataValue| {
                            h.event("timerPut", vec![json!(i), json!(key), leaves(&id)])?;
                            h.timers[i].borrow_mut().insert(key.into(), id);
                            Ok(())
                        })
                    },
                    Rc::new(move |t| h2.timer_delete(t)),
                    Rc::new(move || Ok(h3.disposed.get())),
                    Rc::new(move |t, s, r| h4.recover(t, s, r)),
                    Rc::new(move |t, s| h5.event("rearm", vec![json!(t), json!(s)])),
                )?;
                if h.input["fire"] == true {
                    if h.input["emptyOnFire"] == true {
                        h.handlers.borrow_mut().insert("k".into(), 0);
                    }
                    if h.input["disposeOnFire"] == true {
                        h.disposed.set(true)
                    }
                    let cb = h.callback.borrow().clone();
                    if let Some(cb) = cb {
                        cb()?
                    }
                }
            }
            "existing" => {
                let keys = vec![
                    "k",
                    "s",
                    "kline.1.BTC",
                    "kline.5.NEXT",
                    "kline.1.A.B",
                    "kline.1.EMPTY",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect();
                arm_existing_ohlcv_no_data_timeouts(
                    || Ok(keys),
                    |t| h.handlers(t),
                    |t, s| {
                        h.event("arm", vec![json!(t), json!(s)])?;
                        if h.input["removeOnArm"] == true {
                            h.handlers.borrow_mut().insert("kline.5.NEXT".into(), 0);
                        }
                        Ok(())
                    },
                )?;
            }
            "wrapper" => maybe_recover_ohlcv_stream(
                || Ok(h.input["interval"].as_str().unwrap_or("1m").into()),
                || Ok(h.symbol.borrow().clone()),
                h.input["reason"].as_str(),
                || {
                    h.event("recoverGet", vec![])?;
                    if h.input["mutateSymbol"] == true {
                        *h.symbol.borrow_mut() = "CHANGED".into()
                    }
                    let h = h.clone();
                    Ok(move |t: &str, s: Option<&str>, r: &str| h.recover(t, s, r))
                },
            )?,
            "recovery" => maybe_recover_ohlcv_stream_from_topics(
                "k",
                if h.input["secondary"] == false {
                    None
                } else {
                    Some("s")
                },
                h.input["reason"].as_str().unwrap_or("reason"),
                |t| h.handlers(t),
                || {
                    h.event("clock", vec![])?;
                    Ok(h.input["now"].as_f64().unwrap_or(10000.))
                },
                |_| Ok(Some(h.last.get())),
                |_, t| {
                    h.last.set(t);
                    Ok(())
                },
                |k, v| h.topic_put(h.topic_index.get(), k, TopicSlot::One(v.into())),
                || {
                    h.event("connected", vec![])?;
                    Ok(h.input["connected"] != false)
                },
                |m, s| h.log(m, s),
                || h.event("reconnect", vec![]),
                |p| {
                    let p0 = p.borrow();
                    h.event("diagnostic", vec![json!(p0.op), json!(*p0.args.borrow())])?;
                    drop(p0);
                    h.payloads.borrow_mut().push(p.clone());
                    if h.input["mutateArgs"] == true && h.payloads.borrow().len() == 1 {
                        p.borrow().args.borrow_mut().push("MUTATED".into())
                    }
                    Ok(())
                },
            )?,
            "markets" => {
                let f = on_markets_changed(
                    h.markets.clone(),
                    || Ok(h.input["ws"] != false),
                    || Ok(h.disposed.get()),
                    || h.category(),
                    || {
                        let slot = h.topics[h.topic_index.get()]
                            .borrow()
                            .get("tickers")
                            .cloned();
                        if h.input["swapTickerRead"] == true {
                            h.event("tickers", vec![])?;
                            h.topic_index.set(1)
                        }
                        Ok(slot)
                    },
                    || {
                        let i = h.topic_index.get();
                        let h = h.clone();
                        Ok(move |t| h.topic_put(i, "tickers", TopicSlot::Many(t)))
                    },
                    |f| h.send(f),
                    |n| h.sleep(n),
                );
                before = Some(h.events.borrow().clone());
                drive(f)?;
            }
            "fetchFees" => {
                let f = fetch_fees(category, |e, c| h.get(e, c, None));
                before = Some(h.events.borrow().clone());
                return drive(f);
            }
            "fetchMarkets" => {
                let f = fetch_markets(
                    |c| {
                        h.event("helper", vec![json!(c)])?;
                        if h.input["syncBrandThrow"] == true {
                            return Err(h.helper_error.clone());
                        }
                        let h = h.clone();
                        let c = c.to_owned();
                        Ok(Box::pin(async move {
                            pending_once().await;
                            if h.input["brandError"] == true {
                                Err(h.helper_error.clone())
                            } else if h.input["failAsync"] == "helper" {
                                Err(h.error("helper"))
                            } else if h.input["arrays"] == true {
                                Ok(value(&json!([{"symbol":c}])))
                            } else {
                                Ok(h.reply.clone())
                            }
                        }))
                    },
                    || {
                        h.event("inverseEnabled", vec![])?;
                        Ok(h.input["inverse"] == true)
                    },
                    || {
                        h.event("optionsEnabled", vec![])?;
                        Ok(h.input["options"] == true)
                    },
                    || {
                        h.event("optionMarkets", vec![])?;
                        let h = h.clone();
                        Ok(Box::pin(async move {
                            if h.input["failAsync"] == "optionMarkets" {
                                Err(h.error("optionMarkets"))
                            } else {
                                Ok(value(&json!([{"symbol":"OPT"}])))
                            }
                        }) as Task<DataValue>)
                    },
                    |e| e.origin == "wrapped",
                    || {
                        h.event("stored", vec![])?;
                        Ok(h.stored.borrow().clone())
                    },
                    |m, s| h.log(m, s),
                    |m| {
                        Rc::new(Fault {
                            origin: format!("TypeError: {m}"),
                            cause: None,
                            retry: None,
                        })
                    },
                );
                before = Some(h.events.borrow().clone());
                return drive(f);
            }
            "fetchOptionMarkets" => {
                let f = fetch_option_markets(
                    |e, p| h.option_get(e, p),
                    |row| h.map_option(row),
                    |e| e.origin.clone(),
                    |m, s| h.log(m, s),
                    || {
                        h.event("stored", vec![])?;
                        Ok(h.stored.borrow().clone())
                    },
                    |m| {
                        Rc::new(Fault {
                            origin: format!("TypeError: {m}"),
                            cause: None,
                            retry: None,
                        })
                    },
                );
                before = Some(h.events.borrow().clone());
                return drive(f);
            }
            "fetchRetry" => {
                let f = fetch_trading_markets_response(
                    category,
                    |e, c, l| h.get(e, c, Some(l)),
                    |e| {
                        h.event("retryable", vec![json!(e.origin)])?;
                        Ok(h.input["retryable"] == true)
                    },
                    |e| {
                        h.event("describe", vec![json!(e.origin)])?;
                        Ok("why".into())
                    },
                    |m, s| h.log(m, s),
                    |n| h.sleep(n),
                    |e, retry| {
                        if let Err(f) = h.event("wrap", vec![json!(e.origin), json!(retry)]) {
                            return f;
                        }
                        Rc::new(Fault {
                            origin: "wrapped".into(),
                            cause: Some(e),
                            retry,
                        })
                    },
                );
                before = Some(h.events.borrow().clone());
                return drive(f);
            }
            _ => panic!("unknown row family {op}"),
        }
        Ok(DataValue::Undefined)
    })();
    let before = before.unwrap_or_else(|| h.events.borrow().clone());
    (result, before)
}
#[test]
fn public_data_recovery_native_source_oracle() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-data-recovery-fixtures.cjs"
        ))
        .output()
        .expect("source oracle starts");
    assert!(
        output.status.success(),
        "RECOVERY oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 98, "RECOVERY independent scenario floor");
    let mut counts = BTreeMap::new();
    let mut wire_pairs = vec![];
    for (index, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let op = input["op"].as_str().unwrap();
        *counts.entry(op).or_insert(0) += 1;
        let label = format!("RECOVERY {op} iteration {index}");
        let h = Host::new(input);
        let (result, before) = run(h.clone());
        assert_eq!(
            result.is_err(),
            !row["error"].is_null(),
            "{label}: native rejection discriminant"
        );
        match &result {
            Ok(v) => {
                assert_value(v, &row["result"], &label);
                assert_eq!(
                    same(v, &h.reply),
                    row["sameReply"].as_bool().unwrap(),
                    "{label}: native reply identity"
                );
                assert_eq!(
                    same(v, &h.cache),
                    row["sameCache"].as_bool().unwrap(),
                    "{label}: native cache identity"
                );
                assert_eq!(
                    same(v, &h.replacement),
                    row["sameReplacement"].as_bool().unwrap(),
                    "{label}: native replacement identity"
                );
                let fee = if let DataValue::Object(o) = &h.fee {
                    if let Some(DataValue::Object(r)) =
                        o.borrow().get(&"result".encode_utf16().collect::<Vec<_>>())
                    {
                        r.borrow()
                            .get(&"list".encode_utf16().collect::<Vec<_>>())
                            .cloned()
                            .unwrap_or(DataValue::Undefined)
                    } else {
                        DataValue::Undefined
                    }
                } else {
                    DataValue::Undefined
                };
                assert_eq!(
                    same(v, &fee),
                    row["sameFee"].as_bool().unwrap(),
                    "{label}: native fee identity"
                );
            }
            Err(e) => {
                assert_eq!(
                    e.origin,
                    row["error"]["origin"].as_str().unwrap(),
                    "{label}: native error origin"
                );
                assert_eq!(
                    e.retry.map(u64::from),
                    row["error"]["retry"].as_u64(),
                    "{label}: native retry count"
                );
                assert_eq!(
                    e.cause.as_ref().map(|v| v.origin.as_str()),
                    row["error"]["cause"].as_str(),
                    "{label}: native cause origin"
                );
                if op == "fetchMarkets" && e.origin == "wrapped" {
                    assert!(
                        Rc::ptr_eq(e, &h.helper_error),
                        "{label}: native branded error identity"
                    );
                }
                if e.origin != "wrapped" {
                    assert!(
                        Rc::ptr_eq(e, &h.error(&e.origin)),
                        "{label}: native original error identity"
                    )
                } else if let Some(cause) = &e.cause {
                    if cause.origin != "cause" {
                        assert!(
                            Rc::ptr_eq(cause, &h.error(&cause.origin)),
                            "{label}: native wrapped cause identity"
                        )
                    }
                }
            }
        }
        assert_json(
            &json!(*h.events.borrow()),
            &row["events"],
            &format!("{label}: native event order and arguments"),
        );
        assert_json(
            &json!(before),
            &row["before"],
            &format!("{label}: native eager events"),
        );
        for i in 0..2 {
            let expected = row["topics"][i].as_object().unwrap();
            let slots = h.topics[i].borrow();
            assert_eq!(slots.len(), expected.len(), "{label}: native topic count");
            for (k, s) in slots.iter() {
                match s {
                    TopicSlot::One(v) => assert_eq!(
                        expected[k].as_str(),
                        Some(v.as_str()),
                        "{label}: native topic value"
                    ),
                    TopicSlot::Many(v) => assert_eq!(
                        *v.borrow(),
                        expected[k]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|s| s.as_str().unwrap().to_owned())
                            .collect::<Vec<_>>(),
                        "{label}: native ticker allocation contents"
                    ),
                }
            }
            let expected = row["timers"][i].as_object().unwrap();
            let timers = h.timers[i].borrow();
            assert_eq!(timers.len(), expected.len(), "{label}: native timer count");
            for (k, v) in timers.iter() {
                assert_value(v, &expected[k], &label)
            }
        }
        if row["last"]["k"].is_null() {
            assert!(h.last.get().is_nan(), "{label}: native NaN timestamp")
        } else {
            assert_eq!(
                h.last.get(),
                row["last"]["k"].as_f64().unwrap(),
                "{label}: native recovery timestamp"
            )
        }
        let fresh = match (h.topics[0].borrow().get("tickers"), &h.old_tickers) {
            (Some(TopicSlot::Many(a)), TopicSlot::Many(b)) => !Rc::ptr_eq(a, b),
            (Some(TopicSlot::One(a)), TopicSlot::One(b)) => a != b,
            _ => true,
        };
        assert_eq!(
            fresh,
            row["tickerFresh"].as_bool().unwrap(),
            "{label}: native ticker fresh allocation"
        );
        let payloads = h.payloads.borrow();
        if payloads.len() == 2 {
            assert_eq!(
                payloads[0].borrow().op,
                "unsubscribe",
                "{label}: native unsubscribe discriminant"
            );
            assert_eq!(
                payloads[1].borrow().op,
                "subscribe",
                "{label}: native subscribe discriminant"
            );
            assert_eq!(
                Rc::ptr_eq(&payloads[0].borrow().args, &payloads[1].borrow().args),
                row["payloadShared"].as_bool().unwrap(),
                "{label}: native shared diagnostic args"
            );
            assert_eq!(
                !Rc::ptr_eq(&payloads[0], &payloads[1]),
                row["payloadDistinct"].as_bool().unwrap(),
                "{label}: native distinct diagnostic objects"
            );
        }
        // Decode frames and assert native fields before comparing any wire string.
        let frames = h.frames.borrow();
        let e = row["frames"].as_array().unwrap();
        assert_eq!(frames.len(), e.len(), "{label}: native send frame count");
        for (frame, e) in frames.iter().zip(e) {
            let a: Value = serde_json::from_str(frame).unwrap();
            let e: Value = serde_json::from_str(e.as_str().unwrap()).unwrap();
            assert_eq!(
                a["op"].as_str(),
                e["op"].as_str(),
                "{label}: decoded native frame operation"
            );
            assert_eq!(
                a["args"].as_array(),
                e["args"].as_array(),
                "{label}: decoded native frame topics"
            );
        }
        for (frame, e) in frames.iter().zip(e) {
            wire_pairs.push((frame.clone(), e.as_str().unwrap().to_owned(), label.clone()));
        }
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("onOpen", 8),
            ("clear", 8),
            ("arm", 9),
            ("existing", 3),
            ("wrapper", 5),
            ("recovery", 12),
            ("markets", 11),
            ("fetchFees", 7),
            ("fetchMarkets", 14),
            ("fetchOptionMarkets", 11),
            ("fetchRetry", 10)
        ]),
        "RECOVERY independent family floors"
    );
    for (frame, expected, label) in wire_pairs {
        assert_eq!(
            frame, expected,
            "{label}: wire frame after ALL native assertions"
        );
    }
    eprintln!("RECOVERY completed 98 iterations");
}
