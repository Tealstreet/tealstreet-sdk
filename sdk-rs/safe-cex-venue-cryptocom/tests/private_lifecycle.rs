use safe_cex_venue_cryptocom::{
    private_lifecycle as bodies,
    public_subscriptions::{
        ArraySlot, ChannelValue, OrderedStringSet, SubscriptionFrame, SubscriptionState,
        SubscriptionTransport,
    },
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn text(v: &Value) -> Vec<u16> {
    if let Some(s) = v.as_str() {
        s.encode_utf16().collect()
    } else {
        units(&v["units"])
    }
}
fn input_slot(v: &Value) -> ArraySlot {
    if v["hole"] == true {
        ArraySlot::Hole
    } else if v["undefined"] == true {
        ArraySlot::Value(ChannelValue::Undefined)
    } else {
        ArraySlot::Value(ChannelValue::String(text(v)))
    }
}
fn observed_value(v: &Value) -> ChannelValue {
    match v["kind"].as_str().unwrap() {
        "undefined" => ChannelValue::Undefined,
        "string" => ChannelValue::String(units(&v["units"])),
        kind => panic!("unexpected native value {kind}"),
    }
}
fn observed_slots(v: &Value) -> Vec<ArraySlot> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|slot| match slot["kind"].as_str().unwrap() {
            "hole" => ArraySlot::Hole,
            "value" => ArraySlot::Value(observed_value(&slot["value"])),
            kind => panic!("unexpected native slot {kind}"),
        })
        .collect()
}
fn input(v: &Value) -> Vec<ArraySlot> {
    v.as_array()
        .map(|a| a.iter().map(input_slot).collect())
        .unwrap_or_default()
}
fn number(v: &Value) -> f64 {
    match v.as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("neg0") => -0.0,
        None => v.as_f64().unwrap(),
        other => panic!("unexpected number {other:?}"),
    }
}
fn observed_number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn native_number(actual: f64, expected: f64, label: &str) {
    assert!(
        actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
        "CRYPTOCOM-LIFECYCLE-NATIVE-NUMBER {label}: actual {actual:?}, expected {expected:?}"
    )
}
fn string_value(s: &str) -> ChannelValue {
    ChannelValue::String(s.encode_utf16().collect())
}

