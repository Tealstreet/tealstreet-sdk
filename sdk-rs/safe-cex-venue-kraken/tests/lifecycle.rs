use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_kraken::lifecycle::{
    self, ConnectionHost, ConnectionSocket, DisposalHost, SocketCallback,
};
use serde_json::Value;
use std::{
    any::Any,
    cell::{Cell, RefCell},
    process::Command,
    rc::{Rc, Weak},
    sync::OnceLock,
};

fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/lifecycle-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["header"]["hashes"].as_object().unwrap().len(), 5);
        value
    })
}
fn flag(value: &Value, key: &str) -> bool {
    value[key].as_bool().unwrap_or(false)
}
fn units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u16)
        .collect()
}

#[derive(Debug)]
enum Event {
    Create(String),
    Add(u32, String, bool),
    Callback(String, bool),
    Ready(u32),
    SocketClose(u32),
    HostClose,
    Timeout(u32, bool),
    Inactive(Vec<u16>, bool),
    Clear(&'static str),
    Base(usize, usize, usize),
}
fn assert_events(events: &[Event], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(
        events.len(),
        expected.len(),
        "LIFECYCLE native trace length"
    );
    for (event, value) in events.iter().zip(expected) {
        match event {
            Event::Create(url) => {
                assert_eq!(value["kind"], "create");
                assert_eq!(url, value["url"].as_str().unwrap(), "CONNECT native URL");
            }
            Event::Add(id, name, identity) => {
                assert_eq!(value["kind"], "add");
                assert_eq!(
                    *id,
                    value["id"].as_u64().unwrap() as u32,
                    "CONNECT native registration socket"
                );
                assert_eq!(
                    name,
                    value["event"].as_str().unwrap(),
                    "CONNECT native listener order"
                );
                assert!(*identity, "CONNECT native listener identity");
                assert_eq!(value["identity"], true);
            }
            Event::Callback(name, identity) => {
                assert_eq!(value["kind"], "callback");
                assert_eq!(name, value["event"].as_str().unwrap());
                assert!(*identity, "CONNECT native event identity");
                assert_eq!(value["identity"], true);
            }
            Event::Ready(id) | Event::SocketClose(id) => {
                assert_eq!(
                    value["kind"],
                    if matches!(event, Event::Ready(_)) {
                        "ready"
                    } else {
                        "socket-close"
                    }
                );
                assert_eq!(
                    *id,
                    value["id"].as_u64().unwrap() as u32,
                    "CONNECT native current socket"
                );
            }
            Event::HostClose => assert_eq!(
                value["kind"], "host-close",
                "CONNECT native close continuation"
            ),
            Event::Timeout(id, identity) => {
                assert_eq!(value["kind"], "timeout");
                assert_eq!(
                    *id,
                    value["id"].as_u64().unwrap() as u32,
                    "DISPOSE native timer identity"
                );
                assert!(*identity);
                assert_eq!(value["identity"], true);
            }
            Event::Inactive(key, inactive) => {
                assert_eq!(value["kind"], "inactive");
                assert_eq!(*key, units(&value["key"]), "DISPOSE native live key");
                assert!(!*inactive, "DISPOSE native inactive flag");
                assert_eq!(value["value"], false);
            }
            Event::Clear(map) => {
                assert_eq!(value["kind"], "clear");
                assert_eq!(
                    *map,
                    value["map"].as_str().unwrap(),
                    "DISPOSE native clear order"
                );
            }
            Event::Base(t, a, c) => {
                assert_eq!(value["kind"], "base");
                assert_eq!(
                    (*t, *a, *c),
                    (
                        value["timeouts"].as_u64().unwrap() as usize,
                        value["active"].as_u64().unwrap() as usize,
                        value["candles"].as_u64().unwrap() as usize
                    ),
                    "DISPOSE native base cleanup state"
                );
            }
        }
    }
}
fn assert_result(result: Result<(), ExchangeError>, case: &Value, pointer: usize) -> bool {
    match result {
        Ok(()) => {
            assert!(case["failure"].is_null(), "LIFECYCLE native success");
            false
        }
        Err(error) => {
            assert_eq!(error.name, case["failure"]["name"]);
            assert_eq!(error.message, case["failure"]["message"]);
            if case["failure"]["identity"] == true {
                assert_eq!(
                    (&*error.metadata as *const _) as usize,
                    pointer,
                    "LIFECYCLE native original error identity"
                );
            } else {
                assert_eq!(error.name, "TypeError");
            }
            true
        }
    }
}

struct Host {
    owner: Weak<Host>,
    config: Value,
    disposed: Cell<bool>,
    slot: RefCell<Option<Rc<Socket>>>,
    events: RefCell<Vec<Event>>,
    listeners: RefCell<Vec<(String, SocketCallback)>>,
    open: SocketCallback,
    message: SocketCallback,
    error: RefCell<Option<ExchangeError>>,
    pointer: usize,
    input: Rc<f64>,
}
struct Socket {
    id: u32,
    host: Weak<Host>,
}
impl Host {
    fn new(config: Value) -> Rc<Self> {
        let mut error = ExchangeError::new("connection failure");
        error.name = "ConnectionError".into();
        let pointer = (&*error.metadata as *const _) as usize;
        let input = Rc::new(17.25);
        let host = Rc::new_cyclic(|owner: &Weak<Host>| {
            let callback = |name: &'static str| {
                let owner = owner.clone();
                let input = input.clone();
                Rc::new(move |value: &dyn Any| {
                    let host = owner.upgrade().unwrap();
                    host.events.borrow_mut().push(Event::Callback(
                        name.into(),
                        value
                            .downcast_ref::<f64>()
                            .is_some_and(|v| std::ptr::eq(v, input.as_ref())),
                    ));
                    host.fail(name)
                }) as SocketCallback
            };
            Self {
                owner: owner.clone(),
                disposed: Cell::new(flag(&config, "disposed")),
                config,
                slot: RefCell::new(None),
                events: RefCell::new(vec![]),
                listeners: RefCell::new(vec![]),
                open: callback("open"),
                message: callback("message"),
                error: RefCell::new(Some(error)),
                pointer,
                input: input.clone(),
            }
        });
        if flag(&host.config, "old") {
            host.replace(7);
        }
        host
    }
    fn replace(&self, id: u32) {
        *self.slot.borrow_mut() = Some(Rc::new(Socket {
            id,
            host: self.owner.clone(),
        }));
    }
    fn fail(&self, point: &str) -> Result<(), ExchangeError> {
        if self.config["fail"] == point {
            return Err(self
                .error
                .borrow_mut()
                .take()
                .expect("original error used once"));
        }
        Ok(())
    }
}
impl ConnectionSocket for Socket {
    fn ready_state(&self) -> Result<f64, ExchangeError> {
        let h = self.host.upgrade().unwrap();
        h.events.borrow_mut().push(Event::Ready(self.id));
        h.fail("ready")?;
        if flag(&h.config, "replaceReady") {
            h.replace(41);
        }
        if flag(&h.config, "clearReady") {
            h.slot.borrow_mut().take();
        }
        Ok(if flag(&h.config, "closed") {
            3.0
        } else if flag(&h.config, "nan") {
            f64::NAN
        } else {
            1.0
        })
    }
    fn close(&self) -> Result<(), ExchangeError> {
        let h = self.host.upgrade().unwrap();
        h.events.borrow_mut().push(Event::SocketClose(self.id));
        h.fail("socket-close")
    }
    fn add_event_listener(
        &self,
        event: &str,
        callback: SocketCallback,
    ) -> Result<(), ExchangeError> {
        let h = self.host.upgrade().unwrap();
        let identity = match event {
            "open" => Rc::ptr_eq(&callback, &h.open),
            "message" => Rc::ptr_eq(&callback, &h.message),
            _ => true,
        };
        h.events
            .borrow_mut()
            .push(Event::Add(self.id, event.into(), identity));
        h.listeners.borrow_mut().push((event.into(), callback));
        h.fail(&format!("add-{event}"))?;
        if h.config["replaceAdd"] == event {
            h.replace(37);
        }
        if h.config["clearAdd"] == event {
            h.slot.borrow_mut().take();
        }
        Ok(())
    }
}
impl ConnectionHost for Host {
    type Socket = Socket;
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn testnet(&self) -> bool {
        flag(&self.config, "testnet")
    }
    fn socket(&self) -> &RefCell<Option<Rc<Socket>>> {
        &self.slot
    }
    fn create_websocket(&self, url: &str) -> Result<Rc<Socket>, ExchangeError> {
        self.events.borrow_mut().push(Event::Create(url.into()));
        self.fail("create")?;
        if flag(&self.config, "disposeCreate") {
            self.disposed.set(true);
        }
        Ok(Rc::new(Socket {
            id: 31,
            host: self.owner.clone(),
        }))
    }
    fn on_open(&self) -> SocketCallback {
        if flag(&self.config, "replaceOpen") {
            self.replace(43);
        }
        self.open.clone()
    }
    fn on_message(&self) -> SocketCallback {
        self.message.clone()
    }
    fn on_close(&self) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::HostClose);
        self.fail("host-close")
    }
}
fn connection(family: &str, connect: fn(Rc<Host>) -> Result<(), ExchangeError>) {
    let cases = oracle()["connections"][family].as_array().unwrap();
    assert_eq!(cases.len(), 25, "CONNECT independent cases");
    let mut event_count = 0;
    let mut failures = 0;
    for case in cases {
        let host = Host::new(case["config"].clone());
        let result = connect(host.clone()).and_then(|()| {
            if flag(&host.config, "clearFire") {
                host.slot.borrow_mut().take();
            }
            if flag(&host.config, "replaceFire") {
                host.replace(39);
            }
            for name in ["open", "message", "close", "error"] {
                let callback = host
                    .listeners
                    .borrow()
                    .iter()
                    .find(|(event, _)| event == name)
                    .map(|(_, cb)| cb.clone());
                if let Some(callback) = callback {
                    callback(host.input.as_ref())?;
                }
            }
            Ok(())
        });
        failures += usize::from(assert_result(result, case, host.pointer));
        assert_eq!(
            host.slot.borrow().as_ref().map(|s| s.id),
            case["slot"].as_u64().map(|id| id as u32),
            "CONNECT native final slot"
        );
        assert_eq!(host.disposed.get(), case["disposed"].as_bool().unwrap());
        let events = host.events.borrow();
        assert_events(&events, &case["trace"]);
        event_count += events.len();
    }
    assert_eq!(event_count, 187, "CONNECT independent event floor");
    assert_eq!(failures, 9, "CONNECT independent failures");
}
#[test]
fn futures_public_connect_live_source() {
    connection("FuturesPublic", lifecycle::futures_public_connect)
}
#[test]
fn futures_private_connect_live_source() {
    connection("FuturesPrivate", lifecycle::futures_private_connect)
}
#[test]
fn spot_public_connect_live_source() {
    connection("SpotPublic", lifecycle::spot_public_connect)
}
#[test]
fn spot_private_connect_live_source() {
    connection("SpotPrivate", lifecycle::spot_private_connect)
}

