use safe_cex_venue_mexc::ws_lifecycle::*;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    process::Command,
    rc::Rc,
    sync::OnceLock,
};
#[derive(Debug)]
struct Handle {
    id: u32,
}
#[derive(Debug)]
struct Failure {
    marker: u32,
    message: &'static str,
}
type Error = Rc<Failure>;
type Tick = Rc<dyn Fn() -> Result<(), Error>>;
#[derive(Debug)]
struct Event {
    tag: &'static str,
    handle: Option<u32>,
    text: Option<String>,
    period: Option<u64>,
    kind: Option<&'static str>,
}
impl Event {
    fn new(tag: &'static str) -> Self {
        Self {
            tag,
            handle: None,
            text: None,
            period: None,
            kind: None,
        }
    }
}
struct Host {
    disposed: Cell<bool>,
    connected: Cell<bool>,
    scenario: usize,
    failure_point: String,
    callback_failure: Cell<bool>,
    failure: Error,
    heartbeat: RefCell<Option<Rc<Handle>>>,
    refresh: RefCell<Option<Rc<Handle>>>,
    handler: RefCell<Rc<Handle>>,
    replacement: Rc<Handle>,
    ping_at: RefCell<f64>,
    trace: RefCell<Vec<Event>>,
    active: RefCell<BTreeMap<u32, Tick>>,
    registered: RefCell<Vec<Rc<Handle>>>,
}
impl Host {
    fn event(&self, event: Event) -> Result<(), Error> {
        let tag = event.tag;
        self.trace.borrow_mut().push(event);
        if self.failure_point == tag || (tag == "ping" && self.callback_failure.get()) {
            Err(self.failure.clone())
        } else {
            Ok(())
        }
    }
    fn hook(&self, tag: &'static str) -> Result<(), Error> {
        self.event(Event::new(tag))?;
        if tag == "reconcile" && self.scenario == 9 {
            self.disposed.set(true);
        }
        Ok(())
    }
    fn now(&self) -> Result<f64, Error> {
        self.event(Event::new("now"))?;
        if self.scenario == 9 {
            *self.ping_at.borrow_mut() = 999.0;
        }
        Ok(if self.scenario == 1 { -0.0 } else { 123.75 })
    }
    fn send(&self, text: &str) -> Result<(), Error> {
        let mut event = Event::new("send");
        event.text = Some(text.into());
        self.event(event)
    }
}
impl HeartbeatHost for Host {
    type Handle = Handle;
    type Error = Error;
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn is_connected(&self) -> bool {
        self.connected.get()
    }
    fn clear_interval(&self, handle: Option<Rc<Handle>>) -> Result<(), Error> {
        if let Some(handle) = &handle {
            assert!(
                Rc::ptr_eq(handle, self.heartbeat.borrow().as_ref().unwrap()),
                "native original interval identity"
            );
        }
        let mut event = Event::new("clear");
        event.handle = handle.as_ref().map(|h| h.id);
        self.event(event)?;
        if let Some(handle) = handle {
            self.active.borrow_mut().remove(&handle.id);
        }
        if self.scenario == 9 {
            *self.heartbeat.borrow_mut() = Some(self.replacement.clone());
            *self.refresh.borrow_mut() = Some(self.replacement.clone());
            *self.handler.borrow_mut() = self.replacement.clone();
        }
        Ok(())
    }
    fn set_interval(&self, period: u64, callback: Tick) -> Result<Rc<Handle>, Error> {
        let mut event = Event::new("interval");
        event.period = Some(period);
        self.event(event)?;
        let handle = Rc::new(Handle {
            id: 23 + self.registered.borrow().len() as u32,
        });
        self.registered.borrow_mut().push(handle.clone());
        self.active.borrow_mut().insert(handle.id, callback);
        Ok(handle)
    }
    fn ping(&self) -> Result<(), Error> {
        self.event(Event::new("ping"))
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
            100,
            "independent lifecycle ten-method ten-scenario floor"
        );
        assert_eq!(value[99]["venue"], "private");
        assert_eq!(value[99]["method"], "dispose");
        assert_eq!(value[99]["scenario"], 9, "last lifecycle fixture");
        value
    })
}
#[derive(Clone)]
enum CloseEvent {
    Undefined,
    Null,
    Record(Rc<Handle>),
}
fn stop(host: &Host, venue: &str) -> Result<(), Error> {
    if venue == "public" {
        stop_public_heartbeat(host, &host.heartbeat)
    } else {
        stop_private_heartbeat(host, &host.heartbeat)
    }
}
fn start(host: Rc<Host>, venue: &str) -> Result<(), Error> {
    if venue == "public" {
        start_public_heartbeat(host.clone(), &host.heartbeat)
    } else {
        start_private_heartbeat(host.clone(), &host.heartbeat)
    }
}
fn invoke(host: Rc<Host>, venue: &str, method: &str, close: &CloseEvent) -> Result<(), Error> {
    let closing = |event: CloseEvent| {
        let mut trace = Event::new("onClose");
        match (event, close) {
            (CloseEvent::Undefined, CloseEvent::Undefined) => trace.kind = Some("undefined"),
            (CloseEvent::Null, CloseEvent::Null) => trace.kind = Some("null"),
            (CloseEvent::Record(value), CloseEvent::Record(original)) => {
                assert!(
                    Rc::ptr_eq(&value, original),
                    "native original close event identity"
                );
                trace.kind = Some("record");
                trace.handle = Some(value.id);
            }
            _ => panic!("native close event kind changed"),
        }
        host.event(trace)?;
        Ok(73)
    };
    match method {
        "stopHeartbeat" => stop(&host, venue),
        "startHeartbeat" => start(host.clone(), venue),
        "onOpen" => public_on_open(
            host.disposed.get(),
            || start(host.clone(), venue),
            || host.hook("reconcile"),
            || host.hook("resubscribe"),
            || host.hook("reconnected"),
        ),
        "ping" => ping_public(
            host.disposed.get(),
            &host.ping_at,
            || host.now(),
            |text| host.send(text),
        ),
        "handleClose" => {
            if venue == "public" {
                handle_public_close(|| stop(&host, venue), closing, close.clone())
            } else {
                handle_private_close(|| stop(&host, venue), closing, close.clone())
            }
        }
        "dispose" if venue == "public" => dispose_public(
            || stop(&host, venue),
            |name, actual: Rc<Handle>| {
                assert_eq!(
                    name, "activeTickerSymbols:changed",
                    "native removed event name"
                );
                assert!(
                    Rc::ptr_eq(&actual, &host.handler.borrow()),
                    "native original current handler identity"
                );
                let mut event = Event::new("off");
                event.handle = Some(actual.id);
                event.text = Some(name.into());
                host.event(event)
            },
            || host.handler.borrow().clone(),
            || {
                host.hook("base")?;
                Ok(73)
            },
        ),
        "dispose" => dispose_private(
            || stop(&host, venue),
            &host.refresh,
            |actual| {
                match (&actual, &*host.refresh.borrow()) {
                    (Some(a), Some(b)) => {
                        assert!(Rc::ptr_eq(a, b), "native current refresh identity")
                    }
                    (None, None) => {}
                    _ => panic!("native refresh kind changed"),
                }
                let mut event = Event::new("timeout");
                event.handle = actual.as_ref().map(|h| h.id);
                host.event(event)
            },
            || {
                host.hook("base")?;
                Ok(73)
            },
        ),
        _ => panic!("unknown method"),
    }
}
fn expected_root_tags(
    venue: &str,
    method: &str,
    disposed: bool,
    failure: &str,
) -> Vec<&'static str> {
    let sequence: Vec<_> = match method {
        "stopHeartbeat" => vec!["clear"],
        "startHeartbeat" => vec!["clear", "ping", "interval"],
        "onOpen" if disposed => vec![],
        "onOpen" => vec![
            "clear",
            "ping",
            "interval",
            "reconcile",
            "resubscribe",
            "reconnected",
        ],
        "ping" if disposed => vec![],
        "ping" => vec!["now", "send"],
        "handleClose" => vec!["clear", "onClose"],
        "dispose" if venue == "public" => vec!["clear", "off", "base"],
        "dispose" => vec!["clear", "timeout", "base"],
        _ => panic!("fixed method floor"),
    };
    match sequence.iter().position(|tag| *tag == failure) {
        Some(index) => sequence[..=index].to_vec(),
        None => sequence,
    }
}
fn check_trace(host: &Host, source: &Value, expected_tags: &[&str], label: &str) {
    let trace = host.trace.borrow();
    assert_eq!(
        trace.len(),
        expected_tags.len(),
        "native lifecycle trace:{label}"
    );
    assert_eq!(
        source.as_array().unwrap().len(),
        expected_tags.len(),
        "independent source lifecycle count:{label}"
    );
    for (index, event) in trace.iter().enumerate() {
        assert_eq!(
            event.tag, expected_tags[index],
            "native lifecycle trace:{label}"
        );
        assert_eq!(
            event.tag,
            source[index]["tag"].as_str().unwrap(),
            "native lifecycle trace:{label}"
        );
        assert_eq!(
            event.handle.map(u64::from),
            source[index]["handle"].as_u64(),
            "native lifecycle handle:{label}"
        );
        assert_eq!(
            event.text.as_deref(),
            source[index]["text"].as_str(),
            "native lifecycle text:{label}"
        );
        assert_eq!(
            event.kind,
            source[index]["kind"].as_str(),
            "native close event shape:{label}"
        );
        if let Some(period) = event.period {
            assert_eq!(period, 8000, "native heartbeat period:{label}");
        }
        assert_eq!(
            event.period,
            source[index]["period"].as_u64(),
            "native heartbeat period:{label}"
        );
    }
}
fn failure_point(venue: &str, method: &str, scenario: usize) -> &'static str {
    match scenario {
        4 => "clear",
        5 => "ping",
        6 => "interval",
        7 => match method {
            "onOpen" => "reconcile",
            "ping" => "send",
            "handleClose" => "onClose",
            _ => "base",
        },
        8 => match method {
            "onOpen" => "resubscribe",
            "ping" => "now",
            "dispose" if venue == "public" => "off",
            "dispose" => "timeout",
            _ => "clear",
        },
        _ => "",
    }
}
fn check(venue: &str, method: &str) {
    let label = format!("{venue}.{method}");
    let cases: Vec<_> = oracle()
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["venue"] == venue && case["method"] == method)
        .collect();
    assert_eq!(cases.len(), 10, "independent per-method lifecycle floor");
    for (scenario, case) in cases.iter().enumerate() {
        assert_eq!(case["scenario"], scenario);
        assert_eq!(case["initialDisposed"], scenario == 2);
        assert_eq!(case["initialConnected"], scenario != 3);
        assert_eq!(
            case["failurePoint"],
            failure_point(venue, method, scenario),
            "independent scenario failure point"
        );
        let failure = Rc::new(Failure {
            marker: 47,
            message: "original host failure",
        });
        let host = Rc::new(Host {
            disposed: Cell::new(scenario == 2),
            connected: Cell::new(scenario != 3),
            scenario,
            failure_point: failure_point(venue, method, scenario).into(),
            callback_failure: Cell::new(false),
            failure: failure.clone(),
            heartbeat: RefCell::new((scenario != 1).then(|| Rc::new(Handle { id: 17 }))),
            refresh: RefCell::new((scenario != 1).then(|| Rc::new(Handle { id: 29 }))),
            handler: RefCell::new(Rc::new(Handle { id: 41 })),
            replacement: Rc::new(Handle { id: 31 }),
            ping_at: RefCell::new(111.5),
            trace: RefCell::new(vec![]),
            active: RefCell::new(BTreeMap::new()),
            registered: RefCell::new(vec![]),
        });
        let close = if scenario == 1 {
            CloseEvent::Undefined
        } else if scenario == 3 {
            CloseEvent::Null
        } else {
            CloseEvent::Record(Rc::new(Handle { id: 83 }))
        };
        let mut expected_tags = vec![];
        assert_eq!(
            case["steps"].as_array().unwrap().len(),
            2,
            "independent repeated lifecycle calls"
        );
        for iteration in 0..2 {
            host.disposed.set(scenario == 2);
            host.connected.set(scenario != 3);
            let tags = expected_root_tags(venue, method, scenario == 2, &host.failure_point);
            let should_fail = tags.last().is_some_and(|tag| *tag == host.failure_point);
            expected_tags.extend_from_slice(&tags);
            let result = invoke(host.clone(), venue, method, &close);
            assert_eq!(
                result.is_err(),
                should_fail,
                "native lifecycle failure:{label}"
            );
            if let Err(error) = result {
                assert!(
                    Rc::ptr_eq(&error, &failure),
                    "native original host failure:{label}"
                );
                assert_eq!(error.marker, 47);
                assert_eq!(error.message, "original host failure");
            }
            let source = &case["steps"][iteration];
            let callback = host
                .heartbeat
                .borrow()
                .as_ref()
                .and_then(|h| host.active.borrow().get(&h.id).cloned());
            let mut ticks = 0;
            if let Some(callback) = callback {
                for (disposed, connected) in
                    [(false, true), (false, false), (true, true), (true, false)]
                {
                    host.disposed.set(disposed);
                    host.connected.set(connected);
                    let before = host.trace.borrow().len();
                    let result = callback();
                    let sends = usize::from(!disposed && connected);
                    assert_eq!(
                        host.trace.borrow().len() - before,
                        sends,
                        "independent native live callback guard:{label}"
                    );
                    if sends == 1 {
                        expected_tags.push("ping");
                    }
                    assert!(result.is_ok(), "native callback completion:{label}");
                    assert_eq!(
                        source["ticks"][ticks]["disposed"], disposed,
                        "source callback current disposed flag"
                    );
                    assert_eq!(
                        source["ticks"][ticks]["connected"], connected,
                        "source callback current connected flag"
                    );
                    assert_eq!(
                        source["ticks"][ticks]["error"], false,
                        "source callback completion"
                    );
                    ticks += 1;
                }
                host.disposed.set(false);
                host.connected.set(true);
                host.callback_failure.set(true);
                let before = host.trace.borrow().len();
                let error = callback().expect_err("native scheduled failure required");
                assert!(
                    Rc::ptr_eq(&error, &failure),
                    "native scheduled failure identity"
                );
                assert_eq!(
                    host.trace.borrow().len() - before,
                    1,
                    "independent native scheduled failure count"
                );
                expected_tags.push("ping");
                assert_eq!(source["ticks"][ticks]["disposed"], false);
                assert_eq!(source["ticks"][ticks]["connected"], true);
                assert_eq!(
                    source["ticks"][ticks]["error"], true,
                    "source scheduled failure completion"
                );
                host.callback_failure.set(false);
                ticks += 1;
            }
            check_trace(&host, &source["trace"], &expected_tags, &label);
            assert_eq!(
                source["error"], should_fail,
                "independent source error:{label}"
            );
            let expect_registration = (method == "startHeartbeat"
                || (method == "onOpen" && scenario != 2))
                && !["clear", "ping", "interval"].contains(&host.failure_point.as_str());
            let expected_registered = if expect_registration {
                iteration + 1
            } else {
                0
            };
            assert_eq!(
                host.registered.borrow().len(),
                expected_registered,
                "independent native registration count:{label}"
            );
            assert_eq!(
                source["registered"], expected_registered,
                "independent source registration count:{label}"
            );
            assert_eq!(
                ticks,
                if expect_registration { 5 } else { 0 },
                "independent native callback floor:{label}"
            );
            assert_eq!(
                source["ticks"].as_array().unwrap().len(),
                ticks,
                "source callback floor:{label}"
            );
            assert_eq!(
                host.heartbeat.borrow().as_ref().map(|h| u64::from(h.id)),
                source["heartbeat"].as_u64(),
                "native final heartbeat handle:{label}"
            );
            assert_eq!(
                host.refresh.borrow().as_ref().map(|h| u64::from(h.id)),
                source["refresh"].as_u64(),
                "native retained refresh property:{label}"
            );
            assert_eq!(
                u64::from(host.handler.borrow().id),
                source["handler"].as_u64().unwrap(),
                "native current handler property:{label}"
            );
            assert_eq!(
                host.ping_at.borrow().to_bits(),
                u64::from_str_radix(source["pingBits"].as_str().unwrap(), 16).unwrap(),
                "native ping timestamp bits:{label}"
            );
        }
        // Fixture cleanup of the host registry, after every measured assertion.
        host.active.borrow_mut().clear();
    }
    assert_eq!(cases[9]["scenario"], 9, "last per-method lifecycle case");
}
macro_rules! lifecycle_test {
    ($name:ident,$venue:literal,$method:literal) => {
        #[test]
        fn $name() {
            check($venue, $method);
        }
    };
}
lifecycle_test!(public_start, "public", "startHeartbeat");
lifecycle_test!(public_stop, "public", "stopHeartbeat");
lifecycle_test!(public_close, "public", "handleClose");
lifecycle_test!(public_open, "public", "onOpen");
lifecycle_test!(public_ping, "public", "ping");
lifecycle_test!(public_dispose, "public", "dispose");
lifecycle_test!(private_start, "private", "startHeartbeat");
lifecycle_test!(private_stop, "private", "stopHeartbeat");
lifecycle_test!(private_close, "private", "handleClose");
lifecycle_test!(private_dispose, "private", "dispose");
