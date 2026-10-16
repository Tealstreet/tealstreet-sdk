//! ECMAScript primitives and live receiver access for the existing source bodies.
use super::{Iter, Rt, error};
use crate::{
    account_reads::AccountHost,
    auth_init::{AuthHost, WalletSigner},
    ws_dispatch::Runtime,
};
use safe_cex_exchange::js_runtime::{
    ops,
    promise::{Promise, PromiseWait, await_js},
    value::{Js, Thrown},
};
use std::{
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

pub(crate) struct Operation {
    promise: Promise,
    waiter: std::cell::RefCell<PromiseWait>,
}
impl crate::order_adapters::Completion<Js, Thrown> for Operation {
    fn poll(&self, cx: &mut Context<'_>) -> Poll<Result<Js, Thrown>> {
        Pin::new(&mut *self.waiter.borrow_mut()).poll(cx)
    }
}
pub(crate) type Call = (Js, Js);
impl Runtime for Rt {
    type Value = Js;
    type Error = Thrown;
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn is_text(&self, v: &Js, s: &str) -> bool {
        v.strict_equals(&Js::str(s))
    }
    fn field(&self, v: &Js, k: &str) -> Result<Js, Thrown> {
        v.get(k)
    }
    fn string(&self, v: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(v.to_js_string()?.to_vec())
    }
    fn log(&self, m: &[u16], s: &'static str) -> Result<(), Thrown> {
        let owner = if self.this.has_own("parent") {
            self.this.get("parent")?
        } else {
            self.this.clone()
        };
        owner.call_method("log", vec![Js::utf16(m), Js::str(s)])?;
        Ok(())
    }
}
impl AccountHost for Rt {
    type Callable = Call;
    type Operation = Operation;
    fn start_task(&self, t: Pin<Box<dyn Future<Output = ()>>>) {
        super::retained_task(self, t)
    }
    fn capture(&self, r: &'static str, m: &'static str) -> Result<Call, Thrown> {
        let owner = match r {
            "imports" => self.shared.globals.clone(),
            "private" => self.this.clone(),
            "exchange" => {
                if self.this.has_own("exchange") {
                    self.this.get("exchange")?
                } else {
                    self.this.clone()
                }
            }
            _ => self.path(r)?,
        };
        let f = owner.get(m)?;
        Ok((owner, f))
    }
    fn invoke(&self, c: Call, a: Vec<Js>) -> Result<Rc<Operation>, Thrown> {
        self.resolve(c.1.call(c.0, a)?)
    }
    fn invoke_log(&self, c: Call, m: &[u16], s: &'static str) -> Result<(), Thrown> {
        c.1.call(c.0, vec![Js::utf16(m), Js::str(s)])?;
        Ok(())
    }
    fn resolve(&self, v: Js) -> Result<Rc<Operation>, Thrown> {
        let promise = Promise::spawn(&self.exchange_runtime(), Box::pin(await_js(v)));
        Ok(Rc::new(Operation {
            waiter: std::cell::RefCell::new(promise.wait()),
            promise,
        }))
    }
    fn credentials(&self) -> Result<Js, Thrown> {
        self.this.call_method("getCredentials", vec![])
    }
    fn wallet(&self) -> Result<Js, Thrown> {
        self.this.get("wallet")
    }
    fn exchange_credentials(&self) -> Result<Js, Thrown> {
        self.this.get("credentials")
    }
    fn signer_type(&self) -> Result<Js, Thrown> {
        self.this.get("signerType")
    }
    fn text(&self, s: &'static str) -> Js {
        Js::str(s)
    }
    fn number(&self, n: f64) -> Js {
        Js::Number(n)
    }
    fn null(&self) -> Js {
        Js::Null
    }
    fn object(&self, f: Vec<(&'static str, Js)>) -> Result<Js, Thrown> {
        Ok(Js::object(f))
    }
    fn assign(&self, o: &Js, k: &'static str, v: Js) -> Result<(), Thrown> {
        o.set(k, v)
    }
    fn spread_params(&self, s: &Js, l: Js, o: Js) -> Result<Js, Thrown> {
        let v = s.spread_object()?;
        v.define("limit", l)?;
        v.define("offset", o)?;
        Ok(v)
    }
    fn array(&self, a: Vec<Option<Js>>) -> Js {
        Js::holey_array(a)
    }
    fn map_output(&self, _: &Js, n: usize) -> Result<Js, Thrown> {
        Ok(Js::holey_array(vec![None; n]))
    }
    fn map_set(&self, o: &Js, i: usize, v: Js) -> Result<(), Thrown> {
        o.define(&i.to_string(), v)
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn length(&self, v: &Js) -> Result<usize, Thrown> {
        Ok(v.get("length")?.to_number()? as usize)
    }
    fn at(&self, v: &Js, i: usize) -> Result<Option<Js>, Thrown> {
        let k = i.to_string();
        if v.has_own(&k) {
            Ok(Some(v.get(&k)?))
        } else {
            Ok(None)
        }
    }
    fn push_spread(&self, a: &Js, v: &Js) -> Result<(), Thrown> {
        let mut it = Iter::new(v.clone())?;
        while let Some(v) = it.next()? {
            a.push(v)?;
        }
        Ok(())
    }
    fn less(&self, a: &Js, b: &Js) -> Result<bool, Thrown> {
        ops::less_than(a, b)
    }
    fn greater_equal(&self, a: &Js, b: &Js) -> Result<bool, Thrown> {
        ops::greater_equal(a, b)
    }
    fn add(&self, a: &Js, b: &Js) -> Result<Js, Thrown> {
        ops::add(a, b)
    }
    fn slice(&self, a: &Js, e: &Js) -> Result<Js, Thrown> {
        a.call_method("slice", vec![Js::Number(0.), e.clone()])
    }
    fn order_to_json(&self, o: Js, owner: Js, kind: Js) -> Result<Js, Thrown> {
        self.global("orderToJsonV2")?
            .call(Js::Undefined, vec![o, owner, kind])
    }
    fn new_error(&self, m: &'static str) -> Thrown {
        error(m)
    }
    fn error_message(&self, e: &Thrown) -> Result<Js, Thrown> {
        e.value.get("message")
    }
}

impl crate::exchange::ExchangeHost for Rt {
    fn bind_receiver(&self, r: Js) -> Result<Rc<Self>, Thrown> {
        Ok(Rc::new(self.with_this(r)))
    }
    fn derive(&self, base: Js, m: Vec<(&'static str, Js)>) -> Result<Js, Thrown> {
        Ok(Js::constructor("PolymarketExchange", move |_, a| {
            let r = base.call(Js::Undefined, a)?;
            for (k, v) in &m {
                if *k == "name" {
                    let f = v.clone();
                    r.define_accessor(
                        k,
                        Some(Rc::new(move |recv| f.call(recv.clone(), vec![]))),
                        None,
                    );
                } else {
                    r.define(k, v.clone())?;
                }
            }
            Ok(r)
        }))
    }
    fn prototype_method(&self, n: &'static str) -> Result<Js, Thrown> {
        super::members::exchange(self, n, None)
    }
    fn property(&self, o: &Js, k: Js) -> Result<Js, Thrown> {
        o.get_key(&k.to_js_string()?)
    }
    type Iterator = Iter;
    fn iterate(&self, v: Js) -> Result<Iter, Thrown> {
        Iter::new(v)
    }
    fn next(&self, i: &mut Iter) -> Result<Option<Js>, Thrown> {
        i.next()
    }
    fn spread(&self, v: &Js) -> Result<Js, Thrown> {
        v.spread_object()
    }
    fn greater(&self, a: &Js, b: &Js) -> Result<bool, Thrown> {
        ops::greater_than(a, b)
    }
    fn receiver(&self) -> Js {
        self.this.clone()
    }
    fn define(&self, o: &Js, k: &'static str, v: Js) -> Result<(), Thrown> {
        o.define(k, v)
    }
    fn method(&self, r: &Js, n: &'static str) -> Result<Js, Thrown> {
        super::members::exchange(self, n, Some(r.clone()))
    }
    fn async_method(&self, f: Rc<dyn Fn(Vec<Js>) -> crate::exchange::PinTask<Js, Thrown>>) -> Js {
        super::async_fn(self, "", f)
    }
    fn callback(&self, f: Rc<dyn Fn() -> Result<Js, Thrown>>) -> Js {
        Js::function("", move |_, _| f())
    }
    fn boolean(&self, v: bool) -> Js {
        Js::Bool(v)
    }
    fn strict_true(&self, v: &Js) -> bool {
        v.strict_equals(&Js::Bool(true))
    }
    fn strict_false(&self, v: &Js) -> bool {
        v.strict_equals(&Js::Bool(false))
    }
    fn finite(&self, v: &Js) -> bool {
        matches!(v,Js::Number(n) if n.is_finite())
    }
    fn to_number(&self, v: &Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn fixed(&self, v: &Js, d: &Js) -> Result<Js, Thrown> {
        v.call_method("toFixed", vec![d.clone()])
    }
    fn adjust(&self, v: Js, s: Js) -> Result<Js, Thrown> {
        self.global("adjust")?.call(Js::Undefined, vec![v, s])
    }
    fn error_value(&self, e: &Thrown) -> Js {
        e.value.clone()
    }
    fn error_from_value(&self, m: Js) -> Thrown {
        error(&m.to_display())
    }
    fn error_is_instance(&self, e: &Thrown) -> bool {
        e.value.get("message").is_ok_and(|m| !m.is_undefined())
    }
    fn has(&self, o: &Js, k: &'static str) -> Result<bool, Thrown> {
        if o.has_own(k) {
            return Ok(true);
        }
        if let Js::Object(data) = o {
            if let Some(p) = data.proto.borrow().as_ref() {
                return crate::exchange::ExchangeHost::has(self, p, k);
            }
        }
        Ok(false)
    }
    fn entries(&self, v: &Js) -> Result<Vec<(Js, Js)>, Thrown> {
        Ok(v.own_entries()?
            .into_iter()
            .map(|(k, v)| (Js::utf16(&k), v))
            .collect())
    }
    fn object_assign(&self, t: &Js, s: Js) -> Result<(), Thrown> {
        for (k, v) in s.own_entries()? {
            t.set_key(&k, v)?;
        }
        Ok(())
    }
    fn map_keys(&self, m: &Js) -> Result<Js, Thrown> {
        match m {
            Js::Map(m) => Ok(Js::array(
                m.borrow().iter().map(|(k, _)| k.clone()).collect(),
            )),
            _ => Err(Thrown::type_error("Map expected")),
        }
    }
    fn token_pairs(&self, m: Js) -> Result<Vec<(Js, Js)>, Thrown> {
        let ids = m.get("clobTokenIds")?;
        let outcomes = m.get("outcomes")?;
        let ids = if ids.is_nullish() {
            None
        } else {
            Some(super::text(&ids)?)
        };
        let outcomes = if outcomes.is_nullish() {
            None
        } else {
            Some(super::text(&outcomes)?)
        };
        Ok(
            crate::utilities::parse_gamma_market_tokens(ids.as_deref(), outcomes.as_deref())
                .into_iter()
                .map(|(k, v)| (Js::str(&k), Js::str(&v)))
                .collect(),
        )
    }
    fn tick_precision(&self, v: Js) -> Result<Js, Thrown> {
        let input = precision(&v)?;
        Ok(Js::Number(crate::utilities::precision_from_tick_size(
            input.as_ref(),
        )))
    }
    fn normalize_tick(&self, v: Js) -> Result<Js, Thrown> {
        let input = precision(&v)?;
        crate::utilities::normalize_tick_size(input.as_ref())
            .map(|v| Js::str(&v))
            .map_err(|e| error(&e))
    }
    fn disposed(&self) -> bool {
        self.base().is_disposed()
    }
    fn schedule(&self, f: Rc<dyn Fn()>, d: f64) -> Result<(), Thrown> {
        self.set_timeout(
            Rc::new(move || {
                f();
                Ok(())
            }),
            d,
        );
        Ok(())
    }
    fn detach(&self, t: crate::exchange::PinTask<Js, Thrown>) {
        let rt = self.clone();
        super::retained_task(self, async move {
            if let Err(e) = t.await {
                rt.report(&e, "Polymarket detached task");
            }
        })
    }
}
fn precision(v: &Js) -> Result<Option<safe_cex_num::PrecisionInput>, Thrown> {
    Ok(if v.is_nullish() {
        None
    } else if let Js::Number(n) = v {
        Some(safe_cex_num::PrecisionInput::Number(*n))
    } else {
        Some(safe_cex_num::PrecisionInput::String(super::text(v)?))
    })
}

impl crate::api_factory::FactoryHost for Rt {
    fn table(&self, n: &'static str) -> Result<Js, Thrown> {
        self.global(n)
    }
    fn function(&self, f: Rc<dyn Fn(Vec<Js>) -> Result<Js, Thrown>>) -> Js {
        Js::function("", move |_, a| f(a))
    }
    fn apply(&self, f: Js, a: Vec<Js>) -> Result<Js, Thrown> {
        f.call(Js::Undefined, a)
    }
    fn is_string_value(&self, v: &Js) -> bool {
        v.is_string()
    }
    fn starts_with(&self, v: &Js, p: Js) -> Result<bool, Thrown> {
        v.call_method("startsWith", vec![p]).map(|v| v.truthy())
    }
    fn qs_comma(&self, p: Js) -> Result<Js, Thrown> {
        let p = p.spread_object()?;
        for (k, v) in p.own_entries()? {
            if v.is_array() {
                p.set_key(&k, v.call_method("join", vec![Js::str(",")])?)?;
            }
        }
        self.global("qs.stringify")?.call(Js::Undefined, vec![p])
    }
    fn json_stringify(&self, v: Js) -> Result<Js, Thrown> {
        self.global("JSON.stringify")?.call(Js::Undefined, vec![v])
    }
    fn sleep(&self, d: f64) -> Result<Rc<Operation>, Thrown> {
        let rt = self.shared.runtime.clone();
        self.resolve(self.spawn(Box::pin(async move {
            rt.sleep(d.max(0.) as u64)
                .await
                .map_err(|e| error(&e.message))?;
            Ok(Js::Undefined)
        })))
    }
    fn promise_reject(&self, e: Js) -> Result<Js, Thrown> {
        Ok(Js::Promise(Promise::settled(
            &self.exchange_runtime(),
            Err(Thrown::new(e)),
        )))
    }
    fn inherit(&self, p: Js) -> Result<Js, Thrown> {
        let o = Js::object(vec![]);
        if let Js::Object(data) = &o {
            *data.proto.borrow_mut() = Some(p);
        }
        Ok(o)
    }
}
impl crate::classes::ClassHost for Rt {
    fn define_class(
        &self,
        n: &'static str,
        b: Option<Js>,
        p: Vec<(&'static str, Js)>,
    ) -> Result<Js, Thrown> {
        Ok(Js::constructor(n, move |_, a| {
            let r = match &b {
                Some(b) => b.call(Js::Undefined, a)?,
                None => Js::object_with_class(n, vec![]),
            };
            for (k, v) in &p {
                r.define(k, v.clone())?;
            }
            Ok(r)
        }))
    }
    fn class_method(&self, c: &'static str, n: &'static str, r: Option<Js>) -> Result<Js, Thrown> {
        super::members::class(self, c, n, r)
    }
    fn throttle(&self, f: Js, w: f64, l: bool, t: bool) -> Result<Js, Thrown> {
        self.global("throttle")?.call(
            Js::Undefined,
            vec![
                f,
                Js::Number(w),
                Js::object(vec![("leading", Js::Bool(l)), ("trailing", Js::Bool(t))]),
            ],
        )
    }
    fn for_each_set(&self, s: Js, f: Rc<dyn Fn(Js) -> Result<(), Thrown>>) -> Result<(), Thrown> {
        if !matches!(s, Js::Set(_)) {
            return Err(Thrown::type_error("Set expected"));
        }
        let mut iterator = Iter::new(s)?;
        while let Some(value) = iterator.next()? {
            f(value)?;
        }
        Ok(())
    }
    fn strict_zero(&self, v: &Js) -> bool {
        v.strict_equals(&Js::Number(0.))
    }
}

impl crate::state_sync::StateHost for Rt {
    fn disposed(&self) -> bool {
        self.base().is_disposed()
    }
    fn public_only(&self) -> bool {
        self.this.get("isPublicDataOnly").is_ok_and(|v| v.truthy())
    }
    fn assign_credentials(&self, v: Js) -> Result<(), Thrown> {
        self.this.set("credentials", v)
    }
    fn funder(&self) -> Result<Js, Thrown> {
        self.this.get("funderAddress")
    }
    fn ticker(&self, s: &Js) -> Result<Js, Thrown> {
        self.this
            .get("store")?
            .get("tickersBySymbol")?
            .get_key(&s.to_js_string()?)
    }
    fn loaded(&self) -> Result<Js, Thrown> {
        self.this.get("store")?.get("loaded")
    }
    fn spread_loaded(&self, v: Js, f: &[&'static str]) -> Result<Js, Thrown> {
        let o = v.spread_object()?;
        for k in f {
            o.define(k, Js::Bool(true))?;
        }
        Ok(o)
    }
    fn call_sync(&self, c: Call, a: Vec<Js>) -> Result<Js, Thrown> {
        AuthHost::call_sync(self, c, a)
    }
    fn finite(&self, v: &Js) -> bool {
        crate::exchange::ExchangeHost::finite(self, v)
    }
    fn less_equal(&self, a: &Js, b: &Js) -> Result<bool, Thrown> {
        ops::less_equal(a, b)
    }
    fn multiply(&self, a: &Js, b: &Js) -> Result<Js, Thrown> {
        ops::multiply(a, b)
    }
    fn subtract(&self, a: &Js, b: &Js) -> Result<Js, Thrown> {
        ops::subtract(a, b)
    }
    type Iterator = Iter;
    fn iterate(&self, v: Js) -> Result<Iter, Thrown> {
        Iter::new(v)
    }
    fn next(&self, i: &mut Iter) -> Result<Option<Js>, Thrown> {
        i.next()
    }
    fn sum_by(&self, p: &Js, k: &'static str) -> Result<Js, Thrown> {
        self.global("sumBy")?
            .call(Js::Undefined, vec![p.clone(), Js::str(k)])
    }
    fn error_value(&self, e: &Thrown) -> Js {
        e.value.clone()
    }
    fn optional_token_get(&self) -> Result<Option<Call>, Thrown> {
        let parent = if self.this.has_own("parent") {
            self.this.get("parent")?
        } else {
            self.this.clone()
        };
        let m = parent.get("tokenToSymbol")?;
        if m.is_nullish() {
            Ok(None)
        } else {
            Ok(Some(self.capture_value(m, "get")?))
        }
    }
    fn side_lookup(&self, k: Js) -> Result<Js, Thrown> {
        self.global("ORDER_SIDE_MAP")?.get_key(&k.to_js_string()?)
    }
    fn emit_fill(&self, c: Call, f: crate::order_adapters::NativeFill<Js>) -> Result<(), Thrown> {
        AuthHost::call_sync(self, c, vec![Js::str("fill"), fill(f)])?;
        Ok(())
    }
    fn schedule_tick(&self, f: Rc<dyn Fn() -> Result<Js, Thrown>>, d: f64) -> Result<(), Thrown> {
        self.set_timeout(
            Rc::new(move || {
                f()?;
                Ok(())
            }),
            d,
        );
        Ok(())
    }
    fn catch_rejection(
        &self,
        o: Rc<Operation>,
        f: Rc<dyn Fn(Thrown) -> Result<(), Thrown>>,
    ) -> Result<(), Thrown> {
        let rt = self.clone();
        super::retained_task(self, async move {
            if let Err(e) = o.promise.wait().await {
                if let Err(e) = f(e) {
                    rt.report(&e, "refreshPositions");
                }
            }
        });
        Ok(())
    }
}
pub(crate) fn fill(f: crate::order_adapters::NativeFill<Js>) -> Js {
    Js::object(vec![
        ("id", f.id),
        ("side", f.side),
        ("symbol", f.symbol),
        ("price", Js::Number(f.price)),
        ("amount", Js::Number(f.amount)),
    ])
}
pub(crate) fn order(o: &crate::order_adapters::NativeOrder<Js>) -> Js {
    Js::object(vec![
        ("id", o.id.clone()),
        ("status", o.status.clone()),
        ("symbol", o.symbol.clone()),
        ("type", o.order_type.clone()),
        ("side", o.side.clone()),
        ("price", Js::Number(o.price)),
        ("amount", Js::Number(o.amount)),
        ("filled", Js::Number(o.filled)),
        ("remaining", Js::Number(o.remaining)),
        ("reduceOnly", Js::Bool(o.reduce_only)),
        ("info", o.info.clone()),
        ("lastUpdate", Js::Number(o.last_update)),
    ])
}
impl crate::order_adapters::OrderHost for Rt {
    type TokenMap = Call;
    fn token_map(&self, optional: bool) -> Result<Option<Call>, Thrown> {
        let e = if self.this.has_own("parent") {
            self.this.get("parent")?
        } else {
            self.this.clone()
        };
        let m = e.get("tokenToSymbol")?;
        if optional && m.is_nullish() {
            Ok(None)
        } else {
            Ok(Some(self.capture_value(m, "get")?))
        }
    }
    fn token_symbol(&self, c: &Call, t: &Js) -> Result<Js, Thrown> {
        self.call_sync(c.clone(), vec![t.clone()])
    }
    fn enum_lookup(&self, t: &'static str, k: &Js) -> Result<Js, Thrown> {
        self.global(t)?.get_key(&k.to_js_string()?)
    }
    fn literal(&self, s: &'static str) -> Js {
        Js::str(s)
    }
    fn clock(&self) -> Result<f64, Thrown> {
        Ok(self.now())
    }
    fn error_string(&self, e: &Thrown) -> Result<Vec<u16>, Thrown> {
        Ok(e.value.to_js_string()?.to_vec())
    }
    fn lowercase(&self, v: &Js) -> Result<Js, Thrown> {
        v.call_method("toLowerCase", vec![])
    }
    fn map_order(
        &self,
        v: &Js,
    ) -> Result<Rc<std::cell::RefCell<crate::order_adapters::NativeOrder<Js>>>, Thrown> {
        Ok(Rc::new(std::cell::RefCell::new(
            crate::order_adapters::map_exchange_order(self, v)?,
        )))
    }
    fn add_orders(
        &self,
        o: Vec<Rc<std::cell::RefCell<crate::order_adapters::NativeOrder<Js>>>>,
    ) -> Result<(), Thrown> {
        self.this.get("parent")?.get("store")?.call_method(
            "addOrUpdateOrders",
            vec![Js::array(o.iter().map(|o| order(&o.borrow())).collect())],
        )?;
        Ok(())
    }
    fn emit_fill(&self, f: crate::order_adapters::NativeFill<Js>) -> Result<(), Thrown> {
        self.this
            .get("parent")?
            .get("emitter")?
            .call_method("emit", vec![Js::str("fill"), fill(f)])?;
        Ok(())
    }
}
impl crate::order_adapters::AdapterHost for Rt {
    type Array = Js;
    type Operation = Operation;
    type Callback = Js;
    type Cleanup = Js;
    fn array(&self, v: &Js) -> Result<Js, Thrown> {
        if v.is_array() {
            Ok(v.clone())
        } else {
            Err(Thrown::type_error("value.map is not a function"))
        }
    }
    fn length(&self, v: &Js) -> Result<usize, Thrown> {
        AccountHost::length(self, v)
    }
    fn at(&self, v: &Js, i: usize) -> Result<Option<Js>, Thrown> {
        AccountHost::at(self, v, i)
    }
    fn start_task(&self, t: Pin<Box<dyn Future<Output = ()>>>) {
        AccountHost::start_task(self, t)
    }
    fn call(&self, c: crate::order_adapters::ApiCall<Js>) -> Result<Rc<Operation>, Thrown> {
        use crate::order_adapters::ApiCall;
        let (n, a) = match c {
            ApiCall::OpenOrders => ("getOpenOrders", vec![]),
            ApiCall::CancelOrders(ids) => ("cancelOrders", vec![Js::holey_array(ids)]),
            ApiCall::CancelAllOrders => ("cancelAllOrders", vec![]),
            ApiCall::OrderBook(t) => ("getOrderBook", vec![t]),
        };
        self.invoke(self.capture("api", n)?, a)
    }
    fn token(&self, s: &[u16]) -> Result<Js, Thrown> {
        self.this
            .get("symbolToToken")?
            .call_method("get", vec![Js::utf16(s)])
    }
    fn new_error(&self, m: Vec<u16>) -> Thrown {
        error(&String::from_utf16_lossy(&m))
    }
    fn subscribe_book(&self, t: Js, c: Js) -> Result<Js, Thrown> {
        self.this
            .get("publicWebsocket")?
            .call_method("subscribeToOrderBook", vec![t, c])
    }
    fn connect(&self) -> Result<(), Thrown> {
        self.this
            .get("publicWebsocket")?
            .call_method("connectAndSubscribe", vec![])?;
        Ok(())
    }
}
impl AuthHost for Rt {
    type Constructor = Js;
    fn capture_value(&self, r: Js, m: &'static str) -> Result<Call, Thrown> {
        let f = r.get(m)?;
        Ok((r, f))
    }
    fn call_sync(&self, c: Call, a: Vec<Js>) -> Result<Js, Thrown> {
        c.1.call(c.0, a)
    }
    fn capture_constructor(&self, n: &'static str) -> Result<Js, Thrown> {
        self.global(n)
    }
    fn construct(&self, c: Js, a: Vec<Js>) -> Result<Js, Thrown> {
        c.call(Js::Undefined, a)
    }
    fn clob_auth_types(&self) -> Result<Js, Thrown> {
        self.global("CLOB_AUTH_TYPES")
    }
    fn signer(&self, s: WalletSigner<Self>) -> Result<Js, Thrown> {
        let s = Rc::new(s);
        let g = s.clone();
        Ok(Js::object(vec![
            (
                "getAddress",
                Js::function("getAddress", move |_, _| g.get_address()),
            ),
            (
                "_signTypedData",
                Js::function("_signTypedData", move |_, a| {
                    s.sign_typed_data(
                        safe_cex_exchange::js_runtime::value::arg(&a, 0),
                        safe_cex_exchange::js_runtime::value::arg(&a, 1),
                        safe_cex_exchange::js_runtime::value::arg(&a, 2),
                    )
                }),
            ),
        ]))
    }
    fn sdk_side(&self, b: bool) -> Js {
        Js::str(if b { "BUY" } else { "SELL" })
    }
    fn string_value(&self, u: Vec<u16>) -> Js {
        Js::utf16(&u)
    }
    fn uppercase(&self, v: Js) -> Result<Js, Thrown> {
        v.call_method("toUpperCase", vec![])
    }
    fn replace_ascii(&self, v: Js, from: u16, to: u16) -> Result<Js, Thrown> {
        Ok(Js::utf16(
            &v.to_js_string()?
                .iter()
                .map(|c| if *c == from { to } else { *c })
                .collect::<Vec<_>>(),
        ))
    }
    fn is_undefined(&self, v: &Js) -> bool {
        v.is_undefined()
    }
    fn date_now(&self) -> Result<f64, Thrown> {
        Ok(self.now())
    }
    fn retry_delay(&self, f: Rc<dyn Fn(Js) -> Result<Js, Thrown>>) -> Result<Js, Thrown> {
        Ok(Js::function("retryDelay", move |_, a| {
            f(safe_cex_exchange::js_runtime::value::arg(&a, 0))
        }))
    }
    fn multiply(&self, a: Js, b: Js) -> Result<Js, Thrown> {
        ops::multiply(&a, &b)
    }
}
