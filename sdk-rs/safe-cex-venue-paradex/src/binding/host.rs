use super::{Rt, Shared, scope};
use safe_cex_exchange::js_runtime::{
    promise::{JsFuture, await_js},
    scope::bound_member,
    value::{Js, Thrown},
};
use std::rc::{Rc, Weak};
pub(super) struct Host {
    weak: Weak<Shared>,
    this: Js,
}
impl Host {
    pub(super) fn new(rt: &Rt) -> Self {
        Self {
            weak: Rc::downgrade(&rt.shared),
            this: rt.this.clone(),
        }
    }
    pub(super) fn rt(&self) -> Result<Rt, Thrown> {
        scope(&self.weak, self.this.clone())
    }
    pub(super) fn intrinsic(&self, n: &str, a: Vec<Js>) -> Result<Js, Thrown> {
        super::primitives::call(&self.rt()?, n, a)
    }
    pub(super) fn selected(&self, n: &str) -> Result<Js, Thrown> {
        let rt = self.rt()?;
        super::members::select(&rt, n)
    }
    pub(super) fn future(&self, v: Js) -> JsFuture<'static> {
        {
            let rt = self.rt();
            Box::pin(async move { await_js(&rt?.exchange_runtime(), v).await })
        }
    }
    pub(super) fn operation(&self, n: &str, a: Vec<Js>) -> Result<JsFuture<'static>, Thrown> {
        Ok(self.future(self.selected(n)?.call(Js::Undefined, a)?))
    }
    pub(super) fn logger(&self) -> Result<Js, Thrown> {
        {
            let parent = self.this.get("parent")?;
            bound_member(
                if parent.is_undefined() {
                    &self.this
                } else {
                    &parent
                },
                "log",
            )
        }
    }
    pub(super) fn log(&self, l: Js, t: Vec<u16>, s: &str) -> Result<(), Thrown> {
        l.call(Js::Undefined, vec![Js::utf16(&t), Js::str(s)])?;
        Ok(())
    }
    pub(super) fn error(&self, t: Vec<u16>) -> Thrown {
        Thrown::new(safe_cex_exchange::js_runtime::value::error_object(
            "Error",
            &String::from_utf16_lossy(&t),
        ))
    }
    pub(super) fn emit(&self, v: Js) -> Result<(), Thrown> {
        self.this.call_method("emitAndLogError", vec![v])?;
        Ok(())
    }
}
macro_rules! scalars {
    () => {
        fn get(&self, v: &Js, k: &str) -> Result<Js, Thrown> {
            v.get(k)
        }
        fn truthy(&self, v: &Js) -> bool {
            v.truthy()
        }
        fn literal(&self, s: &str) -> Js {
            Js::str(s)
        }
        fn number(&self, n: f64) -> Js {
            Js::Number(n)
        }
        fn boolean(&self, b: bool) -> Js {
            Js::Bool(b)
        }
        fn object(&self, f: Vec<(&'static str, Js)>) -> Js {
            Js::object(f)
        }
        fn template(&self, v: &Js) -> Result<Vec<u16>, Thrown> {
            v.to_js_string().map(|v| v.to_vec())
        }
    };
}
macro_rules! arrays {
    () => {
        fn array(&self, v: Vec<Js>) -> Js {
            Js::array(v)
        }
        fn undefined(&self) -> Js {
            Js::Undefined
        }
        fn equal(&self, a: &Js, b: &Js) -> bool {
            a.strict_equals(b)
        }
    };
}
impl crate::runtime_assembly::RuntimeAssemblyHost for Host {
    type Value = Js;
    type Error = Thrown;
    scalars!();
    arrays!();
    fn optional(&self, v: &Js, k: &str) -> Result<Js, Thrown> {
        if v.is_nullish() {
            Ok(Js::Undefined)
        } else {
            v.get(k)
        }
    }
    fn numeric(&self, v: &Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn intrinsic(&self, n: &str, a: Vec<Js>) -> Result<Js, Thrown> {
        Host::intrinsic(self, n, a)
    }
    fn global(&self) -> Js {
        self.rt().expect("live host").shared.globals.clone()
    }
    fn select(&self, v: Js, n: &str) -> Result<Js, Thrown> {
        bound_member(&v, n)
    }
    fn apply(&self, f: Js, a: Vec<Js>) -> Result<Js, Thrown> {
        f.call(Js::Undefined, a)
    }
    fn invoke(&self, f: Js, a: Vec<Js>) -> Result<JsFuture<'static>, Thrown> {
        Ok(self.future(f.call(Js::Undefined, a)?))
    }
    fn wait(&self, v: Js) -> Result<JsFuture<'static>, Thrown> {
        Ok(self.future(v))
    }
    fn write(&self, v: &Js, k: &str, x: Js) -> Result<(), Thrown> {
        v.set(k, x)
    }
    fn allocate(&self, c: &'static str, a: Vec<Js>) -> Result<Js, Thrown> {
        super::members::allocate(&self.rt()?, c, a)
    }
    fn define(
        &self,
        v: &Js,
        k: &'static str,
        x: Js,
        w: bool,
        e: bool,
        c: bool,
    ) -> Result<(), Thrown> {
        self.rt()?
            .global("Object.defineProperty")?
            .call(
                Js::Undefined,
                vec![
                    v.clone(),
                    Js::str(k),
                    Js::object(vec![
                        ("value", x),
                        ("writable", Js::Bool(w)),
                        ("enumerable", Js::Bool(e)),
                        ("configurable", Js::Bool(c)),
                    ]),
                ],
            )
            .map(|_| ())
    }
    fn callback(&self, o: Js, c: &'static str, a: Vec<Js>) -> Result<Js, Thrown> {
        Ok(super::members::callback(&self.rt()?.with_this(o), c, a))
    }
    fn start(&self, c: Js) -> Result<(), Thrown> {
        let promise = c.call(Js::Undefined, vec![])?;
        self.observe(
            promise,
            Js::function("report", {
                let weak = self.weak.clone();
                move |_, a| {
                    let rt = scope(&weak, Js::Undefined)?;
                    rt.report(
                        &Thrown::new(safe_cex_exchange::js_runtime::value::arg(&a, 0)),
                        "Paradex detached callback",
                    );
                    Ok(Js::Undefined)
                }
            }),
        )
    }
    fn observe(&self, p: Js, r: Js) -> Result<(), Thrown> {
        p.call_method("catch", vec![r])?;
        Ok(())
    }
    fn make_error(&self, t: Vec<u16>) -> Result<Thrown, Thrown> {
        Ok(self.error(t))
    }
    fn error_value(&self, e: &Thrown) -> Js {
        e.value.clone()
    }
    fn error_from_value(&self, v: Js) -> Thrown {
        Thrown::new(v)
    }
}
impl crate::market_data::MarketDataHost for Host {
    type Value = Js;
    type Error = Thrown;
    scalars!();
    arrays!();
    fn optional(&self, v: &Js, k: &str) -> Result<Js, Thrown> {
        if v.is_nullish() {
            Ok(Js::Undefined)
        } else {
            v.get(k)
        }
    }
    fn state(&self, k: &str) -> Result<Js, Thrown> {
        if k == "this" {
            Ok(self.this.clone())
        } else {
            self.rt()?.path(k)
        }
    }
    fn nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn is_object(&self, v: &Js) -> bool {
        matches!(v, Js::Object(_) | Js::Array(_) | Js::Null)
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn is_undefined(&self, v: &Js) -> bool {
        v.is_undefined()
    }
    fn reduce(
        &self,
        v: Js,
        mut x: Js,
        c: &mut dyn FnMut(Js, Js) -> Result<Js, Thrown>,
    ) -> Result<Js, Thrown> {
        let a = v
            .array_items()
            .ok_or_else(|| Thrown::type_error("value.reduce is not a function"))?;
        for v in a.into_iter().flatten() {
            x = c(x, v)?;
        }
        Ok(x)
    }
    fn for_each(&self, v: Js, c: &mut dyn FnMut(Js) -> Result<(), Thrown>) -> Result<(), Thrown> {
        for v in v
            .array_items()
            .ok_or_else(|| Thrown::type_error("value.forEach is not a function"))?
            .into_iter()
            .flatten()
        {
            c(v)?;
        }
        Ok(())
    }
    fn numeric(&self, v: &Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn intrinsic(&self, n: &str, a: Vec<Js>) -> Result<Js, Thrown> {
        Host::intrinsic(self, n, a)
    }
    fn map(&self, v: Js, c: &mut dyn FnMut(Js) -> Result<Js, Thrown>) -> Result<Js, Thrown> {
        let a = v
            .array_items()
            .ok_or_else(|| Thrown::type_error("value.map is not a function"))?;
        Ok(Js::Array(Rc::new(std::cell::RefCell::new(
            a.into_iter()
                .map(|v| v.map(&mut *c).transpose())
                .collect::<Result<Vec<_>, _>>()?,
        ))))
    }
    fn filter(&self, v: Js, c: &mut dyn FnMut(Js) -> Result<bool, Thrown>) -> Result<Js, Thrown> {
        let mut out = vec![];
        for v in v
            .array_items()
            .ok_or_else(|| Thrown::type_error("value.filter is not a function"))?
            .into_iter()
            .flatten()
        {
            if c(v.clone())? {
                out.push(v);
            }
        }
        Ok(Js::array(out))
    }
    fn sort(&self, v: Js, c: &mut dyn FnMut(Js, Js) -> Result<f64, Thrown>) -> Result<Js, Thrown> {
        let mut a = v
            .array_values()
            .ok_or_else(|| Thrown::type_error("value.sort is not a function"))?;
        let mut error = None;
        a.sort_by(|x, y| match c(x.clone(), y.clone()) {
            Ok(n) => n.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal),
            Err(e) => {
                error = Some(e);
                std::cmp::Ordering::Equal
            }
        });
        if let Some(e) = error {
            return Err(e);
        }
        if let Js::Array(out) = &v {
            *out.borrow_mut() = a.into_iter().map(Some).collect();
        }
        Ok(v)
    }
    fn select_log(&self) -> Result<Js, Thrown> {
        self.logger()
    }
    fn log(&self, l: Js, t: Vec<u16>, s: &str) -> Result<(), Thrown> {
        Host::log(self, l, t, s)
    }
    fn operation(&self, n: &str, a: Vec<Js>) -> Result<JsFuture<'static>, Thrown> {
        Host::operation(self, n, a)
    }
    fn update(&self, f: Vec<(&'static str, Js)>, l: &[&str]) -> Result<(), Thrown> {
        let store = self.this.get("store")?;
        let fields = Js::object(f);
        if !l.is_empty() {
            let loaded = store.get("loaded")?.spread_object()?;
            for key in l {
                loaded.set(key, Js::Bool(true))?;
            }
            fields.set("loaded", loaded)?;
        }
        store.call_method("update", vec![fields])?;
        Ok(())
    }
    fn write(&self, v: &Js, k: &str, x: Js) -> Result<(), Thrown> {
        v.set(k, x)
    }
    fn error_value(&self, e: &Thrown) -> Js {
        e.value.clone()
    }
    fn new_error(&self, t: &str) -> Result<Thrown, Thrown> {
        Ok(self.error(t.encode_utf16().collect()))
    }
    fn schedule_tick(&self, d: f64) -> Result<(), Thrown> {
        let rt = self.rt()?;
        let tick = bound_member(&self.this, "tick")?;
        rt.global("loop")?
            .call(Js::Undefined, vec![tick, Js::Number(d)])?;
        Ok(())
    }
    fn callbacks(&self, s: Js) -> Result<Js, Thrown> {
        self.this
            .get("orderbookCallbacks")?
            .call_method("get", vec![s])
    }
    fn broadcast(&self, c: Js, b: Js) -> Result<(), Thrown> {
        self.intrinsic("broadcast", vec![c, b])?;
        Ok(())
    }
    fn update_ticker(&self, s: Js, f: Vec<(&'static str, Js)>) -> Result<(), Thrown> {
        self.this.get("parent")?.get("store")?.call_method(
            "updateTicker",
            vec![Js::object(vec![("id", s)]), Js::object(f)],
        )?;
        Ok(())
    }
}
impl crate::account_rest::AccountRestHost for Host {
    fn make_error(&self, t: Vec<u16>) -> Result<Thrown, Thrown> {
        Ok(self.error(t))
    }
    fn kind(&self, v: &Js) -> &'static str {
        v.type_of()
    }
    fn select(&self, n: &str) -> Result<Js, Thrown> {
        self.selected(n)
    }
    fn invoke(&self, f: Js, a: Vec<Js>) -> Result<JsFuture<'static>, Thrown> {
        Ok(self.future(f.call(Js::Undefined, a)?))
    }
    fn console(&self, a: Vec<Js>) -> Result<(), Thrown> {
        self.rt()?.global("console.error")?.call(Js::Undefined, a)?;
        Ok(())
    }
    fn emit(&self, v: Js) -> Result<(), Thrown> {
        Host::emit(self, v)
    }
    fn income(
        &self,
        r: Js,
        resolve: &mut dyn FnMut(Js) -> Result<Js, Thrown>,
    ) -> Result<Js, Thrown> {
        let raw = safe_cex_exchange::js_runtime::value::to_json(&r)?;
        let row = raw
            .as_object()
            .ok_or_else(|| Thrown::type_error("Income row must be an object"))?;
        let records = crate::income::map_paradex_fill_to_income_records::<MapperError>(row, |id| {
            let value = resolve(Js::str(id)).map_err(MapperError)?;
            if value.is_nullish() {
                Ok(None)
            } else {
                Ok(Some(super::text(&value).map_err(MapperError)?))
            }
        })
        .map_err(|e| e.0)?;
        safe_cex_exchange::js_runtime::value::from_typed(&records)
    }
    fn historical(&self, r: Js) -> Result<Js, Thrown> {
        let raw = safe_cex_exchange::js_runtime::value::to_json(&r)?;
        safe_cex_exchange::js_runtime::value::from_typed(
            &crate::order_history::map_paradex_historical_order(
                raw.as_object()
                    .ok_or_else(|| Thrown::type_error("History row must be an object"))?,
            )
            .map_err(|e| Thrown::error(e.name, e.message))?,
        )
    }
    fn history_error(&self, e: &Thrown) -> Result<bool, Thrown> {
        Ok(e.value
            .get("name")?
            .strict_equals(&Js::str("ParadexPrivateHistoryAuthNotReadyError")))
    }
}
impl crate::order_execution::OrderExecutionHost for Host {
    fn map_indexed(
        &self,
        v: Js,
        c: &mut dyn FnMut(Js, f64) -> Result<Js, Thrown>,
    ) -> Result<Js, Thrown> {
        let a = v
            .array_items()
            .ok_or_else(|| Thrown::type_error("value.map is not a function"))?;
        Ok(Js::Array(Rc::new(std::cell::RefCell::new(
            a.into_iter()
                .enumerate()
                .map(|(i, v)| v.map(|v| c(v, i as f64)).transpose())
                .collect::<Result<Vec<_>, _>>()?,
        ))))
    }
    fn find(&self, v: Js, c: &mut dyn FnMut(Js) -> Result<bool, Thrown>) -> Result<Js, Thrown> {
        for v in v
            .array_values()
            .ok_or_else(|| Thrown::type_error("value.find is not a function"))?
        {
            if c(v.clone())? {
                return Ok(v);
            }
        }
        Ok(Js::Undefined)
    }
    fn arguments(&self, v: Js) -> Result<Vec<Js>, Thrown> {
        super::primitives::iterate(&v)
    }
    fn sync_call(&self, f: Js, a: Vec<Js>) -> Result<Js, Thrown> {
        f.call(Js::Undefined, a)
    }
    fn write_state(&self, k: &str, v: Js) -> Result<(), Thrown> {
        self.this.set(k, v)
    }
    fn observe_rejection(&self, p: Js, s: Js) -> Result<(), Thrown> {
        let weak = self.weak.clone();
        let owner = self.this.clone();
        p.call_method(
            "catch",
            vec![Js::function("book-slippage-error", move |_, a| {
                crate::order_execution::book_slippage_rejected(
                    &Host::new(&scope(&weak, owner.clone())?),
                    s.clone(),
                    Thrown::new(safe_cex_exchange::js_runtime::value::arg(&a, 0)),
                )
            })],
        )?;
        Ok(())
    }
}
impl crate::auth_initialization::AuthInitializationHost for Host {
    type Value = Js;
    type Error = Thrown;
    type Log = Js;
    type HeaderSetter = Js;
    scalars!();
    fn optional_get(&self, v: &Js, k: &str) -> Result<Js, Thrown> {
        if v.is_nullish() {
            Ok(Js::Undefined)
        } else {
            v.get(k)
        }
    }
    fn null(&self) -> Js {
        Js::Null
    }
    fn text_value(&self, t: Vec<u16>) -> Js {
        Js::utf16(&t)
    }
    fn lowercase(&self, v: Js) -> Result<Js, Thrown> {
        v.call_method("toLowerCase", vec![])
    }
    fn length(&self, v: &Js) -> Result<f64, Thrown> {
        v.get("length")?.to_number()
    }
    fn trim_empty(&self, v: &Js) -> Result<bool, Thrown> {
        Ok(v.call_method("trim", vec![])?.get("length")?.to_number()? == 0.0)
    }
    fn state(&self, k: &str) -> Result<Js, Thrown> {
        self.rt()?.path(k)
    }
    fn write_state(&self, k: &str, v: Js) -> Result<(), Thrown> {
        self.this.set(k, v)
    }
    fn operation(
        &self,
        op: crate::auth_initialization::AuthOperation,
        a: Vec<Js>,
    ) -> Result<JsFuture<'static>, Thrown> {
        let rt = self.rt()?;
        Ok(Box::pin(async move {
            super::sdk::auth_operation(&rt, op, a).await
        }))
    }
    fn construct(
        &self,
        k: crate::auth_initialization::AuthConstruction,
        a: Vec<Js>,
    ) -> Result<Js, Thrown> {
        super::sdk::construct(&self.rt()?, k, a)
    }
    fn await_value(&self, v: Js) -> Result<JsFuture<'static>, Thrown> {
        Ok(self.future(v))
    }
    fn initialize_promise(&self) -> Result<Js, Thrown> {
        self.this.call_method("initializeSDK", vec![])
    }
    fn observe_rejection(&self, p: Js) -> Result<(), Thrown> {
        p.call_method(
            "catch",
            vec![Js::function("observedInit", |_, _| Ok(Js::Undefined))],
        )?;
        Ok(())
    }
    fn reset_sdk(&self) -> Result<(), Thrown> {
        self.this
            .call_method("resetSDKStateAfterInitFailure", vec![])?;
        Ok(())
    }
    fn new_error(&self, t: Vec<u16>) -> Result<Thrown, Thrown> {
        Ok(self.error(t))
    }
    fn history_error(&self, op: &[u16], last: Option<Thrown>) -> Result<Thrown, Thrown> {
        let value = last.map_or(Js::Undefined, |e| e.value);
        let owner = crate::auth_initialization::initialize_history_auth_error(
            op,
            &value,
            |v| Ok::<_, Thrown>(v.get("message").is_ok() && !v.get("name")?.is_undefined()),
            |v| v.get("message"),
            Js::truthy,
            |v| v.to_js_string().map(|v| v.to_vec()),
            |m| Ok(self.error(m).value),
            |o, n| o.set("name", Js::str(n)),
            |o, v| o.set("originalError", v.clone()),
        )?;
        Ok(Thrown::new(owner))
    }
    fn error_value(&self, e: &Thrown) -> Js {
        e.value.clone()
    }
    fn emit(&self, v: Js) -> Result<(), Thrown> {
        Host::emit(self, v)
    }
    fn select_log(&self) -> Result<Js, Thrown> {
        self.logger()
    }
    fn log(&self, l: Js, t: Vec<u16>, s: &'static str) -> Result<(), Thrown> {
        Host::log(self, l, t, s)
    }
    fn console(&self, e: bool, p: &'static str, v: Option<Js>) -> Result<(), Thrown> {
        let mut a = vec![Js::str(p)];
        a.extend(v);
        self.rt()?
            .global(if e { "console.error" } else { "console.log" })?
            .call(Js::Undefined, a)?;
        Ok(())
    }
    fn header_setter(&self, c: &Js) -> Result<Js, Thrown> {
        bound_member(&c.get("headers")?, "set")
    }
    fn set_header(&self, s: Js, n: &'static str, v: Js) -> Result<Js, Thrown> {
        s.call(Js::Undefined, vec![Js::str(n), v])
    }
    fn signer_address(&self, s: Js) -> Result<Js, Thrown> {
        s.call_method("getAccountAddress", vec![])
    }
    fn select_ethereum(&self, _: &Js) -> Result<Js, Thrown> {
        Ok(Js::str("fromEthSigner"))
    }
}

