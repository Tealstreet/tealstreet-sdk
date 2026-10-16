use safe_cex_venue_bybit::stream_delegation::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
type Error = Rc<Value>;
type Callback = Rc<dyn Fn(Rc<Value>) -> Result<(), Error>>;
fn undefined() -> Value {
    json!({"undefined":true})
}
struct Host {
    input: Value,
    events: RefCell<Vec<Value>>,
    errors: BTreeMap<String, Error>,
    tokens: BTreeMap<String, Rc<Value>>,
    public_tag: RefCell<String>,
    private_tag: RefCell<String>,
    base_disposed: Cell<bool>,
    cancelled: Cell<bool>,
    clears: Cell<usize>,
    options: Rc<RefCell<Value>>,
    packet: Rc<Value>,
    returned: Rc<Value>,
    null: Rc<Value>,
    timers: Rc<RefCell<Vec<(String, bool)>>>,
}
impl Host {
    fn new(row: &Value) -> Self {
        let i = &row["input"];
        let timers = i
            .get("timers")
            .cloned()
            .unwrap_or_else(|| json!(["A", "B"]));
        Self{input:i.clone(),events:RefCell::new(vec![]),errors:["getPublic","getPrivate","method","invoke","callback","cancelGet","cancel","keys","next","clear","base","publicDispose","privateDispose"].into_iter().map(|p|(p.into(),Rc::new(json!({"name":format!("{p}Error"),"message":format!("{p} failed"),"code":73})))).collect(),tokens:["PUB-A","PUB-B","PRIV-A","PRIV-B"].into_iter().map(|tag|(tag.into(),Rc::new(json!({"tag":tag})))).collect(),public_tag:RefCell::new("PUB-A".into()),private_tag:RefCell::new("PRIV-A".into()),base_disposed:Cell::new(false),cancelled:Cell::new(false),clears:Cell::new(0),options:Rc::new(RefCell::new(json!({"symbol":"BTC","interval":"1m","nested":{"marker":"OPTS-31"}}))),packet:Rc::new(json!({"symbol":"BTC","open":11,"close":17,"time":23})),returned:Rc::new(json!({"marker":"RETURN-61"})),null:Rc::new(Value::Null),timers:Rc::new(RefCell::new(timers.as_array().unwrap().iter().map(|key|(key.as_str().unwrap().into(),true)).collect()))}
    }
    fn flag(&self, key: &str) -> bool {
        self.input[key] == true
    }
    fn event(&self, phase: &str, value: Value) -> Result<(), Error> {
        self.events.borrow_mut().push(value);
        if self.input["phase"] == phase {
            Err(Rc::clone(&self.errors[phase]))
        } else {
            Ok(())
        }
    }
    fn websocket(&self, property: &str) -> Result<Rc<Value>, Error> {
        let private = property == "privateWebsocket";
        let tag = if private {
            self.private_tag.borrow().clone()
        } else {
            self.public_tag.borrow().clone()
        };
        self.event(
            if private { "getPrivate" } else { "getPublic" },
            json!(["get", property, tag]),
        )?;
        Ok(Rc::clone(&self.tokens[&tag]))
    }
    fn invoke(
        &self,
        receiver: Rc<Value>,
        method: &str,
        arg: Arg,
        cb: Callback,
        expected_cb: &Callback,
    ) -> Result<Option<Rc<Value>>, Error> {
        let tag = receiver["tag"].as_str().unwrap();
        self.event("method", json!(["method", tag, method]))?;
        if self.flag("swapOnMethod") {
            *self.public_tag.borrow_mut() = "PUB-B".into()
        }
        let (value, same) = match &arg {
            Arg::Options(options) => (options.borrow().clone(), Rc::ptr_eq(options, &self.options)),
            Arg::Symbol(symbol) => (
                Value::from(symbol.as_str()),
                symbol
                    == self
                        .input
                        .get("symbol")
                        .and_then(Value::as_str)
                        .unwrap_or("BTC"),
            ),
        };
        self.event(
            "invoke",
            json!([
                "invoke",
                tag,
                method,
                value,
                same,
                Rc::ptr_eq(&cb, expected_cb),
                Rc::ptr_eq(&receiver, &self.tokens[tag])
            ]),
        )?;
        if self.flag("swapOnCall") {
            *self.public_tag.borrow_mut() = "PUB-B".into()
        }
        if self.flag("mutateOptions") {
            if let Arg::Options(options) = arg {
                options.borrow_mut()["nested"]["marker"] = Value::from("MUTATED-37");
            }
        }
        if self.flag("fire") {
            cb(Rc::clone(&self.packet))?;
        }
        Ok(if self.flag("returnUndefined") {
            None
        } else if self.flag("returnNull") {
            Some(Rc::clone(&self.null))
        } else {
            Some(Rc::clone(&self.returned))
        })
    }
    fn base(&self) -> Result<Option<Rc<Value>>, Error> {
        self.event("base", json!(["base"]))?;
        self.base_disposed.set(true);
        if self.flag("swapOnBase") {
            *self.public_tag.borrow_mut() = "PUB-B".into();
            *self.private_tag.borrow_mut() = "PRIV-B".into()
        };
        Ok(Some(Rc::clone(&self.returned)))
    }
    fn dispose(&self, receiver: Rc<Value>) -> Result<Option<Rc<Value>>, Error> {
        let tag = receiver["tag"].as_str().unwrap();
        let public = tag.starts_with("PUB");
        self.event(
            if public {
                "publicDispose"
            } else {
                "privateDispose"
            },
            json!(["dispose", tag, Rc::ptr_eq(&receiver, &self.tokens[tag])]),
        )?;
        if public && self.flag("swapPrivateOnPublic") {
            *self.private_tag.borrow_mut() = "PRIV-B".into()
        };
        Ok(Some(Rc::clone(&self.returned)))
    }
    fn cancel(&self) -> Result<Option<Rc<Value>>, Error> {
        self.event("cancelGet", json!(["cancelGet"]))?;
        self.event("cancel", json!(["cancel"]))?;
        self.cancelled.set(true);
        Ok(Some(Rc::clone(&self.returned)))
    }
    fn clear(&self, key: String) -> Result<Option<Rc<Value>>, Error> {
        let count = self.clears.get() + 1;
        self.clears.set(count);
        self.event("clear", json!(["clear", key]))?;
        if self.input["clearFailAt"].as_u64() == Some(count as u64) {
            return Err(Rc::clone(&self.errors["clear"]));
        }
        let mut timers = self.timers.borrow_mut();
        for (existing, alive) in timers.iter_mut() {
            if *existing == key {
                *alive = false;
            }
        }
        if count == 1 {
            if self.flag("deleteNextOnClear") {
                for (existing, alive) in timers.iter_mut() {
                    if existing == "B" {
                        *alive = false;
                    }
                }
            }
            if self.flag("appendOnClear") {
                timers.push(("LATE".into(), true));
            }
            if self.flag("reinsertOnClear") {
                timers.push((key, true));
            }
        }
        Ok(Some(Rc::clone(&self.returned)))
    }
    fn state(&self) -> Value {
        json!({"publicTag":self.public_tag.borrow().as_str(),"privateTag":self.private_tag.borrow().as_str(),"baseDisposed":self.base_disposed.get(),"cancelled":self.cancelled.get(),"clears":self.clears.get(),"options":self.options.borrow().clone(),"timers":self.timers.borrow().iter().filter(|(_,alive)|*alive).map(|(key,_)|key.clone()).collect::<Vec<_>>()})
    }
}
enum Arg {
    Options(Rc<RefCell<Value>>),
    Symbol(String),
}
struct LiveKeys {
    host: Rc<Host>,
    index: usize,
    polls: usize,
}
impl Iterator for LiveKeys {
    type Item = Result<String, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        self.polls += 1;
        assert!(
            self.polls <= 5,
            "STREAM native timer iterator floor stopped at next 5"
        );
        let key = {
            let timers = self.host.timers.borrow();
            while self.index < timers.len() && !timers[self.index].1 {
                self.index += 1;
            }
            if self.index < timers.len() {
                let key = timers[self.index].0.clone();
                self.index += 1;
                Some(key)
            } else {
                None
            }
        };
        let observed = key
            .as_ref()
            .map(|key| json!({"key":key}))
            .unwrap_or_else(|| json!({"done":true}));
        let result = self.host.event("next", json!(["next", observed]));
        match (key, result) {
            (_, Err(error)) => Some(Err(error)),
            (Some(key), Ok(())) => Some(Ok(key)),
            (None, Ok(())) => None,
        }
    }
}
// Compare decoded native leaves. No native serialization or wire comparison.
fn native(actual: Option<&Value>, expected: &Value, label: &str) {
    if expected.get("undefined") == Some(&Value::Bool(true)) {
        assert!(
            actual.is_none_or(|v| v == &undefined()),
            "{label}: expected native absence, got {actual:?}"
        );
        return;
    }
    let actual = actual.unwrap_or_else(|| panic!("{label}: missing native field"));
    if let Some(bits) = expected.get("number").and_then(Value::as_str) {
        let n = actual.as_f64().unwrap();
        assert_eq!(
            format!("{:016x}", n.to_bits()),
            bits,
            "{label}: native number"
        );
        return;
    }
    match expected {
        Value::Array(values) => {
            let a = actual.as_array().unwrap();
            assert_eq!(a.len(), values.len(), "{label}: native array count");
            for (a, e) in a.iter().zip(values) {
                native(Some(a), e, label)
            }
        }
        Value::Object(fields) => {
            let a = actual.as_object().unwrap();
            assert_eq!(a.len(), fields.len(), "{label}: native object key count");
            for (key, e) in fields {
                native(a.get(key), e, label)
            }
        }
        _ => assert_eq!(actual, expected, "{label}: decoded native value"),
    }
}
#[test]
#[ignore = "requires maintained TypeScript oracle"]
fn stream_delegation_matches_maintained_bodies() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/stream-delegation-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 139, "STREAM independent fixture floor");
    let mut counts = BTreeMap::<String, usize>::new();
    for (iteration, row) in rows.iter().enumerate() {
        let h = Rc::new(Host::new(row));
        let family = row["family"].as_str().unwrap();
        let kind = row["kind"].as_str().unwrap();
        *counts.entry(format!("{family}/{kind}")).or_default() += 1;
        let callback: Callback = {
            let h = Rc::clone(&h);
            Rc::new(move |packet| {
                h.event(
                    "callback",
                    json!(["callback", packet.as_ref(), Rc::ptr_eq(&packet, &h.packet)]),
                )
            })
        };
        let cb = Rc::clone(&callback);
        let options = Rc::clone(&h.options);
        let symbol = h
            .input
            .get("symbol")
            .and_then(Value::as_str)
            .unwrap_or("BTC");
        let result: Result<Option<Rc<Value>>, Error> = match kind {
            "ohlcv" => {
                if family == "inverse" {
                    listen_inverse_ohlcv(
                        options,
                        cb,
                        |p| h.websocket(p),
                        |s, m, o, c| h.invoke(s, m, Arg::Options(o), c, &callback),
                    )
                } else {
                    listen_ohlcv(
                        options,
                        cb,
                        |p| h.websocket(p),
                        |s, m, o, c| h.invoke(s, m, Arg::Options(o), c, &callback),
                    )
                }
            }
            "bulk" => {
                let result = if family == "inverse" {
                    listen_inverse_ohlcvs(
                        options,
                        cb,
                        |p| h.websocket(p),
                        |s, m, o, c| h.invoke(s, m, Arg::Options(o), c, &callback),
                    )
                } else {
                    listen_ohlcvs(
                        options,
                        cb,
                        |p| h.websocket(p),
                        |s, m, o, c| h.invoke(s, m, Arg::Options(o), c, &callback),
                    )
                };
                result.map(|()| None)
            }
            "book" => {
                if family == "inverse" {
                    listen_inverse_order_book(
                        symbol,
                        cb,
                        |p| h.websocket(p),
                        |s, m, o, c| h.invoke(s, m, Arg::Symbol(o.into()), c, &callback),
                    )
                } else {
                    listen_order_book(
                        symbol,
                        cb,
                        |p| h.websocket(p),
                        |s, m, o, c| h.invoke(s, m, Arg::Symbol(o.into()), c, &callback),
                    )
                }
            }
            "dispose" => {
                let result = match family {
                    "primary" => dispose_primary_exchange(
                        || h.cancel(),
                        || {
                            h.event("keys", json!(["keys"]))?;
                            Ok(LiveKeys {
                                host: Rc::clone(&h),
                                index: 0,
                                polls: 0,
                            })
                        },
                        |key| h.clear(key),
                        || h.base(),
                        |p| h.websocket(p),
                        |s| h.dispose(s),
                    ),
                    "inverse" => {
                        dispose_inverse_exchange(|| h.base(), |p| h.websocket(p), |s| h.dispose(s))
                    }
                    "spot" => dispose_exchange(|| h.base(), |p| h.websocket(p), |s| h.dispose(s)),
                    _ => unreachable!(),
                };
                result.map(|()| None)
            }
            _ => unreachable!(),
        };
        let label = if kind == "dispose" && family == "primary" {
            format!("STREAM native dispose event primary timers iteration {iteration}")
        } else {
            format!("STREAM native {kind} event iteration {iteration}")
        };
        native(
            Some(&Value::Array(h.events.borrow().clone())),
            &row["events"],
            &label,
        );
        assert_eq!(
            result.is_ok(),
            row["result"].get("value").is_some(),
            "STREAM native rejection discriminant iteration {iteration}"
        );
        match result {
            Ok(value) => {
                assert_eq!(
                    value.as_ref().is_some_and(|r| Rc::ptr_eq(r, &h.returned)),
                    row["result"]["sameReturn"].as_bool().unwrap(),
                    "STREAM native return identity iteration {iteration}"
                );
                native(
                    value.as_deref(),
                    &row["result"]["value"],
                    "STREAM native decoded return",
                )
            }
            Err(error) => {
                native(
                    error.get("name"),
                    &row["result"]["error"],
                    "STREAM native original error name",
                );
                native(
                    error.get("message"),
                    &row["result"]["message"],
                    "STREAM native original error message",
                );
                assert_eq!(
                    h.errors
                        .iter()
                        .find(|(_, e)| Rc::ptr_eq(e, &error))
                        .map(|(key, _)| key.as_str()),
                    row["result"]["origin"].as_str(),
                    "STREAM native original error identity"
                )
            }
        }
        native(
            Some(&h.state()),
            &row["state"],
            "STREAM native decoded host state",
        );
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("primary/ohlcv".into(), 13),
            ("primary/book".into(), 13),
            ("primary/bulk".into(), 13),
            ("primary/dispose".into(), 19),
            ("inverse/ohlcv".into(), 13),
            ("inverse/book".into(), 13),
            ("inverse/bulk".into(), 13),
            ("inverse/dispose".into(), 8),
            ("spot/ohlcv".into(), 13),
            ("spot/book".into(), 13),
            ("spot/dispose".into(), 8)
        ]),
        "STREAM independent eleven-symbol floors"
    );
}
