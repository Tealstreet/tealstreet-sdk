#![allow(dead_code)]
use super::{
    cache::{self, N, arr, object, s},
    public_host,
};
use safe_cex_venue_hyperliquid as venue;
use std::{
    cell::RefCell,
    collections::HashMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Waker},
};
use venue::{
    core_exchange_class::*,
    core_exchange_leaves::*,
    cross_cache::CrossCacheHost,
    helper_tasks::{HelperObserver, HelperTaskHost},
    json_boundaries::JsPayloadHost,
    spot_json::JsString,
};
pub fn same(a: &N, b: &N) -> bool {
    public_host::same(a, b)
}
pub struct State {
    pub public: public_host::Host,
    pub functions: HashMap<String, CoreExchangeCapture<N>>,
    pub predicates: HashMap<String, ExchangePredicate<N>>,
    pub metadata: Vec<(N, String, u8)>,
    pub getters: Vec<(N, String, N)>,
    pub events: Vec<String>,
    pub factories: Vec<(String, Vec<N>)>,
    pub supers: Vec<(N, [N; 3], N)>,
    pub tasks: Vec<Pin<Box<dyn Future<Output = ()>>>>,
    pub promises: Vec<(N, Rc<RefCell<Option<Result<N, N>>>>)>,
    pub imports: HashMap<String, N>,
    pub now: f64,
    pub prefix_fail: bool,
    pub marker: N,
    pub return_adoptions: usize,
    pub outer_adoptions: usize,
    pub finite_override: Option<N>,
    pub finds: Vec<N>,
    pub folds: Vec<(N, Vec<N>)>,
}
#[derive(Clone)]
pub struct Host {
    pub state: Rc<RefCell<State>>,
    pub lexical: Option<N>,
}
impl Default for Host {
    fn default() -> Self {
        let mut public = public_host::Host::default();
        let base = object(vec![("prototype", object(vec![]))]);
        let pc = venue::public_class::create_core_public_class(&mut public, &base).unwrap();
        let qc = venue::private_class::create_core_private_class(&mut public.inner, &base).unwrap();
        let imports = HashMap::from([
            ("HyperliquidPublicWebSocket".into(), pc),
            ("HyperliquidPrivateWebSocket".into(), qc),
        ]);
        Self {
            state: Rc::new(RefCell::new(State {
                public,
                functions: HashMap::new(),
                predicates: HashMap::new(),
                metadata: vec![],
                getters: vec![],
                events: vec![],
                factories: vec![],
                supers: vec![],
                tasks: vec![],
                promises: vec![],
                imports,
                now: 100.,
                prefix_fail: false,
                marker: object(vec![]),
                return_adoptions: 0,
                outer_adoptions: 0,
                finite_override: None,
                finds: vec![],
                folds: vec![],
            })),
            lexical: None,
        }
    }
}
impl Host {
    pub fn trace(&self, event: String) -> Result<(), N> {
        let mut st = self.state.borrow_mut();
        st.events.push(event);
        if st.prefix_fail {
            Err(st.marker.clone())
        } else {
            Ok(())
        }
    }
    pub fn put(&self, o: &N, k: &str, v: &N) {
        self.state.borrow_mut().public.inner.put(o, k, v, true)
    }
    pub fn get(&mut self, o: &N, k: &str) -> Result<N, N> {
        self.trace(format!("get:{k}"))?;
        let getter = self
            .state
            .borrow()
            .getters
            .iter()
            .find(|(p, n, _)| n == k && same(p, o))
            .map(|(_, _, v)| v.clone());
        if let Some(f) = getter {
            return self.invoke(&f, o, &[]);
        }
        if let N::O(v) = o {
            let proto = v.borrow().prototype.clone();
            if let Some(p) = proto {
                let getter = self
                    .state
                    .borrow()
                    .getters
                    .iter()
                    .find(|(p0, n, _)| n == k && same(p0, &p))
                    .map(|(_, _, v)| v.clone());
                if let Some(f) = getter {
                    return self.invoke(&f, o, &[]);
                }
            }
        }
        if let N::Builtin(b) = o {
            if [
                "Math",
                "Date",
                "Number",
                "Object",
                "MarginType",
                "PositionMode",
                "OrderSide",
                "PositionSide",
            ]
            .contains(&b.as_str())
            {
                return Ok(N::Builtin(format!("{b}.{k}")));
            }
        }
        if matches!(o, N::Num(_)) && k == "toString" {
            return Ok(N::Builtin("number.toString".into()));
        }
        if matches!(o, N::S(_)) && k == "substr" {
            return Ok(N::Builtin("string.substr".into()));
        }
        self.state.borrow_mut().public.get(o, k)
    }
    pub fn invoke(&mut self, f: &N, r: &N, a: &[N]) -> Result<N, N> {
        if let N::Builtin(id) = f {
            let function = self.state.borrow().functions.get(id).cloned();
            if let Some(c) = function {
                return call_core_exchange(self, &c, r, a);
            }
            let pred = self.state.borrow().predicates.get(id).cloned();
            if let Some(c) = pred {
                return run_exchange_predicate(self, &c, a.first().unwrap_or(&N::U));
            }
            self.trace(format!("call:{id}"))?;
            match id.as_str() {
                "Math.random" => return Ok(N::Num(0.5)),
                "number.toString" => {
                    assert!(matches!(r,N::Num(n)if *n==0.5));
                    return Ok(s("0.i"));
                }
                "string.substr" => {
                    assert!(same(r, &s("0.i")));
                    return Ok(s("i"));
                }
                "Object.freeze" => return Ok(a[0].clone()),
                "Date.now" => return Ok(N::Num(self.state.borrow().now)),
                "Math.max" => return Ok(N::Num(cache::num(&a[0]).max(cache::num(&a[1])))),
                "Number.isFinite" => {
                    if let Some(value) = self.state.borrow().finite_override.clone() {
                        return Ok(value);
                    }
                    return Ok(N::B(matches!(&a[0],N::Num(n)if n.is_finite())));
                }
                "Boolean" => {
                    return Ok(N::B(CrossCacheHost::truthy(
                        &self.state.borrow().public.inner.cache,
                        &a[0],
                    )));
                }
                "createExchangeAPI" | "createInfoAPI" | "rateLimit" => {
                    self.state
                        .borrow_mut()
                        .factories
                        .push((id.clone(), a.to_vec()));
                    return Ok(object(vec![("binding", s(id))]));
                }
                "fold" => {
                    self.state.borrow_mut().folds.push((r.clone(), a.to_vec()));
                    return Ok(a[0].clone());
                }
                "find" => {
                    self.state.borrow_mut().finds.push(a[0].clone());
                    let N::A(items) = r else {
                        panic!("native find array")
                    };
                    let items = items.borrow().clone();
                    for item in items {
                        let yes = self.invoke(&a[0], &N::U, &[item.clone()])?;
                        if CrossCacheHost::truthy(&self.state.borrow().public.inner.cache, &yes) {
                            return Ok(item);
                        }
                    }
                    return Ok(N::U);
                }
                "keys" => {
                    if let N::Map(m) = r {
                        return Ok(N::Iter(Rc::new(RefCell::new((
                            m.borrow().iter().map(|(k, _)| k.clone()).collect(),
                            0,
                        )))));
                    }
                }
                _ => {}
            }
        }
        self.state.borrow_mut().public.invoke(f, r, a)
    }
    pub fn construct(&mut self, c: &N, a: &[N]) -> Result<N, N> {
        let public = self.state.borrow().imports["HyperliquidPublicWebSocket"].clone();
        let private = self.state.borrow().imports["HyperliquidPrivateWebSocket"].clone();
        if same(c, &public) {
            return venue::public_class::construct_core_public_class(
                &mut self.state.borrow_mut().public,
                c,
                &a[0],
                None,
            );
        }
        if same(c, &private) {
            return venue::private_class::construct_core_private_class(
                &mut self.state.borrow_mut().public.inner,
                c,
                &a[0],
                None,
            );
        }
        venue::private_constructor::PrivateConstructorHost::construct(
            &mut self.state.borrow_mut().public.inner,
            c,
            a,
        )
    }
    pub fn promise_result(&self, v: &N) -> Option<Result<N, N>> {
        self.state
            .borrow()
            .promises
            .iter()
            .find(|(p, _)| same(p, v))
            .and_then(|(_, s)| s.borrow().clone())
    }
    pub fn promise(&self, mut body: Pin<Box<dyn Future<Output = Result<N, N>>>>) -> N {
        let value = object(vec![]);
        let result = Rc::new(RefCell::new(None));
        self.state
            .borrow_mut()
            .promises
            .push((value.clone(), result.clone()));
        HelperTaskHost::start_task(
            self,
            Box::pin(async move {
                *result.borrow_mut() = Some(std::future::poll_fn(|cx| body.as_mut().poll(cx)).await)
            }),
        );
        value
    }
}
impl HelperTaskHost<N, N> for Host {
    fn start_task(&self, mut task: Pin<Box<dyn Future<Output = ()>>>) {
        let mut cx = Context::from_waker(Waker::noop());
        if task.as_mut().poll(&mut cx).is_pending() {
            self.state.borrow_mut().tasks.push(task)
        }
    }
    fn settle_return(&self, result: Result<N, N>) -> Pin<Box<dyn Future<Output = Result<N, N>>>> {
        self.state.borrow_mut().outer_adoptions += 1;
        Box::pin(std::future::ready(result))
    }
}
impl CoreExchangeClassHost for Host {
    fn allocate_exchange_class(&mut self, base: &N) -> Result<(N, N), N> {
        let p = object(vec![]);
        let c = object(vec![
            ("prototype", p.clone()),
            ("name", s("HyperliquidExchange")),
            ("length", N::Num(3.)),
        ]);
        self.state
            .borrow_mut()
            .public
            .inner
            .put(&p, "constructor", &c, false);
        let bp = self.get(base, "prototype")?;
        let N::O(co) = &c else { unreachable!() };
        co.borrow_mut().prototype = Some(base.clone());
        let N::O(po) = &p else { unreachable!() };
        po.borrow_mut().prototype = Some(bp);
        Ok((c, p))
    }
    fn core_exchange_function(
        &mut self,
        c: CoreExchangeCapture<N>,
        name: &str,
        len: u8,
    ) -> Result<N, N> {
        let mut st = self.state.borrow_mut();
        let id = format!("exchange/{}", st.functions.len());
        let f = N::Builtin(id.clone());
        st.functions.insert(id, c);
        st.metadata.push((f.clone(), name.into(), len));
        Ok(f)
    }
    fn define_exchange_method(&mut self, p: &N, name: &str, v: &N, getter: bool) -> Result<(), N> {
        if getter {
            self.state
                .borrow_mut()
                .getters
                .push((p.clone(), name.into(), v.clone()))
        } else {
            self.state.borrow_mut().public.inner.put(p, name, v, false)
        }
        Ok(())
    }
    fn exchange_super(&mut self, c: &N, a: &[N; 3], t: &N) -> Result<N, N> {
        self.state
            .borrow_mut()
            .supers
            .push((c.clone(), a.clone(), t.clone()));
        let o = object(vec![
            ("name", a[0].clone()),
            ("options", a[1].clone()),
            ("store", a[2].clone()),
        ]);
        let p = self.get(t, "prototype")?;
        let N::O(ob) = &o else { unreachable!() };
        ob.borrow_mut().prototype = Some(p);
        Ok(o)
    }
    fn with_exchange_class(&self, c: &N) -> Self {
        Self {
            state: self.state.clone(),
            lexical: Some(c.clone()),
        }
    }
    fn observe_exchange_task(&self, observer: HelperObserver<N, N>) -> N {
        self.promise(Box::pin(observer))
    }
}
impl venue::ws_ping_lifecycle::WsPingHost for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        N::U
    }
    fn literal(&self, text: &JsString) -> N {
        N::S(text.clone())
    }
    fn truthy(&self, value: &N) -> bool {
        CrossCacheHost::truthy(&self.state.borrow().public.inner.cache, value)
    }
    fn global(&mut self, name: &str) -> Result<N, N> {
        {
            self.trace(format!("global:{name}"))?;
            if name == "HyperliquidExchange" {
                return Ok(self.lexical.clone().expect("native lexical class"));
            }
            Ok(N::Builtin(name.into()))
        }
    }
    fn get(&mut self, object: &N, name: &str) -> Result<N, N> {
        self.get(object, name)
    }
    fn set(&mut self, object: &N, name: &str, value: &N) -> Result<(), N> {
        {
            self.put(object, name, value);
            Ok(())
        }
    }
    fn call(&mut self, function: &N, receiver: &N, args: &[N]) -> Result<N, N> {
        self.invoke(function, receiver, args)
    }
    fn object(&mut self) -> Result<N, N> {
        Ok(object(vec![]))
    }
    fn define_data(&mut self, object: &N, name: &str, value: &N) -> Result<(), N> {
        {
            self.put(object, name, value);
            Ok(())
        }
    }
    fn callback(&mut self, capture: venue::ws_ping_lifecycle::PingCallback<N>) -> Result<N, N> {
        let _ = capture;
        panic!("outside core class finite witness: WsPingHost::callback")
    }
}
impl venue::spot_account_reads::SpotAccountHost for Host {
    fn null(&self) -> N {
        N::Null
    }
    fn boolean(&self, v: bool) -> N {
        N::B(v)
    }
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn is_nullish(&self, v: &N) -> bool {
        matches!(v, N::Null | N::U)
    }
    fn is_array(&mut self, v: &N) -> Result<bool, N> {
        let _ = v;
        panic!("outside core class finite witness: SpotAccountHost::is_array")
    }
    fn candle_binding(&mut self, v: &N) -> Result<[N; 6], N> {
        let _ = v;
        panic!("outside core class finite witness: SpotAccountHost::candle_binding")
    }
    fn is_undefined(&self, v: &N) -> bool {
        matches!(v, N::U)
    }
    fn strict_equal(&self, a: &N, b: &N) -> bool {
        same(a, b)
    }
    fn imported(&mut self, name: &str) -> Result<N, N> {
        {
            self.trace(format!("import:{name}"))?;
            if name == "HyperliquidExchange" {
                return Ok(self.lexical.clone().expect("native lexical class"));
            }
            Ok(self
                .state
                .borrow()
                .imports
                .get(name)
                .cloned()
                .unwrap_or_else(|| N::Builtin(name.into())))
        }
    }
    fn array(&mut self, values: &[N]) -> Result<N, N> {
        Ok(arr(values.to_vec()))
    }
    fn spread_array(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: SpotAccountHost::spread_array")
    }
    fn spread_into(&mut self, target: &N, source: &N) -> Result<(), N> {
        let _ = target;
        let _ = source;
        panic!("outside core class finite witness: SpotAccountHost::spread_into")
    }
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        Ok(
            JsPayloadHost::to_string(&mut self.state.borrow_mut().public.inner.cache, value)
                .unwrap(),
        )
    }
    fn binary(&mut self, op: &str, a: &N, b: &N) -> Result<N, N> {
        {
            let (a, b) = (cache::num(a), cache::num(b));
            Ok(match op {
                "+" => N::Num(a + b),
                "-" => N::Num(a - b),
                ">" => N::B(a > b),
                ">=" => N::B(a >= b),
                _ => panic!("native binary {op}"),
            })
        }
    }
    fn interval_binding(&mut self, value: &N) -> Result<[N; 3], N> {
        let _ = value;
        panic!("outside core class finite witness: SpotAccountHost::interval_binding")
    }
    fn pair_binding(&mut self, value: &N) -> Result<[N; 2], N> {
        let _ = value;
        panic!("outside core class finite witness: SpotAccountHost::pair_binding")
    }
    fn index(&mut self, value: &N, key: &N) -> Result<N, N> {
        {
            let text = String::from_utf16(
                &JsPayloadHost::to_string(&mut self.state.borrow_mut().public.inner.cache, key)
                    .unwrap()
                    .0,
            )
            .unwrap();
            self.get(value, &text)
        }
    }
    fn digit_regex(&mut self) -> Result<N, N> {
        panic!("outside core class finite witness: SpotAccountHost::digit_regex")
    }
    fn response_data(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: SpotAccountHost::response_data")
    }
    fn construct(&mut self, constructor: &N, args: &[N]) -> Result<N, N> {
        self.construct(constructor, args)
    }
    fn throw_value(&self, value: N) -> N {
        value
    }
    fn error_value(&self, error: &N) -> N {
        error.clone()
    }
    fn callback(&mut self, capture: venue::spot_account_reads::AccountCallback<N>) -> Result<N, N> {
        let _ = capture;
        panic!("outside core class finite witness: SpotAccountHost::callback")
    }
    fn await_value(&mut self, value: N) -> venue::spot_account_reads::AccountFuture<N, N> {
        let _ = value;
        panic!("outside core class finite witness: SpotAccountHost::await_value")
    }
    fn spawn_native(&self, body: venue::spot_account_reads::AccountFuture<N, N>) -> N {
        self.promise(body)
    }
}
impl venue::private_message_dispatch::PrivateMessageHost for Host {
    fn typeof_value<'a>(&self, value: &'a N) -> &'a str {
        let _ = value;
        panic!("outside core class finite witness: PrivateMessageHost::typeof_value")
    }
    fn has_property(&mut self, name: &str, object: &N) -> Result<bool, N> {
        let _ = name;
        let _ = object;
        panic!("outside core class finite witness: PrivateMessageHost::has_property")
    }
    fn valid_fill_predicate(&mut self) -> Result<N, N> {
        panic!("outside core class finite witness: PrivateMessageHost::valid_fill_predicate")
    }
    fn for_in(&mut self, object: &N) -> Result<N, N> {
        let _ = object;
        panic!("outside core class finite witness: PrivateMessageHost::for_in")
    }
    fn for_in_next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        let _ = iterator;
        panic!("outside core class finite witness: PrivateMessageHost::for_in_next")
    }
    fn construct(&mut self, function: &N, args: &[N]) -> Result<N, N> {
        let _ = function;
        let _ = args;
        panic!("outside core class finite witness: PrivateMessageHost::construct")
    }
    fn regex(&mut self, pattern: &str) -> Result<N, N> {
        let _ = pattern;
        panic!("outside core class finite witness: PrivateMessageHost::regex")
    }
    fn number(&self, value: f64) -> N {
        N::Num(value)
    }
    fn boolean(&self, value: bool) -> N {
        N::B(value)
    }
    fn is_nullish(&self, value: &N) -> bool {
        matches!(value, N::Null | N::U)
    }
    fn is_null(&self, value: &N) -> bool {
        let _ = value;
        panic!("outside core class finite witness: PrivateMessageHost::is_null")
    }
    fn strict_equal(&self, a: &N, b: &N) -> bool {
        same(a, b)
    }
    fn index(&mut self, object: &N, key: &N) -> Result<N, N> {
        let _ = object;
        let _ = key;
        panic!("outside core class finite witness: PrivateMessageHost::index")
    }
    fn array(&mut self) -> Result<N, N> {
        panic!("outside core class finite witness: PrivateMessageHost::array")
    }
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        Ok(
            JsPayloadHost::to_string(&mut self.state.borrow_mut().public.inner.cache, value)
                .unwrap(),
        )
    }
    fn operator(
        &mut self,
        op: venue::private_message_dispatch::DispatchOperator,
        a: &N,
        b: &N,
    ) -> Result<N, N> {
        let _ = op;
        let _ = a;
        let _ = b;
        panic!("outside core class finite witness: PrivateMessageHost::operator")
    }
}
impl venue::core_account_snapshots::CoreSnapshotHost for Host {
    fn null_value(&self) -> N {
        N::Null
    }
    fn add_values(&mut self, a: &N, b: &N) -> Result<N, N> {
        let _ = a;
        let _ = b;
        panic!("outside core class finite witness: CoreSnapshotHost::add_values")
    }
    fn return_await_value(&mut self, value: N) -> impl Future<Output = Result<N, N>> {
        self.state.borrow_mut().return_adoptions += 1;
        std::future::ready(Ok(value))
    }
    fn await_value(&mut self, value: N) -> impl Future<Output = Result<N, N>> {
        let _ = value;
        std::future::ready(Err(self.state.borrow().marker.clone()))
    }
    fn error_value(&self, error: &N) -> N {
        error.clone()
    }
    fn construct_error(&mut self, constructor: &N, args: &[N]) -> Result<N, N> {
        let _ = constructor;
        let _ = args;
        panic!("outside core class finite witness: CoreSnapshotHost::construct_error")
    }
    fn instance_of(&mut self, value: &N, constructor: &N) -> Result<bool, N> {
        let _ = value;
        let _ = constructor;
        panic!("outside core class finite witness: CoreSnapshotHost::instance_of")
    }
    fn snapshot_predicate(
        &mut self,
        capture: venue::core_account_snapshots::SnapshotPredicate<N>,
    ) -> Result<N, N> {
        let _ = capture;
        panic!("outside core class finite witness: CoreSnapshotHost::snapshot_predicate")
    }
    fn array_define(&mut self, array: &N, index: usize, value: &N) -> Result<(), N> {
        let _ = array;
        let _ = index;
        let _ = value;
        panic!("outside core class finite witness: CoreSnapshotHost::array_define")
    }
    fn spread_into(&mut self, object: &N, source: &N) -> Result<(), N> {
        let _ = object;
        let _ = source;
        panic!("outside core class finite witness: CoreSnapshotHost::spread_into")
    }
    fn iterator(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: CoreSnapshotHost::iterator")
    }
    fn next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        let _ = iterator;
        panic!("outside core class finite witness: CoreSnapshotHost::next")
    }
    fn close_after_throw(&mut self, iterator: &N) {
        let _ = iterator;
        panic!("outside core class finite witness: CoreSnapshotHost::close_after_throw")
    }
    fn close_after_return(&mut self, iterator: &N) -> Result<(), N> {
        let _ = iterator;
        panic!("outside core class finite witness: CoreSnapshotHost::close_after_return")
    }
    fn pair(&mut self, value: &N) -> Result<[N; 2], N> {
        let _ = value;
        panic!("outside core class finite witness: CoreSnapshotHost::pair")
    }
}
impl venue::core_position_order_state::PositionOrderHost for Host {
    fn position_operator(&mut self, operator: &str, left: &N, right: &N) -> Result<N, N> {
        let _ = operator;
        let _ = left;
        let _ = right;
        panic!("outside core class finite witness: PositionOrderHost::position_operator")
    }
    fn position_unary(&mut self, operator: &str, value: &N) -> Result<N, N> {
        let _ = operator;
        let _ = value;
        panic!("outside core class finite witness: PositionOrderHost::position_unary")
    }
    fn position_callback(
        &mut self,
        callback: venue::core_position_order_state::PositionCallback<N>,
    ) -> Result<N, N> {
        let _ = callback;
        panic!("outside core class finite witness: PositionOrderHost::position_callback")
    }
    fn set_position_index(&mut self, object: &N, key: &N, value: &N) -> Result<(), N> {
        let _ = object;
        let _ = key;
        let _ = value;
        panic!("outside core class finite witness: PositionOrderHost::set_position_index")
    }
    fn append_position_spread(&mut self, array: &N, value: &N) -> Result<(), N> {
        let _ = array;
        let _ = value;
        panic!("outside core class finite witness: PositionOrderHost::append_position_spread")
    }
}
impl venue::core_rest_reads::RestReadHost for Host {
    fn rest_callback(&mut self, callback: venue::core_rest_reads::RestCallback<N>) -> Result<N, N> {
        let _ = callback;
        panic!("outside core class finite witness: RestReadHost::rest_callback")
    }
    fn rest_exception(&self, value: N) -> N {
        value
    }
    fn rest_array_binding(&mut self, value: &N, mask: &[bool]) -> Result<Vec<N>, N> {
        let _ = value;
        let _ = mask;
        panic!("outside core class finite witness: RestReadHost::rest_array_binding")
    }
    fn rest_argument_spread(&mut self, args: &mut Vec<N>, value: &N) -> Result<(), N> {
        let _ = args;
        let _ = value;
        panic!("outside core class finite witness: RestReadHost::rest_argument_spread")
    }
}
impl venue::startup_lifecycle::StartupLifecycleHost for Host {
    fn startup_callback(
        &mut self,
        callback: venue::startup_lifecycle::StartupCallback<N>,
    ) -> Result<N, N> {
        let _ = callback;
        panic!("outside core class finite witness: StartupLifecycleHost::startup_callback")
    }
}
impl venue::market_bootstrap::MarketBootstrapHost for Host {
    fn bootstrap_callback(
        &mut self,
        callback: venue::market_bootstrap::MarketBootstrapCallback<N>,
    ) -> Result<N, N> {
        let _ = callback;
        panic!("outside core class finite witness: MarketBootstrapHost::bootstrap_callback")
    }
    fn array_binding(&mut self, value: &N, count: usize) -> Result<Vec<N>, N> {
        let _ = value;
        let _ = count;
        panic!("outside core class finite witness: MarketBootstrapHost::array_binding")
    }
    fn spread_arguments(&mut self, args: &mut Vec<N>, value: &N) -> Result<(), N> {
        let _ = args;
        let _ = value;
        panic!("outside core class finite witness: MarketBootstrapHost::spread_arguments")
    }
    fn iterator(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: MarketBootstrapHost::iterator")
    }
    fn next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        let _ = iterator;
        panic!("outside core class finite witness: MarketBootstrapHost::next")
    }
    fn close_after_throw(&mut self, iterator: &N) {
        let _ = iterator;
        panic!("outside core class finite witness: MarketBootstrapHost::close_after_throw")
    }
    fn close_after_break(&mut self, iterator: &N) -> Result<(), N> {
        let _ = iterator;
        panic!("outside core class finite witness: MarketBootstrapHost::close_after_break")
    }
}
impl venue::spot_market_bootstrap::SpotMarketBootstrapHost for Host {
    fn property_key(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: SpotMarketBootstrapHost::property_key")
    }
    fn set_key(&mut self, object: &N, key: &N, value: &N) -> Result<(), N> {
        let _ = object;
        let _ = key;
        let _ = value;
        panic!("outside core class finite witness: SpotMarketBootstrapHost::set_key")
    }
    fn unary_plus(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: SpotMarketBootstrapHost::unary_plus")
    }
}
impl venue::hip3_position_refresh::PositionRefreshHost for Host {
    fn position_refresh_callback(
        &mut self,
        capture: venue::hip3_position_refresh::PositionRefreshCallback<N>,
    ) -> Result<N, N> {
        let _ = capture;
        panic!("outside core class finite witness: PositionRefreshHost::position_refresh_callback")
    }
    fn iterator(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: PositionRefreshHost::iterator")
    }
    fn next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        let _ = iterator;
        panic!("outside core class finite witness: PositionRefreshHost::next")
    }
    fn close_after_throw(&mut self, iterator: &N) {
        let _ = iterator;
        panic!("outside core class finite witness: PositionRefreshHost::close_after_throw")
    }
}
impl venue::order_reads::OrderReadHost for Host {
    fn order_read_callback(&mut self, c: venue::order_reads::OrderReadCallback<N>) -> Result<N, N> {
        let _ = c;
        panic!("outside core class finite witness: OrderReadHost::order_read_callback")
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        let _ = v;
        panic!("outside core class finite witness: OrderReadHost::iterator")
    }
    fn next(&mut self, v: &N) -> Result<Option<N>, N> {
        let _ = v;
        panic!("outside core class finite witness: OrderReadHost::next")
    }
    fn close_after_throw(&mut self, v: &N) {
        let _ = v;
        panic!("outside core class finite witness: OrderReadHost::close_after_throw")
    }
    fn first_binding(&mut self, v: &N) -> Result<N, N> {
        let _ = v;
        panic!("outside core class finite witness: OrderReadHost::first_binding")
    }
    fn typeof_value(&self, v: &N) -> &'static str {
        let _ = v;
        panic!("outside core class finite witness: OrderReadHost::typeof_value")
    }
    fn argument_spread(&mut self, out: &mut Vec<N>, v: &N) -> Result<(), N> {
        let _ = out;
        let _ = v;
        panic!("outside core class finite witness: OrderReadHost::argument_spread")
    }
}
impl venue::order_retry::OrderRetryHost for Host {
    fn order_retry_callback(
        &mut self,
        c: venue::order_retry::OrderRetryCallback<N>,
    ) -> Result<N, N> {
        let _ = c;
        panic!("outside core class finite witness: OrderRetryHost::order_retry_callback")
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        let _ = v;
        panic!("outside core class finite witness: OrderRetryHost::iterator")
    }
    fn next(&mut self, v: &N) -> Result<Option<N>, N> {
        let _ = v;
        panic!("outside core class finite witness: OrderRetryHost::next")
    }
    fn close_after_throw(&mut self, v: &N) {
        let _ = v;
        panic!("outside core class finite witness: OrderRetryHost::close_after_throw")
    }
    fn retry_operator(&mut self, op: &str, a: &N, b: &N) -> Result<N, N> {
        let _ = op;
        let _ = a;
        let _ = b;
        panic!("outside core class finite witness: OrderRetryHost::retry_operator")
    }
}
impl venue::limit_orders::LimitOrderHost for Host {
    fn unary_plus(&mut self, v: &N) -> Result<N, N> {
        let _ = v;
        panic!("outside core class finite witness: LimitOrderHost::unary_plus")
    }
    fn first_binding(&mut self, v: &N) -> Result<N, N> {
        let _ = v;
        panic!("outside core class finite witness: LimitOrderHost::first_binding")
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        let _ = v;
        panic!("outside core class finite witness: LimitOrderHost::iterator")
    }
    fn next(&mut self, v: &N) -> Result<Option<N>, N> {
        let _ = v;
        panic!("outside core class finite witness: LimitOrderHost::next")
    }
    fn close_after_throw(&mut self, v: &N) {
        let _ = v;
        panic!("outside core class finite witness: LimitOrderHost::close_after_throw")
    }
}
impl venue::placement_dispatch::PlacementHost for Host {
    fn unary_plus(&mut self, v: &N) -> Result<N, N> {
        let _ = v;
        panic!("outside core class finite witness: PlacementHost::unary_plus")
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        let _ = v;
        panic!("outside core class finite witness: PlacementHost::iterator")
    }
    fn next(&mut self, v: &N) -> Result<Option<N>, N> {
        let _ = v;
        panic!("outside core class finite witness: PlacementHost::next")
    }
    fn close_after_throw(&mut self, v: &N) {
        let _ = v;
        panic!("outside core class finite witness: PlacementHost::close_after_throw")
    }
    fn serial_order_callback(
        &mut self,
        c: venue::placement_dispatch::SerialOrderCallback<N>,
    ) -> Result<N, N> {
        let _ = c;
        panic!("outside core class finite witness: PlacementHost::serial_order_callback")
    }
}
impl venue::cancellation::CancellationHost for Host {
    fn unary_plus(&mut self, v: &N) -> Result<N, N> {
        let _ = v;
        panic!("outside core class finite witness: CancellationHost::unary_plus")
    }
    fn instance_of(&mut self, v: &N, class: &N) -> Result<bool, N> {
        let _ = v;
        let _ = class;
        panic!("outside core class finite witness: CancellationHost::instance_of")
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        let _ = v;
        panic!("outside core class finite witness: CancellationHost::iterator")
    }
    fn next(&mut self, v: &N) -> Result<Option<N>, N> {
        let _ = v;
        panic!("outside core class finite witness: CancellationHost::next")
    }
    fn close_after_throw(&mut self, v: &N) {
        let _ = v;
        panic!("outside core class finite witness: CancellationHost::close_after_throw")
    }
    fn cancel_callback(&mut self, c: venue::cancellation::CancelCallback<N>) -> Result<N, N> {
        let _ = c;
        panic!("outside core class finite witness: CancellationHost::cancel_callback")
    }
}
impl venue::order_update::OrderUpdateHost for Host {
    fn update_parameter_binding(&mut self, value: &N) -> Result<(N, N), N> {
        let _ = value;
        panic!("outside core class finite witness: OrderUpdateHost::update_parameter_binding")
    }
    fn has_property(&mut self, o: &N, k: &str) -> Result<bool, N> {
        let _ = o;
        let _ = k;
        panic!("outside core class finite witness: OrderUpdateHost::has_property")
    }
    fn first_rest_binding(&mut self, v: &N) -> Result<(N, N), N> {
        let _ = v;
        panic!("outside core class finite witness: OrderUpdateHost::first_rest_binding")
    }
    fn update_callback(&mut self, c: venue::order_update::UpdateCallback<N>) -> Result<N, N> {
        let _ = c;
        panic!("outside core class finite witness: OrderUpdateHost::update_callback")
    }
}
impl venue::account_position_leaves::AccountPositionHost for Host {
    fn null(&self) -> N {
        N::Null
    }
    fn boolean(&self, value: bool) -> N {
        N::B(value)
    }
    fn number(&self, value: f64) -> N {
        N::Num(value)
    }
    fn is_nullish(&self, value: &N) -> bool {
        matches!(value, N::Null | N::U)
    }
    fn strict_equal(&self, left: &N, right: &N) -> bool {
        same(left, right)
    }
    fn greater_equal(&mut self, left: &N, right: &N) -> Result<bool, N> {
        let _ = left;
        let _ = right;
        panic!("outside core class finite witness: AccountPositionHost::greater_equal")
    }
    fn opposite_predicate(
        &mut self,
        capture: venue::account_position_leaves::OppositePredicate<N>,
    ) -> Result<N, N> {
        let _ = capture;
        panic!("outside core class finite witness: AccountPositionHost::opposite_predicate")
    }
}
impl venue::balance_state::BalanceHost for Host {
    fn null(&self) -> N {
        N::Null
    }
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn boolean(&self, v: bool) -> N {
        N::B(v)
    }
    fn is_nullish(&self, v: &N) -> bool {
        matches!(v, N::Null | N::U)
    }
    fn is_undefined(&self, v: &N) -> bool {
        matches!(v, N::U)
    }
    fn strict_equal(&self, a: &N, b: &N) -> bool {
        same(a, b)
    }
    fn index(&mut self, o: &N, key: &N) -> Result<N, N> {
        let _ = o;
        let _ = key;
        panic!("outside core class finite witness: BalanceHost::index")
    }
    fn set_index(&mut self, o: &N, key: &N, v: &N) -> Result<(), N> {
        let _ = o;
        let _ = key;
        let _ = v;
        panic!("outside core class finite witness: BalanceHost::set_index")
    }
    fn define_index(&mut self, o: &N, key: &N, v: &N) -> Result<(), N> {
        let _ = o;
        let _ = key;
        let _ = v;
        panic!("outside core class finite witness: BalanceHost::define_index")
    }
    fn array(&mut self) -> Result<N, N> {
        panic!("outside core class finite witness: BalanceHost::array")
    }
    fn array_define(&mut self, o: &N, i: usize, v: &N) -> Result<(), N> {
        let _ = o;
        let _ = i;
        let _ = v;
        panic!("outside core class finite witness: BalanceHost::array_define")
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        let _ = v;
        panic!("outside core class finite witness: BalanceHost::iterator")
    }
    fn next(&mut self, i: &N) -> Result<Option<N>, N> {
        let _ = i;
        panic!("outside core class finite witness: BalanceHost::next")
    }
    fn close_after_throw(&mut self, i: &N) {
        let _ = i;
        panic!("outside core class finite witness: BalanceHost::close_after_throw")
    }
    fn close_after_return(&mut self, i: &N) -> Result<(), N> {
        let _ = i;
        panic!("outside core class finite witness: BalanceHost::close_after_return")
    }
    fn pair(&mut self, v: &N) -> Result<[N; 2], N> {
        let _ = v;
        panic!("outside core class finite witness: BalanceHost::pair")
    }
    fn template_string(&mut self, v: &N) -> Result<JsString, N> {
        Ok(JsPayloadHost::to_string(&mut self.state.borrow_mut().public.inner.cache, v).unwrap())
    }
    fn operator(
        &mut self,
        op: venue::balance_state::BalanceOperator,
        a: &N,
        b: &N,
    ) -> Result<N, N> {
        let _ = op;
        let _ = a;
        let _ = b;
        panic!("outside core class finite witness: BalanceHost::operator")
    }
    fn spread_into(&mut self, o: &N, source: &N) -> Result<(), N> {
        let _ = o;
        let _ = source;
        panic!("outside core class finite witness: BalanceHost::spread_into")
    }
    fn spot_market_predicate(
        &mut self,
        capture: venue::balance_state::SpotMarketPredicate<N>,
    ) -> Result<N, N> {
        let _ = capture;
        panic!("outside core class finite witness: BalanceHost::spot_market_predicate")
    }
}
impl venue::market_data_selectors::MarketDataHost for Host {
    fn null(&self) -> N {
        N::Null
    }
    fn number(&self, value: f64) -> N {
        N::Num(value)
    }
    fn is_nullish(&self, value: &N) -> bool {
        matches!(value, N::Null | N::U)
    }
    fn is_string(&self, value: &N) -> bool {
        let _ = value;
        panic!("outside core class finite witness: MarketDataHost::is_string")
    }
    fn strict_equal(&self, left: &N, right: &N) -> bool {
        same(left, right)
    }
    fn index(&mut self, object: &N, key: &N) -> Result<N, N> {
        let _ = object;
        let _ = key;
        panic!("outside core class finite witness: MarketDataHost::index")
    }
    fn iterator(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: MarketDataHost::iterator")
    }
    fn next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        let _ = iterator;
        panic!("outside core class finite witness: MarketDataHost::next")
    }
    fn close_after_throw(&mut self, iterator: &N) {
        let _ = iterator;
        panic!("outside core class finite witness: MarketDataHost::close_after_throw")
    }
    fn construct(&mut self, function: &N, args: &[N]) -> Result<N, N> {
        let _ = function;
        let _ = args;
        panic!("outside core class finite witness: MarketDataHost::construct")
    }
    fn first_binding(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: MarketDataHost::first_binding")
    }
    fn unit_regex(&mut self) -> Result<N, N> {
        panic!("outside core class finite witness: MarketDataHost::unit_regex")
    }
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        Ok(
            JsPayloadHost::to_string(&mut self.state.borrow_mut().public.inner.cache, value)
                .unwrap(),
        )
    }
    fn subtract(&mut self, left: &N, right: &N) -> Result<N, N> {
        let _ = left;
        let _ = right;
        panic!("outside core class finite witness: MarketDataHost::subtract")
    }
}
impl venue::order_request_leaves::OrderRequestHost for Host {
    fn await_value(&mut self, value: N) -> impl Future<Output = Result<N, N>> {
        let _ = value;
        std::future::ready(Err(self.state.borrow().marker.clone()))
    }
    fn construct_error(&mut self, constructor: &N, args: &[N]) -> Result<N, N> {
        let _ = constructor;
        let _ = args;
        panic!("outside core class finite witness: OrderRequestHost::construct_error")
    }
    fn instance_of(&mut self, value: &N, constructor: &N) -> Result<bool, N> {
        let _ = value;
        let _ = constructor;
        panic!("outside core class finite witness: OrderRequestHost::instance_of")
    }
    fn type_is(&self, value: &N, kind: &str) -> bool {
        let _ = value;
        let _ = kind;
        panic!("outside core class finite witness: OrderRequestHost::type_is")
    }
    fn unary_number(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: OrderRequestHost::unary_number")
    }
    fn compare(
        &mut self,
        op: venue::order_request_leaves::RequestComparison,
        left: &N,
        right: &N,
    ) -> Result<N, N> {
        let _ = op;
        let _ = left;
        let _ = right;
        panic!("outside core class finite witness: OrderRequestHost::compare")
    }
}
impl venue::lifecycle_delegates::LifecycleHost for Host {
    fn null(&self) -> N {
        N::Null
    }
    fn boolean(&self, value: bool) -> N {
        N::B(value)
    }
    fn number(&self, value: f64) -> N {
        N::Num(value)
    }
    fn is_nullish(&self, value: &N) -> bool {
        matches!(value, N::Null | N::U)
    }
    fn super_get(&mut self, receiver: &N, name: &str) -> Result<N, N> {
        let _ = receiver;
        let _ = name;
        panic!("outside core class finite witness: LifecycleHost::super_get")
    }
    fn lifecycle_callback(
        &mut self,
        capture: venue::lifecycle_delegates::LifecycleCallback<N>,
    ) -> Result<N, N> {
        let _ = capture;
        panic!("outside core class finite witness: LifecycleHost::lifecycle_callback")
    }
    fn subtract(&mut self, left: &N, right: &N) -> Result<N, N> {
        let _ = left;
        let _ = right;
        panic!("outside core class finite witness: LifecycleHost::subtract")
    }
    fn greater_than(&mut self, left: &N, right: &N) -> Result<bool, N> {
        let _ = left;
        let _ = right;
        panic!("outside core class finite witness: LifecycleHost::greater_than")
    }
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        Ok(
            JsPayloadHost::to_string(&mut self.state.borrow_mut().public.inner.cache, value)
                .unwrap(),
        )
    }
}
impl venue::cache_orders::CacheOrdersHost for Host {
    fn now(&mut self) -> Result<N, N> {
        panic!("outside core class finite witness: CacheOrdersHost::now")
    }
    fn object_keys(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: CacheOrdersHost::object_keys")
    }
    fn get_index(&mut self, value: &N, key: &N) -> Result<N, N> {
        let _ = value;
        let _ = key;
        panic!("outside core class finite witness: CacheOrdersHost::get_index")
    }
    fn delete(&mut self, value: &N, key: &N) -> Result<(), N> {
        let _ = value;
        let _ = key;
        panic!("outside core class finite witness: CacheOrdersHost::delete")
    }
    fn older_than(&mut self, now: &N, timestamp: &N, milliseconds: f64) -> Result<bool, N> {
        let _ = now;
        let _ = timestamp;
        let _ = milliseconds;
        panic!("outside core class finite witness: CacheOrdersHost::older_than")
    }
    fn singleton(&mut self) -> Result<N, N> {
        panic!("outside core class finite witness: CacheOrdersHost::singleton")
    }
}
impl venue::cross_cache::CrossCacheHost for Host {
    fn null(&self) -> N {
        N::Null
    }
    fn is_undefined(&self, value: &N) -> bool {
        matches!(value, N::U)
    }
    fn truthy(&self, value: &N) -> bool {
        let _ = value;
        panic!("outside core class finite witness: CrossCacheHost::truthy")
    }
    fn session_storage(&mut self) -> Result<N, N> {
        panic!("outside core class finite witness: CrossCacheHost::session_storage")
    }
    fn literal(&self, text: &JsString) -> N {
        let _ = text;
        panic!("outside core class finite witness: CrossCacheHost::literal")
    }
    fn object(&mut self) -> Result<N, N> {
        panic!("outside core class finite witness: CrossCacheHost::object")
    }
    fn set(&mut self, object: &N, key: &N, value: &N) -> Result<(), N> {
        let _ = object;
        let _ = key;
        let _ = value;
        panic!("outside core class finite witness: CrossCacheHost::set")
    }
    fn define_data(&mut self, object: &N, key: &JsString, value: &N) -> Result<(), N> {
        let _ = object;
        let _ = key;
        let _ = value;
        panic!("outside core class finite witness: CrossCacheHost::define_data")
    }
    fn call(&mut self, function: &N, receiver: &N, args: &[N]) -> Result<N, N> {
        let _ = function;
        let _ = receiver;
        let _ = args;
        panic!("outside core class finite witness: CrossCacheHost::call")
    }
    fn parse_json_value(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: CrossCacheHost::parse_json_value")
    }
    fn property_key(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: CrossCacheHost::property_key")
    }
    fn entries(&mut self, map: &N) -> Result<N, N> {
        let _ = map;
        panic!("outside core class finite witness: CrossCacheHost::entries")
    }
    fn iterator_step(&mut self, iterator: &N) -> Result<Option<N>, N> {
        let _ = iterator;
        panic!("outside core class finite witness: CrossCacheHost::iterator_step")
    }
    fn entry_pair(&mut self, entry: &N) -> Result<[N; 2], N> {
        let _ = entry;
        panic!("outside core class finite witness: CrossCacheHost::entry_pair")
    }
    fn close_after_throw(&mut self, iterator: &N) {
        let _ = iterator;
        panic!("outside core class finite witness: CrossCacheHost::close_after_throw")
    }
    fn spread(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside core class finite witness: CrossCacheHost::spread")
    }
    fn greater_than_500(&mut self, value: &N) -> Result<bool, N> {
        let _ = value;
        panic!("outside core class finite witness: CrossCacheHost::greater_than_500")
    }
}
impl venue::json_boundaries::JsPayloadHost for Host {
    type Value = N;
    type Error = N;
    fn string_units<'a>(&self, value: &'a N) -> Option<&'a [u16]> {
        let _ = value;
        panic!("outside core class finite witness: JsPayloadHost::string_units")
    }
    fn is_object(&self, value: &N) -> bool {
        let _ = value;
        panic!("outside core class finite witness: JsPayloadHost::is_object")
    }
    fn is_array(&mut self, value: &N) -> Result<bool, N> {
        let _ = value;
        panic!("outside core class finite witness: JsPayloadHost::is_array")
    }
    fn is_nullish(&self, value: &N) -> bool {
        matches!(value, N::Null | N::U)
    }
    fn undefined(&self) -> N {
        panic!("outside core class finite witness: JsPayloadHost::undefined")
    }
    fn parse_json(&mut self, text: &[u16]) -> Result<N, N> {
        let _ = text;
        panic!("outside core class finite witness: JsPayloadHost::parse_json")
    }
    fn get(&mut self, value: &N, property: &str) -> Result<N, N> {
        self.get(value, property)
    }
    fn json_stringify(&mut self, value: &N) -> Result<Option<JsString>, N> {
        let _ = value;
        panic!("outside core class finite witness: JsPayloadHost::json_stringify")
    }
    fn to_string(&mut self, value: &N) -> Result<JsString, N> {
        let _ = value;
        panic!("outside core class finite witness: JsPayloadHost::to_string")
    }
}
impl venue::startup_timeout::StartupTimeoutHost for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        panic!("outside core class finite witness: StartupTimeoutHost::undefined")
    }
    fn null(&self) -> N {
        N::Null
    }
    fn literal(&self, v: &str) -> N {
        let _ = v;
        panic!("outside core class finite witness: StartupTimeoutHost::literal")
    }
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn truthy(&self, v: &N) -> bool {
        let _ = v;
        panic!("outside core class finite witness: StartupTimeoutHost::truthy")
    }
    fn global(&self, k: &str) -> Result<N, N> {
        let _ = k;
        panic!("outside core class finite witness: StartupTimeoutHost::global")
    }
    fn get(&self, owner: &N, k: &str) -> Result<N, N> {
        let _ = owner;
        let _ = k;
        panic!("outside core class finite witness: StartupTimeoutHost::get")
    }
    fn call(&self, callee: N, receiver: N, args: Vec<N>) -> Result<N, N> {
        let _ = callee;
        let _ = receiver;
        let _ = args;
        panic!("outside core class finite witness: StartupTimeoutHost::call")
    }
    fn construct(&self, callee: N, args: Vec<N>) -> Result<N, N> {
        let _ = callee;
        let _ = args;
        panic!("outside core class finite witness: StartupTimeoutHost::construct")
    }
    fn array(&self, values: Vec<N>) -> N {
        let _ = values;
        panic!("outside core class finite witness: StartupTimeoutHost::array")
    }
    fn callback(&self, body: venue::startup_timeout::TimeoutCallback<N, N>) -> N {
        let _ = body;
        panic!("outside core class finite witness: StartupTimeoutHost::callback")
    }
    fn divide(&self, a: N, b: N) -> Result<N, N> {
        let _ = a;
        let _ = b;
        panic!("outside core class finite witness: StartupTimeoutHost::divide")
    }
    fn template(&self, parts: Vec<N>) -> Result<N, N> {
        let _ = parts;
        panic!("outside core class finite witness: StartupTimeoutHost::template")
    }
    fn await_value(
        &self,
        value: N,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<N, N>>>> {
        let _ = value;
        panic!("outside core class finite witness: StartupTimeoutHost::await_value")
    }
}
impl venue::core_exchange_leaves::CoreExchangeLeafHost for Host {
    fn finite_positive_predicate(&mut self) -> Result<N, N> {
        let mut st = self.state.borrow_mut();
        let id = "predicate/finite-singleton".to_owned();
        st.predicates
            .entry(id.clone())
            .or_insert(ExchangePredicate::FinitePositive);
        Ok(N::Builtin(id))
    }

    fn typeof_value(&self, value: &N) -> &'static str {
        match value {
            N::Num(_) => "number",
            N::S(_) => "string",
            N::U => "undefined",
            N::B(_) => "boolean",
            _ => "object",
        }
    }
    fn exchange_predicate(
        &mut self,
        capture: venue::core_exchange_leaves::ExchangePredicate<N>,
    ) -> Result<N, N> {
        {
            let mut st = self.state.borrow_mut();
            let id = format!("predicate/{}", st.predicates.len());
            st.predicates.insert(id.clone(), capture);
            Ok(N::Builtin(id))
        }
    }
}
