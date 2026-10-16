use safe_cex_venue_bitget::stream_lifecycle::{
    self as body, PrivateStreamHealthUpdate, StreamLifecycleHost, StreamSocket, StreamTimeout,
    UndefinedProperty,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};
#[derive(Clone, Debug)]
struct Case {
    op: &'static str,
    write: bool,
    reconcile: bool,
    running: bool,
    fail: i32,
}
impl Case {
    fn input(&self) -> Value {
        json!({"op":self.op,"write":self.write,"reconcile":self.reconcile,"running":self.running,"fail":self.fail})
    }
    fn expected_trace(&self) -> Vec<String> {
        let mut events: Vec<String> = Vec::new();
        let mut timer = |name: &str, present: bool| {
            events.push(format!("get:{name}:1"));
            if present {
                events.extend([
                    format!("get:{name}:2"),
                    format!("clear:{name}"),
                    format!("unset:{name}"),
                ]);
            }
        };
        match self.op {
            "message" => {
                return [
                    "degraded:false",
                    "now",
                    "messageAt",
                    "warned:false",
                    "health",
                    "stop",
                ]
                .map(str::to_owned)
                .to_vec();
            }
            "dispose" => {
                timer("write", self.write);
                timer("reconcile", self.reconcile);
                events.insert(0, "super".into());
                events.extend(
                    [
                        "socket:public",
                        "socket:private",
                        "socket:seconds",
                        "socket:third",
                    ]
                    .map(str::to_owned),
                );
            }
            "start" => {
                events.push("running".into());
                if !self.running {
                    events.push("get:reconcile:1".into());
                    if !self.reconcile {
                        events.push("launch".into());
                    }
                }
            }
            "stop" => timer("reconcile", self.reconcile),
            _ => unreachable!(),
        }
        events
    }
}
fn cases() -> Vec<Case> {
    let mut scenarios = vec![Case {
        op: "message",
        write: false,
        reconcile: false,
        running: false,
        fail: -1,
    }];
    for write in [false, true] {
        for reconcile in [false, true] {
            scenarios.push(Case {
                op: "dispose",
                write,
                reconcile,
                running: false,
                fail: -1,
            });
        }
    }
    for running in [false, true] {
        for reconcile in [false, true] {
            scenarios.push(Case {
                op: "start",
                write: false,
                reconcile,
                running,
                fail: -1,
            });
        }
    }
    for reconcile in [false, true] {
        scenarios.push(Case {
            op: "stop",
            write: false,
            reconcile,
            running: false,
            fail: -1,
        });
    }
    let mut cases = Vec::new();
    for scenario in scenarios {
        for fail in -1..scenario.expected_trace().len() as i32 {
            cases.push(Case {
                fail,
                ..scenario.clone()
            });
        }
    }
    assert_eq!(cases.len(), 69, "STREAM independent native floor");
    cases
}
#[derive(Debug)]
struct Atom {
    truthy: bool,
    slot: Option<StreamTimeout>,
    units: Vec<u16>,
}
struct Host {
    case: Case,
    trace: RefCell<Vec<String>>,
    present: [Cell<bool>; 2],
    reads: [Cell<usize>; 2],
    guards: [Rc<Atom>; 2],
    arguments: [Rc<Atom>; 2],
    stamp: Rc<Atom>,
    error: Rc<Atom>,
    cleared: RefCell<Vec<Rc<Atom>>>,
    sockets: RefCell<Vec<StreamSocket>>,
    degraded: Cell<bool>,
    warned: Cell<bool>,
    last: RefCell<Option<Rc<Atom>>>,
    super_disposed: Cell<bool>,
    health: Cell<Option<PrivateStreamHealthUpdate>>,
    launched: Cell<u32>,
    stopped: Cell<u32>,
}
fn slot(slot: StreamTimeout) -> (usize, &'static str) {
    match slot {
        StreamTimeout::WriteCheck => (0, "write"),
        StreamTimeout::DegradedReconcile => (1, "reconcile"),
    }
}
fn socket_name(socket: StreamSocket) -> &'static str {
    match socket {
        StreamSocket::Public => "public",
        StreamSocket::Private => "private",
        StreamSocket::SecondsTickers => "seconds",
        StreamSocket::ThirdTickers => "third",
    }
}
impl Host {
    fn new(case: Case) -> Self {
        let atom = |truthy, slot| {
            Rc::new(Atom {
                truthy,
                slot,
                units: vec![0x41, 0xd800, 0x42, 0xdc00],
            })
        };
        Self {
            present: [Cell::new(case.write), Cell::new(case.reconcile)],
            guards: [
                atom(case.write, Some(StreamTimeout::WriteCheck)),
                atom(case.reconcile, Some(StreamTimeout::DegradedReconcile)),
            ],
            arguments: [
                atom(true, Some(StreamTimeout::WriteCheck)),
                atom(true, Some(StreamTimeout::DegradedReconcile)),
            ],
            stamp: atom(true, None),
            error: atom(true, None),
            case,
            trace: RefCell::default(),
            reads: [Cell::new(0), Cell::new(0)],
            cleared: RefCell::default(),
            sockets: RefCell::default(),
            degraded: Cell::new(true),
            warned: Cell::new(true),
            last: RefCell::default(),
            super_disposed: Cell::new(false),
            health: Cell::new(None),
            launched: Cell::new(0),
            stopped: Cell::new(0),
        }
    }
    fn record(&self, event: impl Into<String>) -> Result<(), Rc<Atom>> {
        let mut trace = self.trace.borrow_mut();
        trace.push(event.into());
        if trace.len() as i32 - 1 == self.case.fail {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
}
impl StreamLifecycleHost for Host {
    type Error = Rc<Atom>;
    type Timestamp = Rc<Atom>;
    type Timeout = Rc<Atom>;
    fn super_dispose(&self) -> Result<(), Rc<Atom>> {
        self.record("super")?;
        self.super_disposed.set(true);
        Ok(())
    }
    fn timeout(&self, key: StreamTimeout) -> Result<Rc<Atom>, Rc<Atom>> {
        let (i, name) = slot(key);
        self.reads[i].set(self.reads[i].get() + 1);
        self.record(format!("get:{name}:{}", self.reads[i].get()))?;
        Ok(if self.reads[i].get() == 1 {
            self.guards[i].clone()
        } else {
            self.arguments[i].clone()
        })
    }
    fn timeout_truthy(&self, value: &Rc<Atom>) -> bool {
        value.truthy
    }
    fn clear_timeout(&self, value: Rc<Atom>) -> Result<(), Rc<Atom>> {
        let key = value.slot.unwrap();
        self.cleared.borrow_mut().push(value);
        self.record(format!("clear:{}", slot(key).1))
    }
    fn unset_timeout(&self, key: StreamTimeout) -> Result<(), Rc<Atom>> {
        let (i, name) = slot(key);
        self.record(format!("unset:{name}"))?;
        self.present[i].set(false);
        Ok(())
    }
    fn dispose_socket(&self, key: StreamSocket) -> Result<(), Rc<Atom>> {
        self.record(format!("socket:{}", socket_name(key)))?;
        self.sockets.borrow_mut().push(key);
        Ok(())
    }
    fn set_degraded(&self, value: bool) -> Result<(), Rc<Atom>> {
        self.record(format!("degraded:{value}"))?;
        self.degraded.set(value);
        Ok(())
    }
    fn date_now(&self) -> Result<Rc<Atom>, Rc<Atom>> {
        self.record("now")?;
        Ok(self.stamp.clone())
    }
    fn set_last_message_at(&self, value: Rc<Atom>) -> Result<(), Rc<Atom>> {
        assert!(
            Rc::ptr_eq(&value, &self.stamp),
            "STREAM native clock argument identity"
        );
        self.record("messageAt")?;
        *self.last.borrow_mut() = Some(value);
        Ok(())
    }
    fn set_warned(&self, value: bool) -> Result<(), Rc<Atom>> {
        self.record(format!("warned:{value}"))?;
        self.warned.set(value);
        Ok(())
    }
    fn update_socket_health(&self, update: PrivateStreamHealthUpdate) -> Result<(), Rc<Atom>> {
        self.health.set(Some(update));
        self.record("health")
    }
    fn stop_reconcile(&self) -> Result<(), Rc<Atom>> {
        self.record("stop")?;
        self.stopped.set(self.stopped.get() + 1);
        Ok(())
    }
    fn reconcile_running_truthy(&self) -> Result<bool, Rc<Atom>> {
        self.record("running")?;
        Ok(self.case.running)
    }
    fn launch_rest_reconcile(&self) -> Result<(), Rc<Atom>> {
        self.record("launch")?;
        self.launched.set(self.launched.get() + 1);
        Ok(())
    }
}
struct Outcome {
    input: Value,
    trace: Vec<String>,
    cleared: Vec<StreamTimeout>,
    sockets: Vec<StreamSocket>,
    degraded: bool,
    warned: bool,
    last_same: bool,
    super_disposed: bool,
    health: Option<PrivateStreamHealthUpdate>,
    launched: u32,
    stopped: u32,
    present: [bool; 2],
    original_error: bool,
}
impl Outcome {
    fn wire(&self) -> Value {
        json!({"trace":self.trace,"cleared":self.cleared.iter().map(|key|json!({"slot":slot(*key).1,"argument":true})).collect::<Vec<_>>(),"sockets":self.sockets.iter().map(|key|socket_name(*key)).collect::<Vec<_>>(),"degraded":self.degraded,"warned":self.warned,"lastSame":self.last_same,"superDisposed":self.super_disposed,"health":self.health.map(|update|json!({"own":update.private_degraded_reason==UndefinedProperty::PresentUndefined,"undefined":true})),"launched":self.launched,"stopped":self.stopped,"present":self.present,"originalError":self.original_error,"resultUndefined":!self.original_error})
    }
}
fn exercise(case: Case) -> Outcome {
    let expected = case.expected_trace();
    let fail = case.fail;
    let count = if fail < 0 {
        expected.len()
    } else {
        fail as usize + 1
    };
    let invoked = &expected[..count];
    let committed = &expected[..if fail < 0 { count } else { count - 1 }];
    let contains = |name: &str| committed.iter().any(|e| e == name);
    let host = Host::new(case.clone());
    let result = match case.op {
        "message" => body::mark_private_stream_message_received(&host),
        "dispose" => body::dispose(&host),
        "start" => body::start_private_stream_degraded_reconcile(&host),
        "stop" => body::stop_private_stream_degraded_reconcile(&host),
        _ => unreachable!(),
    };
    match result {
        Ok(()) => assert_eq!(fail, -1, "STREAM native unit result"),
        Err(error) => {
            assert!(fail >= 0, "STREAM unexpected native error");
            assert!(
                Rc::ptr_eq(&error, &host.error),
                "STREAM native original error identity"
            );
            assert_eq!(
                error.units,
                [0x41, 0xd800, 0x42, 0xdc00],
                "STREAM native error UTF16 payload"
            );
        }
    }
    assert_eq!(
        host.degraded.get(),
        !contains("degraded:false"),
        "STREAM native degraded value"
    );
    assert_eq!(
        host.warned.get(),
        !contains("warned:false"),
        "STREAM native warned value"
    );
    assert_eq!(
        host.super_disposed.get(),
        contains("super"),
        "STREAM native super disposal"
    );
    match host.last.borrow().as_ref() {
        Some(last) => {
            assert!(contains("messageAt"), "STREAM native timestamp write");
            assert!(
                Rc::ptr_eq(last, &host.stamp),
                "STREAM native retained timestamp identity"
            );
            assert_eq!(
                last.units,
                [0x41, 0xd800, 0x42, 0xdc00],
                "STREAM native timestamp UTF16 payload"
            );
        }
        None => assert!(!contains("messageAt"), "STREAM native absent timestamp"),
    }
    let health = invoked
        .iter()
        .any(|e| e == "health")
        .then_some(PrivateStreamHealthUpdate {
            private_degraded_reason: UndefinedProperty::PresentUndefined,
        });
    assert_eq!(
        host.health.get(),
        health,
        "STREAM native own undefined health property"
    );
    for (i, key) in [StreamTimeout::WriteCheck, StreamTimeout::DegradedReconcile]
        .iter()
        .enumerate()
    {
        let initial = if i == 0 { case.write } else { case.reconcile };
        assert_eq!(
            host.present[i].get(),
            initial && !contains(&format!("unset:{}", slot(*key).1)),
            "STREAM native timer slot value"
        );
    }
    let expected_cleared: Vec<StreamTimeout> = invoked
        .iter()
        .filter_map(|event| match event.as_str() {
            "clear:write" => Some(StreamTimeout::WriteCheck),
            "clear:reconcile" => Some(StreamTimeout::DegradedReconcile),
            _ => None,
        })
        .collect();
    assert_eq!(
        host.cleared.borrow().len(),
        expected_cleared.len(),
        "STREAM native clear count"
    );
    for (actual, key) in host.cleared.borrow().iter().zip(&expected_cleared) {
        assert!(
            Rc::ptr_eq(actual, &host.arguments[slot(*key).0]),
            "STREAM native second-read timeout identity"
        );
        assert_eq!(actual.slot, Some(*key), "STREAM native cleared slot");
        assert_eq!(
            actual.units,
            [0x41, 0xd800, 0x42, 0xdc00],
            "STREAM native timeout UTF16 payload"
        );
    }
    let expected_sockets: Vec<StreamSocket> = committed
        .iter()
        .filter_map(|event| match event.as_str() {
            "socket:public" => Some(StreamSocket::Public),
            "socket:private" => Some(StreamSocket::Private),
            "socket:seconds" => Some(StreamSocket::SecondsTickers),
            "socket:third" => Some(StreamSocket::ThirdTickers),
            _ => None,
        })
        .collect();
    assert_eq!(
        *host.sockets.borrow(),
        expected_sockets,
        "STREAM native socket identities"
    );
    assert_eq!(
        host.launched.get(),
        u32::from(contains("launch")),
        "STREAM native detached launch count"
    );
    assert_eq!(
        host.stopped.get(),
        u32::from(contains("stop")),
        "STREAM native stop call count"
    );
    for (i, name) in ["write", "reconcile"].iter().enumerate() {
        let prefix = format!("get:{name}:");
        let expected_reads = invoked
            .iter()
            .filter(|event| event.starts_with(&prefix))
            .count();
        assert_eq!(
            host.reads[i].get(),
            expected_reads,
            "STREAM native timeout read count"
        );
    }
    assert_eq!(
        host.trace.borrow().as_slice(),
        invoked,
        "STREAM native operation order"
    );
    Outcome {
        input: case.input(),
        trace: host.trace.borrow().clone(),
        cleared: expected_cleared,
        sockets: host.sockets.borrow().clone(),
        degraded: host.degraded.get(),
        warned: host.warned.get(),
        last_same: host.last.borrow().is_some(),
        super_disposed: host.super_disposed.get(),
        health: host.health.get(),
        launched: host.launched.get(),
        stopped: host.stopped.get(),
        present: [host.present[0].get(), host.present[1].get()],
        original_error: fail >= 0,
    }
}
#[test]
fn stream_bodies_preserve_native_state_order_and_identity() {
    for case in cases() {
        exercise(case);
    }
}
#[test]
#[ignore = "requires Node and TypeScript"]
fn stream_bodies_match_actual_typescript() {
    // Every native assertion finishes before output projection or wire comparisons.
    let native: Vec<Outcome> = cases().into_iter().map(exercise).collect();
    let capture = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/stream-lifecycle-capture.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        capture.status.success(),
        "STREAM source capture {}",
        String::from_utf8_lossy(&capture.stderr)
    );
    let data: Value = serde_json::from_slice(&capture.stdout).unwrap();
    let source = data["cases"].as_array().unwrap();
    assert_eq!(source.len(), 69, "STREAM independent captured floor");
    for (i, case) in native.iter().enumerate() {
        assert_eq!(source[i]["input"], case.input, "STREAM source input {i}");
    }
    for (i, case) in native.iter().enumerate() {
        assert_eq!(source[i]["output"], case.wire(), "STREAM source output {i}");
    }
}
