#![cfg(not(target_arch = "wasm32"))]
use indexmap::IndexMap;
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_venue_grvt::{
    api_runtime::*,
    builder_workflow::{JsPromise, JsResult},
    private_commands::PrivateCommands,
    private_lifecycle::*,
    session_host::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
type FnValue = Rc<dyn Fn(N, Vec<N>) -> JsResult<N>>;
#[derive(Clone)]
enum N {
    U,
    Null,
    B(bool),
    Num(f64),
    S(String),
    A(Rc<RefCell<Vec<N>>>),
    O(Rc<RefCell<IndexMap<String, N>>>),
    F(FnValue),
    P(Rc<Job>),
    E(Rc<RefCell<IndexMap<String, N>>>),
}
impl N {
    fn from(v: Value) -> Self {
        match v {
            Value::Null => Self::Null,
            Value::Bool(v) => Self::B(v),
            Value::Number(v) => Self::Num(v.as_f64().unwrap()),
            Value::String(v) => Self::S(v),
            Value::Array(v) => Self::array(v.into_iter().map(Self::from).collect()),
            Value::Object(v) => {
                Self::object(v.into_iter().map(|(k, v)| (k, Self::from(v))).collect())
            }
        }
    }
    fn object(fields: Vec<(String, N)>) -> Self {
        Self::O(Rc::new(RefCell::new(fields.into_iter().collect())))
    }
    fn obj(fields: Vec<(&str, N)>) -> Self {
        Self::object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
    fn array(v: Vec<N>) -> Self {
        Self::A(Rc::new(RefCell::new(v)))
    }
    fn get(&self, key: &str) -> N {
        match self {
            Self::O(v) | Self::E(v) => v.borrow().get(key).cloned().unwrap_or(Self::U),
            Self::A(v) => key
                .parse::<usize>()
                .ok()
                .and_then(|i| v.borrow().get(i).cloned())
                .unwrap_or(Self::U),
            _ => Self::U,
        }
    }
    fn set(&self, key: &str, value: N) {
        match self {
            Self::O(v) | Self::E(v) => {
                v.borrow_mut().insert(key.into(), value);
            }
            _ => panic!("native object"),
        }
    }
    fn same(&self, b: &Self) -> bool {
        match (self, b) {
            (Self::O(a), Self::O(b)) | (Self::E(a), Self::E(b)) => Rc::ptr_eq(a, b),
            (Self::A(a), Self::A(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
    fn text(&self) -> String {
        match self {
            Self::U => "undefined".into(),
            Self::Null => "null".into(),
            Self::B(v) => v.to_string(),
            Self::Num(v) => safe_cex_num::js_number_string(*v),
            Self::S(v) => v.clone(),
            Self::E(_) => format!(
                "{}: {}",
                self.get("name").text(),
                self.get("message").text()
            ),
            _ => "[object Object]".into(),
        }
    }
    fn snapshot(&self) -> Self {
        match self {
            Self::A(v) => Self::array(v.borrow().iter().map(Self::snapshot).collect()),
            Self::O(v) => Self::object(
                v.borrow()
                    .iter()
                    .filter(|(_, v)| !matches!(v, N::F(_)))
                    .map(|(k, v)| (k.clone(), v.snapshot()))
                    .collect(),
            ),
            _ => self.clone(),
        }
    }
}
fn error(name: &str, message: &str) -> N {
    let N::O(v) = N::obj(vec![
        ("name", N::S(name.into())),
        ("message", N::S(message.into())),
    ]) else {
        unreachable!()
    };
    N::E(v)
}
fn assert_native(v: &N, s: &Value, label: &str) {
    match v {
        N::U => assert_eq!(s["undefined"], true, "{label} native undefined"),
        N::Null => assert!(s.is_null(), "{label} native null"),
        N::B(v) => assert_eq!(Some(*v), s.as_bool(), "{label} native bool"),
        N::Num(v) if !v.is_finite() => {
            assert_eq!(s["number"], v.to_string(), "{label} native nonfinite")
        }
        N::Num(v) => assert_eq!(Some(*v), s.as_f64(), "{label} native number"),
        N::S(v) => assert_eq!(Some(v.as_str()), s.as_str(), "{label} native string"),
        N::A(v) => {
            let s = s.as_array().expect(label);
            assert_eq!(v.borrow().len(), s.len(), "{label} native array shape");
            for (v, s) in v.borrow().iter().zip(s) {
                assert_native(v, s, label);
            }
        }
        N::O(v) | N::E(v) => {
            let s = s.as_object().expect(label);
            let values = v.borrow();
            let values: Vec<_> = values
                .iter()
                .filter(|(_, v)| !matches!(v, N::F(_)))
                .collect();
            assert_eq!(values.len(), s.len(), "{label} native object shape");
            for (k, v) in values {
                assert!(s.contains_key(k), "{label} native key {k}");
                assert_native(v, &s[k], label);
            }
        }
        _ => panic!("{label} unresolved callable/promise"),
    }
}
struct Job {
    future: RefCell<Option<JsPromise<N>>>,
    result: RefCell<Option<JsResult<N>>>,
}
struct Handle(Rc<Job>);
impl Future for Handle {
    type Output = JsResult<N>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .result
            .borrow()
            .clone()
            .map(Poll::Ready)
            .unwrap_or(Poll::Pending)
    }
}
struct Later {
    first: bool,
    callback: Option<Box<dyn FnOnce() -> JsResult<N>>>,
}
impl Future for Later {
    type Output = JsResult<N>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.first {
            self.first = true;
            Poll::Pending
        } else {
            Poll::Ready(self.callback.take().unwrap()())
        }
    }
}
struct Host {
    me: RefCell<std::rc::Weak<Host>>,
    name: String,
    state: PrivateCommands<N>,
    clock: Cell<f64>,
    last: Cell<f64>,
    ping: Cell<bool>,
    socket: RefCell<N>,
    events: RefCell<Vec<N>>,
    jobs: RefCell<Vec<Rc<Job>>>,
    timers: RefCell<Vec<Rc<dyn Fn()>>>,
    handlers: RefCell<IndexMap<String, Vec<N>>>,
    store: N,
    positions: N,
    opts: N,
    exchange: N,
    instance: RefCell<N>,
    registration: Cell<usize>,
    response_reads: Cell<usize>,
}
impl Host {
    fn new(group: &str, name: &str) -> Rc<Self> {
        let clock = if group == "watchdog" {
            match name {
                "quiet" | "ping-boundary" => 30000.,
                "ping" => 30001.,
                "reconnect-boundary" => 90000.,
                "reconnect" | "log-throws" | "clear-throws" | "reconnect-throws" => 90001.,
                _ => 50000.,
            }
        } else if name == "healthy-reset" {
            300000.
        } else if group == "throttle" {
            match name {
                "immediate" => 501.,
                "boundary" => 500.,
                _ => 100.,
            }
        } else {
            100000.
        };
        let s = Rc::new(Self {
            me: RefCell::new(std::rc::Weak::new()),
            name: name.into(),
            state: PrivateCommands::default(),
            clock: Cell::new(clock),
            last: Cell::new(if name == "invalid-date" { f64::NAN } else { 0. }),
            ping: Cell::new(["quiet", "ping-boundary", "ping-already"].contains(&name)),
            socket: RefCell::new(N::U),
            events: RefCell::new(vec![]),
            jobs: RefCell::new(vec![]),
            timers: RefCell::new(vec![]),
            handlers: RefCell::new(IndexMap::new()),
            store: N::from(
                json!({"loaded":{"markets":true,"positions":false},"positions":[{"old":true}]}),
            ),
            positions: N::from(json!([{"id":"new"}])),
            opts: N::from(json!({"testnet":name=="testnet"})),
            exchange: N::from(
                json!({"apiExtras":{"sessionCookie":"old","accountId":"old-account"},"grvtClient":{"cookie":"cached"}}),
            ),
            instance: RefCell::new(N::U),
            registration: Cell::new(0),
            response_reads: Cell::new(0),
        });
        *s.me.borrow_mut() = Rc::downgrade(&s);
        s.state.authenticated.set(true);
        s.state.is_disposed.set(name == "disposed");
        s.state.authentication_failed.set(name == "failed");
        s.state.session_refresh_in_flight.set(name == "in-flight");
        s.state
            .session_refresh_attempts
            .set(if name == "budget" || name == "healthy-reset" {
                1.
            } else {
                0.
            });
        if name == "healthy-reset" {
            s.state.private_stream_healthy_at.set(Some(0.));
        }
        s.state.position_fetch_scheduled.set(name == "scheduled");
        s.state.has_pending_position_events.set(true);
        let h = s.clone();
        s.store.set(
            "update",
            N::F(Rc::new(move |receiver, args| {
                assert!(receiver.same(&h.store), "store method receiver");
                assert!(
                    args[0].get("positions").same(&h.positions),
                    "store positions identity"
                );
                h.event(vec![h.string("store"), args[0].clone()]);
                if let N::O(values) = &args[0] {
                    for (k, v) in values.borrow().iter() {
                        h.store.set(k, v.clone());
                    }
                }
                if h.name == "store-throws" {
                    return Err(error("Error", "store-fail"));
                }
                Ok(N::U)
            })),
        );
        let h = s.clone();
        s.exchange.set(
            "log",
            N::F(Rc::new(move |receiver, args| {
                assert!(receiver.same(&h.exchange), "API logger receiver");
                h.event(vec![h.string("log"), args[0].clone(), args[1].clone()]);
                if ["prelog-throws", "proxy-log-throws", "log-throws"].contains(&h.name.as_str()) {
                    Err(error("Error", "log-fail"))
                } else {
                    Ok(N::U)
                }
            })),
        );
        let h = s.clone();
        s.exchange.set(
            "authenticate",
            if name == "auth-missing" {
                N::U
            } else {
                N::F(Rc::new(move |receiver, _| {
                    assert!(receiver.same(&h.exchange), "authenticate receiver");
                    h.event(vec![h.string("auth")]);
                    let host = h.clone();
                    Ok(h.later(move || {
                        if host.name == "auth-reject" {
                            return Err(error("Error", "auth-fail"));
                        }
                        host.exchange.set(
                            "apiExtras",
                            N::from(json!({"sessionCookie":"fresh","accountId":"fresh-account"})),
                        );
                        Ok(N::B(host.name != "auth-false"))
                    }))
                }))
            },
        );
        let instance = N::obj(vec![]);
        let mut managers = vec![];
        for kind in ["request", "response"] {
            let manager = N::obj(vec![]);
            let h = s.clone();
            let expected = manager.clone();
            manager.set(
                "use",
                N::F(Rc::new(move |receiver, args| {
                    assert!(receiver.same(&expected), "interceptor manager receiver");
                    h.event(vec![h.string("use"), h.string(kind)]);
                    h.handlers.borrow_mut().insert(kind.into(), args);
                    Ok(N::Num(0.))
                })),
            );
            managers.push((kind, manager));
        }
        instance.set("interceptors", N::obj(managers));
        let h = s.clone();
        instance.set(
            "__call",
            N::F(Rc::new(move |receiver, args| {
                assert!(matches!(receiver, N::U), "retry bare-call receiver");
                h.event(vec![h.string("retry"), args[0].clone()]);
                if h.name == "retry-sync-throws" {
                    return Err(error("Error", "retry-sync"));
                }
                let name = h.name.clone();
                Ok(h.later(move || {
                    if name == "retry-reject" {
                        Err(error("Error", "retry-async"))
                    } else {
                        Ok(N::S("retried".into()))
                    }
                }))
            })),
        );
        *s.instance.borrow_mut() = instance;
        s
    }
    fn event(&self, values: Vec<N>) {
        self.events
            .borrow_mut()
            .push(N::array(values.into_iter().map(|v| v.snapshot()).collect()));
    }
    fn self_rc(&self) -> Rc<Self> {
        self.me.borrow().upgrade().unwrap()
    }
    fn poll_job(&self, j: &Rc<Job>) {
        let Some(mut f) = j.future.borrow_mut().take() else {
            return;
        };
        let mut cx = Context::from_waker(Waker::noop());
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => *j.result.borrow_mut() = Some(v),
            Poll::Pending => *j.future.borrow_mut() = Some(f),
        }
    }
    fn drain(&self) {
        for _ in 0..32 {
            let jobs = self.jobs.borrow().clone();
            for j in &jobs {
                self.poll_job(j);
            }
            if self
                .jobs
                .borrow()
                .iter()
                .all(|j| j.result.borrow().is_some())
            {
                return;
            }
        }
        panic!("session async continuation did not settle within 32 passes");
    }
    fn later(&self, f: impl FnOnce() -> JsResult<N> + 'static) -> N {
        self.promise(self.start(Box::pin(Later {
            first: false,
            callback: Some(Box::new(f)),
        })))
    }
    fn handler(&self, kind: &str, index: usize, v: N) -> JsResult<N> {
        let f = self.handlers.borrow()[kind][index].clone();
        self.call(f, N::U, vec![v])
    }
    fn make_socket(&self, id: &'static str) -> N {
        let h = self.self_rc();
        let socket = N::obj(vec![]);
        socket.set(
            "addEventListener",
            N::F(Rc::new(move |_, args| {
                let event = args[0].text();
                h.event(vec![h.string("listen"), h.string(id), args[0].clone()]);
                h.registration.set(h.registration.get() + 1);
                if h.name == "swap-socket" && h.registration.get() == 1 {
                    h.set_socket(h.make_socket("second"));
                }
                if h.name == "listener-throws" && event == "message" {
                    return Err(error("Error", "listener-fail"));
                }
                if event == "error" {
                    h.handlers
                        .borrow_mut()
                        .insert("socket".into(), vec![args[1].clone()]);
                }
                Ok(N::U)
            })),
        );
        socket
    }
}
impl SessionHost for Host {
    type Value = N;
    fn get(&self, v: &N, k: &str) -> JsResult<N> {
        if self.name == "response-getter"
            && k == "response"
            && matches!(v, N::E(_))
            && v.get("message").text() == "original"
        {
            self.response_reads.set(self.response_reads.get() + 1);
            self.event(vec![self.string("response-read")]);
            return Ok(if self.response_reads.get() == 1 {
                N::U
            } else {
                N::from(json!({"status":499,"data":{"message":"changed"}}))
            });
        }
        if matches!(v, N::U | N::Null) {
            return Err(error(
                "TypeError",
                &format!("Cannot read properties of {} (reading '{k}')", v.text()),
            ));
        }
        let own = v.get(k);
        if !matches!(own, N::U) {
            return Ok(own);
        }
        match (v, k) {
            (N::S(_), "split" | "startsWith") | (N::A(_), "find") => {
                let h = self.self_rc();
                let key = k.to_owned();
                Ok(N::F(Rc::new(move |receiver, args| match key.as_str() {
                    "split" => Ok(N::array(
                        receiver
                            .text()
                            .split(&args[0].text())
                            .map(|v| N::S(v.into()))
                            .collect(),
                    )),
                    "startsWith" => Ok(N::B(receiver.text().starts_with(&args[0].text()))),
                    "find" => {
                        let N::A(v) = receiver else { panic!("find") };
                        let len = v.borrow().len();
                        for i in 0..len {
                            let value = v.borrow().get(i).cloned().unwrap_or(N::U);
                            if h.truthy(&h.call(args[0].clone(), N::U, vec![value.clone()])?) {
                                return Ok(value);
                            }
                        }
                        Ok(N::U)
                    }
                    _ => unreachable!(),
                })))
            }
            _ => Ok(N::U),
        }
    }
    fn set(&self, v: &N, k: &str, field: N) -> JsResult<N, ()> {
        v.set(k, field);
        Ok(())
    }
    fn call(&self, f: N, receiver: N, args: Vec<N>) -> JsResult<N> {
        match f {
            N::F(f) => f(receiver, args),
            N::O(_) => self.call(f.get("__call"), receiver, args),
            _ => Err(error("TypeError", "value is not a function")),
        }
    }
    fn function(&self, f: Callback<N>) -> N {
        N::F(Rc::new(move |_, args| {
            f(args.first().cloned().unwrap_or(N::U))
        }))
    }
    fn object(&self, f: Vec<(&str, N)>) -> N {
        N::obj(f)
    }
    fn define(&self, v: &N, k: &str, field: N) -> JsResult<N, ()> {
        v.set(k, field);
        Ok(())
    }
    fn array(&self, v: Vec<N>) -> N {
        N::array(v)
    }
    fn spread(&self, target: &N, source: N) -> JsResult<N, ()> {
        if let N::O(v) = source {
            for (k, v) in v.borrow().iter() {
                target.set(k, v.clone());
            }
        }
        Ok(())
    }
    fn string(&self, v: &str) -> N {
        N::S(v.into())
    }
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn boolean(&self, v: bool) -> N {
        N::B(v)
    }
    fn undefined(&self) -> N {
        N::U
    }
    fn null(&self) -> N {
        N::Null
    }
    fn truthy(&self, v: &N) -> bool {
        match v {
            N::U | N::Null => false,
            N::B(v) => *v,
            N::Num(v) => *v != 0. && !v.is_nan(),
            N::S(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn nullish(&self, v: &N) -> bool {
        matches!(v, N::U | N::Null)
    }
    fn is_array(&self, v: &N) -> bool {
        matches!(v, N::A(_))
    }
    fn is_string(&self, v: &N) -> bool {
        matches!(v, N::S(_))
    }
    fn is_function(&self, v: &N) -> bool {
        matches!(v, N::F(_))
    }
    fn equals_number(&self, v: &N, n: f64) -> JsResult<N, bool> {
        Ok(match v {
            N::Num(v) => *v == n,
            N::S(v) => v.parse::<f64>().ok() == Some(n),
            _ => false,
        })
    }
    fn template(&self, values: Vec<N>) -> JsResult<N> {
        Ok(N::S(values.iter().map(N::text).collect()))
    }
    fn error(&self, message: N) -> JsResult<N> {
        Ok(error("Error", &message.text()))
    }
    fn reject(&self, e: N) -> N {
        let j = Rc::new(Job {
            future: RefCell::new(None),
            result: RefCell::new(Some(Err(e))),
        });
        N::P(j)
    }
    fn start(&self, task: JsPromise<N>) -> JsPromise<N> {
        let j = Rc::new(Job {
            future: RefCell::new(Some(task)),
            result: RefCell::new(None),
        });
        self.jobs.borrow_mut().push(j.clone());
        self.poll_job(&j);
        Box::pin(Handle(j))
    }
    fn promise(&self, task: JsPromise<N>) -> N {
        let task = Box::pin(async move {
            let mut v = task.await?;
            loop {
                if let N::P(j) = v {
                    v = Handle(j).await?;
                } else {
                    return Ok(v);
                }
            }
        });
        let j = Rc::new(Job {
            future: RefCell::new(Some(task)),
            result: RefCell::new(None),
        });
        self.jobs.borrow_mut().push(j.clone());
        self.poll_job(&j);
        N::P(j)
    }
    fn await_value(&self, value: N) -> JsPromise<N> {
        Box::pin(async move {
            Later {
                first: false,
                callback: Some(Box::new(|| Ok(N::U))),
            }
            .await?;
            match value {
                N::P(job) => Handle(job).await,
                value => Ok(value),
            }
        })
    }
}
impl ApiHost for Host {
    fn base_urls(&self) -> N {
        N::from(
            json!({"PRODUCTION_MDG":"https://market-data.grvt.io","TESTNET_MDG":"https://market-data.testnet.grvt.io","PRODUCTION_TDG":"https://trades.grvt.io","TESTNET_TDG":"https://trades.testnet.grvt.io","PRODUCTION_EDGE":"https://edge.grvt.io","TESTNET_EDGE":"https://edge.testnet.grvt.io"}),
        )
    }
    fn create_axios(&self, c: N, o: N, e: N) -> JsResult<N> {
        assert!(o.same(&self.opts), "original options identity");
        assert!(e.same(&self.exchange), "original exchange identity");
        self.event(vec![self.string("create"), c]);
        Ok(self.instance.borrow().clone())
    }
    fn read_retry_condition(&self) -> JsResult<N> {
        self.event(vec![self.string("read-condition")]);
        Ok(self.string("read-condition"))
    }
    fn install_retry(&self, i: N, o: N) -> JsResult<N, ()> {
        assert!(
            i.same(&self.instance.borrow()),
            "retry install instance identity"
        );
        assert_eq!(
            o.get("retryCondition").text(),
            "read-condition",
            "read-only retry callback"
        );
        self.event(vec![self.string("retry-install"), o.get("retries")]);
        Ok(())
    }
    fn severity(&self, s: LogSeverity) -> N {
        self.string(match s {
            LogSeverity::Debug => "debug",
            LogSeverity::Warning => "warning",
            LogSeverity::Info => "info",
            LogSeverity::Error => "error",
        })
    }
}
impl PrivateLifecycleHost for Host {
    type FillKey = String;
    fn state(&self) -> &PrivateCommands<N> {
        &self.state
    }
    fn now(&self) -> JsResult<N, f64> {
        self.event(vec![self.string("now")]);
        if self.name == "clock-throws" {
            Err(error("Error", "clock-fail"))
        } else {
            Ok(self.clock.get())
        }
    }
    fn last_received(&self) -> f64 {
        self.last.get()
    }
    fn set_last_received(&self, v: f64) {
        self.last.set(v)
    }
    fn last_received_timer(&self) -> N {
        N::Num(7.)
    }
    fn ping_sent(&self) -> bool {
        self.ping.get()
    }
    fn set_ping_sent(&self, v: bool) {
        self.ping.set(v)
    }
    fn iso_date(&self, v: f64) -> JsResult<N> {
        self.event(vec![self.string("iso")]);
        if !v.is_finite() {
            Err(error("RangeError", "Invalid time value"))
        } else {
            assert_eq!(v, 0., "date primitive fixture floor");
            Ok(self.string("1970-01-01T00:00:00.000Z"))
        }
    }
    fn log(&self, msg: N, s: LogSeverity) -> JsResult<N, ()> {
        self.event(vec![self.string("log"), msg, self.severity(s)]);
        if self.name == "log-throws" {
            Err(error("Error", "log-fail"))
        } else {
            Ok(())
        }
    }
    fn clear_timeout(&self, t: N) -> JsResult<N, ()> {
        self.event(vec![self.string("clear"), t]);
        if self.name == "clear-throws" {
            Err(error("Error", "clear-fail"))
        } else {
            Ok(())
        }
    }
    fn ping(&self) -> JsResult<N, ()> {
        self.event(vec![self.string("ping")]);
        if self.name == "ping-throws" {
            Err(error("Error", "ping-fail"))
        } else {
            Ok(())
        }
    }
    fn reconnect(&self) -> JsResult<N, JsPromise<N>> {
        self.event(vec![self.string("reconnect")]);
        if self.name == "reconnect-throws" {
            return Err(error("Error", "reconnect-fail"));
        }
        let name = self.name.clone();
        Ok(Box::pin(async move {
            if name == "reconnect-reject" {
                Err(error("Error", "reconnect-fail"))
            } else {
                Ok(N::U)
            }
        }))
    }
    fn routing_host(&self) -> JsResult<N> {
        self.event(vec![self.string("routing")]);
        Ok(if self.name == "missing" || self.name == "emit-throws" {
            N::U
        } else {
            self.string("wss://proxy")
        })
    }
    fn socket(&self) -> N {
        self.socket.borrow().clone()
    }
    fn set_socket(&self, v: N) {
        *self.socket.borrow_mut() = v
    }
    fn create_socket(&self, host: N) -> JsResult<N> {
        self.event(vec![self.string("socket"), host]);
        Ok(self.make_socket("first"))
    }
    fn listener(&self, _: &str) -> JsResult<N> {
        Ok(N::F(Rc::new(|_, _| Ok(N::U))))
    }
    fn emit_error(&self, msg: N) -> JsResult<N, ()> {
        self.event(vec![self.string("error"), msg]);
        if self.name == "emit-throws" {
            Err(error("Error", "emit-fail"))
        } else {
            Ok(())
        }
    }
    fn reset_retry_budget(&self) -> JsResult<N, ()> {
        if let Some(time) = self.state.private_stream_healthy_at.get() {
            if self.now()? - time >= 300000. {
                self.state.session_refresh_attempts.set(0.);
                self.state.private_stream_healthy_at.set(None);
            }
        }
        Ok(())
    }
    fn fail_closed(&self, reason: &'static str) -> JsResult<N, ()> {
        self.event(vec![self.string("fail"), self.string(reason)]);
        self.state.authentication_failed.set(true);
        self.state.authenticated.set(false);
        self.state.is_disposed.set(true);
        Ok(())
    }
    fn authenticate(&self) -> JsResult<N, JsPromise<N>> {
        self.event(vec![self.string("auth")]);
        let h = self.self_rc();
        Ok(Box::pin(Later {
            first: false,
            callback: Some(Box::new(move || {
                if h.name == "auth-reject" {
                    return Err(error("Error", "auth-fail"));
                }
                if h.name.starts_with("dispose-") {
                    h.state.is_disposed.set(true);
                }
                Ok(N::B(h.name != "auth-false"))
            })),
        }))
    }
    fn api_extras(&self) -> JsResult<N> {
        Ok(N::from(
            json!({"sessionCookie":"cookie","accountId":if self.name=="incomplete"||self.name=="dispose-incomplete"{""}else{"account"}}),
        ))
    }
    fn schedule(&self, f: Rc<dyn Fn()>, delay: f64) -> JsResult<N> {
        self.event(vec![self.string("schedule"), N::Num(delay)]);
        if self.name == "schedule-throws" {
            return Err(error("Error", "schedule-fail"));
        }
        self.timers.borrow_mut().push(f);
        Ok(N::Num(1.))
    }
    fn fetch_positions(&self) -> JsResult<N, JsPromise<N>> {
        self.event(vec![self.string("fetch")]);
        let h = self.self_rc();
        Ok(Box::pin(Later {
            first: false,
            callback: Some(Box::new(move || {
                if h.name == "fetch-reject" {
                    return Err(error("Error", "fetch-fail"));
                }
                if h.name == "dispose-during" {
                    h.state.is_disposed.set(true);
                }
                if h.name == "loaded-mutation" {
                    h.store.get("loaded").set("extra", N::B(true));
                }
                Ok(h.positions.clone())
            })),
        }))
    }
    fn store(&self) -> JsResult<N> {
        Ok(self.store.clone())
    }
    fn sync_positions(&self, p: N) -> JsResult<N, ()> {
        assert!(p.same(&self.positions), "sync positions identity");
        self.event(vec![self.string("sync")]);
        if self.name == "sync-throws" {
            Err(error("Error", "sync-fail"))
        } else {
            Ok(())
        }
    }
}
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(format!(
                "{}/tests/session-routing-source.cjs",
                env!("CARGO_MANIFEST_DIR")
            ))
            .output()
            .expect("Node source oracle");
        assert!(
            output.status.success(),
            "source oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("source oracle JSON")
    })
}
fn error_input(name: &str) -> N {
    let data = match name {
        "nested-error" => json!({"error":{"message":"nested","code":27}}),
        "plain-error" => json!({"message":"plain"}),
        "string-error" => json!("text"),
        _ => json!({"c":"1000","m":"expired"}),
    };
    let e = error("Error", "original");
    if name != "raw-error" && name != "request-error" && name != "response-getter" {
        e.set("response", N::from(json!({"status":401,"data":data})));
    }
    if name == "request-error" {
        e.set("request", N::obj(vec![]));
    }
    if name != "raw-error" {
        let config = N::from(json!({"headers":{}}));
        if name == "already-retried" {
            config.set("_retry", N::B(true));
        }
        e.set("config", config);
    }
    e
}
fn run_group(group: &str, expected: usize) {
    let rows = oracle()[group].as_array().expect(group);
    assert_eq!(rows.len(), expected, "independent {group} case floor");
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let h = Host::new(group, name);
        let label = format!("{group}/{name}");
        let mut same = false;
        let mut input_error = N::U;
        let mut concurrent = None;
        let result: JsResult<N> = match group {
            "watchdog" => check_last_received(h.as_ref()).map(|()| N::U),
            "connect" => {
                let result = connect_and_subscribe(h.clone());
                if result.is_ok() && name == "missing" {
                    connect_and_subscribe(h.clone())
                        .unwrap_or_else(|_| panic!("missing second call"));
                }
                if result.is_ok() && name == "error-event" {
                    h.handler("socket", 0, N::from(json!({"message":""})))
                        .map(|_| N::U)
                } else {
                    result.map(|()| N::U)
                }
            }
            "refresh" => refresh_session_and_reconnect(h.clone(), "GRVT error 1000").map(|()| N::U),
            "throttle" => throttled_fetch_positions(h.clone()).map(|()| N::U),
            "fetch" => Ok(h.promise(execute_position_fetch(h.clone()))),
            _ => {
                let instance = match group {
                    "market" => create_api(h.clone(), h.opts.clone(), h.exchange.clone()),
                    "edge" => create_edge_api(h.clone(), h.opts.clone(), h.exchange.clone()),
                    _ => create_trading_api(h.clone(), h.opts.clone(), h.exchange.clone()),
                }
                .unwrap_or_else(|_| panic!("factory"));
                assert!(
                    instance.same(&h.instance.borrow()),
                    "factory returns original instance"
                );
                if name == "production" || name == "testnet" {
                    same = true;
                    Ok(h.string("instance"))
                } else if name.starts_with("headers-") {
                    let config = N::from(json!({"headers":{}}));
                    let host = h.clone();
                    let expected = config.get("headers");
                    let headers = expected.clone();
                    headers.set("set",N::F(Rc::new(move |receiver,args|{assert!(receiver.same(&expected),"header setter receiver");host.event(vec![host.string("header-set"),args[0].clone(),args[1].clone()]);receiver.set(&args[0].text(),args[1].clone());if host.name=="headers-mutate"{host.exchange.set("apiExtras",N::from(json!({"sessionCookie":"mutated","accountId":"mutated-account"})));}Ok(N::U)})));
                    let result = h.handler("request", 0, config.clone());
                    same = result.as_ref().is_ok_and(|v| v.same(&config));
                    result
                } else if [
                    "cookie",
                    "empty-cookie",
                    "proxy",
                    "proxy-log-throws",
                    "body-error",
                ]
                .contains(&name)
                {
                    let response = N::from(
                        json!({"data":if name=="body-error"{json!({"c":7,"m":"body"})}else{json!({"ok":true})},"headers":{"set-cookie":if name=="empty-cookie"{json!("gravity=; Path=/")}else{json!(["other=0","gravity=abc=truncated; Path=/"])},"x-grvt-account-id":"account"}}),
                    );
                    if name == "proxy" || name == "proxy-log-throws" {
                        response
                            .get("headers")
                            .set("x-ts-grvt-session", h.string("proxy"));
                    }
                    let result = h.handler("response", 0, response.clone());
                    same = result.as_ref().is_ok_and(|v| v.same(&response));
                    result
                } else {
                    input_error = error_input(name);
                    let result = h.handler("response", 1, input_error.clone());
                    if name == "concurrent" {
                        concurrent = Some(h.handler("response", 1, error_input("auth-success")));
                    }
                    result
                }
            }
        };
        let initial = N::array(h.events.borrow().clone());
        let async_return = result.as_ref().is_ok_and(|v| matches!(v, N::P(_)));
        if group == "throttle" && !h.timers.borrow().is_empty() {
            if name == "no-pending" {
                h.state.has_pending_position_events.set(false);
            }
            if name == "dispose-timer" {
                h.state.is_disposed.set(true);
            }
            let f = h.timers.borrow_mut().remove(0);
            f();
        }
        h.drain();
        let result = match result {
            Ok(N::P(j)) => j.result.borrow().clone().expect("returned promise settled"),
            r => r,
        };
        let (result, err, original, response_identity) = match result {
            Ok(v) => (v, N::Null, false, false),
            Err(e) => {
                let original = e.get("originalError").same(&input_error);
                let identity = e.get("response").same(&input_error.get("response"));
                (
                    N::U,
                    N::obj(vec![("name", e.get("name")), ("message", e.get("message"))]),
                    original,
                    identity,
                )
            }
        };
        if group == "trading" && name == "prelog-throws" {
            concurrent = Some(h.handler("response", 1, error_input("auth-success")));
            h.drain();
        }
        if let Some(r) = concurrent {
            let r = r.unwrap_or_else(|_| panic!("concurrent callback sync failure"));
            if let N::P(j) = r {
                if let Some(Err(e)) = j.result.borrow().clone() {
                    h.event(vec![h.string("concurrent-error"), e.get("message")]);
                }
            }
        }
        assert_native(&result, &row["result"], &format!("{label} result"));
        assert_native(&err, &row["error"], &format!("{label} error"));
        assert_native(&initial, &row["initial"], &format!("{label} eager effects"));
        assert_native(
            &N::array(h.events.borrow().clone()),
            &row["events"],
            &format!("{label} effects"),
        );
        assert_eq!(same, row["same"], "{label} native input identity");
        assert_eq!(
            original, row["original"],
            "{label} native original error identity"
        );
        assert_eq!(
            response_identity, row["responseIdentity"],
            "{label} native response identity"
        );
        assert_eq!(
            async_return, row["asyncReturn"],
            "{label} native async return"
        );
        assert_native(
            &h.exchange.get("apiExtras"),
            &row["extras"],
            &format!("{label} extras"),
        );
        assert_native(
            &h.exchange.get("grvtClient").get("cookie"),
            &row["clientCookie"],
            &format!("{label} SDK cookie"),
        );
        let state = N::obj(vec![
            ("disposed", N::B(h.state.is_disposed.get())),
            ("failed", N::B(h.state.authentication_failed.get())),
            ("authenticated", N::B(h.state.authenticated.get())),
            ("inFlight", N::B(h.state.session_refresh_in_flight.get())),
            ("attempts", N::Num(h.state.session_refresh_attempts.get())),
            ("reported", N::B(h.state.reported_missing_proxy_host.get())),
            ("scheduled", N::B(h.state.position_fetch_scheduled.get())),
            ("pending", N::B(h.state.has_pending_position_events.get())),
            ("lastFetch", N::Num(h.state.last_position_fetch_time.get())),
            ("lastReceived", N::Num(h.last.get())),
            ("pingSent", N::B(h.ping.get())),
        ]);
        assert_native(&state, &row["state"], &format!("{label} state"));
        assert_native(&h.store, &row["store"], &format!("{label} store"));
    }
}
#[test]
fn watchdog_source() {
    run_group("watchdog", 11)
}
#[test]
fn connect_source() {
    run_group("connect", 7)
}
#[test]
fn refresh_source() {
    run_group("refresh", 13)
}
#[test]
fn throttle_source() {
    run_group("throttle", 9)
}
#[test]
fn fetch_source() {
    run_group("fetch", 8)
}
#[test]
fn market_api_source() {
    run_group("market", 10)
}
#[test]
fn edge_api_source() {
    run_group("edge", 8)
}
#[test]
fn trading_api_source() {
    run_group("trading", 18)
}
