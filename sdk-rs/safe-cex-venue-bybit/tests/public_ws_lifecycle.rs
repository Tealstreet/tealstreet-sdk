use safe_cex_venue_bybit::public_ws_lifecycle::*;
use serde_json::{Map, Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn drive<F: Future>(f: F) -> F::Output {
    let mut f = Box::pin(f);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..5 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("PUBLIC native future did not settle by poll 5")
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
enum Event {
    Data(Value),
    Number(&'static str, f64),
    Frame(String),
    Sleep(u64),
}
struct Created {
    protocols: Option<Vec<String>>,
    options: Option<Rc<Value>>,
}
struct Host {
    input: Value,
    family: String,
    events: RefCell<Vec<Event>>,
    frames: RefCell<Vec<String>>,
    errors: BTreeMap<String, Rc<Value>>,
    disposed: Cell<bool>,
    enabled: Cell<bool>,
    ping_at: Cell<f64>,
    sends: Cell<u64>,
    sleeps: Cell<u64>,
    handler_reads: Cell<u64>,
    tokens: BTreeMap<&'static str, Rc<Value>>,
    markets: RefCell<Rc<Value>>,
    message: RefCell<Rc<Value>>,
    socket_a: Rc<Value>,
    socket_b: Rc<Value>,
    ws: RefCell<Option<Rc<Value>>>,
    created_socket: RefCell<Option<Rc<Value>>>,
    created: RefCell<Option<Created>>,
    options: Rc<Value>,
    registrations: RefCell<Vec<Value>>,
    topics: RefCell<Map<String, Value>>,
}
impl Host {
    fn new(row: &Value) -> Self {
        let input = &row["input"];
        let tokens: BTreeMap<_, _> = [
            "M0",
            "M1",
            "active",
            "onOpen",
            "onMessage",
            "onClose",
            "onMessageNew",
        ]
        .into_iter()
        .map(|s| (s, Rc::new(json!(s))))
        .collect();
        Self{
  input:input.clone(),family:row["family"].as_str().unwrap().into(),events:RefCell::new(vec![]),frames:RefCell::new(vec![]),
  errors:["disposed","route","direct","resolve","proxy","demo","options","id","create","setBefore","setAfter","handler_open","listener_open","handler_message","listener_message","handler_close","listener_close","sync","start","ping","handler1","handler2","off","on","reconnected","activeHandler","base","now","markBefore","markAfter","send","topics","sleep"].into_iter().map(|p|(p.into(),Rc::new(json!({"name":format!("{p}Error"),"message":format!("{p} failure"),"code":37})))).collect(),
  disposed:Cell::new(input["disposed"]==true),enabled:Cell::new(input["enabled"]==true),ping_at:Cell::new(101.25),sends:Cell::new(0),sleeps:Cell::new(0),handler_reads:Cell::new(0),markets:RefCell::new(Rc::clone(&tokens["M0"])),message:RefCell::new(Rc::clone(&tokens["onMessage"])),tokens,socket_a:Rc::new(json!("SOCKET-A")),socket_b:Rc::new(json!("SOCKET-B")),ws:RefCell::new(None),created_socket:RefCell::new(None),created:RefCell::new(None),options:Rc::new(json!({"headers":{"X-Tag":"OPT-73"}})),registrations:RefCell::new(vec![]),topics:RefCell::new(input.get("topics").and_then(Value::as_object).cloned().unwrap_or_default()),
 }
    }
    fn failure(&self, phase: &str) -> Result<(), Rc<Value>> {
        if self.input["phase"] == phase {
            Err(Rc::clone(&self.errors[phase]))
        } else {
            Ok(())
        }
    }
    fn call(&self, phase: &str, event: Value) -> Result<(), Rc<Value>> {
        self.events.borrow_mut().push(Event::Data(event));
        self.failure(phase)
    }
    fn disposed(&self) -> Result<bool, Rc<Value>> {
        self.call("disposed", json!(["disposed"]))?;
        Ok(self.disposed.get())
    }
    fn route(&self) -> Result<(bool, Option<String>, Option<String>), Rc<Value>> {
        self.call("route", json!(["route"]))?;
        Ok((
            self.input["tunnelled"] == true,
            self.input["host"].as_str().map(str::to_owned),
            Some(
                self.input["fallback"]
                    .as_str()
                    .unwrap_or("wss://FALLBACK")
                    .into(),
            ),
        ))
    }
    fn direct(&self) -> Result<String, Rc<Value>> {
        self.call("direct", json!(["direct"]))?;
        let urls = if self.family == "spot" {
            &safe_cex_venue_bybit::spot_types::BASE_WS_URL
        } else {
            &safe_cex_venue_bybit::types::BASE_WS_URL
        };
        Ok(
            urls["public"][&self.family][if self.input["testnet"] == true {
                "testnet"
            } else {
                "livenet"
            }]
            .as_str()
            .unwrap()
            .into(),
        )
    }
    fn resolve(&self, primary: &str, fallback: Option<&str>) -> Result<String, Rc<Value>> {
        self.call(
            "resolve",
            json!([
                "resolve",
                primary,
                optional_value(fallback.map(|s| json!(s)))
            ]),
        )?;
        Ok(if self.input["forceFallback"] == true {
            fallback.unwrap().into()
        } else {
            primary.into()
        })
    }
    fn demo(&self) -> Result<bool, Rc<Value>> {
        self.call("demo", json!(["demo"]))?;
        Ok(self.input["demo"] == true)
    }
    fn options(&self) -> Result<Option<Rc<Value>>, Rc<Value>> {
        self.call("options", json!(["options"]))?;
        Ok(if self.input["optionsMissing"] == true {
            None
        } else {
            Some(Rc::clone(&self.options))
        })
    }
    fn id(&self) -> Result<String, Rc<Value>> {
        self.call("id", json!(["id"]))?;
        Ok("ID-59".into())
    }
    fn create(
        &self,
        url: String,
        protocols: Option<Vec<String>>,
        options: Option<Rc<Value>>,
    ) -> Result<Rc<Value>, Rc<Value>> {
        self.events.borrow_mut().push(Event::Data(json!([
            "create",
            url,
            optional_value(protocols.as_ref().map(|v| json!(v))),
            optional_value(options.as_ref().map(|v| v.as_ref().clone()))
        ])));
        *self.created.borrow_mut() = Some(Created { protocols, options });
        self.failure("create")?;
        *self.created_socket.borrow_mut() = Some(Rc::clone(&self.socket_a));
        Ok(Rc::clone(&self.socket_a))
    }
    fn set_ws(&self, ws: Rc<Value>) -> Result<(), Rc<Value>> {
        self.call("setBefore", json!(["setWs", ws.as_str().unwrap()]))?;
        *self.ws.borrow_mut() = Some(ws);
        self.failure("setAfter")
    }
    fn handler(&self, field: &str) -> Result<Rc<Value>, Rc<Value>> {
        let value = match field {
            "onOpen" => Rc::clone(&self.tokens["onOpen"]),
            "onMessage" => self.message.borrow().clone(),
            "onClose" => Rc::clone(&self.tokens["onClose"]),
            _ => panic!("PUBLIC native unknown handler field"),
        };
        let event = match field {
            "onOpen" => "open",
            "onMessage" => "message",
            _ => "close",
        };
        self.call(
            &format!("handler_{event}"),
            json!(["handler", field, value.as_str().unwrap()]),
        )?;
        if self.input["replaceWsInHandler"] == true && field == "onOpen" {
            *self.ws.borrow_mut() = Some(Rc::clone(&self.socket_b));
        }
        Ok(value)
    }
    fn listen(&self, ws: Rc<Value>, key: &str, handler: Rc<Value>) -> Result<(), Rc<Value>> {
        let tag = handler.as_str().unwrap();
        self.call(
            &format!("listener_{key}"),
            json!([
                "listener",
                key,
                tag,
                ws.as_str().unwrap(),
                Rc::ptr_eq(&handler, &self.tokens[tag])
            ]),
        )?;
        self.registrations
            .borrow_mut()
            .push(json!([ws.as_str().unwrap(), key, tag]));
        if key == "open" {
            if self.input["replaceWsOnOpen"] == true {
                *self.ws.borrow_mut() = Some(Rc::clone(&self.socket_b))
            }
            if self.input["replaceHandlerOnOpen"] == true {
                *self.message.borrow_mut() = Rc::clone(&self.tokens["onMessageNew"])
            }
        }
        Ok(())
    }
    fn markets_handler(&self) -> Result<Rc<Value>, Rc<Value>> {
        self.handler_reads.set(self.handler_reads.get() + 1);
        let value = self.markets.borrow().clone();
        self.call(
            &format!("handler{}", self.handler_reads.get()),
            json!(["handler", "markets", value.as_str().unwrap()]),
        )?;
        Ok(value)
    }
    fn off(&self, key: &str, handler: Rc<Value>) -> Result<(), Rc<Value>> {
        let tag = handler.as_str().unwrap();
        self.call(
            "off",
            json!(["off", key, tag, Rc::ptr_eq(&handler, &self.tokens[tag])]),
        )?;
        if self.input["replaceHandlerOnOff"] == true {
            *self.markets.borrow_mut() = Rc::clone(&self.tokens["M1"])
        }
        Ok(())
    }
    fn on(&self, key: &str, handler: Rc<Value>) -> Result<(), Rc<Value>> {
        let tag = handler.as_str().unwrap();
        self.call(
            "on",
            json!(["on", key, tag, Rc::ptr_eq(&handler, &self.tokens[tag])]),
        )
    }
    fn enabled(&self) -> bool {
        self.events
            .borrow_mut()
            .push(Event::Data(json!(["enabled"])));
        self.enabled.get()
    }
    fn open_ping(&self) -> Result<(), Rc<Value>> {
        self.call("ping", json!(["ping"]))?;
        if self.input["enableOnPing"] == true {
            self.enabled.set(true)
        }
        if self.input["disposeOnPing"] == true {
            self.disposed.set(true)
        }
        Ok(())
    }
    fn active_handler(&self) -> Result<Rc<Value>, Rc<Value>> {
        self.call("activeHandler", json!(["handler", "active", "active"]))?;
        Ok(Rc::clone(&self.tokens["active"]))
    }
    fn base_dispose(&self) -> Result<(), Rc<Value>> {
        self.call("base", json!(["base"]))?;
        self.disposed.set(true);
        Ok(())
    }
    fn now(&self) -> Result<f64, Rc<Value>> {
        let value =
            safe_cex_num::number_from_string(self.input["clock"].as_str().unwrap_or("123.5"));
        self.events.borrow_mut().push(Event::Number("now", value));
        self.failure("now")?;
        if self.input["disposeOnNow"] == true {
            self.disposed.set(true)
        }
        Ok(value)
    }
    fn mark(&self, value: f64) -> Result<(), Rc<Value>> {
        self.events.borrow_mut().push(Event::Number("mark", value));
        self.failure("markBefore")?;
        self.ping_at.set(value);
        self.failure("markAfter")
    }
    fn send(&self, frame: &str) -> Result<(), Rc<Value>> {
        self.sends.set(self.sends.get() + 1);
        self.frames.borrow_mut().push(frame.into());
        self.events.borrow_mut().push(Event::Frame(frame.into()));
        self.failure("send")?;
        if self.input["sendFailAt"].as_u64() == Some(self.sends.get()) {
            Err(Rc::clone(&self.errors["send"]))
        } else {
            Ok(())
        }
    }
    fn topics(&self) -> Result<Map<String, Value>, Rc<Value>> {
        self.call("topics", json!(["topics"]))?;
        Ok(self.topics.borrow().clone())
    }
    async fn sleep(&self, delay: u64) -> Result<(), Rc<Value>> {
        self.sleeps.set(self.sleeps.get() + 1);
        self.events.borrow_mut().push(Event::Sleep(delay));
        pending_once().await;
        if self.input["disposeOnSleep"] == true {
            self.disposed.set(true)
        }
        if self.input["mutateTopicsOnSleep"] == true {
            self.topics
                .borrow_mut()
                .get_mut("tickers")
                .unwrap()
                .as_array_mut()
                .unwrap()
                .push(json!("LATE"))
        }
        if self.input["sleepFailAt"].as_u64() == Some(self.sleeps.get()) {
            Err(Rc::clone(&self.errors["sleep"]))
        } else {
            Ok(())
        }
    }
    fn outcome(&self, result: &Result<(), Rc<Value>>, expected: &Value) {
        assert_eq!(
            result.is_ok(),
            expected.get("error").is_none(),
            "PUBLIC native rejection"
        );
        if let Err(error) = result {
            for (key, source) in [("name", "error"), ("message", "message"), ("code", "code")] {
                assert_eq!(
                    error.get(key),
                    expected.get(source),
                    "PUBLIC native original error {key}"
                )
            }
            assert!(
                Rc::ptr_eq(error, &self.errors[expected["origin"].as_str().unwrap()]),
                "PUBLIC native error identity"
            );
        }
    }
    fn assert_state(&self, row: &Value) {
        assert_eq!(
            self.disposed.get(),
            row["disposed"] == true,
            "PUBLIC native disposed state"
        );
        assert_eq!(
            self.enabled.get(),
            row["enabled"] == true,
            "PUBLIC native enabled state"
        );
        number(self.ping_at.get(), &row["pingAt"], "ping timestamp");
        assert_eq!(
            optional_value(self.ws.borrow().as_ref().map(|v| v.as_ref().clone())),
            row["ws"],
            "PUBLIC native socket value"
        );
        let same = self
            .ws
            .borrow()
            .as_ref()
            .zip(self.created_socket.borrow().as_ref())
            .is_some_and(|(a, b)| Rc::ptr_eq(a, b));
        assert_eq!(
            same,
            row["sameCreated"] == true,
            "PUBLIC native socket identity"
        );
        assert_eq!(
            optional_value(
                self.created_socket
                    .borrow()
                    .as_ref()
                    .map(|v| v.as_ref().clone())
            ),
            row["created"],
            "PUBLIC native created socket"
        );
        let created = self.created.borrow();
        let opts = created.as_ref().and_then(|v| v.options.as_ref());
        assert_eq!(
            optional_value(opts.map(|v| v.as_ref().clone())),
            row["createdOptions"],
            "PUBLIC native option value"
        );
        assert_eq!(
            opts.is_some_and(|v| Rc::ptr_eq(v, &self.options)),
            row["optionsSame"] == true,
            "PUBLIC native option identity"
        );
        if let Some(created) = created.as_ref() {
            assert!(
                created.protocols.is_none(),
                "PUBLIC native absent protocols"
            );
        }
        assert_eq!(
            row["protocols"]["undefined"], true,
            "PUBLIC independent source undefined protocols"
        );
        assert_eq!(
            *self.registrations.borrow(),
            row["registrations"].as_array().unwrap().clone(),
            "PUBLIC native registrations"
        );
        assert_eq!(
            Value::Object(self.topics.borrow().clone()),
            row["topics"],
            "PUBLIC native live topic state"
        );
        assert_eq!(
            self.sends.get(),
            row["sends"].as_u64().unwrap(),
            "PUBLIC native send count"
        );
        assert_eq!(
            self.sleeps.get(),
            row["sleeps"].as_u64().unwrap(),
            "PUBLIC native sleep count"
        );
        let events = self.events.borrow();
        let expected = row["events"].as_array().unwrap();
        assert_eq!(events.len(), expected.len(), "PUBLIC native callback count");
        for (event, expected) in events.iter().zip(expected) {
            match event {
                Event::Data(value) => {
                    assert_eq!(value[0], expected[0], "PUBLIC native callback order");
                    if value[0] == "off" || value[0] == "on" {
                        assert_eq!(value[1], expected[1], "PUBLIC native listener key");
                        assert_eq!(value[2], expected[2], "PUBLIC native handler token");
                        assert_eq!(value[3], expected[3], "PUBLIC native handler identity");
                    } else {
                        assert_eq!(value, expected, "PUBLIC native callback fields");
                    }
                }
                Event::Number(key, value) => {
                    assert_eq!(
                        *key,
                        expected[0].as_str().unwrap(),
                        "PUBLIC native number callback order"
                    );
                    number(*value, &expected[1], "clock/mark argument")
                }
                Event::Sleep(delay) => {
                    assert_eq!(expected[0], "sleep", "PUBLIC native sleep order");
                    assert_eq!(
                        *delay,
                        expected[1].as_u64().unwrap(),
                        "PUBLIC native subscription delay"
                    )
                }
                Event::Frame(frame) => {
                    assert_eq!(expected[0], "send", "PUBLIC native send order");
                    let value: Value =
                        serde_json::from_str(frame).expect("PUBLIC native frame decode");
                    assert_eq!(value["op"], expected[1]["op"], "PUBLIC native frame op");
                    assert_eq!(
                        value.get("args"),
                        expected[1].get("args"),
                        "PUBLIC native ordered frame args"
                    );
                    assert_eq!(
                        value.as_object().unwrap().len(),
                        if value["op"] == "ping" { 1 } else { 2 },
                        "PUBLIC independent frame field count"
                    )
                }
            }
        }
    }
}
fn optional_value(value: Option<Value>) -> Value {
    value.unwrap_or_else(|| json!({"undefined":true}))
}
fn number(value: f64, expected: &Value, label: &str) {
    let bits = if value.is_nan() {
        "nan".into()
    } else {
        format!("{:016x}", value.to_bits())
    };
    assert_eq!(
        bits,
        expected["number"].as_str().unwrap(),
        "PUBLIC native number {label}"
    )
}
#[test]
#[ignore = "requires installed live TypeScript"]
fn public_ws_lifecycle_native_source_parity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-ws-lifecycle-fixtures.cjs"
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
        ("connect", 96),
        ("open", 38),
        ("dispose", 10),
        ("ping", 36),
        ("subscribe", 30),
    ] {
        assert_eq!(
            fixtures[group].as_array().unwrap().len(),
            count,
            "PUBLIC independent {group} floor"
        );
    }
    let mut wires = vec![];
    for group in ["connect", "open", "dispose", "ping", "subscribe"] {
        for row in fixtures[group].as_array().unwrap() {
            let h = Host::new(row);
            let result = match group {
                "connect" => connect_and_subscribe(
                    &h.family,
                    || h.disposed(),
                    || h.route(),
                    || h.direct(),
                    |a, b| h.resolve(a, b),
                    |a, b| h.call("proxy", json!(["proxy", a, b])),
                    || h.demo(),
                    || h.options(),
                    || h.id(),
                    |a, b, c| h.create(a, b, c),
                    |s| h.set_ws(s),
                    || Ok(h.ws.borrow().as_ref().unwrap().clone()),
                    |s| h.handler(s),
                    |ws, s, handler| h.listen(ws, s, handler),
                ),
                "open" => on_open(
                    || h.disposed(),
                    || h.call("sync", json!(["sync"])),
                    || h.call("start", json!(["start"])),
                    || h.open_ping(),
                    || h.markets_handler(),
                    |key, handler| h.off(key, handler),
                    |key, handler| h.on(key, handler),
                    || h.enabled(),
                    || h.call("reconnected", json!(["reconnected"])),
                ),
                "dispose" => dispose(
                    || h.active_handler(),
                    |key, handler| h.off(key, handler),
                    || h.base_dispose(),
                ),
                "ping" => ping(|| h.disposed(), || h.now(), |v| h.mark(v), |s| h.send(s)),
                _ => drive(subscribe(|| h.topics(), |s| h.send(s), |d| h.sleep(d))),
            };
            h.outcome(&result, &row["result"]);
            h.assert_state(row);
            let frames = h.frames.borrow();
            let expected = row["frames"].as_array().unwrap();
            assert_eq!(frames.len(), expected.len(), "PUBLIC native frame count");
            for (frame, expected) in frames.iter().zip(expected) {
                wires.push((frame.clone(), expected.as_str().unwrap().to_owned()));
            }
        }
    }
    // Every native field, float, identity and decoded frame assertion above precedes all wire comparisons.
    for (actual, expected) in wires {
        assert_eq!(actual, expected, "PUBLIC wire frame")
    }
}
