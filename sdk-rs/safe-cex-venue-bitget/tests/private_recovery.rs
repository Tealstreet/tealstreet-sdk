use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_venue_bitget::private_recovery::{self as body, PrivateRecoveryHost};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
const SWITCH: &str = "Bitget private WS received no authenticated stream payloads after a write; switching to Tealstreet fallback route. Reason: ";
const CURRENT: &str = "Bitget private WS received no authenticated stream payloads after a write; reconnecting current route. Reason: ";
#[derive(Clone, Debug)]
struct Case {
    op: &'static str,
    disposed: bool,
    uta: bool,
    using: bool,
    has_fallback: bool,
    enabled: bool,
    now: f64,
    last: f64,
    cooldown: f64,
    reason: Vec<u16>,
    fail: i32,
}
impl Case {
    fn input(&self) -> Value {
        fn n(v: f64) -> Value {
            if v.is_nan() {
                json!("nan")
            } else if v == f64::INFINITY {
                json!("inf")
            } else if v == f64::NEG_INFINITY {
                json!("-inf")
            } else {
                json!(v)
            }
        }
        json!({"op":self.op,"disposed":self.disposed,"uta":self.uta,"using":self.using,"hasFallback":self.has_fallback,"enabled":self.enabled,"now":n(self.now),"last":n(self.last),"cooldown":n(self.cooldown),"reason":self.reason,"fail":self.fail})
    }
    fn switched(&self) -> bool {
        !self.uta && !self.using && self.has_fallback
    }
    fn events(&self) -> Vec<&'static str> {
        let mut e = vec!["disposed"];
        if self.disposed {
            return e;
        }
        if self.op == "open" {
            e.extend(["auth", "ping"]);
            if self.enabled {
                e.push("launch");
            }
        } else {
            e.extend(["now", "last", "cooldown"]);
            if (self.now - self.last).partial_cmp(&self.cooldown) == Some(std::cmp::Ordering::Less)
            {
                return e;
            }
            e.extend(["set-last", "routing", "uta"]);
            if !self.uta {
                e.push("using");
            }
            if self.switched() {
                e.push("set-using");
            }
            e.extend(["log", "reconnect"]);
        }
        e
    }
}
fn base() -> Case {
    Case {
        op: "recover",
        disposed: false,
        uta: false,
        using: false,
        has_fallback: true,
        enabled: true,
        now: 30000.,
        last: 0.,
        cooldown: 30000.,
        reason: vec![65, 0xd800, 66, 0xdc00],
        fail: -1,
    }
}
fn cases() -> Vec<Case> {
    let mut cases = vec![];
    let times = [
        (30000., 0., 30000.),
        (29999., 0., 30000.),
        (60000., 30000., 30000.),
        (30000., 40000., 30000.),
        (f64::NAN, 0., 30000.),
        (f64::INFINITY, f64::INFINITY, 30000.),
        (0., 0., f64::NAN),
        (-1., 0., 0.),
    ];
    for disposed in [false, true] {
        for uta in [false, true] {
            for using in [false, true] {
                for has_fallback in [false, true] {
                    for (now, last, cooldown) in times {
                        for reason in [
                            vec![],
                            "reason: ✓".encode_utf16().collect(),
                            vec![65, 0xd800, 66, 0xdc00],
                        ] {
                            cases.push(Case {
                                disposed,
                                uta,
                                using,
                                has_fallback,
                                now,
                                last,
                                cooldown,
                                reason,
                                ..base()
                            });
                        }
                    }
                }
            }
        }
    }
    for disposed in [false, true] {
        for enabled in [false, true] {
            cases.push(Case {
                op: "open",
                disposed,
                enabled,
                ..base()
            });
        }
    }
    for fail in 0..11 {
        cases.push(Case { fail, ..base() });
    }
    for fail in 0..4 {
        cases.push(Case {
            op: "open",
            fail,
            ..base()
        });
    }
    assert_eq!(cases.len(), 403, "RECOVERY independent native floor");
    cases
}
struct Host {
    c: Case,
    error: Rc<Vec<u16>>,
    trace: RefCell<Vec<&'static str>>,
    last: Cell<f64>,
    using: Cell<bool>,
    logs: RefCell<Vec<(Vec<u16>, LogSeverity)>>,
    reconnects: Cell<usize>,
    launches: Cell<usize>,
}
impl Host {
    fn new(c: Case) -> Self {
        Self {
            last: Cell::new(c.last),
            using: Cell::new(c.using),
            c,
            error: Rc::new(vec![65, 0xd800, 66, 0xdc00]),
            trace: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            reconnects: Cell::new(0),
            launches: Cell::new(0),
        }
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
}
impl PrivateRecoveryHost for Host {
    type Error = Rc<Vec<u16>>;
    fn disposed(&self) -> Result<bool, Self::Error> {
        self.record("disposed")?;
        Ok(self.c.disposed)
    }
    fn date_now(&self) -> Result<f64, Self::Error> {
        self.record("now")?;
        Ok(self.c.now)
    }
    fn last_recovery_at(&self) -> Result<f64, Self::Error> {
        self.record("last")?;
        Ok(self.last.get())
    }
    fn recovery_cooldown_ms(&self) -> Result<f64, Self::Error> {
        self.record("cooldown")?;
        Ok(self.c.cooldown)
    }
    fn set_last_recovery_at(&self, v: f64) -> Result<(), Self::Error> {
        self.record("set-last")?;
        self.last.set(v);
        Ok(())
    }
    fn resolved_private_fallback_truthy(&self) -> Result<bool, Self::Error> {
        self.record("routing")?;
        Ok(self.c.has_fallback)
    }
    fn uses_uta_routing(&self) -> Result<bool, Self::Error> {
        self.record("uta")?;
        Ok(self.c.uta)
    }
    fn using_fallback(&self) -> Result<bool, Self::Error> {
        self.record("using")?;
        Ok(self.using.get())
    }
    fn set_using_fallback(&self, v: bool) -> Result<(), Self::Error> {
        assert!(v, "RECOVERY native fallback write value");
        self.record("set-using")?;
        self.using.set(v);
        Ok(())
    }
    fn log(&self, message: &[u16], severity: LogSeverity) -> Result<(), Self::Error> {
        self.logs.borrow_mut().push((message.to_vec(), severity));
        self.record("log")
    }
    fn reconnect(&self) -> Result<(), Self::Error> {
        self.record("reconnect")?;
        self.reconnects.set(self.reconnects.get() + 1);
        Ok(())
    }
    fn auth(&self) -> Result<(), Self::Error> {
        self.record("auth")
    }
    fn ping(&self) -> Result<(), Self::Error> {
        self.record("ping")
    }
    fn reconnecting_enabled(&self) -> bool {
        self.c.enabled
    }
    fn launch_orders_fetch(&self) -> Result<(), Self::Error> {
        self.record("launch")?;
        self.launches.set(self.launches.get() + 1);
        Ok(())
    }
}
fn run(c: Case) -> Value {
    let h = Host::new(c.clone());
    let result = if c.op == "open" {
        body::on_open(&h)
    } else {
        body::recover_missing_private_stream(&h, &c.reason)
    };
    let expected = c.events();
    let attempted = &expected[..if c.fail < 0 {
        expected.len()
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
        "RECOVERY native operation prefix: {c:?}"
    );
    assert_eq!(
        result.is_err(),
        c.fail >= 0,
        "RECOVERY native original failure disposition"
    );
    if let Err(e) = result {
        assert!(
            Rc::ptr_eq(&e, &h.error),
            "RECOVERY native original error identity"
        );
        assert_eq!(
            &*e,
            &[65, 0xd800, 66, 0xdc00],
            "RECOVERY native error UTF16"
        );
    }
    assert_eq!(
        h.last.get().to_bits(),
        if completed.contains(&"set-last") {
            c.now
        } else {
            c.last
        }
        .to_bits(),
        "RECOVERY native cooldown timestamp"
    );
    assert_eq!(
        h.using.get(),
        c.using || completed.contains(&"set-using"),
        "RECOVERY native fallback state"
    );
    assert_eq!(
        h.reconnects.get(),
        usize::from(completed.contains(&"reconnect")),
        "RECOVERY native reconnect count"
    );
    assert_eq!(
        h.launches.get(),
        usize::from(completed.contains(&"launch")),
        "RECOVERY native open launch count"
    );
    let logs = h.logs.borrow();
    assert_eq!(
        logs.len(),
        usize::from(attempted.contains(&"log")),
        "RECOVERY native independent warning count"
    );
    for (message, severity) in logs.iter() {
        let mut expected: Vec<u16> = if c.switched() { SWITCH } else { CURRENT }
            .encode_utf16()
            .collect();
        expected.extend_from_slice(&c.reason);
        assert_eq!(message, &expected, "RECOVERY native decoded warning UTF16");
        assert_eq!(
            *severity,
            LogSeverity::Warning,
            "RECOVERY native warning severity"
        );
    }
    // Native values are checked above; no re-encode/decode comparison is used.
    json!({"trace":*h.trace.borrow(),"logs":logs.iter().map(|(units,_)|json!({"units":units,"severity":"warning"})).collect::<Vec<_>>(),"lastBits":h.last.get().to_le_bytes(),"using":h.using.get(),"reconnects":h.reconnects.get(),"launches":h.launches.get(),"thrown":c.fail>=0})
}
#[test]
fn native_private_recovery() {
    for c in cases() {
        run(c);
    }
}
#[test]
#[ignore = "requires local TypeScript oracle"]
fn source_private_recovery() {
    let cs = cases();
    let native: Vec<_> = cs.iter().cloned().map(run).collect();
    let inputs: Vec<_> = cs.iter().map(Case::input).collect();
    let mut child = Command::new("node")
        .arg(format!(
            "{}/scripts/private-recovery-capture.mjs",
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
    assert_eq!(source.len(), 403, "RECOVERY independent source floor");
    for (i, (actual, expected)) in native.iter().zip(source.iter()).enumerate() {
        assert_eq!(actual, expected, "RECOVERY source case {i}: {:?}", cs[i]);
    }
}
