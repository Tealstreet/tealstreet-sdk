use safe_cex_venue_bybit::ticker_readers::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Error = Rc<Value>;
fn undefined() -> Value {
    json!({"undefined":true})
}
fn from_json(v: &Value) -> TickerValue {
    match v {
        Value::Null => TickerValue::Null,
        Value::Bool(v) => TickerValue::Bool(*v),
        Value::Number(v) => TickerValue::Number(v.as_f64().unwrap()),
        Value::String(v) => TickerValue::String(v.encode_utf16().collect()),
        Value::Array(rows) => {
            TickerValue::Array(Rc::new(RefCell::new(rows.iter().map(from_json).collect())))
        }
        Value::Object(fields) => {
            if fields.get("fixtureUndefined") == Some(&Value::Bool(true)) {
                TickerValue::Undefined
            } else {
                TickerValue::Object(Rc::new(RefCell::new(
                    fields
                        .iter()
                        .map(|(k, v)| (k.encode_utf16().collect(), from_json(v)))
                        .collect(),
                )))
            }
        }
    }
}
fn property(v: &TickerValue, key: &str) -> TickerValue {
    match v {
        TickerValue::Object(f) => f
            .borrow()
            .get(&key.encode_utf16().collect::<Vec<_>>())
            .cloned()
            .unwrap_or(TickerValue::Undefined),
        _ => TickerValue::Undefined,
    }
}
fn same(a: &TickerValue, b: &TickerValue) -> bool {
    match (a, b) {
        (TickerValue::Undefined, TickerValue::Undefined)
        | (TickerValue::Null, TickerValue::Null) => true,
        (TickerValue::Bool(a), TickerValue::Bool(b)) => a == b,
        (TickerValue::Number(a), TickerValue::Number(b)) => a == b,
        (TickerValue::String(a), TickerValue::String(b)) => a == b,
        (TickerValue::Array(a), TickerValue::Array(b)) => Rc::ptr_eq(a, b),
        (TickerValue::Object(a), TickerValue::Object(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
// This is an assertion-only projection to leaves; no serialization/re-encoding.
fn leaves(v: &TickerValue) -> Value {
    match v {
        TickerValue::Undefined => undefined(),
        TickerValue::Null => Value::Null,
        TickerValue::Bool(v) => Value::Bool(*v),
        TickerValue::Number(v) => json!(v),
        TickerValue::String(v) => Value::String(String::from_utf16(v).unwrap()),
        TickerValue::Array(v) => Value::Array(v.borrow().iter().map(leaves).collect()),
        TickerValue::Object(v) => Value::Object(
            v.borrow()
                .iter()
                .map(|(k, v)| (String::from_utf16(k).unwrap(), leaves(v)))
                .collect(),
        ),
    }
}
fn native_value(v: &TickerValue, e: &Value, label: &str) {
    match v {
        TickerValue::Undefined => assert_eq!(e, &undefined(), "{label}: native undefined"),
        TickerValue::Number(v) => assert_eq!(
            e["number"].as_str(),
            Some(format!("{:016x}", v.to_bits()).as_str()),
            "{label}: native f64"
        ),
        TickerValue::Array(v) => {
            let rows = e
                .as_array()
                .unwrap_or_else(|| panic!("{label}: native array expected"));
            assert_eq!(v.borrow().len(), rows.len(), "{label}: native array count");
            for (v, e) in v.borrow().iter().zip(rows) {
                native_value(v, e, label)
            }
        }
        TickerValue::Object(v) => {
            let fields = e.as_object().unwrap();
            assert_eq!(
                v.borrow().len(),
                fields.len(),
                "{label}: native object key count"
            );
            for (key, v) in v.borrow().iter() {
                native_value(v, &fields[&String::from_utf16(key).unwrap()], label)
            }
        }
        _ => assert_eq!(&leaves(v), e, "{label}: native decoded scalar"),
    }
}
fn native_json(a: &Value, e: &Value, label: &str) {
    if let Some(bits) = e.get("number").and_then(Value::as_str) {
        assert_eq!(
            format!("{:016x}", a.as_f64().unwrap().to_bits()),
            bits,
            "{label}: native number"
        );
        return;
    }
    match e {
        Value::Array(v) => {
            let a = a.as_array().unwrap();
            assert_eq!(a.len(), v.len(), "{label}: native array count");
            for (a, e) in a.iter().zip(v) {
                native_json(a, e, label)
            }
        }
        Value::Object(v) => {
            let a = a.as_object().unwrap();
            assert_eq!(a.len(), v.len(), "{label}: native object count");
            for (k, e) in v {
                native_json(&a[k], e, label)
            }
        }
        _ => assert_eq!(a, e, "{label}: native decoded leaf"),
    }
}
fn drive<F: Future>(f: F) -> F::Output {
    let mut f = Box::pin(f);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..16 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("TICKER native future stopped at poll 16")
}
async fn pending_once() {
    let mut pending = true;
    std::future::poll_fn(|cx| {
        if pending {
            pending = false;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
}
struct Host {
    input: Value,
    events: RefCell<Vec<Value>>,
    errors: BTreeMap<String, Error>,
    disposed: Cell<bool>,
    helper_tag: RefCell<String>,
    helper_calls: Cell<usize>,
    raw: TickerValue,
    stored: TickerValue,
    parsed: TickerValue,
    data: TickerValue,
    categories: Rc<Vec<String>>,
    sets: Vec<TickerValue>,
    tasks: RefCell<Vec<Rc<RefCell<Task>>>>,
}
impl Host {
    fn new(row: &Value) -> Self {
        let i = &row["input"];
        Self{input:i.clone(),events:RefCell::new(vec![]),errors:["rawGet","rawSync","rawAsync","disposed","stored","parse","helperGet","helperSync","helperAsync","categories","all","getSync","getAsync","emit","inverseEnabled"].into_iter().map(|p|(p.into(),Rc::new(json!({"name":format!("{p}Error"),"message":format!("{p} failed"),"code":41})))).collect(),disposed:Cell::new(i["disposed"]==true),helper_tag:RefCell::new("H0".into()),helper_calls:Cell::new(0),raw:from_json(&i.get("raw").cloned().unwrap_or_else(||json!([{"symbol":"BTC","tag":"RAW-17"}]))),stored:from_json(&json!([{"symbol":"BTC","tag":"STORED-29"}])),parsed:from_json(&json!([{"symbol":"BTC","tag":"PARSED-43"}])),data:from_json(&i.get("data").cloned().unwrap_or_else(||json!({"retMsg":"OK","result":{"list":[{"symbol":"BTC","tag":"LIST-53"}]}}))),categories:Rc::new(i.get("categories").cloned().unwrap_or_else(||json!(["inverse"])).as_array().unwrap().iter().map(|v|v.as_str().unwrap().into()).collect()),sets:i.get("sets").cloned().unwrap_or_else(||json!([[{"symbol":"BTC","tag":"SET0"}],[{"symbol":"ETH","tag":"SET1"}]])).as_array().unwrap().iter().map(from_json).collect(),tasks:RefCell::new(vec![])}
    }
    fn event(&self, phase: &str, e: Value) -> Result<(), Error> {
        self.events.borrow_mut().push(e);
        if self.input["phase"] == phase {
            Err(Rc::clone(&self.errors[phase]))
        } else {
            Ok(())
        }
    }
    fn raw(&self) -> Result<impl Future<Output = Result<TickerValue, Error>> + '_, Error> {
        self.event("rawGet", json!(["rawGet"]))?;
        self.event("rawSync", json!(["raw"]))?;
        Ok(async move {
            pending_once().await;
            self.event("rawAsync", json!(["rawResolved"]))?;
            if self.input["disposeOnResolve"] == true {
                self.disposed.set(true)
            }
            Ok(self.raw.clone())
        })
    }
    fn disposed(&self) -> Result<bool, Error> {
        self.event("disposed", json!(["disposed"]))?;
        if self.input["mutateOnDisposed"] == true {
            if let TickerValue::Array(rows) = &self.raw {
                rows.borrow_mut()
                    .push(from_json(&json!({"tag":"DISPOSED-ALIAS"})));
            }
        }
        Ok(self.disposed.get())
    }
    fn stored(&self) -> Result<TickerValue, Error> {
        self.event("stored", json!(["stored"]))?;
        Ok(self.stored.clone())
    }
    fn parse(&self, raw: TickerValue) -> Result<TickerValue, Error> {
        self.event(
            "parse",
            json!(["parse", leaves(&raw), same(&raw, &self.raw)]),
        )?;
        if self.input["mutateOnParse"] == true {
            if let TickerValue::Array(rows) = &raw {
                rows.borrow_mut()
                    .push(from_json(&json!({"tag":"PARSE-ALIAS"})));
                return Ok(raw);
            }
        }
        Ok(self.parsed.clone())
    }
    fn get<'a>(
        &'a self,
        endpoint: &str,
        category: &str,
    ) -> Result<impl Future<Output = Result<TickerValue, Error>> + 'a + use<'a>, Error> {
        self.event(
            "getSync",
            json!(["get",endpoint,{"params":{"category":category}}]),
        )?;
        Ok(async move {
            pending_once().await;
            self.event("getAsync", json!(["getResolved"]))?;
            Ok(self.data.clone())
        })
    }
    fn inverse_enabled(&self) -> Result<bool, Error> {
        self.event("inverseEnabled", json!(["inverseEnabled", true]))?;
        Ok(self.input["inverse"] == true)
    }
    fn emit(&self, message: TickerValue) -> Result<(), Error> {
        self.event("emit", json!(["emit", leaves(&message)]))
    }
    fn type_error(&self, message: &str) -> Error {
        Rc::new(json!({"name":"TypeError","message":message}))
    }
    fn categories(&self) -> Result<Rc<Vec<String>>, Error> {
        self.event(
            "categories",
            json!(["categories", self.categories.as_ref()]),
        )?;
        if self.input["swapOnCategories"] == true {
            *self.helper_tag.borrow_mut() = "H1".into()
        };
        Ok(Rc::clone(&self.categories))
    }
    fn bind(&self) -> Result<String, Error> {
        let tag = self.helper_tag.borrow().clone();
        self.event("helperGet", json!(["helperGet", tag]))?;
        Ok(tag)
    }
    fn helper(
        self: &Rc<Self>,
        tag: &str,
        category: &str,
        index: Option<usize>,
        original: Option<&Rc<Vec<String>>>,
        primary: bool,
    ) -> Result<Rc<RefCell<Task>>, Error> {
        let count = self.helper_calls.get();
        self.helper_calls.set(count + 1);
        self.event(
            "helperSync",
            json!([
                "helper",
                tag,
                category,
                index.map(Value::from).unwrap_or_else(undefined),
                original.is_some_and(|v| Rc::ptr_eq(v, &self.categories)),
                if index.is_some() { 3 } else { 1 }
            ]),
        )?;
        if self.input["syncFailAt"].as_u64() == Some(count as u64) {
            return Err(Rc::clone(&self.errors["helperSync"]));
        }
        if self.input["replaceHelperOnStart"] == true {
            *self.helper_tag.borrow_mut() = "H1".into()
        }
        let task = Rc::new(RefCell::new(Task {
            host: Rc::clone(self),
            count,
            pending: self
                .input
                .get("delays")
                .and_then(|v| v.get(count))
                .and_then(Value::as_u64)
                .unwrap_or(1) as usize,
            value: if primary && count == 0 {
                self.raw.clone()
            } else if primary {
                self.sets.get(1).cloned().unwrap_or(TickerValue::Undefined)
            } else {
                self.sets
                    .get(count)
                    .cloned()
                    .unwrap_or(TickerValue::Undefined)
            },
            done: None,
        }));
        self.tasks.borrow_mut().push(Rc::clone(&task));
        Ok(task)
    }
    fn all(&self, tasks: Vec<Rc<RefCell<Task>>>) -> Result<Joined, Error> {
        self.event("all", json!(["all", tasks.len()]))?;
        Ok(Joined { tasks })
    }
    fn finish_host_tasks(&self) {
        let tasks = self.tasks.borrow().clone();
        let mut cx = Context::from_waker(Waker::noop());
        for _ in 0..16 {
            let mut pending = false;
            for task in &tasks {
                if task.borrow_mut().poll(&mut cx).is_pending() {
                    pending = true
                }
            }
            if !pending {
                self.tasks.borrow_mut().clear();
                return;
            }
        }
        panic!("TICKER native host siblings stopped at poll 16")
    }
}
struct Task {
    host: Rc<Host>,
    count: usize,
    pending: usize,
    value: TickerValue,
    done: Option<Result<TickerValue, Error>>,
}
impl Task {
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Result<TickerValue, Error>> {
        if let Some(done) = &self.done {
            return Poll::Ready(done.clone());
        }
        if self.pending > 0 {
            self.pending -= 1;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        let result = self
            .host
            .event("helperAsync", json!(["helperResolved", self.count]))
            .and_then(|()| {
                if self.host.input["rejectAt"].as_u64() == Some(self.count as u64) {
                    Err(Rc::clone(&self.host.errors["helperAsync"]))
                } else {
                    Ok(self.value.clone())
                }
            });
        self.done = Some(result.clone());
        Poll::Ready(result)
    }
}
struct OneTask(Rc<RefCell<Task>>);
impl Future for OneTask {
    type Output = Result<TickerValue, Error>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.borrow_mut().poll(cx)
    }
}
struct Joined {
    tasks: Vec<Rc<RefCell<Task>>>,
}
impl Future for Joined {
    type Output = Result<Vec<TickerValue>, Error>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut pending = false;
        let mut values = vec![];
        let mut error = None;
        for task in &self.tasks {
            match task.borrow_mut().poll(cx) {
                Poll::Pending => pending = true,
                Poll::Ready(Ok(value)) => values.push(value),
                Poll::Ready(Err(e)) => {
                    if error.is_none() {
                        error = Some(e)
                    }
                }
            }
        }
        if let Some(error) = error {
            Poll::Ready(Err(error))
        } else if pending {
            Poll::Pending
        } else {
            Poll::Ready(Ok(values))
        }
    }
}
#[test]
#[ignore = "requires maintained TypeScript oracle"]
fn ticker_readers_match_maintained_bodies() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ticker-readers-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 179, "TICKER independent fixture floor");
    let mut counts = BTreeMap::<String, usize>::new();
    for (iteration, row) in rows.iter().enumerate() {
        let h = Rc::new(Host::new(row));
        let family = row["family"].as_str().unwrap();
        let kind = row["kind"].as_str().unwrap();
        *counts.entry(format!("{family}/{kind}")).or_default() += 1;
        let finish = |future| {
            let before = h.events.borrow().clone();
            native_json(
                &Value::Array(before),
                &row["preAwait"],
                &format!("TICKER native {kind} eager calls iteration {iteration}"),
            );
            drive(future)
        };
        // Box only erases the different host-future types; factories were already called.
        let future: Pin<Box<dyn Future<Output = Result<TickerValue, Error>> + '_>> = match kind {
            "without" => Box::pin(fetch_tickers_without_parse(|| h.raw(), || h.disposed())),
            "parsed" => Box::pin(fetch_tickers(
                || h.raw(),
                || h.disposed(),
                || h.stored(),
                |raw| h.parse(raw),
            )),
            "primaryRaw" => Box::pin(fetch_tickers_raw(
                |cat| {
                    let tag = h.bind()?;
                    h.helper(&tag, cat, None, None, true).map(OneTask)
                },
                || h.inverse_enabled(),
                |m| h.emit(m),
                |m| h.type_error(m),
            )),
            "inverseRaw" => Box::pin(fetch_inverse_tickers_raw(
                || h.categories(),
                || {
                    let tag = h.bind()?;
                    let h = Rc::clone(&h);
                    Ok(move |cat: &str, index: usize, original: &Rc<Vec<String>>| {
                        h.helper(&tag, cat, Some(index), Some(original), false)
                    })
                },
                |tasks| h.all(tasks),
                |m| h.emit(m),
                |m| h.type_error(m),
            )),
            "helper" => {
                let category = h
                    .input
                    .get("category")
                    .and_then(Value::as_str)
                    .unwrap_or("linear");
                if family == "spot" {
                    Box::pin(fetch_spot_tickers_raw(
                        |e, c| h.get(e, c),
                        |m| h.emit(m),
                        |m| h.type_error(m),
                    ))
                } else {
                    Box::pin(fetch_tickers_raw_helper(
                        category,
                        |e, c| h.get(e, c),
                        |m| h.emit(m),
                        |m| h.type_error(m),
                    ))
                }
            }
            _ => unreachable!(),
        };
        let result = finish(future);
        // Host-owned requests run independently while the caller's promise
        // continuation/rejection jobs settle. Two fixed turns, not an oracle
        // event count, model this boundary; slow siblings remain unfinished.
        let mut caller_cx = Context::from_waker(Waker::noop());
        for _ in 0..2 {
            for task in h.tasks.borrow().iter() {
                let _ = task.borrow_mut().poll(&mut caller_cx);
            }
        }
        native_json(
            &Value::Array(h.events.borrow().clone()),
            &row["atSettlement"],
            &format!("TICKER native settlement iteration {iteration}"),
        );
        h.finish_host_tasks();
        native_json(
            &Value::Array(h.events.borrow().clone()),
            &row["events"],
            &format!("TICKER native {kind} event iteration {iteration}"),
        );
        assert_eq!(
            result.is_ok(),
            row["result"].get("value").is_some(),
            "TICKER native rejection discriminant iteration {iteration}"
        );
        match result {
            Ok(value) => {
                native_value(
                    &value,
                    &row["result"]["value"],
                    &format!("TICKER native {kind} return iteration {iteration}"),
                );
                for (key, original) in [
                    ("sameRaw", &h.raw),
                    ("sameStored", &h.stored),
                    ("sameParsed", &h.parsed),
                ] {
                    assert_eq!(
                        same(&value, original),
                        row["result"][key].as_bool().unwrap(),
                        "TICKER native return identity {key} iteration {iteration}"
                    )
                }
                assert_eq!(
                    same(&value, &property(&property(&h.data, "result"), "list")),
                    row["result"]["sameList"].as_bool().unwrap(),
                    "TICKER native original list identity"
                );
                let row_flags = if let TickerValue::Array(values) = &value {
                    values
                        .borrow()
                        .iter()
                        .map(|value| {
                            h.sets.iter().any(|set| {
                                if let TickerValue::Array(set) = set {
                                    set.borrow().iter().any(|v| same(value, v))
                                } else {
                                    false
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                };
                assert_eq!(
                    row_flags,
                    row["result"]["rowIdentities"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_bool().unwrap())
                        .collect::<Vec<_>>(),
                    "TICKER native flattened row identity"
                )
            }
            Err(error) => {
                assert_eq!(
                    error["name"], row["result"]["error"],
                    "TICKER native original error name"
                );
                assert_eq!(
                    error["message"], row["result"]["message"],
                    "TICKER native original error message iteration {iteration}"
                );
                assert_eq!(
                    h.errors
                        .iter()
                        .find(|(_, e)| Rc::ptr_eq(&error, e))
                        .map(|(key, _)| key.as_str()),
                    row["result"]["origin"].as_str(),
                    "TICKER native original error identity"
                )
            }
        }
        assert_eq!(
            h.disposed.get(),
            row["state"]["disposed"].as_bool().unwrap(),
            "TICKER native disposal state"
        );
        assert_eq!(
            h.helper_tag.borrow().as_str(),
            row["state"]["helperTag"].as_str().unwrap(),
            "TICKER native helper state"
        );
        assert_eq!(
            h.helper_calls.get(),
            row["state"]["helperCalls"].as_u64().unwrap() as usize,
            "TICKER native helper count"
        );
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("primary/without".into(), 14),
            ("primary/parsed".into(), 14),
            ("primary/helper".into(), 19),
            ("inverse/without".into(), 14),
            ("inverse/parsed".into(), 14),
            ("inverse/helper".into(), 19),
            ("spot/without".into(), 14),
            ("spot/parsed".into(), 14),
            ("spot/helper".into(), 19),
            ("primary/primaryRaw".into(), 20),
            ("inverse/inverseRaw".into(), 18)
        ]),
        "TICKER independent eleven-symbol floors"
    );
}
