use safe_cex_venue_hyperliquid::{
    helper_tasks::HelperTaskHost,
    startup_timeout::{StartupTimeoutHost, TimeoutCallback, with_startup_timeout},
};
use std::{
    cell::RefCell,
    fmt,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Task = Pin<Box<dyn Future<Output = ()>>>;
type Reaction = Rc<dyn Fn(Result<V, V>)>;
#[derive(Default)]
struct P {
    result: Option<Result<V, V>>,
    reactions: Vec<Reaction>,
}
#[derive(Clone)]
enum V {
    U,
    Null,
    N(f64),
    S(Vec<u16>),
    O(Rc<()>),
    F(TimeoutCallback<V, V>),
    B(&'static str),
    A(Vec<V>),
    P(Rc<RefCell<P>>),
}
impl fmt::Debug for V {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::S(v) => write!(f, "UTF16{v:?}"),
            Self::N(v) => write!(f, "{v:?}"),
            _ => write!(f, "native handle"),
        }
    }
}
fn text(s: &str) -> V {
    V::S(s.encode_utf16().collect())
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::O(a), V::O(b)) => Rc::ptr_eq(a, b),
        (V::P(a), V::P(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn promise() -> V {
    V::P(Rc::new(RefCell::new(P::default())))
}
fn settle(p: &V, result: Result<V, V>) {
    let V::P(p) = p else { panic!("native promise") };
    let reactions = {
        let mut state = p.borrow_mut();
        if state.result.is_some() {
            return;
        }
        state.result = Some(result.clone());
        std::mem::take(&mut state.reactions)
    };
    for reaction in reactions {
        reaction(result.clone())
    }
}
fn subscribe(p: &V, reaction: Reaction) {
    let V::P(p) = p else { panic!("native promise") };
    let result = p.borrow().result.clone();
    if let Some(result) = result {
        reaction(result)
    } else {
        p.borrow_mut().reactions.push(reaction)
    }
}
fn resolve(p: &V, value: V) {
    if matches!(value, V::P(_)) {
        let p = p.clone();
        subscribe(&value, Rc::new(move |v| settle(&p, v)))
    } else {
        settle(p, Ok(value))
    }
}
struct State {
    tasks: Vec<Task>,
    timer: Option<V>,
    operation: Option<V>,
    timeout: Option<V>,
    events: Vec<String>,
    owner: V,
    error: V,
    log_error: V,
    clear_error: V,
    scene: &'static str,
}
#[derive(Clone)]
struct H(Rc<RefCell<State>>);
impl H {
    fn new(scene: &'static str) -> Self {
        Self(Rc::new(RefCell::new(State {
            tasks: vec![],
            timer: None,
            operation: None,
            timeout: None,
            events: vec![],
            owner: V::O(Rc::new(())),
            error: V::O(Rc::new(())),
            log_error: V::O(Rc::new(())),
            clear_error: V::O(Rc::new(())),
            scene,
        })))
    }
    fn drain(&self) {
        let tasks = std::mem::take(&mut self.0.borrow_mut().tasks);
        for mut task in tasks {
            if task
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                self.0.borrow_mut().tasks.push(task)
            }
        }
    }
    fn fire(&self) -> Result<V, V> {
        let callback = self.0.borrow().timer.clone().unwrap();
        self.call(callback, V::U, vec![])
    }
    fn event(&self, event: &str) {
        self.0.borrow_mut().events.push(event.into())
    }
}
impl HelperTaskHost<V, V> for H {
    fn start_task(&self, mut task: Task) {
        if task
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
        {
            self.0.borrow_mut().tasks.push(task)
        }
    }
    fn settle_return(&self, result: Result<V, V>) -> Pin<Box<dyn Future<Output = Result<V, V>>>> {
        let p = promise();
        match result {
            Ok(v) => resolve(&p, v),
            Err(e) => settle(&p, Err(e)),
        }
        self.await_value(p)
    }
}
impl StartupTimeoutHost for H {
    type Value = V;
    type Error = V;
    fn undefined(&self) -> V {
        V::U
    }
    fn null(&self) -> V {
        V::Null
    }
    fn literal(&self, v: &str) -> V {
        text(v)
    }
    fn number(&self, v: f64) -> V {
        V::N(v)
    }
    fn truthy(&self, v: &V) -> bool {
        match v {
            V::U | V::Null => false,
            V::N(v) => *v != 0. && !v.is_nan(),
            V::S(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn global(&self, k: &str) -> Result<V, V> {
        Ok(V::B(match k {
            "Promise" => "Promise",
            "Math" => "Math",
            "LogSeverity" => "LogSeverity",
            "setTimeout" => "setTimeout",
            "clearTimeout" => "clearTimeout",
            _ => panic!("native global {k}"),
        }))
    }
    fn get(&self, owner: &V, k: &str) -> Result<V, V> {
        match (owner, k) {
            (V::B("Promise"), "race") => Ok(V::B("race")),
            (V::B("Math"), "round") => Ok(V::B("round")),
            (V::B("LogSeverity"), "Warning") => Ok(text("warning")),
            (_, "log") => {
                assert!(
                    same(owner, &self.0.borrow().owner),
                    "timeout logger owner lookup"
                );
                Ok(V::B("log"))
            }
            _ => panic!("native property {k}"),
        }
    }
    fn call(&self, callee: V, receiver: V, args: Vec<V>) -> Result<V, V> {
        match callee {
            V::F(body) => body(args),
            V::B("setTimeout") => {
                self.event("setTimeout");
                assert!(matches!(receiver, V::U), "timeout global receiver");
                let V::N(ms) = args[1] else {
                    panic!("native timeout delay")
                };
                assert_eq!(ms.to_bits(), 1500f64.to_bits(), "timeout native delay bits");
                self.0.borrow_mut().timer = Some(args[0].clone());
                Ok(V::N(if self.0.borrow().scene == "zero-id" {
                    0.
                } else {
                    99.
                }))
            }
            V::B("clearTimeout") => {
                assert!(matches!(receiver, V::U), "timeout cleanup global receiver");
                assert_eq!(args.len(), 1, "timeout cleanup argument count");
                self.event("clearTimeout");
                let V::N(id) = args[0] else { panic!() };
                assert_eq!(
                    id.to_bits(),
                    99f64.to_bits(),
                    "timeout native cleanup handle"
                );
                if self.0.borrow().scene == "clear-error" {
                    Err(self.0.borrow().clear_error.clone())
                } else {
                    self.0.borrow_mut().timer = None;
                    Ok(V::U)
                }
            }
            V::B("round") => {
                assert!(matches!(receiver, V::B("Math")), "timeout round receiver");
                let V::N(v) = args[0] else { panic!() };
                Ok(V::N(safe_cex_num::math_round(v)))
            }
            V::B("log") => {
                assert!(
                    same(&receiver, &self.0.borrow().owner),
                    "timeout logger receiver"
                );
                let V::S(msg) = &args[0] else { panic!() };
                assert_eq!(
                    msg,
                    &"probe timed out after 2s; continuing with fallback"
                        .encode_utf16()
                        .collect::<Vec<_>>(),
                    "timeout native warning message"
                );
                let V::S(severity) = &args[1] else { panic!() };
                assert_eq!(
                    severity,
                    &"warning".encode_utf16().collect::<Vec<_>>(),
                    "timeout native severity"
                );
                self.event("log");
                if self.0.borrow().scene == "log-error" {
                    Err(self.0.borrow().log_error.clone())
                } else {
                    Ok(V::U)
                }
            }
            V::B("race") => {
                self.event("race");
                assert!(matches!(receiver, V::B("Promise")), "timeout race receiver");
                let V::A(entries) = &args[0] else { panic!() };
                assert_eq!(entries.len(), 2, "timeout independent race input count");
                assert!(
                    same(&entries[0], self.0.borrow().operation.as_ref().unwrap()),
                    "timeout native race first original operation"
                );
                assert!(
                    same(&entries[1], self.0.borrow().timeout.as_ref().unwrap()),
                    "timeout native race second constructed timeout"
                );
                let p = promise();
                for entry in entries {
                    let target = p.clone();
                    subscribe(entry, Rc::new(move |v| settle(&target, v)));
                }
                Ok(p)
            }
            _ => panic!("native callable"),
        }
    }
    fn construct(&self, callee: V, args: Vec<V>) -> Result<V, V> {
        assert!(
            matches!(callee, V::B("Promise")),
            "timeout original Promise constructor"
        );
        assert_eq!(args.len(), 1, "timeout constructor argument count");
        self.event("construct");
        let p = promise();
        self.0.borrow_mut().timeout = Some(p.clone());
        let target = p.clone();
        let resolve = V::F(Rc::new(move |args| {
            resolve(&target, args.first().cloned().unwrap_or(V::U));
            Ok(V::U)
        }));
        if let Err(e) = self.call(args[0].clone(), V::U, vec![resolve]) {
            settle(&p, Err(e));
        }
        Ok(p)
    }
    fn array(&self, values: Vec<V>) -> V {
        V::A(values)
    }
    fn callback(&self, body: TimeoutCallback<V, V>) -> V {
        V::F(body)
    }
    fn divide(&self, a: V, b: V) -> Result<V, V> {
        let (V::N(a), V::N(b)) = (a, b) else { panic!() };
        assert_eq!(
            a.to_bits(),
            1500f64.to_bits(),
            "timeout native divide numerator bits"
        );
        assert_eq!(
            b.to_bits(),
            1000f64.to_bits(),
            "timeout native divide denominator bits"
        );
        Ok(V::N(a / b))
    }
    fn template(&self, parts: Vec<V>) -> Result<V, V> {
        let mut units = vec![];
        for v in parts {
            match v {
                V::S(v) => units.extend(v),
                V::N(v) => units.extend(safe_cex_num::js_number_string(v).encode_utf16()),
                _ => panic!("template fixture"),
            }
        }
        Ok(V::S(units))
    }
    fn await_value(&self, value: V) -> Pin<Box<dyn Future<Output = Result<V, V>>>> {
        Box::pin(std::future::poll_fn(move |_| {
            let V::P(p) = &value else {
                panic!("native await promise")
            };
            p.borrow().result.clone().map_or(Poll::Pending, Poll::Ready)
        }))
    }
}
fn poll<F: Future + Unpin>(f: &mut F) -> Poll<F::Output> {
    Pin::new(f).poll(&mut Context::from_waker(Waker::noop()))
}
#[test]
fn timeout_native_identity_cleanup_and_dropped_observer() {
    const SCENES: [&str; 8] = [
        "operation",
        "rejection",
        "fallback",
        "zero-id",
        "clear-error",
        "log-error",
        "drop-operation",
        "drop-fallback",
    ];
    let mut completed = 0;
    let mut last = None;
    for scene in SCENES {
        let h = H::new(scene);
        let owner = h.0.borrow().owner.clone();
        let original = h.0.borrow().error.clone();
        let value = V::O(Rc::new(()));
        let fallback = V::O(Rc::new(()));
        let operation = promise();
        h.0.borrow_mut().operation = Some(operation.clone());
        let mut observer = with_startup_timeout(
            &h,
            owner,
            operation.clone(),
            V::N(1500.),
            text("probe"),
            fallback.clone(),
        );
        assert!(
            poll(&mut observer).is_pending(),
            "timeout observer pending after eager prefix"
        );
        assert_eq!(
            h.0.borrow().events,
            ["construct", "setTimeout", "race"],
            "timeout independent eager prefix sequence"
        );
        let dropped = scene.starts_with("drop-");
        let mut observer = if dropped {
            drop(observer);
            None
        } else {
            Some(observer)
        };
        if matches!(
            scene,
            "fallback" | "zero-id" | "log-error" | "drop-fallback"
        ) {
            let fired = h.fire();
            if scene == "log-error" {
                let error = fired.expect_err("timeout logging failure must escape timer callback");
                assert!(
                    same(&error, &h.0.borrow().log_error),
                    "timeout native original logging error"
                );
            } else {
                fired.expect("timeout callback");
            }
        } else {
            settle(
                &operation,
                if scene == "rejection" {
                    Err(original.clone())
                } else {
                    Ok(value.clone())
                },
            );
        }
        h.drain();
        if let Some(observer) = observer.as_mut() {
            match poll(observer) {
                Poll::Pending => assert_eq!(
                    scene, "log-error",
                    "timeout result must settle except throwing timer log"
                ),
                Poll::Ready(result) => {
                    if scene == "rejection" {
                        assert!(
                            same(
                                &result.expect_err("timeout original operation rejection"),
                                &original
                            ),
                            "timeout native original operation error"
                        );
                    } else if scene == "clear-error" {
                        assert!(
                            same(
                                &result.expect_err("timeout finally error overrides return"),
                                &h.0.borrow().clear_error
                            ),
                            "timeout native original cleanup error"
                        );
                    } else {
                        let result = result.expect("timeout success");
                        assert!(
                            same(
                                &result,
                                if matches!(scene, "fallback" | "zero-id") {
                                    &fallback
                                } else {
                                    &value
                                }
                            ),
                            "timeout native result alias"
                        );
                    }
                }
            }
        }
        let events = &h.0.borrow().events;
        let expected = if scene == "zero-id" || scene == "log-error" {
            vec!["construct", "setTimeout", "race", "log"]
        } else if scene == "fallback" || scene == "drop-fallback" {
            vec!["construct", "setTimeout", "race", "log", "clearTimeout"]
        } else {
            vec!["construct", "setTimeout", "race", "clearTimeout"]
        };
        assert_eq!(
            events, &expected,
            "timeout independent final effect order {scene}"
        );
        completed += 1;
        last = Some(scene);
    }
    assert_eq!(completed, 8, "independent completed native timeout count");
    assert_eq!(
        last,
        Some("drop-fallback"),
        "native timeout last completed scene"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/startup-timeout-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "live startup timeout source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["cases"], 8, "independent timeout source case count");
    assert_eq!(
        source["last"], "drop-fallback",
        "timeout source enumeration floor"
    );
}
