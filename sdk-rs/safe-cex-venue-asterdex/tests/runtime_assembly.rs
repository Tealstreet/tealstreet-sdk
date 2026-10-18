#![cfg(not(target_arch = "wasm32"))]
use safe_cex_venue_asterdex::{api_runtime::*, exchange_runtime::*, runtime_host::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};
type R<T = N> = Result<T, N>;
type NativeFunction = Rc<dyn Fn(N, Vec<N>) -> R>;
#[derive(Clone)]
enum N {
    U,
    Null,
    B(bool),
    Num(f64),
    S(String),
    Big(String),
    O(Rc<RefCell<Vec<(String, N)>>>),
    A(Rc<RefCell<Vec<N>>>),
    F(NativeFunction),
    P(Rc<Job>),
    E(Rc<RefCell<Vec<(String, N)>>>),
}
impl N {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(v) => Self::B(v),
            Value::Number(v) => Self::Num(v.as_f64().unwrap()),
            Value::String(v) => Self::S(v),
            Value::Array(v) => Self::array(v.into_iter().map(Self::from).collect()),
            Value::Object(v) => Self::obj(v.into_iter().map(|(k, v)| (k, Self::from(v))).collect()),
        }
    }
    fn obj(fields: Vec<(String, N)>) -> Self {
        Self::O(Rc::new(RefCell::new(fields)))
    }
    fn array(values: Vec<N>) -> Self {
        Self::A(Rc::new(RefCell::new(values)))
    }
    fn get(&self, key: &str) -> Self {
        match self {
            Self::O(v) | Self::E(v) => v
                .borrow()
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or(Self::U),
            Self::A(v) if key == "length" => Self::Num(v.borrow().len() as f64),
            Self::A(v) => key
                .parse::<usize>()
                .ok()
                .and_then(|i| v.borrow().get(i).cloned())
                .unwrap_or(Self::U),
            _ => Self::U,
        }
    }
    fn set(&self, key: &str, value: Self) {
        let (Self::O(v) | Self::E(v)) = self else {
            panic!("object assignment")
        };
        let mut fields = v.borrow_mut();
        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
            *v = value
        } else {
            fields.push((key.into(), value))
        }
    }
    fn delete(&self, key: &str) {
        let (Self::O(v) | Self::E(v)) = self else {
            panic!("object deletion")
        };
        v.borrow_mut().retain(|(k, _)| k != key);
    }
    fn text(&self) -> String {
        match self {
            Self::U => "undefined".into(),
            Self::Null => "null".into(),
            Self::B(v) => v.to_string(),
            Self::Num(v) => safe_cex_num::js_number_string(*v),
            Self::S(v) | Self::Big(v) => v.clone(),
            _ => "[object Object]".into(),
        }
    }
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::O(a), Self::O(b)) | (Self::E(a), Self::E(b)) => Rc::ptr_eq(a, b),
            (Self::A(a), Self::A(b)) => Rc::ptr_eq(a, b),
            (Self::F(a), Self::F(b)) => Rc::ptr_eq(a, b),
            (Self::P(a), Self::P(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
    fn snapshot(&self) -> Self {
        match self {
            Self::O(v) => Self::obj(
                v.borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.snapshot()))
                    .collect(),
            ),
            Self::A(v) => Self::array(v.borrow().iter().map(Self::snapshot).collect()),
            _ => self.clone(),
        }
    }
    fn json(&self) -> Value {
        match self {
            Self::U => json!({"undefined":true}),
            Self::Null => Value::Null,
            Self::B(v) => json!(v),
            Self::Num(v) if !v.is_finite() || (*v == 0. && v.is_sign_negative()) => {
                json!({"number":self.text()})
            }
            Self::Num(v) => json!(v),
            Self::S(v) => json!(v),
            Self::Big(v) => json!({"bigint":v}),
            Self::F(_) => json!({"callable":true}),
            Self::O(v) | Self::E(v) => Value::Object(
                v.borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.json()))
                    .collect(),
            ),
            Self::A(v) => Value::Array(v.borrow().iter().map(Self::json).collect()),
            Self::P(_) => panic!("unresolved promise"),
        }
    }
    fn wire(&self) -> Value {
        match self {
            Self::O(v) | Self::E(v) => Value::Object(
                v.borrow()
                    .iter()
                    .filter(|(_, v)| !matches!(v, Self::U | Self::F(_)))
                    .map(|(k, v)| (k.clone(), v.wire()))
                    .collect(),
            ),
            Self::A(v) => Value::Array(v.borrow().iter().map(Self::wire).collect()),
            _ => self.json(),
        }
    }
}
impl std::fmt::Debug for N {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text())
    }
}
fn error(message: &str) -> N {
    N::E(Rc::new(RefCell::new(vec![
        ("name".into(), N::S("Error".into())),
        ("message".into(), N::S(message.into())),
    ])))
}
fn assert_native(actual: &N, expected: &Value, label: &str) {
    match actual {
        N::U => assert_eq!(
            expected,
            &json!({"undefined":true}),
            "{label} native undefined"
        ),
        N::Null => assert!(expected.is_null(), "{label} native null"),
        N::B(v) => assert_eq!(Some(*v), expected.as_bool(), "{label} native bool"),
        N::Num(v) => {
            if !v.is_finite() {
                assert_eq!(
                    expected["number"],
                    actual.text(),
                    "{label} native nonfinite"
                );
            } else if *v == 0. && v.is_sign_negative() {
                assert_eq!(expected["number"], "-0", "{label} native negative zero");
            } else {
                assert_eq!(
                    v.to_bits(),
                    expected.as_f64().expect(label).to_bits(),
                    "{label} native number bits"
                );
            }
        }
        N::S(v) => assert_eq!(Some(v.as_str()), expected.as_str(), "{label} native text"),
        N::Big(v) => assert_eq!(&expected["bigint"], v, "{label} native BigInt"),
        N::F(_) => assert_eq!(expected["callable"], true, "{label} native callable"),
        N::A(v) => {
            let expected = expected.as_array().expect(label);
            assert_eq!(
                v.borrow().len(),
                expected.len(),
                "{label} native array shape"
            );
            for (i, (v, e)) in v.borrow().iter().zip(expected).enumerate() {
                assert_native(v, e, &format!("{label}/{i}"));
            }
        }
        N::O(v) | N::E(v) => {
            let expected = expected.as_object().expect(label);
            let fields = v.borrow();
            assert_eq!(fields.len(), expected.len(), "{label} native object shape");
            for (k, v) in fields.iter() {
                assert!(expected.contains_key(k), "{label} native key {k}");
                assert_native(v, &expected[k], &format!("{label}/{k}"));
            }
        }
        N::P(_) => panic!("native unresolved promise {label}"),
    }
}
struct Job {
    future: RefCell<Option<Task<N>>>,
    result: RefCell<Option<R>>,
}
impl Job {
    fn poll(&self, cx: &mut Context<'_>) {
        let Some(mut task) = self.future.borrow_mut().take() else {
            return;
        };
        match task.as_mut().poll(cx) {
            Poll::Ready(result) => *self.result.borrow_mut() = Some(result),
            Poll::Pending => *self.future.borrow_mut() = Some(task),
        }
    }
}
struct Observer(Rc<Job>);
impl Future for Observer {
    type Output = R;
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<R> {
        self.0
            .result
            .borrow()
            .clone()
            .map(Poll::Ready)
            .unwrap_or(Poll::Pending)
    }
}
struct AwaitValue {
    value: N,
    yielded: bool,
}
impl Future for AwaitValue {
    type Output = R;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        if !self.yielded {
            self.yielded = true;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        match &self.value {
            N::P(job) => job
                .result
                .borrow()
                .clone()
                .map(Poll::Ready)
                .unwrap_or(Poll::Pending),
            value => Poll::Ready(Ok(value.clone())),
        }
    }
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture unexpectedly pending"),
    }
}
struct PromiseTask {
    task: Task<N>,
    adopting: Option<Rc<Job>>,
    host: Weak<Host>,
}
impl Future for PromiseTask {
    type Output = R;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        if let Some(job) = &self.adopting {
            return job
                .result
                .borrow()
                .clone()
                .map(Poll::Ready)
                .unwrap_or(Poll::Pending);
        }
        match self.task.as_mut().poll(cx) {
            Poll::Ready(Ok(value @ N::P(_))) => {
                let h = self.host.upgrade().unwrap();
                if let Some(original) = h.request_promise.borrow().as_ref() {
                    assert!(
                        value.same(original),
                        "native raw retry return promise identity"
                    );
                    h.event("retry.return", vec![h.boolean(value.same(original))]);
                }
                let N::P(job) = value else { unreachable!() };
                self.adopting = Some(job);
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            result => result,
        }
    }
}
struct Host {
    me: Weak<Host>,
    config: N,
    base_owner: RefCell<Option<N>>,
    events: RefCell<Vec<N>>,
    apis: RefCell<Vec<N>>,
    jobs: RefCell<Vec<Rc<Job>>>,
    now: Cell<f64>,
    response_reads: Cell<usize>,
    rejection_reads: Cell<usize>,
    clock_objects: RefCell<Vec<N>>,
    loop_callback: RefCell<Option<N>>,
    request_promise: RefCell<Option<N>>,
}
impl Host {
    fn new(config: N) -> Rc<Self> {
        Rc::new_cyclic(|me| Self {
            me: me.clone(),
            config: config.clone(),
            base_owner: RefCell::new(None),
            events: RefCell::new(vec![]),
            apis: RefCell::new(vec![]),
            jobs: RefCell::new(vec![]),
            response_reads: Cell::new(0),
            rejection_reads: Cell::new(0),
            clock_objects: RefCell::new(vec![]),
            loop_callback: RefCell::new(None),
            request_promise: RefCell::new(None),
            now: Cell::new(config.get("now").wire().as_f64().unwrap_or(100000.)),
        })
    }
    fn drive(&self, task: Task<N>) -> R {
        let job = Rc::new(Job {
            future: RefCell::new(Some(task)),
            result: RefCell::new(None),
        });
        self.jobs.borrow_mut().push(job.clone());
        for _ in 0..256 {
            let tasks = self.jobs.borrow().clone();
            for task in tasks {
                task.poll(&mut Context::from_waker(Waker::noop()));
            }
            if self
                .jobs
                .borrow()
                .iter()
                .all(|job| job.result.borrow().is_some())
            {
                return job.result.borrow().clone().unwrap();
            }
        }
        panic!("fixture task queue did not settle")
    }
    fn rc(&self) -> Rc<Self> {
        self.me.upgrade().unwrap()
    }
    fn f(&self, f: impl Fn(N, Vec<N>) -> R + 'static) -> N {
        N::F(Rc::new(f))
    }
    fn event(&self, name: &str, args: Vec<N>) {
        let mut row = vec![self.text(name)];
        row.extend(args.into_iter().map(|v| v.snapshot()));
        self.events.borrow_mut().push(N::array(row));
    }
    fn fail(&self, name: &str) -> R<()> {
        if self.config.get("fail").text() == name {
            Err(error("fixture failure"))
        } else {
            Ok(())
        }
    }
    fn log(&self) -> N {
        let h = self.rc();
        self.f(move |_, a| {
            h.event("log", a.clone());
            if h.config.get("failLog").text() == a[0].text() {
                return Err(error("fixture failure"));
            }
            Ok(N::U)
        })
    }
    fn owner(&self, options: N) -> N {
        let h = self.rc();
        self.object(vec![
            ("name", self.text("asterdex")),
            ("options", options),
            ("log", self.log()),
            (
                "emitAndLogError",
                self.f(move |_, a| {
                    h.event("emit", a);
                    Ok(N::U)
                }),
            ),
        ])
    }
    fn axios(&self, config: N, transport: N, exchange: N) -> N {
        let id = self.apis.borrow().len();
        let api = self.object(vec![("id", self.number(id as f64))]);
        let mut interceptors = vec![];
        for name in ["request", "response"] {
            let h = self.rc();
            let owner = api.clone();
            let target = name.to_string();
            let section = self.object(vec![(
                "use",
                self.f(move |_, a| {
                    owner.set(&(target.clone() + "OK"), a[0].clone());
                    owner.set(&(target.clone() + "Bad"), a[1].clone());
                    h.event(&(target.clone() + ".use"), vec![h.number(id as f64)]);
                    Ok(N::U)
                }),
            )]);
            interceptors.push((name, section));
        }
        api.set("interceptors", self.object(interceptors));
        let h = self.rc();
        api.set(
            "request",
            self.f(move |_, a| {
                h.event("api.request", a);
                let value = h.promise(Box::pin(async { Ok(N::from(json!({"retried":true}))) }));
                *h.request_promise.borrow_mut() = Some(value.clone());
                Ok(value)
            }),
        );
        self.apis.borrow_mut().push(api.clone());
        self.event(
            "create",
            vec![config, transport, self.boolean(self.truthy(&exchange))],
        );
        api
    }
    fn leaf(&self, name: &str, owner: N) -> N {
        let h = self.rc();
        let name = name.to_owned();
        self.f(move |_, a| {
            h.event(&name, a);
            h.fail(&name)?;
            Ok(match name.as_str() {
                "getPositionStateVersion" => owner.get("positionStateVersion"),
                "fetchPositions" => {
                    if h.truthy(&h.config.get("race")) {
                        owner.set(
                            "positionStateVersion",
                            h.number(owner.get("positionStateVersion").json().as_f64().unwrap() + 1.),
                        );
                    }
                    if h.config.get("disposeAt").text() == name {
                        owner.set("isDisposed", h.boolean(true));
                    }
                    N::from(json!([{"id":"position"}]))
                }
                "fetchOrders" => {
                    if h.config.get("disposeAt").text() == name {
                        owner.set("isDisposed", h.boolean(true));
                    }
                    N::from(json!([{"id":"order"}]))
                }
                "fetchMarkets" => {
                    if h.truthy(&h.config.get("empty")) {
                        N::array(vec![])
                    } else {
                        N::from(json!([{"symbol":"ETH","precision":{"price":0.1,"amount":0.01}},{"symbol":"BTCUSDT","precision":{"price":0.25,"amount":0.5}}]))
                    }
                }
                "fetchTickers" => N::from(json!([{"symbol":"BTCUSDT"}])),
                "reconcileBalanceAfterPrivateUpdate" => N::B(true),
                _ => N::U,
            })
        })
    }
    fn options(&self, value: &N) -> safe_cex_types::ExchangeOptions {
        let mut v = value.wire();
        let o = v.as_object_mut().unwrap();
        o.entry("key").or_insert(json!(""));
        o.entry("secret").or_insert(json!(""));
        serde_json::from_value(v).unwrap()
    }
    fn param_map(&self, value: &N) -> serde_json::Map<String, Value> {
        let N::O(fields) = value else {
            panic!("params object")
        };
        fields
            .borrow()
            .iter()
            .filter(|(_, v)| !matches!(v, N::U))
            .map(|(k, v)| (k.clone(), v.wire()))
            .collect()
    }
}
impl RuntimeHost for Host {
    type Value = N;
    fn get(&self, v: &N, key: &str) -> R {
        if key == "start"
            && self.truthy(&self.config.get("startGetter"))
            && self.clock_objects.borrow().iter().any(|o| o.same(v))
        {
            self.event("clock.start.get", vec![]);
            self.base_owner
                .borrow()
                .as_ref()
                .unwrap()
                .set("options", N::from(json!({"testnet":true})));
        }
        if key == "response"
            && matches!(v, N::E(_))
            && self.truthy(&self.config.get("responseGetter"))
        {
            let reads = self.rejection_reads.get() + 1;
            self.rejection_reads.set(reads);
            self.event("error.response", vec![self.number(reads as f64)]);
            if reads > 1 {
                return Ok(N::Null);
            }
        }

        if key == "data"
            && self.truthy(&self.config.get("dataGetter"))
            && v.same(&self.config.get("response"))
        {
            let reads = self.response_reads.get() + 1;
            self.response_reads.set(reads);
            self.event("response.data", vec![self.number(reads as f64)]);
            if reads == 4 {
                return Ok(N::from(json!({"code":-5025,"msg":"ordinary"})));
            }
        }

        if matches!(v, N::E(_)) && key == "config" && self.truthy(&self.config.get("getter")) {
            self.event("config.get", vec![]);
            v.get("response")
                .get("data")
                .set("msg", self.text("Unknown order"));
        }
        if matches!(v, N::U | N::Null) {
            let e = error(&format!(
                "Cannot read properties of {} (reading '{key}')",
                v.text()
            ));
            e.set("name", self.text("TypeError"));
            return Err(e);
        }
        if matches!(v, N::S(_)) && matches!(key, "includes" | "startsWith") {
            let key = key.to_string();
            return Ok(self.f(move |receiver, a| {
                Ok(N::B(if key == "includes" {
                    receiver.text().contains(&a[0].text())
                } else {
                    receiver.text().starts_with(&a[0].text())
                }))
            }));
        }
        if matches!(v, N::A(_)) && key == "find" {
            let h = self.rc();
            return Ok(self.f(move |receiver, a| {
                let N::A(values) = receiver else { panic!() };
                for value in values.borrow().iter() {
                    if h.truthy(&h.call(a[0].clone(), N::U, vec![value.clone()])?) {
                        return Ok(value.clone());
                    }
                }
                Ok(N::U)
            }));
        }
        Ok(v.get(key))
    }
    fn destructure(&self, value: N, keys: &[&str], binding: &str) -> R<Vec<N>> {
        if self.nullish(&value) {
            let e = error(&format!(
                "Cannot destructure property '{}' of '{}' as it is {}.",
                keys[0],
                binding,
                value.text()
            ));
            e.set("name", self.text("TypeError"));
            return Err(e);
        }
        keys.iter().map(|key| self.get(&value, key)).collect()
    }
    fn set(&self, o: &N, k: &str, v: N) -> R<()> {
        o.set(k, v);
        Ok(())
    }
    fn define(&self, o: &N, k: &str, v: N) -> R<()> {
        self.set(o, k, v)
    }
    fn delete(&self, o: &N, k: &str) -> R<()> {
        o.delete(k);
        Ok(())
    }
    fn call(&self, f: N, receiver: N, a: Vec<N>) -> R {
        let N::F(f) = f else {
            let e = error("not callable");
            e.set("name", self.text("TypeError"));
            return Err(e);
        };
        f(receiver, a)
    }
    fn function(&self, f: Function<N>) -> N {
        self.f(move |_, a| f(a))
    }
    fn object(&self, fields: Vec<(&str, N)>) -> N {
        N::obj(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
    fn spread(&self, target: &N, source: N) -> R<()> {
        match source {
            N::O(v) | N::E(v) => {
                for (k, v) in v.borrow().iter() {
                    target.set(k, v.clone())
                }
            }
            N::Null | N::U => {}
            _ => panic!("unselected spread primitive"),
        };
        Ok(())
    }
    fn array(&self, v: Vec<N>) -> N {
        N::array(v)
    }
    fn text(&self, v: &str) -> N {
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
    fn strict_equal(&self, a: &N, b: &N) -> bool {
        match (a, b) {
            (N::U, N::U) | (N::Null, N::Null) => true,
            (N::S(a), N::S(b)) | (N::Big(a), N::Big(b)) => a == b,
            (N::Num(a), N::Num(b)) => a == b,
            (N::B(a), N::B(b)) => a == b,
            _ => a.same(b),
        }
    }
    fn less(&self, a: N, b: N) -> R<bool> {
        Ok(safe_cex_num::js_number(Some(&a.wire())).unwrap()
            < safe_cex_num::js_number(Some(&b.wire())).unwrap())
    }
    fn greater_equal(&self, a: N, b: N) -> R<bool> {
        Ok(safe_cex_num::js_number(Some(&a.wire())).unwrap()
            >= safe_cex_num::js_number(Some(&b.wire())).unwrap())
    }
    fn subtract(&self, a: N, b: N) -> R {
        Ok(N::Num(
            safe_cex_num::js_number(Some(&a.wire())).unwrap()
                - safe_cex_num::js_number(Some(&b.wire())).unwrap(),
        ))
    }
    fn template(&self, parts: Vec<N>) -> R {
        Ok(N::S(parts.iter().map(N::text).collect()))
    }
    fn error(&self, message: N) -> N {
        error(&message.text())
    }
    fn constant(&self, name: &str) -> N {
        if name == "ASTERDEX_ENDPOINTS" {
            let roots = N::obj(vec![]);
            for (path, url) in safe_cex_venue_asterdex::tables::ASTERDEX_ENDPOINTS {
                let keys = path.split('.').collect::<Vec<_>>();
                if keys.len() == 2 {
                    if matches!(roots.get(keys[0]), N::U) {
                        roots.set(keys[0], N::obj(vec![]));
                    }
                    roots.get(keys[0]).set(keys[1], self.text(url));
                }
            }
            return roots;
        }
        if name == "isAsterdexRead" {
            return self.f(|_, a| {
                let c = &a[0];
                let r = safe_cex_transport::RestRequest {
                    method: match c.get("method") {
                        N::U => "".into(),
                        v => v.text(),
                    },
                    url: match c.get("url") {
                        N::U => "".into(),
                        v => v.text(),
                    },
                    ..Default::default()
                };
                Ok(N::B(
                    safe_cex_venue_asterdex::api_helpers::is_asterdex_read(&r),
                ))
            });
        }
        if name == "asterDEXVirtualClock" {
            let mut fields = vec![];
            for key in [
                "needsSync",
                "getCurrentTimeMs",
                "syncClientServerTime",
                "start",
                "stop",
            ] {
                let h = self.rc();
                let k = key.to_string();
                fields.push((
                    key,
                    self.f(move |_, a| match k.as_str() {
                        "needsSync" => Ok(h.boolean(h.truthy(&h.config.get("sync")))),
                        "getCurrentTimeMs" => Ok(h.number(h.now.get())),
                        _ => {
                            let name = match k.as_str() {
                                "syncClientServerTime" => "clock.sync",
                                "start" => "clock.start",
                                _ => "clock.stop",
                            };
                            h.event(name, a);
                            h.fail(name)?;
                            Ok(N::U)
                        }
                    }),
                ));
            }
            let object = self.object(fields);
            self.clock_objects.borrow_mut().push(object.clone());
            return object;
        }
        self.text(name)
    }
    fn start(&self, task: Task<N>) -> Task<N> {
        let job = Rc::new(Job {
            future: RefCell::new(Some(task)),
            result: RefCell::new(None),
        });
        job.poll(&mut Context::from_waker(Waker::noop()));
        self.jobs.borrow_mut().push(job.clone());
        Box::pin(Observer(job))
    }
    fn promise(&self, task: Task<N>) -> N {
        let job = Rc::new(Job {
            future: RefCell::new(Some(Box::pin(PromiseTask {
                task,
                adopting: None,
                host: self.me.clone(),
            }))),
            result: RefCell::new(None),
        });
        job.poll(&mut Context::from_waker(Waker::noop()));
        self.jobs.borrow_mut().push(job.clone());
        N::P(job)
    }
    fn await_value(&self, value: N) -> Task<N> {
        Box::pin(AwaitValue {
            value,
            yielded: false,
        })
    }
    fn import(&self, name: &str, a: Vec<N>) -> R {
        use safe_cex_venue_asterdex::{api_helpers as ah, helpers as hp};
        let arg = |i: usize| a.get(i).cloned().unwrap_or(N::U);
        let err = |e: String| error(&e);
        let wire: Vec<Value> = a.iter().map(N::wire).collect();
        match name {
            "Date.now" => Ok(self.number(self.now.get())),
            "multiply" => {
                Ok(self.number(a[0].wire().as_f64().unwrap() * a[1].wire().as_f64().unwrap()))
            }
            "safeCexAxiosCreate" => Ok(self.axios(arg(0), arg(1), arg(2))),
            "axios.create" => Ok(self.axios(arg(0), N::U, N::U)),
            "installTransportAdapter" => {
                self.event("tunnel", vec![a[0].get("id"), a[1].clone(), a[2].clone()]);
                Ok(N::U)
            }
            "axiosRetry" => {
                a[0].set("retry", a[1].clone());
                self.event("retry", vec![a[0].get("id"), a[1].get("retries")]);
                Ok(N::U)
            }
            "axiosRateLimit" => {
                self.event("rate", vec![a[0].get("id"), a[1].clone()]);
                Ok(a[0].clone())
            }
            "axiosRetry.isNetworkOrIdempotentRequestError" => {
                Ok(self.boolean(self.truthy(&a[0].get("network"))))
            }
            "onlyReads" => {
                let h = self.rc();
                let condition = a[0].clone();
                let read = a[1].clone();
                Ok(self.f(move |_, a| {
                    let c = a[0].get("config");
                    if !h.truthy(&h.call(read.clone(), N::U, vec![c])?) {
                        return Ok(N::B(false));
                    }
                    h.call(condition.clone(), N::U, a)
                }))
            }
            "Promise.reject" => {
                let error = arg(0);
                Ok(self.promise(Box::pin(async move { Err(error) })))
            }
            "resolveAsterDEXV3AuthContext" => {
                let auth = hp::resolve_aster_dex_v3_auth_context(&self.options(&a[0]));
                Ok(self.object(vec![
                    (
                        "userAddress",
                        auth.user_address.map(|v| self.text(&v)).unwrap_or(N::U),
                    ),
                    (
                        "signerAddress",
                        auth.signer_address.map(|v| self.text(&v)).unwrap_or(N::U),
                    ),
                    (
                        "signerPrivateKey",
                        auth.signer_private_key
                            .map(|v| self.text(&v))
                            .unwrap_or(N::U),
                    ),
                ]))
            }
            "resolveAsterDEXApiVersion" => Ok(self.text(
                match hp::resolve_aster_dex_api_version(&self.options(&a[0])) {
                    safe_cex_venue_asterdex::types::AsterDEXAPIVersion::V1 => "v1",
                    _ => "v3",
                },
            )),
            "createApiWithSecondaryKey" => {
                self.event("secondary.factory", vec![a[0].clone()]);
                let primary = self.call(a[2].clone(), N::U, vec![a[0].clone()])?;
                if self.truthy(&a[0].get("secondaryKey")) {
                    let o = a[0].snapshot();
                    o.set("key", o.get("secondaryKey"));
                    o.set("secret", self.text("secondary"));
                    o.set("overrideProxyUrl", self.text("secondary-proxy"));
                    self.call(a[2].clone(), N::U, vec![o])?;
                }
                Ok(N::array(vec![primary, N::U]))
            }
            "iteratorFirst" => Ok(a[0].get("0")),
            "stripQueryString" => Ok(self.text(ah::strip_query_string(&a[0].text()))),
            "isPublicEndpoint" => Ok(self.boolean(ah::is_public_endpoint(Some(&a[0].text())))),
            "isAsterDEXCodeSignedEndpoint" => {
                Ok(self.boolean(ah::is_aster_dex_code_signed_endpoint(
                    Some(&a[0].text()),
                    if matches!(arg(1), N::U) {
                        None
                    } else {
                        a.get(1).and_then(|v| {
                            if let N::S(v) = v {
                                Some(v.as_str())
                            } else {
                                None
                            }
                        })
                    },
                )))
            }
            "isAsterDEXV3PrivateKey" => {
                Ok(self.boolean(hp::is_aster_dex_v3_private_key(Some(&a[0].text()))))
            }
            "ethers.getAddress" => Ok(a[0].clone()),
            "buildV3Nonce" => ah::build_v3_nonce(&a[0].text(), self.now.get())
                .map(|v| self.text(&v))
                .map_err(|e| error(&e.to_string())),
            "getAsterDEXAsterChain" => {
                Ok(self.text(ah::get_aster_dex_aster_chain(self.truthy(&a[0]))))
            }
            "buildAsterDEXCodeQueryString" | "buildQueryString" => {
                ah::build_query_string(&self.param_map(&a[0]))
                    .map(|v| self.text(&v))
                    .map_err(|e| error(&e.message))
            }
            "generateV1Signature" => ready(ah::generate_v1_signature(&a[0].text(), &a[1].text()))
                .map(|v| self.text(&v))
                .map_err(err),
            "generateAsterDEXCodeMessageSignature" => {
                self.event("code.sign", a);
                Ok(self.text("code-signature"))
            }
            "buildV3SigningPayload" => ah::build_v3_signing_payload(&self.param_map(&a[0]))
                .map(|v| N::from(json!(v)))
                .map_err(|e| error(&e.message)),
            "JSON.stringify" => Ok(self.text(&serde_json::to_string(&a[0].wire()).unwrap())),
            "BigInt" => Ok(N::Big(a[0].text())),
            "ethers.AbiCoder.encode" => {
                self.event("abi.encode", a);
                Ok(self.text("encoded"))
            }
            "ethers.keccak256" => {
                self.event("keccak", a);
                Ok(self.text("hash"))
            }
            "ethers.getBytes" => {
                self.event("getBytes", a);
                Ok(N::from(json!([1, 2])))
            }
            "ethers.hashMessage" => {
                self.event("hashMessage", a);
                Ok(self.text("digest"))
            }
            "new ethers.SigningKey" => {
                self.event("signingKey", a);
                let h = self.rc();
                Ok(self.object(vec![(
                    "sign",
                    self.f(move |_, a| {
                        h.event("sign", a);
                        Ok(N::from(json!({"r":"r","s":"s","v":27})))
                    }),
                )]))
            }
            "ethers.Signature.from" => {
                self.event("signature.from", a);
                Ok(N::from(json!({"serialized":"v3-signature"})))
            }
            "getAsterDEXErrorDetail" => ah::get_aster_dex_error_detail(
                if matches!(arg(0), N::U) {
                    None
                } else {
                    Some(&wire[0])
                },
                if matches!(arg(1), N::U) {
                    None
                } else {
                    Some(&wire[1])
                },
            )
            .map(|v| self.text(&v))
            .map_err(|e| error(&e.message)),
            "shouldSuppressUnknownOrderError" => ah::should_suppress_unknown_order_error(
                if matches!(arg(0), N::U) {
                    None
                } else {
                    Some(&wire[0])
                },
                if matches!(arg(1), N::U) {
                    None
                } else {
                    Some(&wire[1])
                },
                if matches!(arg(2), N::U) {
                    None
                } else {
                    Some(&wire[2])
                },
            )
            .map(|v| self.boolean(v))
            .map_err(|e| error(&e.message)),
            "appendHttpFailure" => {
                self.event("http.failure", vec![a[0].clone(), a[1].get("status")]);
                Ok(self.text(&(a[0].text() + " HTTP fixture")))
            }
            "loop" => {
                *self.loop_callback.borrow_mut() = Some(a[0].clone());
                self.event("loop", vec![a[1].clone()]);
                Ok(N::U)
            }
            "withoutSigningFields" => {
                let kept = ah::without_signing_fields(&a[0].wire());
                let entries: Vec<(String, N)> = match &a[0] {
                    N::O(fields) => fields
                        .borrow()
                        .iter()
                        .filter(|(key, _)| kept.contains_key(key))
                        .cloned()
                        .collect(),
                    N::A(items) => kept
                        .keys()
                        .map(|key| {
                            (
                                key.clone(),
                                items.borrow()[key.parse::<usize>().unwrap()].clone(),
                            )
                        })
                        .collect(),
                    _ => vec![],
                };
                assert_eq!(
                    entries.len(),
                    kept.len(),
                    "native withoutSigningFields keys"
                );
                Ok(N::O(Rc::new(RefCell::new(entries))))
            }
            "hasRetriedSignature" => {
                Ok(self.boolean(
                    matches!(a[0], N::O(_))
                        && self.truthy(&a[0].get(
                            safe_cex_transport::signature_rejection::SIGNATURE_RETRY_CONFIG_KEY,
                        )),
                ))
            }
            "isBinanceStyleSignatureRejection" => {
                safe_cex_transport::signature_rejection::is_binance_style_signature_rejection(
                    &a[0].wire().into(),
                )
                .map(|v| self.boolean(v))
                .map_err(|e| error(&e.message))
            }
            _ => panic!("unbound primitive import {name}"),
        }
    }
}
impl AssemblyHost for Host {
    fn base_exchange(&self, name: N, options: N, store: N) -> R {
        let h = self.rc();
        let owner = self.object(vec![
            ("name", name),
            ("options", options),
            ("store", store),
            ("isDisposed", N::B(false)),
            (
                "isPublicDataOnly",
                self.boolean(self.truthy(&self.config.get("publicOnly"))),
            ),
            ("log", self.log()),
            (
                "initExternalPublicData",
                self.f(move |_, _| {
                    h.event("external", vec![]);
                    h.fail("external")?;
                    Ok(h.boolean(h.truthy(&h.config.get("external"))))
                }),
            ),
        ]);
        *self.base_owner.borrow_mut() = Some(owner.clone());
        Ok(owner)
    }
    fn member(&self, name: &'static str, owner: &N) -> R {
        let h = self.rc();
        let ex = owner.clone();
        Ok(match name {
            "start" => self.f(move |_, _| Ok(h.promise(start_exchange(h.clone(), ex.clone())))),
            "tick" => self.f(move |_, _| Ok(h.promise(tick(h.clone(), ex.clone())))),
            "dispose" => self.f(move |_, _| dispose(h.as_ref(), ex.clone())),
            _ => self.leaf(name, owner.clone()),
        })
    }
    fn construct(&self, name: &str, a: Vec<N>) -> R {
        let kind = if name == "AsterDEXPublicWebsocket" {
            "public"
        } else {
            "private"
        };
        self.event(
            "new.socket",
            vec![
                self.text(kind),
                self.boolean(self.base_owner.borrow().as_ref().unwrap().same(&a[0])),
                self.boolean(a[1].same(&a[0].get("store"))),
            ],
        );
        let h = self.rc();
        let h2 = self.rc();
        Ok(self.object(vec![
            ("kind", self.text(kind)),
            (
                "connectAndSubscribe",
                self.f(move |_, _| {
                    h.event("connect", vec![h.text(kind)]);
                    h.fail(&format!("connect.{kind}"))?;
                    Ok(N::U)
                }),
            ),
            (
                "dispose",
                self.f(move |_, _| {
                    h2.event("dispose", vec![h2.text(kind)]);
                    h2.fail(&format!("dispose.{kind}"))?;
                    Ok(N::U)
                }),
            ),
        ]))
    }
    fn base_dispose(&self, owner: N) -> R {
        self.event("base.dispose", vec![]);
        owner.set("isDisposed", N::B(true));
        Ok(N::U)
    }
}
fn options(_h: &Host) -> N {
    N::from(
        json!({"apiKey":"test-key","apiSecret":"test-secret","apiVersion":"v1","v3UserAddress":format!("0x{}","1".repeat(40)),"v3SignerAddress":format!("0x{}","2".repeat(40)),"v3SignerPrivateKey":format!("{:0>64}","1"),"testnet":false}),
    )
}
fn run_case(h: Rc<Host>, case: &Value) -> R {
    let group = case["group"].as_str().unwrap();
    let c = &h.config;
    if matches!(group, "constructor" | "start" | "tick" | "dispose") {
        let owner = Rc::downgrade(&h);
        let update = h.f(move |_, a| {
            owner.upgrade().unwrap().event("store.update", a);
            Ok(N::U)
        });
        let host = h.clone();
        let positions = h.f(move |_, a| {
            host.event("store.positions", a);
            Ok(N::U)
        });
        let store = h.object(vec![
            ("loaded", N::from(json!({"seed":true}))),
            ("update", update),
            ("addOrUpdatePositionsFull", positions),
        ]);
        let opts = if group == "constructor" {
            let opts = N::from(
                json!({"key":"primary","secret":"primary-secret","testnet":h.truthy(&c.get("testnet")),"tealstreetTunnel":h.truthy(&c.get("tunnel"))}),
            );
            if h.truthy(&c.get("secondary")) {
                opts.set("secondaryKey", h.text("secondary-key"));
            }
            opts
        } else if group == "start" {
            N::from(json!({"key":"primary","secret":"primary-secret"}))
        } else {
            N::obj(vec![])
        };
        let exchange = AsterDEXExchange::new(h.clone(), h.text("asterdex"), opts, store)?;
        let ex = exchange.owner.clone();
        if group == "constructor" {
            let N::O(fields) = &ex else { panic!() };
            let keys = fields.borrow().iter().map(|(k, _)| h.text(k)).collect();
            return Ok(h.object(vec![
                ("fields", N::array(keys)),
                ("version", ex.get("apiVersion")),
                (
                    "apis",
                    N::array(
                        ["primaryAPI", "secondaryAPI", "unlimitedAPI", "publicAPI"]
                            .iter()
                            .map(|key| ex.get(key).get("id"))
                            .collect(),
                    ),
                ),
                ("public", ex.get("publicWebsocket").get("kind")),
                ("private", ex.get("privateWebsocket").get("kind")),
            ]));
        }
        if group == "tick" {
            ex.set("isDisposed", h.boolean(h.truthy(&c.get("disposed"))));
            for (key, delta) in [
                ("lastBalanceReconciliationAt", 25000.),
                ("lastPositionSafetyReconciliationAt", 25000.),
                ("lastOrderSafetyReconciliationAt", 75000.),
            ] {
                if h.truthy(&c.get("fresh")) {
                    ex.set(key, h.number(h.now.get()));
                }
                if h.truthy(&c.get("boundary")) {
                    ex.set(key, h.number(h.now.get() - delta));
                }
            }
        }
        if group == "dispose" && h.truthy(&c.get("absent")) {
            ex.set("publicWebsocket", N::Null);
            ex.set("privateWebsocket", N::Null);
        }
        h.events.borrow_mut().clear();
        if group == "start" {
            h.drive(exchange.start())?;
        } else if group == "tick" {
            h.drive(exchange.tick())?;
            if h.truthy(&c.get("fireLoop")) {
                let token = N::from(json!({"kept":true}));
                let value = token.clone();
                let host = h.clone();
                let owner = ex.clone();
                ex.set(
                    "tick",
                    h.f(move |receiver, _| {
                        host.event(
                            "replacement.tick",
                            vec![host.boolean(receiver.same(&owner))],
                        );
                        Ok(value.clone())
                    }),
                );
                let callback = h.loop_callback.borrow().clone().unwrap();
                let returned = h.call(callback, N::from(json!({"foreign":true})), vec![])?;
                assert!(
                    returned.same(&token),
                    "native retained loop return identity"
                );
                h.event("loop.return", vec![h.boolean(returned.same(&token))]);
            }
        } else {
            exchange.dispose()?;
            return Ok(ex.get("isDisposed"));
        }
        return Ok(h.object(vec![
            ("positionTime", ex.get("lastPositionSafetyReconciliationAt")),
            ("orderTime", ex.get("lastOrderSafetyReconciliationAt")),
        ]));
    }
    if group == "public" {
        let owner = if h.truthy(&c.get("absent")) {
            N::U
        } else {
            h.owner(N::from(json!({"tealstreetTunnel":h.truthy(&c.get("tunnel")),"testnet":h.truthy(&c.get("testnet"))})))
        };
        return Ok(create_public_api(h, owner)?.get("id"));
    }
    let opts = options(&h);
    let owner = h.owner(h.object(vec![(
        "remoteSigner",
        if h.truthy(&c.get("remote")) {
            h.object(vec![])
        } else {
            N::U
        },
    )]));
    for (flag, key, value) in [
        ("noKey", "apiKey", ""),
        ("missingSecret", "apiSecret", ""),
        ("missingV3", "v3UserAddress", ""),
        ("badKey", "v3SignerPrivateKey", "bad"),
    ] {
        if h.truthy(&c.get(flag)) {
            opts.set(key, h.text(value));
        }
    }
    let api = create_api(h.clone(), owner, opts, N::U)?;
    h.events.borrow_mut().clear();
    match group {
        "request" => {
            let request = c.get("request");
            let result = h.drive(h.await_value(h.call(
                api.get("requestOK"),
                N::U,
                vec![request.clone()],
            )?))?;
            assert!(result.same(&request), "native original request identity");
            Ok(result)
        }
        "response" => {
            let response = c.get("response");
            let result = h.drive(h.await_value(h.call(
                api.get("responseOK"),
                N::U,
                vec![response.clone()],
            )?))?;
            if case["name"] == "signature-v1" {
                assert!(!result.same(&response), "native signature resend result");
            } else {
                assert!(result.same(&response), "native original response identity");
            }
            Ok(result)
        }
        "rejection" => {
            let error = error("network");
            let config = N::from(
                json!({"params":{"keep":"yes","timestamp":1,"signature":"old","recvWindow":1,"user":"old","signer":"old","nonce":"old"}}),
            );
            if h.truthy(&c.get("retried")) {
                config.set("__isRetryRequest", N::B(true));
            }
            error.set("config", config.clone());
            if h.truthy(&c.get("timestamp")) || h.truthy(&c.get("status")) {
                let data = if h.truthy(&c.get("timestamp")) {
                    N::from(
                        json!({"code":-1021,"msg":if h.truthy(&c.get("lowercase")){"timestamp"}else{"Timestamp outside window"}}),
                    )
                } else if h.truthy(&c.get("signature")) {
                    N::from(json!({"code":-1022,"msg":"Signature for this request is not valid."}))
                } else if h.truthy(&c.get("suppressed")) {
                    N::from(json!({"code":-2011,"msg":"Unknown order"}))
                } else if h.truthy(&c.get("errorField")) {
                    N::from(json!({"error":"denied"}))
                } else {
                    N::from(json!({"msg":"denied","code":-1}))
                };
                error.set(
                    "response",
                    h.object(vec![
                        (
                            "status",
                            if h.truthy(&c.get("status")) {
                                c.get("status")
                            } else {
                                h.number(400.)
                            },
                        ),
                        ("data", data),
                    ]),
                );
            }
            if h.truthy(&c.get("suppressed")) {
                config.set("__suppressAsterUnknownOrderError", N::B(true));
            }
            if h.truthy(&c.get("getter")) {
                error
                    .get("response")
                    .get("data")
                    .set("msg", h.text("denied"));
            }
            let result = h.drive(h.await_value(h.call(
                api.get("responseBad"),
                N::U,
                vec![error.clone()],
            )?));
            if case["name"] == "network" {
                assert!(
                    result.as_ref().unwrap_err().same(&error),
                    "native original rejection identity"
                );
            }
            result
        }
        "retry" => {
            let retry = api.get("retry");
            let delay = h.call(retry.get("retryDelay"), retry.clone(), vec![h.number(3.)])?;
            let error = h.object(vec![
                (
                    "config",
                    h.object(vec![("method", c.get("method")), ("url", c.get("url"))]),
                ),
                ("response", h.object(vec![("status", c.get("status"))])),
                ("network", c.get("network")),
            ]);
            let result = h.call(retry.get("retryCondition"), retry, vec![error])?;
            Ok(h.object(vec![("delay", delay), ("retry", result)]))
        }
        _ => panic!("unknown runtime group"),
    }
}
#[test]
#[ignore = "requires maintained TypeScript source oracle"]
fn runtime_bodies_match_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/runtime-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        data["staticFields"].as_array().unwrap().len(),
        2,
        "two source class static fields"
    );
    assert_native(
        &N::Num(AsterDEXExchange::<Host>::OPEN_INTEREST_POLL_INTERVAL_MS),
        &data["staticFields"][0],
        "native poll interval",
    );
    assert_native(
        &N::Num(AsterDEXExchange::<Host>::OPEN_INTEREST_BATCH_SIZE as f64),
        &data["staticFields"][1],
        "native batch size",
    );
    let fields = data["methodFields"].as_array().unwrap();
    assert_eq!(fields.len(), 44, "independent arrow field floor");
    assert_eq!(METHOD_FIELDS.len(), 44, "native arrow field floor");
    for (key, expected) in METHOD_FIELDS.iter().zip(fields) {
        assert_eq!(
            *key,
            expected.as_str().unwrap(),
            "native source field initializer order"
        );
    }
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 75, "independent runtime case floor");
    let mut groups = std::collections::BTreeMap::new();
    for (iteration, case) in cases.iter().enumerate() {
        let h = Host::new(N::from(case["config"].clone()));
        let result = run_case(h.clone(), case);
        let label = format!("{}:{} iteration {iteration}", case["group"], case["name"]);
        *groups
            .entry(case["group"].as_str().unwrap())
            .or_insert(0usize) += 1;
        if let Some(expected) = case.get("error") {
            let err = result.unwrap_err();
            assert!(matches!(err, N::E(_)), "{label} native genuine error");
            assert_native(&err.get("name"), &expected["name"], &label);
            assert_native(&err.get("message"), &expected["message"], &label);
        } else {
            assert!(
                result.is_ok(),
                "{label} native runtime result should resolve"
            );
            assert_native(&result.unwrap(), &case["value"], &label);
        }
        assert_native(&N::array(h.events.borrow().clone()), &case["trace"], &label);
    }
    assert_eq!(
        groups,
        std::collections::BTreeMap::from([
            ("start", 16),
            ("tick", 11),
            ("dispose", 4),
            ("constructor", 3),
            ("public", 3),
            ("request", 13),
            ("response", 7),
            ("rejection", 13),
            ("retry", 5)
        ]),
        "independent family floors; last iteration 74"
    );
}
#[test]
#[ignore = "requires maintained TypeScript source oracle"]
fn runtime_source_control_fails() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "runtime_bodies_match_source",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_RUNTIME_CONTROL", "window")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("native text") || text.contains("native number bits"),
        "{text}"
    );
}
