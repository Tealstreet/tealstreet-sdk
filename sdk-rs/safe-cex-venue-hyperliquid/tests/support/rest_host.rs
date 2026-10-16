use super::cache::{self, N, arr, eq, object, s};
use safe_cex_venue_hyperliquid::{
    api_errors::{ApiErrorClass, ApiErrorHost},
    cross_cache::CrossCacheHost,
    helper_tasks::HelperTaskHost,
    income::{IncomeFuture, IncomeHost},
    json_boundaries::JsPayloadHost,
    market_helpers::MetaTupleHost,
    market_startup::StartupHost,
    spot_json::JsString,
};
use serde_json::Value;
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
pub fn decode(v: &Value) -> N {
    match v[0].as_str().unwrap() {
        "u" => N::U,
        "null" => N::Null,
        "bool" => N::B(v[1].as_bool().unwrap()),
        "number" => N::Num(if v[1] == "nan" {
            f64::NAN
        } else {
            f64::from_bits(u64::from_str_radix(v[1].as_str().unwrap(), 16).unwrap())
        }),
        "string" => N::S(JsString(
            v[1].as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u16)
                .collect(),
        )),
        "array" => arr(v[1].as_array().unwrap().iter().map(decode).collect()),
        "object" => {
            let n = object(vec![]);
            let N::O(o) = &n else { unreachable!() };
            o.borrow_mut().fields = v[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| (p[0].as_str().unwrap().into(), decode(&p[1]), true))
                .collect();
            n
        }
        _ => panic!("input tag"),
    }
}
pub fn error(name: &str, msg: &str) -> N {
    let n = object(vec![("name", s(name)), ("message", s(msg))]);
    if let N::O(o) = &n {
        for p in &mut o.borrow_mut().fields {
            p.2 = false;
        }
    }
    n
}
fn text(v: &N) -> JsString {
    match v {
        N::S(t) => t.clone(),
        N::Num(n) => JsString(safe_cex_num::js_number_string(*n).encode_utf16().collect()),
        N::U => JsString("undefined".encode_utf16().collect()),
        N::Null => JsString("null".encode_utf16().collect()),
        N::B(b) => JsString(b.to_string().encode_utf16().collect()),
        N::A(a) => {
            let mut u = vec![];
            for (i, v) in a.borrow().iter().enumerate() {
                if i > 0 {
                    u.push(44)
                }
                if !matches!(v, N::U | N::Null) {
                    u.extend(text(v).0)
                }
            }
            JsString(u)
        }
        N::O(o) if o.borrow().fields.len() == 2 && o.borrow().fields.iter().all(|p| !p.2) => {
            let o = o.borrow();
            let a = o.fields.iter().find(|p| p.0 == "name").unwrap();
            let b = o.fields.iter().find(|p| p.0 == "message").unwrap();
            let mut u = text(&a.1).0;
            u.extend(": ".encode_utf16());
            u.extend(text(&b.1).0);
            JsString(u)
        }
        _ => JsString("[object Object]".encode_utf16().collect()),
    }
}
struct Shared {
    base: cache::Host,
    input: N,
    effects: Vec<N>,
    promises: Vec<Result<N, N>>,
    tasks: Vec<Pin<Box<dyn Future<Output = ()>>>>,
    post_count: usize,
    log_count: usize,
    request_body: Option<N>,
    config: N,
    closes: usize,
}
#[derive(Clone)]
pub struct Host {
    inner: Rc<RefCell<Shared>>,
}
impl Host {
    pub fn new(input: N) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Shared {
                base: cache::Host::default(),
                input,
                effects: vec![],
                promises: vec![],
                tasks: vec![],
                post_count: 0,
                log_count: 0,
                request_body: None,
                config: object(vec![(
                    "headers",
                    object(vec![("Content-Type", s("application/json"))]),
                )]),
                closes: 0,
            })),
        }
    }
    pub fn input(&self) -> N {
        self.inner.borrow().input.clone()
    }
    pub fn effects(&self) -> N {
        arr(self.inner.borrow().effects.clone())
    }
    pub fn request_body(&self) -> Option<N> {
        self.inner.borrow().request_body.clone()
    }
    pub fn closes(&self) -> usize {
        self.inner.borrow().closes
    }
    pub fn truthy(&self, v: &N) -> bool {
        self.inner.borrow().base.truthy(v)
    }
    pub fn field(&mut self, v: &N, k: &str) -> N {
        self.get(v, k).unwrap()
    }
    fn promise(&self, result: Result<N, N>) -> N {
        let mut st = self.inner.borrow_mut();
        let index = st.promises.len();
        st.promises.push(result);
        object(vec![("$promise", N::Num(index as f64))])
    }
    pub fn drain(&self) {
        let w = Waker::noop();
        let mut cx = Context::from_waker(w);
        for _ in 0..1000 {
            let tasks = std::mem::take(&mut self.inner.borrow_mut().tasks);
            if tasks.is_empty() {
                return;
            }
            let mut pending = vec![];
            for mut task in tasks {
                if task.as_mut().poll(&mut cx).is_pending() {
                    pending.push(task)
                }
            }
            self.inner.borrow_mut().tasks.extend(pending);
        }
        panic!("REST bounded task drain exhausted")
    }
    fn client_post(&mut self, label: &str, args: &[N]) -> Result<N, N> {
        self.inner
            .borrow_mut()
            .effects
            .push(arr([vec![s("post"), s(label)], args.to_vec()].concat()));
        let input = self.input();
        let pos = self.inner.borrow().post_count;
        self.inner.borrow_mut().post_count += 1;
        if label == "direct" {
            let pages = self.field(&input, "pages");
            let page = self.get(&pages, &pos.to_string())?;
            return Ok(self.promise(Ok(page)));
        }
        let plan = self.field(&input, "plan");
        let next = self.get(&plan, &pos.to_string())?;
        if matches!(next, N::U) {
            return Err(error("Error", "unexpected post"));
        }
        if let Some(body) = self.request_body() {
            assert!(eq(&body, &args[1]), "REST request body reused")
        } else {
            self.inner.borrow_mut().request_body = Some(args[1].clone());
        }
        assert!(
            eq(&args[2], &self.inner.borrow().config),
            "REST request config reused"
        );
        let err = self.get(&next, "error")?;
        if self.truthy(&err) {
            let synchronous = self.get(&next, "sync")?;
            if self.truthy(&synchronous) {
                Err(err)
            } else {
                Ok(self.promise(Err(err)))
            }
        } else {
            let data = self.get(&next, "data")?;
            Ok(self.promise(Ok(object(vec![("data", data)]))))
        }
    }
}
impl JsPayloadHost for Host {
    type Value = N;
    type Error = N;
    fn string_units<'a>(&self, v: &'a N) -> Option<&'a [u16]> {
        if let N::S(t) = v { Some(&t.0) } else { None }
    }
    fn is_object(&self, v: &N) -> bool {
        self.inner.borrow().base.is_object(v)
    }
    fn is_array(&mut self, v: &N) -> Result<bool, N> {
        self.inner.borrow_mut().base.is_array(v).map_err(|e| s(&e))
    }
    fn is_nullish(&self, v: &N) -> bool {
        matches!(v, N::U | N::Null)
    }
    fn undefined(&self) -> N {
        N::U
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if matches!(v, N::U | N::Null) {
            return Err(error(
                "TypeError",
                &format!(
                    "Cannot read properties of {} (reading '{k}')",
                    if matches!(v, N::U) {
                        "undefined"
                    } else {
                        "null"
                    }
                ),
            ));
        }
        if matches!(v, N::S(_)) && matches!(k, "toLowerCase" | "startsWith" | "includes") {
            return Ok(N::Builtin(k.into()));
        }
        self.inner.borrow_mut().base.get(v, k).map_err(|e| s(&e))
    }
    fn parse_json(&mut self, v: &[u16]) -> Result<N, N> {
        self.inner
            .borrow_mut()
            .base
            .parse_json(v)
            .map_err(|e| s(&e))
    }
    fn json_stringify(&mut self, v: &N) -> Result<Option<JsString>, N> {
        self.inner
            .borrow_mut()
            .base
            .json_stringify(v)
            .map_err(|e| s(&e))
    }
    fn to_string(&mut self, v: &N) -> Result<JsString, N> {
        Ok(text(v))
    }
}
impl CrossCacheHost for Host {
    fn null(&self) -> N {
        N::Null
    }
    fn is_undefined(&self, v: &N) -> bool {
        matches!(v, N::U)
    }
    fn truthy(&self, v: &N) -> bool {
        self.inner.borrow().base.truthy(v)
    }
    fn session_storage(&mut self) -> Result<N, N> {
        Ok(N::U)
    }
    fn literal(&self, t: &JsString) -> N {
        N::S(t.clone())
    }
    fn object(&mut self) -> Result<N, N> {
        Ok(object(vec![]))
    }
    fn set(&mut self, o: &N, k: &N, v: &N) -> Result<(), N> {
        self.inner.borrow_mut().base.set(o, k, v).map_err(|e| s(&e))
    }
    fn define_data(&mut self, o: &N, k: &JsString, v: &N) -> Result<(), N> {
        self.inner
            .borrow_mut()
            .base
            .define_data(o, k, v)
            .map_err(|e| s(&e))
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let a = args.first().cloned().unwrap_or(N::U);
        if let N::Builtin(k) = f {
            match k.as_str() {
                "resolve" => {
                    self.inner
                        .borrow_mut()
                        .effects
                        .push(arr(vec![s("resolve"), a]));
                    let input = self.input();
                    return self.get(&input, "resolved");
                }
                "key" => {
                    let input = self.input();
                    let pages = self.field(&input, "pages");
                    let N::A(pages) = pages else { panic!("pages") };
                    let originals = pages
                        .borrow()
                        .iter()
                        .flat_map(|p| {
                            let N::A(p) = p else { panic!("page") };
                            p.borrow().clone()
                        })
                        .collect::<Vec<_>>();
                    assert!(
                        originals.iter().any(|row| eq(row, &a)),
                        "REST key callback original row identity"
                    );
                    let key = self.get(&a, "id")?;
                    self.inner
                        .borrow_mut()
                        .effects
                        .push(arr(vec![s("key"), key.clone()]));
                    let input = self.input();
                    let throwing = self.field(&input, "keyThrow");
                    if self.truthy(&throwing) {
                        return Err(N::U);
                    }
                    return Ok(key);
                }
                "cap" => {
                    self.inner
                        .borrow_mut()
                        .effects
                        .push(arr([vec![s("cap")], args.to_vec()].concat()));
                    return Ok(N::U);
                }
                "log" => {
                    self.inner
                        .borrow_mut()
                        .effects
                        .push(arr([vec![s("log")], args.to_vec()].concat()));
                    let n = {
                        let mut st = self.inner.borrow_mut();
                        st.log_count += 1;
                        st.log_count
                    };
                    let input = self.input();
                    let limit = self.get(&input, "logThrow")?;
                    if self.equal_number(&limit, n as f64) {
                        return Err(s("logger"));
                    }
                    return Ok(N::U);
                }
                "toLowerCase" => {
                    let input = self.input();
                    let custom = self.field(&input, "lowerCustom");
                    if self.truthy(&custom) {
                        return Ok(object(vec![
                            ("startsWith", N::Builtin("custom-starts".into())),
                            ("includes", N::Builtin("custom-includes".into())),
                        ]));
                    }
                    return Ok(N::S(JsString(
                        String::from_utf16_lossy(&text(r).0)
                            .to_lowercase()
                            .encode_utf16()
                            .collect(),
                    )));
                }
                "custom-starts" => return Ok(N::B(true)),
                "custom-includes" => return Ok(N::B(false)),
                "startsWith" => {
                    let input = self.input();
                    let override_value = self.field(&input, "startsOverride");
                    return Ok(N::B(
                        self.truthy(&override_value) || text(r).0.starts_with(&text(&a).0),
                    ));
                }
                "includes" => {
                    let input = self.input();
                    let override_value = self.field(&input, "includesOverride");
                    let needle = text(&a).0;
                    return Ok(N::B(
                        self.truthy(&override_value)
                            || needle.is_empty()
                            || text(r).0.windows(needle.len()).any(|v| v == needle),
                    ));
                }
                _ => {
                    if let Some(label) = k.strip_prefix("post-") {
                        return self.client_post(label, args);
                    }
                }
            }
        }
        self.inner
            .borrow_mut()
            .base
            .call(f, r, args)
            .map_err(|e| s(&e))
    }
    fn parse_json_value(&mut self, v: &N) -> Result<N, N> {
        let t = self.to_string(v)?;
        self.parse_json(&t.0)
    }
    fn property_key(&mut self, v: &N) -> Result<N, N> {
        Ok(N::S(text(v)))
    }
    fn entries(&mut self, v: &N) -> Result<N, N> {
        self.inner.borrow_mut().base.entries(v).map_err(|e| s(&e))
    }
    fn iterator_step(&mut self, v: &N) -> Result<Option<N>, N> {
        self.inner
            .borrow_mut()
            .base
            .iterator_step(v)
            .map_err(|e| s(&e))
    }
    fn entry_pair(&mut self, v: &N) -> Result<[N; 2], N> {
        self.inner
            .borrow_mut()
            .base
            .entry_pair(v)
            .map_err(|e| s(&e))
    }
    fn close_after_throw(&mut self, _: &N) {
        self.inner.borrow_mut().closes += 1;
    }
    fn spread(&mut self, v: &N) -> Result<N, N> {
        self.inner.borrow_mut().base.spread(v).map_err(|e| s(&e))
    }
    fn greater_than_500(&mut self, v: &N) -> Result<bool, N> {
        self.inner
            .borrow_mut()
            .base
            .greater_than_500(v)
            .map_err(|e| s(&e))
    }
}
struct Yield {
    value: Option<Result<N, N>>,
    yielded: bool,
}
impl Future for Yield {
    type Output = Result<N, N>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.yielded {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(self.value.take().unwrap())
        }
    }
}
impl IncomeHost for Host {
    fn number(&mut self, v: &N) -> Result<f64, N> {
        use safe_cex_num::{JsPrimitive, js_to_number};
        let p = match v {
            N::U => JsPrimitive::Undefined,
            N::Null => JsPrimitive::Null,
            N::Num(n) => JsPrimitive::Number(*n),
            N::B(b) => JsPrimitive::Boolean(*b),
            _ => JsPrimitive::String(String::from_utf16_lossy(&text(v).0)),
        };
        js_to_number(&p).map_err(|_| s("number"))
    }
    fn number_value(&self, n: f64) -> N {
        N::Num(n)
    }
    fn equal_number(&self, v: &N, n: f64) -> bool {
        matches!(v,N::Num(v)if *v==n)
    }
    fn parse_float(&mut self, t: &JsString) -> Result<f64, N> {
        Ok(safe_cex_num::parse_float(&String::from_utf16_lossy(&t.0)))
    }