type Error = Rc<()>;
type State = bodies::PrivateSocketState<&'static str, &'static str>;
type Callback = Box<dyn Fn() -> Result<(), Error>>;
#[derive(Clone)]
struct NativeFrame {
    id: f64,
    method: &'static str,
    channels: Vec<ArraySlot>,
    nonce: f64,
}
struct Snapshot {
    subscriptions: Vec<ChannelValue>,
    retained: Vec<ChannelValue>,
    id: f64,
    disposed: bool,
    authenticated: bool,
    timer: Option<&'static str>,
    retained_timer: Option<&'static str>,
    urls_used: Vec<Vec<u16>>,
    socket: Option<&'static str>,
    events: Vec<String>,
    frames: Vec<NativeFrame>,
    emitted: Vec<Vec<u16>>,
    auth_messages: usize,
    listeners: Vec<(&'static str, &'static str)>,
    closed: Vec<&'static str>,
    callback_results: Vec<bool>,
    input: Vec<ArraySlot>,
}
struct Host {
    spec: Value,
    state: Rc<State>,
    input: Rc<RefCell<Vec<ArraySlot>>>,
    retained: Rc<OrderedStringSet>,
    error: Error,
    coerce_error: Error,
    catch_error: Error,
    message: Rc<()>,
    prepared: Rc<()>,
    urls: Rc<()>,
    environment: Cell<&'static str>,
    urls_used: RefCell<Vec<Vec<u16>>>,
    old_timer: Option<Rc<&'static str>>,
    events: RefCell<Vec<String>>,
    frames: RefCell<Vec<NativeFrame>>,
    emitted: RefCell<Vec<Vec<u16>>>,
    auth_messages: Cell<usize>,
    listeners: RefCell<Vec<(&'static str, &'static str)>>,
    closed: RefCell<Vec<&'static str>>,
    callback_results: RefCell<Vec<bool>>,
    callback: RefCell<Option<Callback>>,
    clock_count: Cell<u32>,
}
impl Host {
    fn event(&self, phase: &str) -> Result<(), Error> {
        self.events.borrow_mut().push(phase.to_owned());
        if self.spec["fail"] == phase {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn catch_fail(&self, phase: &str) -> Result<(), Error> {
        if self.spec["catchFail"] == phase {
            Err(self.catch_error.clone())
        } else {
            Ok(())
        }
    }
    fn current_set(&self) -> Rc<OrderedStringSet> {
        self.state.channels.subscriptions.borrow().clone()
    }
    fn replace_set(&self, key: &str) {
        if self.spec[key].is_array() {
            let set = OrderedStringSet::new(input(&self.spec[key]).into_iter().map(|s| match s {
                ArraySlot::Hole => ChannelValue::Undefined,
                ArraySlot::Value(v) => v,
            }));
            self.state.channels.subscriptions.replace(Rc::new(set));
        }
    }
    fn prepare(&self) -> Result<Rc<()>, Error> {
        self.event("resolve-send")?;
        self.replace_set("replaceSet");
        if self.spec["resetAuth"] == true {
            self.state.is_authenticated.set(false);
        }
        self.event("resolve-json")?;
        Ok(self.prepared.clone())
    }
    fn clock(&self) -> Result<f64, Error> {
        self.event("clock")?;
        let n = self.clock_count.get() + 1;
        self.clock_count.set(n);
        self.input
            .borrow_mut()
            .extend(input(&self.spec["growAtClock"]));
        Ok(1700000000000.0 + 13.0 * f64::from(n))
    }
    fn send_effects(&self) -> Result<(), Error> {
        self.event("send")?;
        self.input
            .borrow_mut()
            .extend(input(&self.spec["growAtSend"]));
        self.replace_set("replaceSetAtSend");
        Ok(())
    }
    fn capture(
        &self,
        prepared: Rc<()>,
        id: f64,
        method: &'static str,
        channels: Vec<ArraySlot>,
        nonce: f64,
    ) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&prepared, &self.prepared),
            "CRYPTOCOM-LIFECYCLE-NATIVE-PREPARED"
        );
        self.events.borrow_mut().push("stringify".to_owned());
        self.frames.borrow_mut().push(NativeFrame {
            id,
            method,
            channels,
            nonce,
        });
        if self.spec["fail"] == "stringify" {
            return Err(self.error.clone());
        }
        self.send_effects()
    }
    fn clear(&self, timer: Option<Rc<&'static str>>) -> Result<(), Error> {
        assert!(
            match (&timer, &self.old_timer) {
                (None, None) => true,
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                _ => false,
            },
            "CRYPTOCOM-LIFECYCLE-NATIVE-TIMER-IDENTITY"
        );
        self.event("clear")?;
        if self.spec["replaceSocketAtClear"] == true {
            self.state.ws.replace(Some(Rc::new("replacement-socket")));
        }
        if self.spec["clearSocketAtClear"] == true {
            self.state.ws.replace(None);
        }
        Ok(())
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            subscriptions: self.current_set().values(),
            retained: self.retained.values(),
            id: self.state.channels.message_id.get(),
            disposed: self.state.is_disposed.get(),
            authenticated: self.state.is_authenticated.get(),
            retained_timer: self.old_timer.as_ref().map(|v| **v),
            urls_used: self.urls_used.borrow().clone(),
            timer: self
                .state
                .reconnect_auth_timer
                .borrow()
                .as_ref()
                .map(|v| **v),
            socket: self.state.ws.borrow().as_ref().map(|v| **v),
            events: self.events.borrow().clone(),
            frames: self.frames.borrow().clone(),
            emitted: self.emitted.borrow().clone(),
            auth_messages: self.auth_messages.get(),
            listeners: self.listeners.borrow().clone(),
            closed: self.closed.borrow().clone(),
            callback_results: self.callback_results.borrow().clone(),
            input: self.input.borrow().clone(),
        }
    }
}
impl bodies::ConnectionHost<&'static str> for Host {
    type Error = Error;
    type Urls = Rc<()>;
    fn testnet(&self) -> Result<bool, Error> {
        self.event("testnet")?;
        Ok(match &self.spec["testnet"] {
            Value::Null => false,
            Value::Bool(v) => *v,
            Value::Number(n) => n.as_f64().unwrap() != 0.0,
            Value::String(v) => !v.is_empty(),
            _ => true,
        })
    }
    fn urls(&self, environment: &'static str) -> Result<Rc<()>, Error> {
        self.event("urls")?;
        assert!(matches!(environment, "PRODUCTION" | "TESTNET"));
        self.environment.set(environment);
        Ok(self.urls.clone())
    }
    fn create(&self, urls: Rc<()>) -> Result<Rc<&'static str>, Error> {
        assert!(
            Rc::ptr_eq(&urls, &self.urls),
            "CRYPTOCOM-LIFECYCLE-NATIVE-URL-IDENTITY"
        );
        self.event("resolve-create")?;
        self.event("user-url")?;
        self.event("create")?;
        let url = if self.environment.get() == "TESTNET" {
            "wss://uat-stream.3ona.co/exchange/v1/user"
        } else {
            "wss://stream.crypto.com/exchange/v1/user"
        };
        self.urls_used
            .borrow_mut()
            .push(url.encode_utf16().collect());
        Ok(Rc::new("new-socket"))
    }
    fn add_listener(
        &self,
        socket: Option<Rc<&'static str>>,
        event: &'static str,
    ) -> Result<(), Error> {
        let socket = socket.expect("source socket is present");
        self.event(&format!("resolve-add-{event}"))?;
        self.event(&format!("listener-{event}"))?;
        self.event(&format!("add-{event}"))?;
        self.listeners.borrow_mut().push((*socket, event));
        if self.spec["replaceOnListener"] == true && event == "open" {
            self.state.ws.replace(Some(Rc::new("replacement-socket")));
        }
        Ok(())
    }
}
impl bodies::TimerHost<&'static str> for Host {
    type Error = Error;
    type AuthReturn = Rc<()>;
    fn track_last_received(&self) -> Result<(), Error> {
        self.event("track")?;
        if self.spec["disposeDuringTrack"] == true {
            self.state.is_disposed.set(true);
        }
        Ok(())
    }
    fn clear_timeout(&self, timer: Option<Rc<&'static str>>) -> Result<(), Error> {
        self.clear(timer)
    }
    fn set_timeout<F: Fn() -> Result<(), Error> + 'static>(
        &self,
        callback: F,
        delay: f64,
    ) -> Result<Rc<&'static str>, Error> {
        self.event("schedule")?;
        assert_eq!(delay, 1000.0, "CRYPTOCOM-LIFECYCLE-NATIVE-TIMER-DELAY");
        self.callback.replace(Some(Box::new(callback)));
        if self.spec["synchronousTimer"] == true {
            self.callback.borrow().as_ref().unwrap()()?;
        }
        Ok(Rc::new("new-timer"))
    }
    fn authenticate(&self) -> Result<Rc<()>, Error> {
        self.event("timer-auth-get")?;
        self.event("timer-auth-call")?;
        Ok(self.message.clone())
    }
}
impl bodies::AuthenticationHost for Host {
    type Error = Error;
    type Message = Rc<()>;
    type Send = Rc<()>;
    type Emit = Rc<()>;
    fn generate(&self) -> Result<Rc<()>, Error> {
        self.event("generate")?;
        Ok(self.message.clone())
    }
    fn prepare_send(&self) -> Result<Rc<()>, Error> {
        self.prepare()
    }
    fn stringify_and_send(&self, send: Rc<()>, message: Rc<()>) -> Result<(), Error> {
        assert!(Rc::ptr_eq(&send, &self.prepared));
        assert!(
            Rc::ptr_eq(&message, &self.message),
            "CRYPTOCOM-LIFECYCLE-NATIVE-AUTH-MESSAGE-IDENTITY"
        );
        self.events.borrow_mut().push("stringify".to_owned());
        self.auth_messages.set(self.auth_messages.get() + 1);
        if self.spec["fail"] == "stringify" {
            return Err(self.error.clone());
        }
        self.send_effects()
    }
    fn prepare_error_emit(&self) -> Result<Rc<()>, Error> {
        self.event("resolve-emit")?;
        self.catch_fail("emit-get")?;
        Ok(self.prepared.clone())
    }
    fn error_as_template(&self, error: Error) -> Result<Vec<u16>, Error> {
        assert!(
            Rc::ptr_eq(&error, &self.error),
            "CRYPTOCOM-LIFECYCLE-NATIVE-CATCH-IDENTITY"
        );
        if self.spec["errorSymbol"] == true {
            return Err(self.coerce_error.clone());
        }
        self.event("coerce")?;
        self.catch_fail("coerce")?;
        Ok(if !self.spec["errorText"].is_null() {
            text(&self.spec["errorText"])
        } else {
            format!("original {}", self.spec["name"].as_str().unwrap())
                .encode_utf16()
                .collect()
        })
    }
    fn emit_error(&self, emit: Rc<()>, message: Vec<u16>) -> Result<(), Error> {
        assert!(Rc::ptr_eq(&emit, &self.prepared));
        self.event("emit")?;
        self.emitted.borrow_mut().push(message);
        self.catch_fail("emit")
    }
}
impl SubscriptionTransport for Host {
    type Prepared = Rc<()>;
    type Error = Error;
    fn prepare(&self) -> Result<Rc<()>, Error> {
        Host::prepare(self)
    }
    fn clock(&self) -> Result<f64, Error> {
        Host::clock(self)
    }
    fn send(&self, prepared: Rc<()>, frame: SubscriptionFrame) -> Result<(), Error> {
        self.capture(
            prepared,
            frame.id,
            frame.method,
            frame.params.channels,
            frame.nonce,
        )
    }
}
impl SubscriptionTransport<Rc<RefCell<Vec<ArraySlot>>>> for Host {
    type Prepared = Rc<()>;
    type Error = Error;
    fn prepare(&self) -> Result<Rc<()>, Error> {
        Host::prepare(self)
    }
    fn clock(&self) -> Result<f64, Error> {
        Host::clock(self)
    }
    fn send(
        &self,
        prepared: Rc<()>,
        frame: SubscriptionFrame<Rc<RefCell<Vec<ArraySlot>>>>,
    ) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&frame.params.channels, &self.input),
            "CRYPTOCOM-LIFECYCLE-NATIVE-ARRAY-IDENTITY"
        );
        let channels = frame.params.channels.borrow().clone();
        self.capture(prepared, frame.id, frame.method, channels, frame.nonce)
    }
}
struct Channels<'a>(&'a Host);
impl bodies::UnsubscribeChannels<Error> for Channels<'_> {
    type Native = Rc<RefCell<Vec<ArraySlot>>>;
    fn length(&self) -> Result<f64, Error> {
        Ok(self.0.input.borrow().len() as f64)
    }
    fn native(&self) -> Self::Native {
        self.0.input.clone()
    }
    fn for_each<F: FnMut(ChannelValue) -> Result<bool, Error>>(
        &self,
        mut callback: F,
    ) -> Result<(), Error> {
        let host = self.0;
        if host.spec["customForeach"] == true {
            host.event("resolve-foreach")?;
            host.event("foreach")?;
        }
        let len = host.input.borrow().len();
        for i in 0..len {
            if host.spec["foreachGetterThrow"] == true && i == 1 {
                host.event("foreach-get-1")?;
                return Err(host.error.clone());
            }
            let slot = host
                .input
                .borrow()
                .get(i)
                .cloned()
                .unwrap_or(ArraySlot::Hole);
            if let ArraySlot::Value(v) = slot {
                let result = callback(v)?;
                if host.spec["customForeach"] == true {
                    host.callback_results.borrow_mut().push(result);
                }
            }
        }
        Ok(())
    }
}
impl bodies::DisposeHost<&'static str, &'static str> for Host {
    type Error = Error;
    fn clear_timeout(&self, timer: Option<Rc<&'static str>>) -> Result<(), Error> {
        self.clear(timer)
    }
    fn close(&self, socket: Option<Rc<&'static str>>) -> Result<(), Error> {
        let socket = socket.expect("guarded fresh socket");
        self.event("resolve-close")?;
        self.event("close")?;
        self.closed.borrow_mut().push(*socket);
        Ok(())
    }
}
fn observe_snapshot(actual: &Snapshot, expected: &Value, name: &str, phase: &str) {
    let label = format!("{name} {phase}");
    let expected_values = |key: &str| {
        expected[key]
            .as_array()
            .unwrap()
            .iter()
            .map(observed_value)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        actual.subscriptions,
        expected_values("subscriptions"),
        "CRYPTOCOM-LIFECYCLE-NATIVE-STATE {label}"
    );
    assert_eq!(
        actual.retained,
        expected_values("retainedSubscriptions"),
        "CRYPTOCOM-LIFECYCLE-NATIVE-RETAINED-SET {label}"
    );
    native_number(actual.id, observed_number(&expected["messageId"]), &label);
    assert_eq!(
        actual.disposed,
        expected["disposed"].as_bool().unwrap(),
        "CRYPTOCOM-LIFECYCLE-NATIVE-DISPOSED {label}"
    );
    assert_eq!(
        actual.authenticated,
        expected["authenticated"].as_bool().unwrap(),
        "CRYPTOCOM-LIFECYCLE-NATIVE-AUTHENTICATED {label}"
    );
    assert_eq!(
        actual.retained_timer,
        expected["retainedTimer"].as_str(),
        "CRYPTOCOM-LIFECYCLE-NATIVE-RETAINED-TIMER {label}"
    );
    assert_eq!(
        actual.urls_used,
        expected["urlsUsed"]
            .as_array()
            .unwrap()
            .iter()
            .map(units)
            .collect::<Vec<_>>(),
        "CRYPTOCOM-LIFECYCLE-NATIVE-URL {label}"
    );
    assert_eq!(
        actual.timer,
        expected["timer"].as_str(),
        "CRYPTOCOM-LIFECYCLE-NATIVE-TIMER {label}"
    );
    assert_eq!(
        actual.socket,
        expected["socket"].as_str(),
        "CRYPTOCOM-LIFECYCLE-NATIVE-SOCKET {label}"
    );
    for (i, (frame, source)) in actual
        .frames
        .iter()
        .zip(expected["frames"].as_array().unwrap())
        .enumerate()
    {
        native_number(
            frame.id,
            observed_number(&source["id"]),
            &format!("{label} frame{i} id"),
        );
        assert_eq!(
            frame.method.encode_utf16().collect::<Vec<_>>(),
            units(&source["method"]),
            "CRYPTOCOM-LIFECYCLE-NATIVE-METHOD {label} frame{i}"
        );
        assert_eq!(
            frame.channels,
            observed_slots(&source["channels"]),
            "CRYPTOCOM-LIFECYCLE-NATIVE-CHANNELS {label} frame{i}"
        );
        native_number(
            frame.nonce,
            observed_number(&source["nonce"]),
            &format!("{label} frame{i} nonce"),
        );
    }
    assert_eq!(
        actual.emitted,
        expected["emitted"]
            .as_array()
            .unwrap()
            .iter()
            .map(units)
            .collect::<Vec<_>>(),
        "CRYPTOCOM-LIFECYCLE-NATIVE-EMIT {label}"
    );
    assert_eq!(
        actual.callback_results,
        expected["callbackResults"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_bool().unwrap())
            .collect::<Vec<_>>(),
        "CRYPTOCOM-LIFECYCLE-NATIVE-CALLBACK-RESULT {label}"
    );
    assert_eq!(
        actual.input,
        observed_slots(&expected["input"]),
        "CRYPTOCOM-LIFECYCLE-NATIVE-INPUT {label}"
    );
    let listeners = expected["listeners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v[0].as_str().unwrap(), v[1].as_str().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        actual.listeners, listeners,
        "CRYPTOCOM-LIFECYCLE-NATIVE-LISTENERS {label}"
    );
    assert_eq!(
        actual.closed,
        expected["closed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        "CRYPTOCOM-LIFECYCLE-NATIVE-CLOSED {label}"
    );
    assert_eq!(
        actual.events,
        expected["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        "CRYPTOCOM-LIFECYCLE-NATIVE-EFFECTS {label}"
    );
    assert_eq!(
        actual.frames.len(),
        expected["frames"].as_array().unwrap().len(),
        "CRYPTOCOM-LIFECYCLE-INVENTORY-FRAMES {label}"
    );
    assert_eq!(
        actual.auth_messages,
        expected["authMessages"].as_array().unwrap().len(),
        "CRYPTOCOM-LIFECYCLE-INVENTORY-AUTH {label}"
    );
}
fn complete(future: impl Future<Output = Result<(), Error>>) -> Result<(), Error> {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("source no-await promise must be ready"),
    }
}
fn result_name(result: Result<(), Error>, host: &Host) -> &'static str {
    match result {
        Ok(()) => "void",
        Err(error) if Rc::ptr_eq(&error, &host.error) => "original",
        Err(error) if Rc::ptr_eq(&error, &host.catch_error) => "catch-body",
        Err(error) if Rc::ptr_eq(&error, &host.coerce_error) => "coercion",
        Err(_) => panic!("CRYPTOCOM-LIFECYCLE-NATIVE-ERROR-IDENTITY"),
    }
}
#[test]
fn private_lifecycle_matches_actual_typescript_native() {
    let fixtures: Value =
        serde_json::from_str(include_str!("private_lifecycle_fixtures.json")).unwrap();
    let compiler =
        std::env::var("SAFE_CEX_TYPESCRIPT_PATH").expect("SAFE_CEX_TYPESCRIPT_PATH required");
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private_lifecycle_oracle.cjs"
        ))
        .env("SAFE_CEX_TYPESCRIPT_PATH", compiler)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual TypeScript oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (index, spec) in fixtures.as_array().unwrap().iter().enumerate() {
        let source = &rows[index];
        let name = spec["name"].as_str().unwrap();
        let initial = if let Some(count) = spec["count"].as_u64() {
            (0..count)
                .map(|i| string_value(&format!("channel-{i}")))
                .collect::<Vec<_>>()
        } else {
            input(&spec["initial"])
                .into_iter()
                .map(|s| match s {
                    ArraySlot::Hole => ChannelValue::Undefined,
                    ArraySlot::Value(v) => v,
                })
                .collect()
        };
        let retained = Rc::new(OrderedStringSet::new(initial));
        let old_timer = (spec["noTimer"] != true).then(|| Rc::new("old-timer"));
        let state = Rc::new(State {
            channels: SubscriptionState {
                subscriptions: RefCell::new(retained.clone()),
                message_id: Cell::new(if spec["messageId"].is_null() {
                    1.0
                } else {
                    number(&spec["messageId"])
                }),
            },
            is_disposed: Cell::new(spec["disposed"] == true),
            is_authenticated: Cell::new(spec["authenticated"].as_bool().unwrap_or(true)),
            reconnect_auth_timer: RefCell::new(old_timer.clone()),
            ws: RefCell::new((spec["noSocket"] != true).then(|| Rc::new("old-socket"))),
        });
        let host = Rc::new(Host {
            spec: spec.clone(),
            state: state.clone(),
            input: Rc::new(RefCell::new(input(&spec["input"]))),
            retained,
            error: Rc::new(()),
            coerce_error: Rc::new(()),
            catch_error: Rc::new(()),
            message: Rc::new(()),
            prepared: Rc::new(()),
            urls: Rc::new(()),
            environment: Cell::new(""),
            urls_used: RefCell::new(vec![]),
            old_timer,
            events: RefCell::new(vec![]),
            frames: RefCell::new(vec![]),
            emitted: RefCell::new(vec![]),
            auth_messages: Cell::new(0),
            listeners: RefCell::new(vec![]),
            closed: RefCell::new(vec![]),
            callback_results: RefCell::new(vec![]),
            callback: RefCell::new(None),
            clock_count: Cell::new(0),
        });
        let op = spec["op"].as_str().unwrap();
        let result = match op {
            "connect" => {
                let ready = bodies::connect_and_subscribe(&state, &*host);
                observe_snapshot(&host.snapshot(), &source["immediate"], name, "before-poll");
                complete(ready)
            }
            "auth" => {
                let ready = bodies::authenticate(&*host);
                observe_snapshot(&host.snapshot(), &source["immediate"], name, "before-poll");
                complete(ready)
            }
            "replay" => {
                let ready = bodies::resubscribe(&state, &*host);
                observe_snapshot(&host.snapshot(), &source["immediate"], name, "before-poll");
                complete(ready)
            }
            "open" => bodies::on_open(state.clone(), host.clone()),
            "order" => bodies::subscribe_to_order_updates(&state, &*host),
            "position" => bodies::subscribe_to_position_updates(&state, &*host),
            "balance" => bodies::subscribe_to_balance_updates(&state, &*host),
            "unsubscribe" => bodies::unsubscribe(&state, &Channels(&host), &*host),
            "dispose" => bodies::dispose(&state, &*host),
            _ => panic!("unknown op"),
        };
        if !matches!(op, "connect" | "auth" | "replay") {
            observe_snapshot(&host.snapshot(), &source["immediate"], name, "immediate");
        }
        assert_eq!(
            result_name(result, &host),
            source["result"].as_str().unwrap(),
            "CRYPTOCOM-LIFECYCLE-NATIVE-RETURN {name}"
        );
        let mut timer_result = "not-fired";
        if op == "open" && (spec["fireTimer"] == true) && host.callback.borrow().is_some() {
            if spec["disposeBeforeFire"] == true {
                state.is_disposed.set(true);
            }
            timer_result = result_name(host.callback.borrow().as_ref().unwrap()(), &host);
        }
        observe_snapshot(&host.snapshot(), &source["final"], name, "final");
        assert_eq!(
            timer_result,
            source["timerResult"].as_str().unwrap(),
            "CRYPTOCOM-LIFECYCLE-NATIVE-TIMER-RETURN {name}"
        );
        assert_eq!(
            host.frames.borrow().len() as u64,
            spec["expectedFrames"].as_u64().unwrap(),
            "CRYPTOCOM-LIFECYCLE-INDEPENDENT-FRAME-FLOOR {name}"
        );
        assert_eq!(
            host.auth_messages.get() as u64,
            spec["expectedAuthMessages"].as_u64().unwrap(),
            "CRYPTOCOM-LIFECYCLE-INDEPENDENT-AUTH-FLOOR {name}"
        );
        assert_eq!(
            source["name"].as_str().unwrap(),
            name,
            "LIFECYCLE scenario identity"
        );
    }
    assert_eq!(
        fixtures.as_array().unwrap().len(),
        84,
        "LIFECYCLE independent input floor"
    );
    assert_eq!(
        rows.as_array().unwrap().len(),
        84,
        "LIFECYCLE independent result floor"
    );
    assert_eq!(fixtures[83]["name"], "dispose-retained-set");
}
