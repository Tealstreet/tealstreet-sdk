use safe_cex_venue_bitget::{
    stream_lifecycle::*,
    stream_timers::{self as body, *},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};
const MESSAGE: &str = "Bitget private WS is not receiving live order/position updates. Tealstreet will reconcile through REST; if this account uses IP whitelisting, verify the terminal-listed browser/server WebSocket IPs are allowed.";
const UNITS: [u16; 4] = [0x41, 0xd800, 0x42, 0xdc00];
#[derive(Clone, Debug)]
struct Case {
    op: &'static str,
    reason: &'static str,
    force: &'static str,
    warned: bool,
    old: bool,
    disposed: bool,
    degraded: bool,
    callback_disposed: bool,
    mode: &'static str,
    inline: bool,
    fail: i32,
}
impl Case {
    fn input(&self) -> Value {
        json!({"op":self.op,"reason":self.reason,"force":self.force,"warned":self.warned,"old":self.old,"disposed":self.disposed,"degraded":self.degraded,"callbackDisposed":self.callback_disposed,"mode":self.mode,"inline":self.inline,"fail":self.fail})
    }
    fn events(&self) -> Vec<&'static str> {
        let mut v = vec![];
        match self.op {
            "mark" => {
                v.extend(["degraded:true", "health", "log", "recover", "start"]);
                if matches!(self.force, "true" | "object") {
                    v.push("warned?");
                    if !self.warned {
                        v.extend(["warned:true", "emit"]);
                    }
                }
            }
            "reconcile" => {
                v.push("disposed?");
                if !self.disposed {
                    v.push("degraded?");
                    if self.degraded {
                        v.push("timer?");
                        if !self.old {
                            v.extend(["set", "assign", "unset", "launch"]);
                        }
                    }
                }
            }
            "write" => {
                v.extend(["frame?", "timer?"]);
                if self.old {
                    v.extend(["timer?", "clear"]);
                }
                v.push("set");
                if !self.inline {
                    v.push("assign");
                }
                v.extend(["unset", "disposed?"]);
                if !self.callback_disposed {
                    v.extend(["frame?", "coerce-current", "coerce-captured"]);
                    if self.mode != "newer" {
                        v.push("mark");
                    }
                }
                if self.inline {
                    v.push("assign");
                }
            }
            _ => unreachable!(),
        }
        v
    }
}
fn cases() -> Vec<Case> {
    let base = Case {
        op: "mark",
        reason: "undefined",
        force: "undefined",
        warned: false,
        old: false,
        disposed: false,
        degraded: false,
        callback_disposed: false,
        mode: "equal",
        inline: false,
        fail: -1,
    };
    let mut scenarios = vec![];
    for reason in ["undefined", "text", "object", "null"] {
        for force in ["undefined", "false", "true", "object"] {
            for warned in [false, true] {
                scenarios.push(Case {
                    reason,
                    force,
                    warned,
                    ..base.clone()
                });
            }
        }
    }
    for disposed in [false, true] {
        for degraded in [false, true] {
            for old in [false, true] {
                scenarios.push(Case {
                    op: "reconcile",
                    disposed,
                    degraded,
                    old,
                    callback_disposed: true,
                    ..base.clone()
                });
            }
        }
    }
    for old in [false, true] {
        for callback_disposed in [false, true] {
            for mode in ["equal", "newer", "alias", "nan"] {
                for inline in [false, true] {
                    scenarios.push(Case {
                        op: "write",
                        old,
                        callback_disposed,
                        mode,
                        inline,
                        ..base.clone()
                    });
                }
            }
        }
    }
    let mut out = vec![];
    for c in scenarios {
        for fail in -1..c.events().len() as i32 {
            out.push(Case { fail, ..c.clone() });
        }
    }
    assert_eq!(out.len(), 566, "TIMERS independent case floor");
    out
}
#[derive(Debug, PartialEq)]
enum Atom {
    Undefined,
    Null,
    Text(Vec<u16>),
    Object(Vec<u16>),
    Bool(bool),
}
fn value(k: &str) -> Rc<Atom> {
    Rc::new(match k {
        "undefined" => Atom::Undefined,
        "null" => Atom::Null,
        "text" => Atom::Text(UNITS.to_vec()),
        "object" => Atom::Object(UNITS.to_vec()),
        "true" => Atom::Bool(true),
        _ => Atom::Bool(false),
    })
}
struct Seen {
    at: &'static str,
    value: Rc<Atom>,
    force: Option<bool>,
}
struct Host {
    c: Case,
    trace: RefCell<Vec<&'static str>>,
    seen: RefCell<Vec<Seen>>,
    delays: RefCell<Vec<u64>>,
    error: Rc<Atom>,
    reason: Rc<Atom>,
    degraded: Cell<bool>,
    warned: Cell<bool>,
    disposed: Cell<bool>,
    kind: Cell<&'static str>,
    reads: Cell<usize>,
    old: Rc<Atom>,
    clear: Rc<Atom>,
    new: Rc<Atom>,
    timer_same: Cell<bool>,
    cleared_same: Cell<bool>,
    queue: RefCell<Option<StreamTimerCallback<Rc<Atom>>>>,
    callback_ran: Cell<bool>,
    marks: Cell<usize>,
    launches: Cell<usize>,
    clock: Rc<Cell<f64>>,
    current: RefCell<Rc<Cell<f64>>>,
}
impl Host {
    fn new(c: Case) -> Rc<Self> {
        let clock = Rc::new(Cell::new(10.));
        Rc::new(Self {
            reason: value(c.reason),
            error: Rc::new(Atom::Object(UNITS.to_vec())),
            degraded: Cell::new(c.op == "reconcile" && c.degraded),
            warned: Cell::new(c.warned),
            disposed: Cell::new(c.disposed),
            kind: Cell::new(if c.old { "old" } else { "none" }),
            c,
            trace: RefCell::new(vec![]),
            seen: RefCell::new(vec![]),
            delays: RefCell::new(vec![]),
            reads: Cell::new(0),
            old: value("object"),
            clear: value("object"),
            new: value("object"),
            timer_same: Cell::new(false),
            cleared_same: Cell::new(false),
            queue: RefCell::new(None),
            callback_ran: Cell::new(false),
            marks: Cell::new(0),
            launches: Cell::new(0),
            current: RefCell::new(clock.clone()),
            clock,
        })
    }
    fn record(&self, s: &'static str) -> Result<(), Rc<Atom>> {
        let mut t = self.trace.borrow_mut();
        t.push(s);
        if t.len() as i32 - 1 == self.c.fail {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn see(&self, at: &'static str, v: &Rc<Atom>, force: Option<bool>) -> Result<(), Rc<Atom>> {
        self.seen.borrow_mut().push(Seen {
            at,
            value: v.clone(),
            force,
        });
        self.record(at)
    }
    fn mutate(&self) {
        self.disposed.set(self.c.callback_disposed);
        if self.c.op == "reconcile" {
            self.degraded.set(false);
        } else {
            match self.c.mode {
                "newer" => *self.current.borrow_mut() = Rc::new(Cell::new(20.)),
                "alias" => {
                    self.clock.set(30.);
                    *self.current.borrow_mut() = Rc::new(Cell::new(20.));
                }
                "nan" => *self.current.borrow_mut() = Rc::new(Cell::new(f64::NAN)),
                _ => (),
            }
        }
    }
}
impl StreamLifecycleHost for Host {
    type Error = Rc<Atom>;
    type Timestamp = Rc<Cell<f64>>;
    type Timeout = Option<Rc<Atom>>;
    fn super_dispose(&self) -> Result<(), Self::Error> {
        panic!("unexpected dispose")
    }
    fn timeout(&self, s: StreamTimeout) -> Result<Self::Timeout, Self::Error> {
        assert_eq!(
            s,
            if self.c.op == "write" {
                StreamTimeout::WriteCheck
            } else {
                StreamTimeout::DegradedReconcile
            }
        );
        self.record("timer?")?;
        self.reads.set(self.reads.get() + 1);
        Ok(match self.kind.get() {
            "new" => Some(self.new.clone()),
            "old" => Some(if self.reads.get() == 1 {
                self.old.clone()
            } else {
                self.clear.clone()
            }),
            _ => None,
        })
    }
    fn timeout_truthy(&self, v: &Self::Timeout) -> bool {
        v.is_some()
    }
    fn clear_timeout(&self, v: Self::Timeout) -> Result<(), Self::Error> {
        assert!(
            Rc::ptr_eq(&v.unwrap(), &self.clear),
            "TIMERS native second timer read"
        );
        self.cleared_same.set(true);
        self.record("clear")
    }
    fn unset_timeout(&self, s: StreamTimeout) -> Result<(), Self::Error> {
        assert_eq!(
            s,
            if self.c.op == "write" {
                StreamTimeout::WriteCheck
            } else {
                StreamTimeout::DegradedReconcile
            }
        );
        self.record("unset")?;
        self.kind.set("none");
        Ok(())
    }
    fn dispose_socket(&self, _: StreamSocket) -> Result<(), Self::Error> {
        panic!("unexpected socket")
    }
    fn set_degraded(&self, v: bool) -> Result<(), Self::Error> {
        assert!(v);
        self.record("degraded:true")?;
        self.degraded.set(v);
        Ok(())
    }
    fn date_now(&self) -> Result<Self::Timestamp, Self::Error> {
        panic!("unexpected now")
    }
    fn set_last_message_at(&self, _: Self::Timestamp) -> Result<(), Self::Error> {
        panic!("unexpected timestamp write")
    }
    fn set_warned(&self, v: bool) -> Result<(), Self::Error> {
        assert!(v);
        self.record("warned:true")?;
        self.warned.set(v);
        Ok(())
    }
    fn update_socket_health(&self, _: PrivateStreamHealthUpdate) -> Result<(), Self::Error> {
        panic!("unexpected clear health")
    }
    fn stop_reconcile(&self) -> Result<(), Self::Error> {
        panic!("unexpected stop")
    }
    fn reconcile_running_truthy(&self) -> Result<bool, Self::Error> {
        panic!("unexpected running")
    }
    fn launch_rest_reconcile(&self) -> Result<(), Self::Error> {
        self.record("launch")?;
        self.launches.set(self.launches.get() + 1);
        Ok(())
    }
}
impl StreamTimerHost for Host {
    type Value = Rc<Atom>;
    fn is_undefined(v: &Self::Value) -> bool {
        matches!(**v, Atom::Undefined)
    }
    fn string_literal(s: &'static str) -> Self::Value {
        Rc::new(Atom::Text(s.encode_utf16().collect()))
    }
    fn boolean_literal(v: bool) -> Self::Value {
        Rc::new(Atom::Bool(v))
    }
    fn value_truthy(v: &Self::Value) -> bool {
        match &**v {
            Atom::Undefined | Atom::Null | Atom::Bool(false) => false,
            Atom::Text(s) => !s.is_empty(),
            _ => true,
        }
    }
    fn update_reason_health(
        &self,
        u: ReasonHealthUpdate<'_, Self::Value>,
    ) -> Result<(), Self::Error> {
        self.see("health", u.private_degraded_reason, None)
    }
    fn log_warning(&self, v: &Self::Value) -> Result<(), Self::Error> {
        self.see("log", v, None)
    }
    fn recover_missing_private_stream(&self, v: &Self::Value) -> Result<(), Self::Error> {
        self.see("recover", v, None)
    }
    fn start_reconcile(&self) -> Result<(), Self::Error> {
        self.record("start")
    }
    fn warned_truthy(&self) -> Result<bool, Self::Error> {
        self.record("warned?")?;
        Ok(self.warned.get())
    }
    fn emit_warning(&self, v: &Self::Value, o: ForcedWarningOptions) -> Result<(), Self::Error> {
        assert!(o.force_toast, "TIMERS native forced warning options");
        self.see("emit", v, Some(o.force_toast))
    }
    fn disposed_truthy(&self) -> Result<bool, Self::Error> {
        self.record("disposed?")?;
        Ok(self.disposed.get())
    }
    fn degraded_truthy(&self) -> Result<bool, Self::Error> {
        self.record("degraded?")?;
        Ok(self.degraded.get())
    }
    fn set_timeout(
        &self,
        f: StreamTimerCallback<Self::Error>,
        ms: u64,
    ) -> Result<Self::Timeout, Self::Error> {
        self.delays.borrow_mut().push(ms);
        self.record("set")?;
        if self.c.inline {
            self.mutate();
            self.callback_ran.set(true);
            f()?;
        } else {
            *self.queue.borrow_mut() = Some(f);
        }
        Ok(Some(self.new.clone()))
    }
    fn assign_timeout(&self, s: StreamTimeout, v: Self::Timeout) -> Result<(), Self::Error> {
        assert_eq!(
            s,
            if self.c.op == "write" {
                StreamTimeout::WriteCheck
            } else {
                StreamTimeout::DegradedReconcile
            }
        );
        assert!(
            Rc::ptr_eq(&v.unwrap(), &self.new),
            "TIMERS native assigned token"
        );
        self.record("assign")?;
        self.timer_same.set(true);
        self.kind.set("new");
        Ok(())
    }
    fn last_message_at(&self) -> Result<Self::Timestamp, Self::Error> {
        self.record("frame?")?;
        Ok(self.current.borrow().clone())
    }
    fn is_later(&self, a: &Self::Timestamp, b: &Self::Timestamp) -> Result<bool, Self::Error> {
        assert!(
            Rc::ptr_eq(b, &self.clock),
            "TIMERS native captured timestamp identity"
        );
        assert!(Rc::ptr_eq(a, &self.current.borrow()));
        self.record("coerce-current")?;
        let a = a.get();
        self.record("coerce-captured")?;
        Ok(a > b.get())
    }
    fn mark_degraded(&self, v: Self::Value, f: Self::Value) -> Result<(), Self::Error> {
        assert_eq!(*f, Atom::Bool(true), "TIMERS native force argument");
        self.see("mark", &v, Some(true))?;
        self.marks.set(self.marks.get() + 1);
        Ok(())
    }
}
fn run(c: Case) -> Value {
    let h = Host::new(c.clone());
    let result = match c.op {
        "mark" => body::mark_private_stream_degraded(h.as_ref(), h.reason.clone(), value(c.force)),
        "reconcile" => body::schedule_private_stream_degraded_reconcile(h.clone()),
        _ => body::schedule_private_stream_write_check(h.clone()),
    };
    let setup_error = result.is_err();
    if let Err(e) = result {
        assert!(
            Rc::ptr_eq(&e, &h.error),
            "TIMERS native original setup error"
        );
    }
    let mut callback_error = false;
    if !setup_error {
        let f = h.queue.borrow_mut().take();
        if let Some(f) = f {
            h.mutate();
            h.callback_ran.set(true);
            if let Err(e) = f() {
                assert!(
                    Rc::ptr_eq(&e, &h.error),
                    "TIMERS native original callback error"
                );
                callback_error = true;
            }
        }
    }
    let baseline = c.events();
    let end = if c.fail < 0 {
        baseline.len()
    } else {
        c.fail as usize + 1
    };
    let expected = &baseline[..end];
    assert_eq!(
        &*h.trace.borrow(),
        expected,
        "TIMERS native event prefix: {c:?}"
    );
    let successful = &expected[..if c.fail < 0 { end } else { end - 1 }];
    let attempted = |s| expected.contains(&s);
    let completed = |s| successful.contains(&s);
    let set_completed = completed("set");
    let assign_completed = completed("assign");
    let callback_ran = set_completed && (c.inline || assign_completed);
    assert_eq!(
        h.callback_ran.get(),
        callback_ran,
        "TIMERS native callback execution"
    );
    let mut kind = if c.old { "old" } else { "none" };
    for event in successful {
        match *event {
            "unset" => kind = "none",
            "assign" => kind = "new",
            _ => (),
        }
    }
    assert_eq!(h.kind.get(), kind, "TIMERS native timer slot");
    assert_eq!(
        h.timer_same.get(),
        assign_completed,
        "TIMERS native timer assignment"
    );
    assert_eq!(h.cleared_same.get(), attempted("clear"));
    assert_eq!(
        h.marks.get(),
        usize::from(completed("mark")),
        "TIMERS native mark count"
    );
    assert_eq!(
        h.launches.get(),
        usize::from(completed("launch")),
        "TIMERS native launch count"
    );
    assert_eq!(h.warned.get(), c.warned || completed("warned:true"));
    assert_eq!(
        h.degraded.get(),
        if c.op == "reconcile" {
            c.degraded && !callback_ran
        } else {
            completed("degraded:true")
        },
        "TIMERS native degraded state"
    );
    let callback_start = baseline.iter().position(|s| *s == "unset");
    let expected_callback_error =
        c.fail >= 0 && !c.inline && callback_start.is_some_and(|i| c.fail as usize >= i);
    assert_eq!(callback_error, expected_callback_error);
    assert_eq!(setup_error, c.fail >= 0 && !expected_callback_error);
    let expected_pending = set_completed && !c.inline && !assign_completed;
    assert_eq!(
        h.queue.borrow().is_some(),
        expected_pending,
        "TIMERS native pending callback"
    );
    assert_eq!(
        &*h.delays.borrow(),
        if attempted("set") {
            vec![if c.op == "write" { 8000 } else { 10000 }]
        } else {
            vec![]
        }
        .as_slice(),
        "TIMERS native decoded delay"
    );
    let seen = h.seen.borrow();
    let expected_seen: Vec<_> = expected
        .iter()
        .filter(|s| matches!(**s, "health" | "log" | "recover" | "emit" | "mark"))
        .copied()
        .collect();
    assert_eq!(
        seen.iter().map(|s| s.at).collect::<Vec<_>>(),
        expected_seen,
        "TIMERS native argument call sites"
    );
    for s in seen.iter() {
        if c.reason == "undefined" {
            assert_eq!(
                *s.value,
                Atom::Text(MESSAGE.encode_utf16().collect()),
                "TIMERS native default message"
            );
        } else {
            assert!(
                Rc::ptr_eq(&s.value, &h.reason),
                "TIMERS native reason alias"
            );
            assert_eq!(
                *s.value,
                *value(c.reason),
                "TIMERS native UTF16 or null reason"
            );
        }
        if matches!(s.at, "emit" | "mark") {
            assert_eq!(s.force, Some(true));
        }
    }
    if seen.len() > 1 {
        assert!(
            seen.iter().all(|s| Rc::ptr_eq(&s.value, &seen[0].value)),
            "TIMERS native repeated reason identity"
        );
    }
    // All native values above are asserted before this wire projection.
    let seen_wire: Vec<_> = seen
        .iter()
        .map(|s| {
            let (kind, units) = match &*s.value {
                Atom::Null => ("null", vec![]),
                Atom::Object(u) => ("object", u.clone()),
                Atom::Text(u) => (
                    if c.reason == "undefined" {
                        "default"
                    } else {
                        "text"
                    },
                    u.clone(),
                ),
                _ => panic!("unexpected reason"),
            };
            let mut v = json!({"at":s.at,"kind":kind,"units":units,"same":true});
            if s.at == "log" {
                v["severity"] = json!("warning");
            }
            if let Some(f) = s.force {
                v["force"] = json!(f);
                if s.at == "emit" {
                    v["own"] = json!(true);
                }
            }
            v
        })
        .collect();
    let wire = json!({"trace":*h.trace.borrow(),"seen":seen_wire,"delays":*h.delays.borrow(),"degraded":h.degraded.get(),"warned":h.warned.get(),"kind":h.kind.get(),"timerSame":h.timer_same.get(),"clearedSame":h.cleared_same.get(),"setupError":setup_error,"callbackError":callback_error,"callbackRan":h.callback_ran.get(),"markCount":h.marks.get(),"launchCount":h.launches.get(),"pending":h.queue.borrow().is_some()});
    drop(seen);
    h.queue.borrow_mut().take();
    wire
}
#[test]
fn native_stream_timers() {
    for c in cases() {
        run(c);
    }
}
#[test]
#[ignore = "requires local TypeScript oracle"]
fn source_stream_timers() {
    let cs = cases();
    let native: Vec<_> = cs.iter().cloned().map(run).collect();
    let output = Command::new("node")
        .arg(format!(
            "{}/scripts/stream-timers-capture.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = oracle["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 566, "TIMERS independent source floor");
    for (i, c) in cs.iter().enumerate() {
        assert_eq!(rows[i]["input"], c.input(), "TIMERS source input {i}");
        assert_eq!(
            rows[i]["output"], native[i],
            "TIMERS source output {i}: {c:?}"
        );
    }
}

#[test]
fn callback_retains_receiver_until_fired() {
    let mut c = cases()
        .into_iter()
        .find(|c| {
            c.op == "write"
                && c.fail == -1
                && !c.old
                && !c.inline
                && !c.callback_disposed
                && c.mode == "equal"
        })
        .unwrap();
    c.fail = -1;
    let h = Host::new(c);
    let weak = Rc::downgrade(&h);
    body::schedule_private_stream_write_check(h.clone()).unwrap();
    drop(h);
    let retained = weak
        .upgrade()
        .expect("TIMERS native queued receiver retention");
    let callback = retained.queue.borrow_mut().take().unwrap();
    retained.mutate();
    callback().unwrap();
    assert_eq!(
        retained.marks.get(),
        1,
        "TIMERS native retained callback action"
    );
    drop(retained);
    assert!(weak.upgrade().is_some());
    drop(callback);
    assert!(
        weak.upgrade().is_none(),
        "TIMERS native receiver released after callback"
    );
}