    fn array(&mut self, v: &[N]) -> Result<N, N> {
        Ok(arr(v.to_vec()))
    }
    fn new_set(&mut self) -> Result<N, N> {
        Ok(N::Set(Rc::new(RefCell::new(vec![]))))
    }
    fn math_max(&mut self, a: f64, b: f64) -> Result<f64, N> {
        Ok(if a.is_nan() || b.is_nan() {
            f64::NAN
        } else if a == 0. && b == 0. {
            if a.is_sign_negative() && b.is_sign_negative() {
                -0.
            } else {
                0.
            }
        } else {
            a.max(b)
        })
    }
    fn await_value(&mut self, v: N) -> IncomeFuture<'_, N, N> {
        let result = if let N::O(o) = &v {
            let p = o
                .borrow()
                .fields
                .iter()
                .find(|p| p.0 == "$promise")
                .map(|p| cache::num(&p.1) as usize);
            p.map(|i| self.inner.borrow().promises[i].clone())
                .unwrap_or_else(|| Ok(v.clone()))
        } else {
            Ok(v)
        };
        Box::pin(Yield {
            value: Some(result),
            yielded: false,
        })
    }
}
impl ApiErrorHost for Host {
    fn new_error(&mut self, _: ApiErrorClass, v: &N) -> Result<N, N> {
        Ok(error("Error", &String::from_utf16_lossy(&text(v).0)))
    }
    fn define_field(&mut self, o: &N, k: &str, v: &N) -> Result<(), N> {
        self.define_data(o, &JsString(k.encode_utf16().collect()), v)
    }
    fn set_property(&mut self, o: &N, k: &str, v: &N) -> Result<(), N> {
        self.set(o, &s(k), v)
    }
    fn literal(&self, t: &str) -> N {
        s(t)
    }
    fn truthy(&self, v: &N) -> bool {
        CrossCacheHost::truthy(self, v)
    }
    fn exception_from_value(&self, v: N) -> N {
        v
    }
}
impl MetaTupleHost for Host {
    fn first_two(&mut self, v: &N) -> Result<[N; 2], N> {
        Ok([self.get(v, "0")?, self.get(v, "1")?])
    }
    fn type_of(&self, v: &N) -> &'static str {
        match v {
            N::U => "undefined",
            N::B(_) => "boolean",
            N::Num(_) => "number",
            N::S(_) => "string",
            _ => "object",
        }
    }
    fn implicit_string(&mut self, v: &N) -> Result<JsString, N> {
        self.to_string(v)
    }
    fn log_warning(&mut self, _: &JsString) -> Result<(), N> {
        panic!("unbound logger must use captured Logged adapter")
    }
}
impl StartupHost for Host {
    fn object(&mut self) -> Result<N, N> {
        CrossCacheHost::object(self)
    }
    fn null(&self) -> N {
        N::Null
    }
    fn tuple(&mut self, v: &[N; 2]) -> Result<N, N> {
        Ok(arr(v.to_vec()))
    }
    fn string_value(&self, t: &JsString) -> N {
        N::S(t.clone())
    }
    fn call(&mut self, f: &N, r: &N, a: &[N]) -> Result<N, N> {
        CrossCacheHost::call(self, f, r, a)
    }
    fn await_value(&mut self, v: N) -> IncomeFuture<'_, N, N> {
        IncomeHost::await_value(self, v)
    }
    fn error_value(&self, e: &N) -> N {
        e.clone()
    }
    fn helper_promise(&mut self, result: Result<N, N>) -> N {
        self.promise(result)
    }
    fn info_endpoint(&mut self) -> Result<N, N> {
        Ok(s("/info"))
    }
    fn meta_info_type(&mut self) -> Result<N, N> {
        Ok(s("metaAndAssetCtxs"))
    }
    fn request_config(&self) -> N {
        self.inner.borrow().config.clone()
    }
    fn warning_severity(&mut self) -> Result<N, N> {
        Ok(s("warning"))
    }
}
impl HelperTaskHost<N, N> for Host {
    fn start_task(&self, mut task: Pin<Box<dyn Future<Output = ()>>>) {
        let mut cx = Context::from_waker(Waker::noop());
        if task.as_mut().poll(&mut cx).is_pending() {
            self.inner.borrow_mut().tasks.push(task)
        }
    }
    fn settle_return(&self, r: Result<N, N>) -> Pin<Box<dyn Future<Output = Result<N, N>>>> {
        Box::pin(std::future::ready(r))
    }
}
pub fn assert_value(actual: &N, expected: &Value, label: &str) {
    match expected[0].as_str().unwrap() {
        "number" => {
            let N::Num(n) = actual else {
                panic!("{label}: native number")
            };
            if expected[1] == "nan" {
                assert!(n.is_nan(), "{label}")
            } else {
                assert_eq!(
                    n.to_bits(),
                    u64::from_str_radix(expected[1].as_str().unwrap(), 16).unwrap(),
                    "{label}"
                )
            }
        }
        "error" => {
            let N::O(o) = actual else {
                panic!("{label}: native error")
            };
            let o = o.borrow();
            for (key, index) in [("name", 1), ("message", 2)] {
                let v = &o.fields.iter().find(|p| p.0 == key).unwrap().1;
                let N::S(t) = v else { panic!("error string") };
                assert_eq!(
                    t.0,
                    expected[index]
                        .as_str()
                        .unwrap()
                        .encode_utf16()
                        .collect::<Vec<_>>(),
                    "{label}"
                )
            }
        }
        "array" => {
            let N::A(a) = actual else {
                panic!("{label}: native array {actual:?}")
            };
            let a = a.borrow();
            let e = expected[1].as_array().unwrap();
            assert_eq!(a.len(), e.len(), "{label}");
            for (a, e) in a.iter().zip(e) {
                assert_value(a, e, label)
            }
        }
        "object" => {
            let N::O(o) = actual else {
                panic!("{label}: native object")
            };
            let o = o.borrow();
            let fields = o.fields.iter().filter(|p| p.2).collect::<Vec<_>>();
            let e = expected[1].as_array().unwrap();
            assert_eq!(fields.len(), e.len(), "{label}");
            for (p, e) in fields.iter().zip(e) {
                assert_eq!(p.0, e[0].as_str().unwrap(), "{label}: own key order");
                assert_value(&p.1, &e[1], label)
            }
        }
        _ => cache::assert_native(actual, expected, label),
    }
}
