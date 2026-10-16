use safe_cex_venue_bitget::private_throttles::{
    self as body, FetchKind, FetchThrottleCallback, FetchThrottleHost,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
#[derive(Clone, Debug)]
struct Case {
    kind: FetchKind,
    disposed: bool,
    scheduled: bool,
    pending: bool,
    now: f64,
    last: f64,
    first: f64,
    second: f64,
    callback_disposed: bool,
    callback_pending: bool,
    inline: bool,
    fail: i32,
}
fn base(kind: FetchKind) -> Case {
    let throttle = if kind == FetchKind::Balance {
        300.
    } else {
        500.
    };
    Case {
        kind,
        disposed: false,
        scheduled: false,
        pending: true,
        now: 30000.,
        last: 30000. - (throttle - 1.),
        first: throttle,
        second: throttle,
        callback_disposed: false,
        callback_pending: true,
        inline: false,
        fail: -1,
    }
}
impl Case {
    fn input(&self) -> Value {
        fn n(v: f64) -> Value {
            if v.is_nan() { json!("nan") } else { json!(v) }
        }
        json!({"kind":match self.kind{FetchKind::Positions=>"positions",FetchKind::Balance=>"balance",FetchKind::Orders=>"orders"},"disposed":self.disposed,"scheduled":self.scheduled,"pending":self.pending,"now":n(self.now),"last":n(self.last),"first":n(self.first),"second":n(self.second),"callbackDisposed":self.callback_disposed,"callbackPending":self.callback_pending,"inline":self.inline,"fail":self.fail})
    }
    fn events(&self) -> Vec<&'static str> {
        let mut e = vec!["now", "disposed"];
        if self.disposed {
            return e;
        }
        if self.kind == FetchKind::Balance {
            e.push("pending:true");
        }
        e.push("scheduled");
        if self.scheduled {
            return e;
        }
        e.extend(["last", "throttle"]);
        if self.now - self.last >= self.first {
            e.push("launch");
        } else {
            e.extend([
                "scheduled:true",
                "throttle",
                "set",
                "scheduled:false",
                "disposed",
            ]);
            if !self.callback_disposed {
                e.push("pending");
                if self.callback_pending {
                    e.push("launch");
                }
            }
        }
        e
    }
}
fn cases() -> Vec<Case> {
    let mut out = vec![];
    for kind in [FetchKind::Positions, FetchKind::Balance, FetchKind::Orders] {
        for disposed in [false, true] {
            for scheduled in [false, true] {
                for pending in [false, true] {
                    for time in ["boundary", "below", "nan"] {
                        for callback_disposed in [false, true] {
                            let mut c = base(kind);
                            c.disposed = disposed;
                            c.scheduled = scheduled;
                            c.pending = pending;
                            c.callback_pending = pending;
                            c.callback_disposed = callback_disposed;
                            if time == "boundary" {
                                c.last = c.now - c.first;
                            } else if time == "nan" {
                                c.now = f64::NAN;
                                c.last = 0.;
                            }
                            out.push(c);
                        }
                    }
                }
            }
        }
    }
    for kind in [FetchKind::Positions, FetchKind::Balance, FetchKind::Orders] {
        out.push(Case {
            inline: true,
            ..base(kind)
        });
        out.push(Case {
            pending: false,
            callback_pending: true,
            ..base(kind)
        });
        out.push(Case {
            second: 200.,
            ..base(kind)
        });
    }
    for kind in [FetchKind::Positions, FetchKind::Balance, FetchKind::Orders] {
        for fail in 0..if kind == FetchKind::Balance { 13 } else { 12 } {
            out.push(Case { fail, ..base(kind) });
        }
    }
    assert_eq!(out.len(), 190, "THROTTLE independent native floor");
    out
}
struct Host {
    c: Case,
    error: Rc<Vec<u16>>,
    trace: RefCell<Vec<&'static str>>,
    delays: RefCell<Vec<f64>>,
    disposed: Cell<bool>,
    scheduled: Cell<bool>,
    pending: Cell<bool>,
    reads: Cell<usize>,
    launches: Cell<usize>,
    callback_ran: Cell<bool>,
    queue: RefCell<Option<FetchThrottleCallback<Rc<Vec<u16>>>>>,
}
impl Host {
    fn new(c: Case) -> Rc<Self> {
        Rc::new(Self {
            disposed: Cell::new(c.disposed),
            scheduled: Cell::new(c.scheduled),
            pending: Cell::new(c.pending),
            c,
            error: Rc::new(vec![65, 0xd800, 66, 0xdc00]),
            trace: RefCell::new(vec![]),
            delays: RefCell::new(vec![]),
            reads: Cell::new(0),
            launches: Cell::new(0),
            callback_ran: Cell::new(false),
            queue: RefCell::new(None),
        })
    }
    fn record(&self, e: &'static str) -> Result<(), Rc<Vec<u16>>> {
        let mut t = self.trace.borrow_mut();
        t.push(e);
        if t.len() as i32 - 1 == self.c.fail {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn kind(&self, k: FetchKind) {
        assert_eq!(k, self.c.kind, "THROTTLE native field or fetch target");
    }
    fn mutate(&self) {
        self.disposed.set(self.c.callback_disposed);
        self.pending.set(self.c.callback_pending);
    }
}
impl FetchThrottleHost for Host {
    type Error = Rc<Vec<u16>>;
    fn date_now(&self) -> Result<f64, Self::Error> {
        self.record("now")?;
        Ok(self.c.now)
    }
    fn disposed(&self) -> Result<bool, Self::Error> {
        self.record("disposed")?;
        Ok(self.disposed.get())
    }
    fn scheduled(&self, k: FetchKind) -> Result<bool, Self::Error> {
        self.kind(k);
        self.record("scheduled")?;
        Ok(self.scheduled.get())
    }
    fn last_fetch_at(&self, k: FetchKind) -> Result<f64, Self::Error> {
        self.kind(k);
        self.record("last")?;
        Ok(self.c.last)
    }
    fn throttle_ms(&self, k: FetchKind) -> Result<f64, Self::Error> {
        self.kind(k);
        self.record("throttle")?;
        self.reads.set(self.reads.get() + 1);
        Ok(if self.reads.get() == 1 {
            self.c.first
        } else {
            self.c.second
        })
    }
    fn set_scheduled(&self, k: FetchKind, v: bool) -> Result<(), Self::Error> {
        self.kind(k);
        self.record(if v {
            "scheduled:true"
        } else {
            "scheduled:false"
        })?;
        self.scheduled.set(v);
        Ok(())
    }
    fn pending(&self, k: FetchKind) -> Result<bool, Self::Error> {
        self.kind(k);
        self.record("pending")?;
        Ok(self.pending.get())
    }
    fn set_pending(&self, k: FetchKind, v: bool) -> Result<(), Self::Error> {
        self.kind(k);
        assert_eq!(
            k,
            FetchKind::Balance,
            "THROTTLE native balance-only pending write"
        );
        assert!(v);
        self.record("pending:true")?;
        self.pending.set(v);
        Ok(())
    }
    fn launch(&self, k: FetchKind) -> Result<(), Self::Error> {
        self.kind(k);
        self.record("launch")?;
        self.launches.set(self.launches.get() + 1);
        Ok(())
    }
    fn set_timeout(
        &self,
        f: FetchThrottleCallback<Self::Error>,
        delay: f64,
    ) -> Result<(), Self::Error> {
        self.delays.borrow_mut().push(delay);
        self.record("set")?;
        if self.c.inline {
            self.mutate();
            self.callback_ran.set(true);
            f()?;
        } else {
            *self.queue.borrow_mut() = Some(f);
        }
        Ok(())
    }
}
fn invoke(h: Rc<Host>) -> Result<(), Rc<Vec<u16>>> {
    match h.c.kind {
        FetchKind::Positions => body::throttled_fetch_positions(h),
        FetchKind::Balance => body::throttled_fetch_balance(h),
        FetchKind::Orders => body::throttled_fetch_orders(h),
    }
}
fn run(c: Case) -> Value {
    let h = Host::new(c.clone());
    let setup = invoke(h.clone());
    let setup_error = setup.is_err();
    if let Err(e) = setup {
        assert!(
            Rc::ptr_eq(&e, &h.error),
            "THROTTLE native setup error identity"
        );
        assert_eq!(&*e, &[65, 0xd800, 66, 0xdc00]);
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
                    "THROTTLE native callback error identity"
                );
                assert_eq!(&*e, &[65, 0xd800, 66, 0xdc00]);
                callback_error = true;
            }
        }
    }
    let baseline = c.events();
    let attempted = &baseline[..if c.fail < 0 {
        baseline.len()
    } else {
        c.fail as usize + 1
    }];
    let completed = &attempted[..if c.fail < 0 {
        attempted.len()
    } else {
        attempted.len() - 1
    }];
    assert_eq!(
        &*h.trace.borrow(),
        attempted,
        "THROTTLE native operation prefix: {c:?}"
    );
    let fired = completed.contains(&"set");
    assert_eq!(
        h.callback_ran.get(),
        fired,
        "THROTTLE native callback invocation"
    );
    let mut scheduled = c.scheduled;
    for e in completed {
        match *e {
            "scheduled:true" => scheduled = true,
            "scheduled:false" => scheduled = false,
            _ => (),
        }
    }
    assert_eq!(
        h.scheduled.get(),
        scheduled,
        "THROTTLE native scheduled state"
    );
    let pending = if fired {
        c.callback_pending
    } else {
        c.pending || completed.contains(&"pending:true")
    };
    assert_eq!(h.pending.get(), pending, "THROTTLE native pending state");
    assert_eq!(
        h.launches.get(),
        usize::from(completed.contains(&"launch")),
        "THROTTLE native fetch count"
    );
    assert_eq!(
        h.reads.get(),
        completed.iter().filter(|e| **e == "throttle").count(),
        "THROTTLE native independent throttle read count"
    );
    let in_callback = c.fail >= 0
        && baseline
            .iter()
            .position(|e| *e == "scheduled:false")
            .is_some_and(|i| c.fail as usize >= i);
    assert_eq!(callback_error, in_callback && !c.inline);
    assert_eq!(setup_error, c.fail >= 0 && (!in_callback || c.inline));
    assert_eq!(
        h.delays.borrow().len(),
        usize::from(attempted.contains(&"set")),
        "THROTTLE native independent timer count"
    );
    for delay in h.delays.borrow().iter() {
        let expected = c.second - (c.now - c.last);
        if expected.is_nan() {
            assert!(delay.is_nan(), "THROTTLE native NaN delay");
        } else {
            assert_eq!(
                delay.to_bits(),
                expected.to_bits(),
                "THROTTLE native decoded raw delay"
            );
        }
    }
    assert!(h.queue.borrow().is_none(), "THROTTLE native queue drained");
    // Native decoded state and values above precede output projection/comparison.
    json!({"trace":*h.trace.borrow(),"delays":h.delays.borrow().iter().map(|v|if v.is_nan(){json!("nan")}else{json!(v.to_le_bytes())}).collect::<Vec<_>>(),"scheduled":h.scheduled.get(),"pending":h.pending.get(),"launches":h.launches.get(),"callbackRan":h.callback_ran.get(),"setupError":setup_error,"callbackError":callback_error})
}
#[test]
fn native_private_throttles() {
    for c in cases() {
        run(c);
    }
}
#[test]
#[ignore = "requires local TypeScript oracle"]
fn source_private_throttles() {
    let cs = cases();
    let native: Vec<_> = cs.iter().cloned().map(run).collect();
    let inputs: Vec<_> = cs.iter().map(Case::input).collect();
    let mut child = Command::new("node")
        .arg(format!(
            "{}/scripts/private-throttles-capture.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&inputs).unwrap().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(source.len(), 190, "THROTTLE independent source floor");
    for (i, (a, b)) in native.iter().zip(source.iter()).enumerate() {
        assert_eq!(a, b, "THROTTLE source case {i}: {:?}", cs[i]);
    }
}
#[test]
fn queued_fetch_retains_receiver() {
    let h = Host::new(base(FetchKind::Positions));
    let weak = Rc::downgrade(&h);
    invoke(h.clone()).unwrap();
    drop(h);
    let h = weak.upgrade().expect("THROTTLE native receiver retained");
    let f = h.queue.borrow_mut().take().unwrap();
    h.mutate();
    f().unwrap();
    assert_eq!(h.launches.get(), 1);
    drop(h);
    assert!(weak.upgrade().is_some());
    drop(f);
    assert!(
        weak.upgrade().is_none(),
        "THROTTLE native receiver released"
    );
}