struct Disposal {
    config: Value,
    timeouts: RefCell<Vec<Option<Rc<u32>>>>,
    active: RefCell<Vec<Option<(Rc<Vec<u16>>, bool)>>>,
    candles: Cell<usize>,
    events: RefCell<Vec<Event>>,
    error: RefCell<Option<ExchangeError>>,
    pointer: usize,
    mutated_timeout: Cell<bool>,
    mutated_active: Cell<bool>,
    held: Vec<Rc<u32>>,
}
struct TimeoutIterator {
    host: Rc<Disposal>,
    cursor: usize,
}
struct KeyIterator {
    host: Rc<Disposal>,
    cursor: usize,
}
impl Iterator for TimeoutIterator {
    type Item = Result<Rc<u32>, ExchangeError>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.host.timeouts.borrow().get(self.cursor).cloned()?;
            self.cursor += 1;
            if let Some(item) = item {
                return Some(Ok(item));
            }
        }
    }
}
impl Iterator for KeyIterator {
    type Item = Result<Rc<Vec<u16>>, ExchangeError>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.host.active.borrow().get(self.cursor).cloned()?;
            self.cursor += 1;
            if let Some((key, _)) = item {
                return Some(Ok(key));
            }
        }
    }
}
impl Disposal {
    fn new(config: Value) -> Rc<Self> {
        let mut error = ExchangeError::new("disposal failure");
        error.name = "DisposalError".into();
        let pointer = (&*error.metadata as *const _) as usize;
        let held = if flag(&config, "empty") {
            vec![]
        } else {
            vec![Rc::new(17), Rc::new(23)]
        };
        let active = if flag(&config, "empty") {
            vec![]
        } else {
            vec![
                Some((Rc::new(vec![38634]), true)),
                Some((Rc::new(vec![0xd800]), true)),
            ]
        };
        Rc::new(Self {
            candles: Cell::new(usize::from(!flag(&config, "empty"))),
            timeouts: RefCell::new(held.iter().cloned().map(Some).collect()),
            active: RefCell::new(active),
            config,
            events: RefCell::new(vec![]),
            error: RefCell::new(Some(error)),
            pointer,
            mutated_timeout: Cell::new(false),
            mutated_active: Cell::new(false),
            held,
        })
    }
    fn fail(&self, point: &str) -> Result<(), ExchangeError> {
        if self.config["fail"] == point {
            Err(self.error.borrow_mut().take().unwrap())
        } else {
            Ok(())
        }
    }
    fn clear(&self, name: &'static str) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::Clear(name));
        self.fail(&format!("clear-{name}"))
    }
}
struct DisposalBinding(Rc<Disposal>);
impl std::ops::Deref for DisposalBinding {
    type Target = Disposal;
    fn deref(&self) -> &Disposal {
        &self.0
    }
}
impl DisposalHost for DisposalBinding {
    type Timeout = u32;
    type Key = Vec<u16>;
    type Timeouts = TimeoutIterator;
    type Keys = KeyIterator;
    fn polling_timeouts(&self) -> Result<TimeoutIterator, ExchangeError> {
        Ok(TimeoutIterator {
            host: self.0.clone(),
            cursor: 0,
        })
    }
    fn clear_timeout(&self, timeout: &Rc<u32>) -> Result<(), ExchangeError> {
        let identity = self
            .held
            .iter()
            .any(|original| Rc::ptr_eq(original, timeout))
            || self
                .timeouts
                .borrow()
                .iter()
                .flatten()
                .any(|original| Rc::ptr_eq(original, timeout));
        self.events
            .borrow_mut()
            .push(Event::Timeout(**timeout, identity));
        self.fail("timeout")?;
        if flag(&self.config, "mutateTimeout") && !self.mutated_timeout.replace(true) {
            let mut timers = self.timeouts.borrow_mut();
            timers[1] = None;
            timers.push(Some(Rc::new(31)));
        }
        Ok(())
    }
    fn active_keys(&self) -> Result<KeyIterator, ExchangeError> {
        Ok(KeyIterator {
            host: self.0.clone(),
            cursor: 0,
        })
    }
    fn set_inactive(&self, key: &Rc<Vec<u16>>) -> Result<(), ExchangeError> {
        self.events
            .borrow_mut()
            .push(Event::Inactive((**key).clone(), false));
        self.fail("inactive")?;
        let mut active = self.active.borrow_mut();
        let (_, value) = active
            .iter_mut()
            .flatten()
            .find(|(original, _)| original.as_ref() == key.as_ref())
            .unwrap();
        *value = false;
        if flag(&self.config, "mutateActive") && !self.mutated_active.replace(true) {
            active[1] = None;
            active.push(Some((Rc::new("added".encode_utf16().collect()), true)));
        }
        Ok(())
    }
    fn clear_timeouts(&self) -> Result<(), ExchangeError> {
        self.clear("timeouts")?;
        self.timeouts.borrow_mut().clear();
        Ok(())
    }
    fn clear_active(&self) -> Result<(), ExchangeError> {
        self.clear("active")?;
        self.active.borrow_mut().clear();
        Ok(())
    }
    fn clear_candles(&self) -> Result<(), ExchangeError> {
        self.clear("candles")?;
        self.candles.set(0);
        Ok(())
    }
    fn dispose_base(&self) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::Base(
            self.timeouts.borrow().iter().flatten().count(),
            self.active.borrow().iter().flatten().count(),
            self.candles.get(),
        ));
        self.fail("base")
    }
}
#[test]
fn futures_dispose_live_source() {
    let cases = oracle()["disposal"].as_array().unwrap();
    assert_eq!(cases.len(), 11, "DISPOSE independent cases");
    let mut events = 0;
    let mut failures = 0;
    for case in cases {
        let host = Disposal::new(case["config"].clone());
        failures += usize::from(assert_result(
            lifecycle::futures_dispose(&DisposalBinding(host.clone())),
            case,
            host.pointer,
        ));
        let trace = host.events.borrow();
        assert_events(&trace, &case["trace"]);
        events += trace.len();
        let timers: Vec<u32> = host
            .timeouts
            .borrow()
            .iter()
            .flatten()
            .map(|id| **id)
            .collect();
        assert_eq!(
            timers,
            case["timeouts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect::<Vec<_>>(),
            "DISPOSE native retained timers"
        );
        let active = host.active.borrow();
        let rows: Vec<_> = active.iter().flatten().collect();
        assert_eq!(rows.len(), case["active"].as_array().unwrap().len());
        for ((key, value), expected) in rows.iter().zip(case["active"].as_array().unwrap()) {
            assert_eq!(key.as_ref(), &units(&expected["key"]));
            assert_eq!(*value, expected["value"].as_bool().unwrap());
        }
        assert_eq!(
            host.candles.get(),
            case["candles"].as_u64().unwrap() as usize,
            "DISPOSE native retained candles"
        );
    }
    assert_eq!(events, 66, "DISPOSE independent event floor");
    assert_eq!(failures, 6, "DISPOSE independent failures");
}