struct MapperError(Thrown);
impl From<safe_cex_num::JsCoercionError> for MapperError {
    fn from(e: safe_cex_num::JsCoercionError) -> Self {
        Self(Thrown::error(e.name, e.message))
    }
}
pub(super) struct CommandIterator {
    value: Js,
    index: std::cell::Cell<usize>,
}
impl crate::order_commands::OrderCommandHost for Host {
    type Value = Js;
    type Error = Thrown;
    type Log = Js;
    type Iterator = CommandIterator;
    fn iterator(&self, v: &Js) -> Result<CommandIterator, Thrown> {
        super::primitives::iterate(v)?;
        Ok(CommandIterator {
            value: v.clone(),
            index: std::cell::Cell::new(0),
        })
    }
    fn next(&self, it: &CommandIterator) -> Result<Option<Js>, Thrown> {
        let i = it.index.get();
        let values = super::primitives::iterate(&it.value)?;
        it.index.set(i + 1);
        Ok(values.get(i).cloned())
    }
    fn close_iterator(&self, _: &CommandIterator, e: Thrown) -> Thrown {
        e
    }
    fn entries(&self, v: &Js) -> Result<CommandIterator, Thrown> {
        self.iterator(v)
    }
    fn next_entry(&self, it: &CommandIterator) -> Result<Option<(Js, Js)>, Thrown> {
        let i = it.index.get();
        Ok(self.next(it)?.map(|v| (Js::Number(i as f64), v)))
    }
    fn get(&self, v: &Js, k: &str) -> Result<Js, Thrown> {
        v.get(k)
    }
    fn optional_get(&self, v: &Js, k: &str) -> Result<Js, Thrown> {
        if v.is_nullish() {
            Ok(Js::Undefined)
        } else {
            v.get(k)
        }
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn is_undefined(&self, v: &Js) -> bool {
        v.is_undefined()
    }
    fn is_null(&self, v: &Js) -> bool {
        matches!(v, Js::Null)
    }
    fn is_string(&self, v: &Js) -> bool {
        v.is_string()
    }
    fn is_object(&self, v: &Js) -> bool {
        v.type_of() == "object"
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn literal(&self, s: &str) -> Js {
        Js::str(s)
    }
    fn array(&self, a: Vec<Js>) -> Js {
        Js::array(a)
    }
    fn object(&self, a: Vec<(&'static str, Js)>) -> Js {
        Js::object(a)
    }
    fn template(&self, v: &Js) -> Result<Vec<u16>, Thrown> {
        v.to_js_string().map(|v| v.to_vec())
    }
    fn string(&self, v: &Js) -> Result<Vec<u16>, Thrown> {
        self.rt()?
            .global("String")?
            .call(Js::Undefined, vec![v.clone()])?
            .to_js_string()
            .map(|v| v.to_vec())
    }
    fn text_value(&self, v: Vec<u16>) -> Js {
        Js::utf16(&v)
    }
    fn trim_length(&self, v: &Js) -> Result<f64, Thrown> {
        v.call_method("trim", vec![])?.get("length")?.to_number()
    }
    fn length(&self, v: &Js) -> Result<f64, Thrown> {
        v.get("length")?.to_number()
    }
    fn at(&self, v: &Js, i: f64) -> Result<Js, Thrown> {
        v.get(&safe_cex_num::js_number_string(i))
    }
    fn spread(&self, v: &Js) -> Result<Vec<Js>, Thrown> {
        super::primitives::iterate(v)
    }
    fn slice(&self, v: &Js, start: f64, end: f64) -> Result<Js, Thrown> {
        v.call_method("slice", vec![Js::Number(start), Js::Number(end)])
    }
    fn map_ids(&self, v: &Js) -> Result<Js, Thrown> {
        Ok(Js::array(
            super::primitives::iterate(v)?
                .iter()
                .map(|v| v.get("id"))
                .collect::<Result<_, _>>()?,
        ))
    }
    fn state(&self, k: &str) -> Result<Js, Thrown> {
        self.rt()?.path(k)
    }
    fn await_value(&self, v: Js) -> Result<JsFuture<'static>, Thrown> {
        Ok(self.future(v))
    }
    fn command(
        &self,
        c: crate::order_commands::OrderCommand,
        a: Vec<Js>,
    ) -> Result<JsFuture<'static>, Thrown> {
        use crate::order_commands::OrderCommand as C;
        self.operation(
            match c {
                C::Main => "placeMainOrder",
                C::Brackets => "placeTakeProfitStopLoss",
                C::NativeBatch => "placeNativeBatchChunk",
                C::Single => "placeOrder",
                C::Build => "buildMainOrderPayload",
                C::CancelBatch => "cancelNativeBatchChunk",
                C::CancelSingle => "cancelOrder",
            },
            a,
        )
    }
    fn resolve_client_id(&self, o: &Js) -> Result<Js, Thrown> {
        self.this
            .call_method("resolveCallerClientOrderId", vec![o.clone()])
    }
    fn generate_client_id(&self) -> Result<Js, Thrown> {
        self.this.call_method("generateClientId", vec![])
    }
    fn assert_client_ids(&self, o: &Js) -> Result<(), Thrown> {
        self.this
            .call_method("assertCallerClientOrderIds", vec![o.clone()])?;
        Ok(())
    }
    fn native_batch_order(&self, o: &Js) -> Result<bool, Thrown> {
        Ok(self
            .this
            .call_method("isNativeBatchOrder", vec![o.clone()])?
            .truthy())
    }
    fn post(&self, e: &'static str, p: Js, h: Js) -> Result<JsFuture<'static>, Thrown> {
        let mut a = vec![Js::str(e), p];
        if h.truthy() {
            a.push(Js::object(vec![("headers", h)]));
        }
        self.operation("post", a)
    }
    fn delete(&self, e: Vec<u16>, c: Option<Js>) -> Result<JsFuture<'static>, Thrown> {
        let mut a = vec![Js::utf16(&e)];
        a.extend(c);
        let v = self.this.get("xhr")?.call_method("delete", a)?;
        Ok(self.future(v))
    }
    fn select_log(&self) -> Result<Js, Thrown> {
        self.logger()
    }
    fn log(&self, l: Js, t: Vec<u16>, s: &'static str) -> Result<(), Thrown> {
        Host::log(self, l, t, s)
    }
    fn new_error(&self, t: Vec<u16>) -> Result<Thrown, Thrown> {
        Ok(self.error(t))
    }
    fn error_value(&self, e: &Thrown) -> Js {
        e.value.clone()
    }
    fn error_is_instance(&self, e: &Thrown) -> Result<bool, Thrown> {
        Ok(matches!(&e.value, Js::Object(object) if object.class == "Error"))
    }
    fn emit(&self, v: Js) -> Result<(), Thrown> {
        Host::emit(self, v)
    }
    fn stringify(&self, v: &Js) -> Result<Vec<u16>, Thrown> {
        self.rt()?
            .global("JSON.stringify")?
            .call(Js::Undefined, vec![v.clone()])?
            .to_js_string()
            .map(|v| v.to_vec())
    }
    fn replace_endpoint(&self, e: &'static str, s: &str, v: &Js) -> Result<Vec<u16>, Thrown> {
        Js::str(e)
            .call_method("replace", vec![Js::str(s), v.clone()])?
            .to_js_string()
            .map(|v| v.to_vec())
    }
}
impl crate::typed_data_signing::TypedDataSigningHost for Host {
    type Value = Js;
    type Error = Thrown;
    fn get(&self, v: &Js, k: &str) -> Result<Js, Thrown> {
        v.get(k)
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn literal(&self, s: &str) -> Js {
        Js::str(s)
    }
    fn object(&self, f: Vec<(&str, Js)>) -> Result<Js, Thrown> {
        Ok(Js::object(f))
    }
    fn array(&self, a: Vec<Js>) -> Result<Js, Thrown> {
        Ok(Js::array(a))
    }
    fn import_starknet(&self) -> Result<Js, Thrown> {
        super::sdk::starknet_module(&self.rt()?)
    }
    fn await_value(&self, v: Js) -> JsFuture<'static> {
        self.future(v)
    }
    fn call(&self, f: Js, r: &Js, a: Vec<Js>) -> Result<Js, Thrown> {
        f.call(r.clone(), a)
    }
    fn is_array(&self, v: &Js) -> Result<bool, Thrown> {
        Ok(v.is_array())
    }
    fn is_bigint(&self, v: &Js) -> bool {
        matches!(v, Js::BigInt(_))
    }
    fn select_string(&self) -> Result<Js, Thrown> {
        self.rt()?.global("String")
    }
    fn string(&self, f: Js, v: Js) -> Result<Js, Thrown> {
        f.call(Js::Undefined, vec![v])
    }
    fn stringify(&self, v: Js) -> Result<Js, Thrown> {
        self.rt()?
            .global("JSON.stringify")?
            .call(Js::Undefined, vec![v])
    }
    fn map(
        &self,
        method: Js,
        r: &Js,
        c: &mut dyn FnMut(Js) -> Result<Js, Thrown>,
    ) -> Result<Js, Thrown> {
        if !method.is_callable() {
            return Err(Thrown::type_error("sig.map is not a function"));
        }
        let values = r
            .array_items()
            .ok_or_else(|| Thrown::type_error("sig.map receiver must be an array"))?;
        Ok(Js::Array(Rc::new(std::cell::RefCell::new(
            values
                .into_iter()
                .map(|v| v.map(&mut *c).transpose())
                .collect::<Result<Vec<_>, _>>()?,
        ))))
    }
}
