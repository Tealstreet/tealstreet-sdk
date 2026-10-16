use safe_cex_venue_polymarket::ws_lifecycle::*;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::OnceLock,
};
#[derive(Debug)]
struct Timer {
    id: u32,
}
#[derive(Debug)]
struct Failure {
    marker: u32,
    message: &'static str,
}
type Error = Rc<Failure>;
type Callback = Rc<dyn Fn() -> Result<(), Error>>;
#[derive(Debug)]
struct Event {
    tag: &'static str,
    text: Option<String>,
    severity: Option<&'static str>,
    frame: Option<SubscriptionFrame>,
    delay: Option<f64>,
    event_undefined: Option<bool>,
    synthetic: Option<bool>,
}
impl Event {
    fn new(tag: &'static str) -> Self {
        Self {
            tag,
            text: None,
            severity: None,
            frame: None,
            delay: None,
            event_undefined: None,
            synthetic: None,
        }
    }
}
struct Host {
    venue: String,
    scenario: usize,
    failure_point: &'static str,
    failure: Error,
    callback_failure: Cell<bool>,
    disposed: Cell<bool>,
    connected: Cell<bool>,
    ping_slot: RefCell<Option<Rc<Timer>>>,
    socket: Cell<Option<u32>>,
    strings: Vec<Vec<u16>>,
    trace: RefCell<Vec<Event>>,
    timers: RefCell<Vec<(Rc<Timer>, Callback)>>,
}
impl Host {
    fn event(&self, event: Event) -> Result<(), Error> {
        let tag = event.tag;
        self.trace.borrow_mut().push(event);
        if tag == self.failure_point
            || (self.callback_failure.get() && ["childPing", "childConnect"].contains(&tag))
        {
            Err(self.failure.clone())
        } else {
            Ok(())
        }
    }
    fn assets(&self) -> Result<Vec<Vec<u16>>, Error> {
        self.event(Event::new("assets"))?;
        Ok(if self.scenario == 1 {
            vec![]
        } else {
            vec![
                "base".encode_utf16().collect(),
                self.strings[0].clone(),
                self.strings[1].clone(),
            ]
        })
    }
    fn credentials(&self) -> Result<Option<WsCredentials>, Error> {
        self.event(Event::new("credentials"))?;
        Ok((self.scenario != 3).then(|| WsCredentials {
            api_key: self.strings[0].clone(),
            secret: self.strings[1].clone(),
            passphrase: self.strings[2].clone(),
        }))
    }
}
impl SocketHost for Host {
    type Timer = Timer;
    type Error = Error;
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn is_connected(&self) -> bool {
        self.connected.get()
    }
    fn log(&self, text: &str, severity: &str) -> Result<(), Error> {
        let mut event = Event::new("log");
        event.text = Some(text.into());
        event.severity = Some(match severity {
            "info" => "info",
            "debug" => "debug",
            "warning" => "warning",
            "error" => "error",
            _ => panic!("native severity"),
        });
        self.event(event)?;
        if self.scenario == 9 && severity == "info" {
            self.disposed.set(true);
        }
        Ok(())
    }
    fn send(&self, text: &str, frame: Option<&SubscriptionFrame>) -> Result<(), Error> {
        let mut event = Event::new("send");
        event.text = Some(text.into());
        event.frame = frame.cloned();
        self.event(event)
    }
    fn set_timeout(&self, delay: f64, callback: Callback) -> Result<Rc<Timer>, Error> {
        let mut event = Event::new("timeout");
        event.delay = Some(delay);
        self.event(event)?;
        let handle = Rc::new(Timer {
            id: 23 + self.timers.borrow().len() as u32,
        });
        self.timers.borrow_mut().push((handle.clone(), callback));
        Ok(handle)
    }
    fn record_close_diagnostics(&self, event: Option<()>, synthetic: bool) -> Result<(), Error> {
        let mut trace = Event::new("diagnostics");
        trace.event_undefined = Some(event.is_none());
        trace.synthetic = Some(synthetic);
        self.event(trace)
    }
    fn clear_ping_interval(&self) -> Result<(), Error> {
        self.event(Event::new("clearPing"))?;
        *self.ping_slot.borrow_mut() = None;
        Ok(())
    }
    fn clear_socket_field(&self) -> Result<(), Error> {
        self.event(Event::new("clearSocket"))?;
        self.socket.set(None);
        if self.scenario == 9 {
            self.disposed.set(true);
        }
        Ok(())
    }
    fn ping(&self) -> Result<(), Error> {
        self.event(Event::new("childPing"))
    }
    fn connect_and_subscribe(&self) -> Result<(), Error> {
        self.event(Event::new("childConnect"))
    }
    fn send_subscription(&self) -> Result<(), Error> {
        send_public_subscription(self, || self.assets())
    }
    fn authenticate(&self) -> Result<(), Error> {
        authenticate_private(self, || self.credentials())
    }
    fn emit_reconnected(&self) -> Result<(), Error> {
        self.event(Event::new("reconnected"))
    }
}
fn oracle() -> &'static Value {
    static CAPTURE: OnceLock<Value> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let result = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/ws_lifecycle_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(
            value.as_array().unwrap().len(),
            80,
            "independent eight-method ten-scenario floor"
        );
        assert_eq!(value[79]["venue"], "private");
        assert_eq!(value[79]["method"], "authenticate");
        assert_eq!(value[79]["scenario"], 9, "last socket fixture");
        value
    })
}
fn units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn failure_point(venue: &str, method: &str, scenario: usize) -> &'static str {
    match scenario {
        4 => "log",
        5 => "send",
        6 => "timeout",
        7 => "diagnostics",
        8 if venue == "private" && ["authenticate", "onOpen"].contains(&method) => "credentials",
        8 => "clearPing",
        _ => "",
    }
}
fn root_tags(venue: &str, method: &str, scenario: usize) -> Vec<&'static str> {
    let mut tags = match method {
        "onOpen" if scenario == 2 => vec![],
        "onOpen" if venue == "public" => {
            if scenario == 1 {
                vec!["log", "assets", "childPing", "reconnected"]
            } else {
                vec!["log", "assets", "send", "log", "childPing", "reconnected"]
            }
        }
        "onOpen" => {
            if scenario == 3 {
                vec!["log", "credentials", "log", "childPing"]
            } else {
                vec!["log", "credentials", "send", "log", "childPing"]
            }
        }
        "onClose" => {
            let mut v = vec!["diagnostics", "log", "clearPing", "clearSocket"];
            if ![2, 9].contains(&scenario) {
                v.push("timeout");
            }
            v
        }
        "ping" => {
            if scenario == 3 {
                vec!["timeout"]
            } else {
                vec!["send", "timeout"]
            }
        }
        "sendSubscription" => {
            if scenario == 1 {
                vec!["assets"]
            } else {
                vec!["assets", "send", "log"]
            }
        }
        "authenticate" => {
            if scenario == 3 {
                vec!["credentials", "log"]
            } else {
                vec!["credentials", "send", "log"]
            }
        }
        _ => panic!("fixed method floor"),
    };
    if let Some(index) = tags
        .iter()
        .position(|tag| *tag == failure_point(venue, method, scenario))
    {
        tags.truncate(index + 1);
    }
    tags
}
fn invoke(host: Rc<Host>, method: &str) -> Result<(), Error> {
    match (method, host.venue.as_str()) {
        ("onOpen", "public") => public_on_open(host.as_ref()),
        ("onOpen", _) => private_on_open(host.as_ref()),
        ("onClose", "public") => public_on_close(host.clone()),
        ("onClose", _) => private_on_close(host.clone()),
        ("ping", "public") => ping_public(host.clone(), &host.ping_slot),
        ("ping", _) => ping_private(host.clone(), &host.ping_slot),
        ("sendSubscription", _) => send_public_subscription(host.as_ref(), || host.assets()),
        ("authenticate", _) => authenticate_private(host.as_ref(), || host.credentials()),
        _ => panic!("fixedmethod"),
    }
}
fn check_native_frame(frame: &SubscriptionFrame, source: &Value, host: &Host, label: &str) {
    assert_eq!(
        frame.channel,
        if host.venue == "public" {
            "market"
        } else {
            "user"
        },
        "native subscription channel:{label}"
    );
    assert_eq!(
        source["channel"], frame.channel,
        "source subscription channel:{label}"
    );
    if host.venue == "public" {
        let assets = frame.assets_ids.as_ref().unwrap();
        assert_eq!(assets.len(), 3, "independent native asset count:{label}");
        assert_eq!(
            source["assets"].as_array().unwrap().len(),
            3,
            "independent source asset count:{label}"
        );
        for (index, expected) in [
            "base".encode_utf16().collect::<Vec<_>>(),
            host.strings[0].clone(),
            host.strings[1].clone(),
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(
                &assets[index], expected,
                "native ordered asset units:{label}"
            );
            assert_eq!(assets[index], units(&source["assets"][index]));
        }
        assert_eq!(
            frame.custom_feature_enabled,
            Some(true),
            "native custom feature:{label}"
        );
        assert_eq!(source["feature"], true);
        assert!(frame.auth.is_none());
        assert!(source["auth"].is_null());
    } else {
        assert!(frame.assets_ids.is_none());
        assert!(source["assets"].is_null());
        assert!(frame.custom_feature_enabled.is_none());
        assert!(source["feature"].is_null());
        let auth = frame.auth.as_ref().unwrap();
        for (index, (actual, key)) in [
            (&auth.api_key, "apiKey"),
            (&auth.secret, "secret"),
            (&auth.passphrase, "passphrase"),
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(
                *actual, &host.strings[index],
                "native auth field:{key}:{label}"
            );
            assert_eq!(**actual, units(&source["auth"][key]));
        }
    }
}
fn native_trace(host: &Host, source: &Value, tags: &[&str], method: &str, label: &str) {
    let trace = host.trace.borrow();
    assert_eq!(trace.len(), tags.len(), "native socket trace:{label}");
    assert_eq!(
        source.as_array().unwrap().len(),
        tags.len(),
        "independent source trace count:{label}"
    );
    for (index, event) in trace.iter().enumerate() {
        let src = &source[index];
        assert_eq!(event.tag, tags[index], "native socket trace:{label}");
        assert_eq!(src["tag"], event.tag, "source socket trace:{label}");
        if event.tag == "send" {
            if method == "ping" {
                assert!(event.frame.is_none());
                assert!(src["frame"].is_null());
            } else {
                check_native_frame(event.frame.as_ref().unwrap(), &src["frame"], host, label);
            }
        }
        if event.tag == "log" {
            assert_eq!(
                event.text.as_deref(),
                src["text"].as_str(),
                "native log text:{label}"
            );
            assert_eq!(
                event.severity,
                src["severity"].as_str(),
                "native log severity:{label}"
            );
        }
        if event.tag == "timeout" {
            let delay = event.delay.unwrap();
            assert_eq!(
                delay.to_bits(),
                if method == "ping" {
                    10000.0_f64
                } else {
                    1000.0_f64
                }
                .to_bits(),
                "native timeout delay:{label}"
            );
            assert_eq!(delay.to_bits(), src["delay"].as_f64().unwrap().to_bits());
        }
        if event.tag == "diagnostics" {
            assert_eq!(event.event_undefined, Some(true));
            assert_eq!(event.synthetic, Some(true));
            assert_eq!(src["eventUndefined"], true);
            assert_eq!(src["synthetic"], true);
        }
    }
}
fn check(venue: &str, method: &str) -> Vec<(String, String)> {
    let mut wire_pairs = Vec::new();
    let label = format!("{venue}.{method}");
    let cases: Vec<_> = oracle()
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["venue"] == venue && v["method"] == method)
        .collect();
    assert_eq!(cases.len(), 10, "independent per-method socket floor");
    for (scenario, case) in cases.iter().enumerate() {
        assert_eq!(case["scenario"], scenario);
        assert_eq!(case["failurePoint"], failure_point(venue, method, scenario));
        assert_eq!(case["strings"].as_array().unwrap().len(), 3);
        let failure = Rc::new(Failure {
            marker: 47,
            message: "original host failure",
        });
        let host = Rc::new(Host {
            venue: venue.into(),
            scenario,
            failure_point: failure_point(venue, method, scenario),
            failure: failure.clone(),
            callback_failure: Cell::new(false),
            disposed: Cell::new(scenario == 2),
            connected: Cell::new(scenario != 3),
            ping_slot: RefCell::new(Some(Rc::new(Timer { id: 17 }))),
            socket: Cell::new(Some(83)),
            strings: case["strings"]
                .as_array()
                .unwrap()
                .iter()
                .map(units)
                .collect(),
            trace: RefCell::new(vec![]),
            timers: RefCell::new(vec![]),
        });
        let mut expected_tags = vec![];
        assert_eq!(
            case["steps"].as_array().unwrap().len(),
            2,
            "independent repeated root floor"
        );
        for iteration in 0..2 {
            host.disposed.set(scenario == 2);
            host.connected.set(scenario != 3);
            let tags = root_tags(venue, method, scenario);
            let fails = tags.last().is_some_and(|tag| *tag == host.failure_point);
            let registers = tags.last() == Some(&"timeout") && !fails;
            let before_root_trace = host.trace.borrow().len();
            expected_tags.extend(tags);
            let result = invoke(host.clone(), method);
            assert_eq!(result.is_err(), fails, "native root failure:{label}");
            if let Err(actual) = result {
                assert!(
                    Rc::ptr_eq(&actual, &failure),
                    "native original failure:{label}"
                );
                assert_eq!(actual.marker, 47);
                assert_eq!(actual.message, "original host failure");
            }
            let source = &case["steps"][iteration];
            let callback = host
                .timers
                .borrow()
                .last()
                .map(|(_, callback)| callback.clone());
            let mut tick_count = 0;
            if let Some(callback) = callback {
                let reconnect = method == "onClose";
                for (disposed, connected) in
                    [(false, true), (false, false), (true, true), (true, false)]
                {
                    host.disposed.set(disposed);
                    host.connected.set(connected);
                    let before = host.trace.borrow().len();
                    assert!(callback().is_ok());
                    let count = usize::from(reconnect || (!disposed && connected));
                    assert_eq!(
                        host.trace.borrow().len() - before,
                        count,
                        "independent native callback guard:{label}"
                    );
                    if count == 1 {
                        expected_tags.push(if reconnect {
                            "childConnect"
                        } else {
                            "childPing"
                        });
                    }
                    assert_eq!(source["ticks"][tick_count]["disposed"], disposed);
                    assert_eq!(source["ticks"][tick_count]["connected"], connected);
                    assert_eq!(source["ticks"][tick_count]["error"], false);
                    tick_count += 1;
                }
                host.disposed.set(false);
                host.connected.set(true);
                host.callback_failure.set(true);
                let before = host.trace.borrow().len();
                let error = callback().expect_err("native scheduled failure");
                assert!(Rc::ptr_eq(&error, &failure));
                assert_eq!(host.trace.borrow().len() - before, 1);
                expected_tags.push(if reconnect {
                    "childConnect"
                } else {
                    "childPing"
                });
                assert_eq!(source["ticks"][tick_count]["disposed"], false);
                assert_eq!(source["ticks"][tick_count]["connected"], true);
                assert_eq!(source["ticks"][tick_count]["error"], true);
                tick_count += 1;
                host.callback_failure.set(false);
            }
            let old_tick = method == "ping" && iteration == 1 && registers;
            if old_tick {
                let old_callback = host.timers.borrow()[0].1.clone();
                host.disposed.set(false);
                host.connected.set(true);
                let before = host.trace.borrow().len();
                assert!(
                    old_callback().is_ok(),
                    "native old timeout callback completion"
                );
                assert_eq!(
                    host.trace.borrow().len() - before,
                    1,
                    "native old timeout executable after replacement"
                );
                expected_tags.push("childPing");
            }
            assert_eq!(
                source["oldTick"], old_tick,
                "source older timeout callback witness"
            );
            native_trace(&host, &source["trace"], &expected_tags, method, &label);
            assert_eq!(source["error"], fails);
            let count = if registers { iteration + 1 } else { 0 };
            assert_eq!(
                host.timers.borrow().len(),
                count,
                "independent native registered timeout count:{label}"
            );
            assert_eq!(source["registered"], count);
            assert_eq!(tick_count, if registers { 5 } else { 0 });
            assert_eq!(source["ticks"].as_array().unwrap().len(), tick_count);
            assert_eq!(
                host.ping_slot.borrow().as_ref().map(|h| u64::from(h.id)),
                source["pingHandle"].as_u64(),
                "native ping handle:{label}"
            );
            assert_eq!(
                host.socket.get().map(u64::from),
                source["socket"].as_u64(),
                "native socket field:{label}"
            );
            if method == "ping" && count == 2 {
                let timers = host.timers.borrow();
                assert_eq!(timers[0].0.id, 23, "native old timeout retained");
                assert_eq!(timers[1].0.id, 24, "native new timeout retained");
                assert!(
                    Rc::ptr_eq(host.ping_slot.borrow().as_ref().unwrap(), &timers[1].0),
                    "native stored latest timeout identity"
                );
            }
            for (index, event) in host
                .trace
                .borrow()
                .iter()
                .enumerate()
                .skip(before_root_trace)
            {
                if event.tag == "send" {
                    wire_pairs.push((
                        event.text.clone().unwrap(),
                        source["trace"][index]["text"].as_str().unwrap().into(),
                    ));
                }
            }
        }
        host.timers.borrow_mut().clear();
    }
    assert_eq!(cases[9]["scenario"], 9, "last socket scenario");
    wire_pairs
}
macro_rules! body_test {
    ($name:ident,$venue:literal,$method:literal) => {
        #[test]
        fn $name() {
            let _ = check($venue, $method);
        }
    };
}
body_test!(public_open, "public", "onOpen");
body_test!(public_close, "public", "onClose");
body_test!(public_ping, "public", "ping");
body_test!(public_subscription, "public", "sendSubscription");
body_test!(private_open, "private", "onOpen");
body_test!(private_close, "private", "onClose");
body_test!(private_ping, "private", "ping");
body_test!(private_auth, "private", "authenticate");

#[test]
fn wire_after_all_native_assertions() {
    let mut pairs = Vec::new();
    for (venue, method) in [
        ("public", "onOpen"),
        ("public", "onClose"),
        ("public", "ping"),
        ("public", "sendSubscription"),
        ("private", "onOpen"),
        ("private", "onClose"),
        ("private", "ping"),
        ("private", "authenticate"),
    ] {
        pairs.extend(check(venue, method));
    }
    assert_eq!(pairs.len(), 96, "independent unique wire-send floor");
    for (actual, source) in pairs {
        assert_eq!(
            actual, source,
            "wire send text after complete native corpus"
        );
    }
}
