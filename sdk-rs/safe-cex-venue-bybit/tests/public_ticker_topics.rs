use safe_cex_venue_bybit::public_ticker_topics::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn drive<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..5 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
    }
    panic!("TOPICS native future did not settle by poll 5");
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
    frames: RefCell<Vec<String>>,
    current: RefCell<Option<Rc<RefCell<Value>>>>,
    original: Option<Rc<RefCell<Value>>>,
    received: RefCell<Option<Rc<RefCell<Value>>>>,
    errors: BTreeMap<&'static str, Rc<Value>>,
    disposed: Cell<bool>,
    connected: Cell<bool>,
    sends: Cell<u64>,
    sleeps: Cell<u64>,
}
impl Host {
    fn new(input: &Value) -> Self {
        let original = input
            .get("current")
            .cloned()
            .map(|v| Rc::new(RefCell::new(v)));
        Self{input:input.clone(),events:RefCell::new(vec![]),frames:RefCell::new(vec![]),current:RefCell::new(original.clone()),original,received:RefCell::new(None),errors:["current","symbols","setter","send","sleep"].into_iter().map(|p|(p,Rc::new(json!({"name":format!("{p}Error"),"message":format!("{p} failure"),"code":37})))).collect(),disposed:Cell::new(input["disposed"]==true),connected:Cell::new(input["connected"]==true),sends:Cell::new(0),sleeps:Cell::new(0)}
    }
    fn event(&self, event: Value) {
        self.events.borrow_mut().push(event);
    }
    fn phase(&self, phase: &str) -> bool {
        self.input["phase"] == phase
    }
    fn current(&self) -> Result<Rc<RefCell<Value>>, Rc<Value>> {
        get_current_ticker_topics(|| {
            self.event(json!(["current"]));
            if self.phase("current") {
                Err(Rc::clone(&self.errors["current"]))
            } else {
                Ok(self.current.borrow().clone())
            }
        })
    }
    fn current_strings(&self) -> Result<Vec<String>, Rc<Value>> {
        Ok(strings(&self.current()?.borrow()))
    }
    fn desired(&self) -> Result<Vec<String>, Rc<Value>> {
        get_desired_ticker_topics(|| {
            self.event(json!(["symbols"]));
            if self.phase("symbols") {
                return Err(Rc::clone(&self.errors["symbols"]));
            }
            if self.input["mutateOriginal"] == true {
                if let Some(original) = &self.original {
                    if let Some(a) = original.borrow_mut().as_array_mut() {
                        a.push(json!("tickers.LATE"));
                    }
                }
            }
            if self.input["disposeOnSymbols"] == true {
                self.disposed.set(true)
            }
            if self.input["connectOnSymbols"] == true {
                self.connected.set(true)
            }
            Ok(self.input.get("symbols").map(strings).unwrap_or_default())
        })
    }
    fn set(&self, value: Rc<RefCell<Value>>) -> Result<(), Rc<Value>> {
        self.event(json!(["setter", value.borrow().clone()]));
        *self.received.borrow_mut() = Some(Rc::clone(&value));
        if self.phase("setterBefore") {
            return Err(Rc::clone(&self.errors["setter"]));
        }
        *self.current.borrow_mut() = Some(value);
        if self.phase("setterAfter") {
            return Err(Rc::clone(&self.errors["setter"]));
        }
        Ok(())
    }
    fn sync(&self) -> Result<Rc<RefCell<Value>>, Rc<Value>> {
        sync_ticker_topics(
            || self.current_strings(),
            || self.desired(),
            |v| self.set(v),
        )
    }
    fn disposed(&self) -> bool {
        self.event(json!(["disposed"]));
        self.disposed.get()
    }
    fn connected(&self) -> bool {
        self.event(json!(["connected"]));
        self.connected.get()
    }
    fn has_ws(&self) -> bool {
        self.event(json!(["ws"]));
        self.input["hasWs"] != false
    }
    fn send(&self, frame: &str) -> Result<(), Rc<Value>> {
        self.sends.set(self.sends.get() + 1);
        self.frames.borrow_mut().push(frame.into());
        let decoded: Value = serde_json::from_str(frame).expect("TOPICS native frame decode");
        self.event(json!(["send", decoded]));
        if self.input["sendFailAt"].as_u64() == Some(self.sends.get()) {
            Err(Rc::clone(&self.errors["send"]))
        } else {
            Ok(())
        }
    }
    async fn sleep(&self, delay: u64) -> Result<(), Rc<Value>> {
        self.sleeps.set(self.sleeps.get() + 1);
        self.event(json!(["sleep", delay]));
        pending_once().await;
        if self.input["disposeOnSleep"] == true {
            self.disposed.set(true)
        }
        if self.input["sleepFailAt"].as_u64() == Some(self.sleeps.get()) {
            Err(Rc::clone(&self.errors["sleep"]))
        } else {
            Ok(())
        }
    }
    fn assert_state(&self, row: &Value) {
        optional(
            self.current.borrow().as_ref(),
            &row["current"],
            "stored current",
        );
        optional(self.original.as_ref(), &row["original"], "original current");
        optional(
            self.received.borrow().as_ref(),
            &row["received"],
            "received current",
        );
        assert_eq!(
            self.disposed.get(),
            row["disposed"] == true,
            "TOPICS native disposal state"
        );
        assert_eq!(
            self.connected.get(),
            row["connected"] == true,
            "TOPICS native connection state"
        );
        let events = self.events.borrow();
        let expected = row["events"].as_array().unwrap();
        assert_eq!(events.len(), expected.len(), "TOPICS native callback count");
        for (actual, expected) in events.iter().zip(expected) {
            assert_eq!(actual[0], expected[0], "TOPICS native callback order");
            match actual[0].as_str().unwrap() {
                "send" => {
                    assert_eq!(actual[1]["op"], expected[1]["op"], "TOPICS native frame op");
                    native(&actual[1]["args"], &expected[1]["args"]);
                    assert_eq!(
                        actual[1].as_object().unwrap().len(),
                        2,
                        "TOPICS independent frame fields"
                    );
                }
                "setter" => native(&actual[1], &expected[1]),
                "sleep" => assert_eq!(
                    actual[1].as_u64(),
                    expected[1].as_u64(),
                    "TOPICS native sleep delay"
                ),
                _ => assert_eq!(
                    actual.as_array().unwrap().len(),
                    1,
                    "TOPICS independent getter event fields"
                ),
            }
        }
    }
    fn outcome<T>(&self, result: &Result<T, Rc<Value>>, expected: &Value) {
        assert_eq!(
            result.is_ok(),
            expected.get("error").is_none(),
            "TOPICS native rejection"
        );
        if let Err(error) = result {
            for (native_key, source_key) in
                [("name", "error"), ("message", "message"), ("code", "code")]
            {
                assert_eq!(
                    error.get(native_key),
                    expected.get(source_key),
                    "TOPICS native error {native_key}"
                );
            }
            let origin = expected["origin"]
                .as_str()
                .expect("source callback error identity");
            assert!(
                Rc::ptr_eq(error, &self.errors[origin]),
                "TOPICS native original error identity"
            );
        }
    }
}
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}
fn native(actual: &Value, expected: &Value) {
    if let Some(values) = actual.as_array() {
        let expected = expected.as_array().expect("source array");
        assert_eq!(values.len(), expected.len(), "TOPICS native topic count");
        for (v, e) in values.iter().zip(expected) {
            assert_eq!(v, e, "TOPICS native ordered topic");
        }
    } else {
        assert_eq!(actual, expected, "TOPICS native scalar topic state");
    }
}
fn optional(actual: Option<&Rc<RefCell<Value>>>, expected: &Value, label: &str) {
    if expected["undefined"] == true {
        assert!(actual.is_none(), "TOPICS native absent {label}");
    } else {
        assert!(actual.is_some(), "TOPICS native present {label}");
        native(&actual.unwrap().borrow(), expected);
    }
}
#[test]
#[ignore = "requires installed live TypeScript"]
fn public_ticker_topics_native_source_parity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-ticker-topics-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixtures: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (group, count) in [
        ("current", 20),
        ("desired", 16),
        ("sync", 20),
        ("active", 50),
        ("markets", 36),
    ] {
        assert_eq!(
            fixtures[group].as_array().unwrap().len(),
            count,
            "TOPICS independent {group} floor"
        );
    }
    let mut wires = vec![];
    for row in fixtures["current"].as_array().unwrap() {
        let h = Host::new(&row["input"]);
        let result = h.current();
        h.outcome(&result, &row["result"]);
        if let Ok(value) = result {
            assert_eq!(
                h.original
                    .as_ref()
                    .is_some_and(|old| Rc::ptr_eq(old, &value)),
                row["result"]["same"] == true,
                "TOPICS native current array identity"
            );
            value
                .borrow_mut()
                .as_array_mut()
                .unwrap()
                .push(json!("MUTATED"));
            native(&value.borrow(), &row["result"]["value"]);
        }
        h.assert_state(row);
    }
    for row in fixtures["desired"].as_array().unwrap() {
        let h = Host::new(&row["input"]);
        let result = h.desired();
        h.outcome(&result, &row["result"]);
        if let Ok(mut value) = result {
            value.push("MUTATED".into());
            assert_eq!(
                value.len(),
                row["result"]["value"].as_array().unwrap().len(),
                "TOPICS native desired count"
            );
            for (v, e) in value.iter().zip(row["result"]["value"].as_array().unwrap()) {
                assert_eq!(Some(v.as_str()), e.as_str(), "TOPICS native desired topic");
            }
        }
        assert_eq!(
            h.input["symbols"], row["symbols"],
            "TOPICS native original symbols"
        );
        h.assert_state(row);
    }
    for row in fixtures["sync"].as_array().unwrap() {
        let h = Host::new(&row["input"]);
        let result = h.sync();
        h.outcome(&result, &row["result"]);
        if let Ok(value) = result {
            assert_eq!(
                h.current
                    .borrow()
                    .as_ref()
                    .is_some_and(|v| Rc::ptr_eq(v, &value)),
                row["result"]["sameCurrent"] == true,
                "TOPICS native merged state identity"
            );
            assert_eq!(
                h.received
                    .borrow()
                    .as_ref()
                    .is_some_and(|v| Rc::ptr_eq(v, &value)),
                row["result"]["sameReceived"] == true,
                "TOPICS native merged setter identity"
            );
            value
                .borrow_mut()
                .as_array_mut()
                .unwrap()
                .push(json!("MUTATED"));
            native(&value.borrow(), &row["result"]["value"]);
        }
        h.assert_state(row);
    }
    for group in ["active", "markets"] {
        for row in fixtures[group].as_array().unwrap() {
            let h = Host::new(&row["input"]);
            let result = if group == "active" {
                handle_active_ticker_symbols_changed(
                    || h.disposed(),
                    || h.current_strings(),
                    || h.sync().map(|v| strings(&v.borrow())),
                    || h.connected(),
                    |s| h.send(s),
                )
            } else {
                drive(on_markets_changed(
                    strings(&row["input"]["added"]),
                    || h.has_ws(),
                    || h.disposed(),
                    || h.current_strings(),
                    |v| h.set(v),
                    |s| h.send(s),
                    |delay| h.sleep(delay),
                ))
            };
            h.outcome(&result, &row["result"]);
            h.assert_state(row);
            let frames = h.frames.borrow();
            let expected = row["frames"].as_array().unwrap();
            assert_eq!(frames.len(), expected.len(), "TOPICS native frame count");
            for (frame, expected) in frames.iter().zip(expected) {
                wires.push((frame.clone(), expected.as_str().unwrap().to_owned()));
            }
        }
    }
    // All decoded native values, aliases, callback effects and errors above precede wire assertions.
    for (actual, expected) in wires {
        assert_eq!(actual, expected, "TOPICS wire subscription frame");
    }
}
