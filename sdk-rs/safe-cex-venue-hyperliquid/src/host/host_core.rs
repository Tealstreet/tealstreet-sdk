//! Base JS host traits over the production realm.
use super::Host;
use crate::{
    cache_orders::CacheOrdersHost,
    cross_cache::CrossCacheHost,
    helper_tasks::HelperTaskHost,
    json_boundaries::JsPayloadHost,
    spot_account_reads::{self, AccountCallback, AccountFuture, SpotAccountHost},
    spot_json::JsString,
    startup_timeout::{StartupTimeoutHost, TimeoutCallback},
    ws_ping_lifecycle::{self, PingCallback, WsPingHost},
};
use safe_cex_exchange::js_runtime::promise::JsFuture;
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::{future::Future, pin::Pin, rc::Rc};

pub(super) fn js(units: Rc<[u16]>) -> JsString {
    JsString(units.to_vec())
}

impl WsPingHost for Host {
    type Value = Js;
    type Error = Thrown;
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn literal(&self, text: &JsString) -> Js {
        Js::utf16(&text.0)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn global(&mut self, name: &str) -> Result<Js, Thrown> {
        self.global_binding(name)
    }
    fn get(&mut self, object: &Js, name: &str) -> Result<Js, Thrown> {
        self.get_str(object, name)
    }
    fn set(&mut self, object: &Js, name: &str, value: &Js) -> Result<(), Thrown> {
        self.set_str(object, name, value.clone())
    }
    fn call(&mut self, function: &Js, receiver: &Js, args: &[Js]) -> Result<Js, Thrown> {
        Host::call(self, function, receiver, args)
    }
    fn object(&mut self) -> Result<Js, Thrown> {
        Ok(self.new_object())
    }
    fn define_data(&mut self, object: &Js, name: &str, value: &Js) -> Result<(), Thrown> {
        self.define_str(object, name, value.clone())
    }
    fn callback(&mut self, capture: PingCallback<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 0, move |r, _, _| {
            ws_ping_lifecycle::run_ping_timeout(r, &capture)?;
            Ok(Js::Undefined)
        }))
    }
}

impl HelperTaskHost<Js, Thrown> for Host {
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>) {
        self.start(task)
    }
    fn settle_return(&self, result: Result<Js, Thrown>) -> JsFuture<'static> {
        match result {
            Ok(v) => self.await_value(v),
            Err(e) => Box::pin(std::future::ready(Err(e))),
        }
    }
}

impl JsPayloadHost for Host {
    type Value = Js;
    type Error = Thrown;
    fn string_units<'a>(&self, value: &'a Js) -> Option<&'a [u16]> {
        value.as_str().map(|units| units.as_ref())
    }
    fn is_object(&self, value: &Js) -> bool {
        value.is_object()
    }
    fn is_array(&mut self, value: &Js) -> Result<bool, Thrown> {
        Ok(value.is_array())
    }
    fn is_nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn parse_json(&mut self, text: &[u16]) -> Result<Js, Thrown> {
        Host::parse_json(self, text)
    }
    fn get(&mut self, value: &Js, property: &str) -> Result<Js, Thrown> {
        self.get_str(value, property)
    }
    fn json_stringify(&mut self, value: &Js) -> Result<Option<JsString>, Thrown> {
        Ok(self.stringify(value)?.map(JsString))
    }
    fn to_string(&mut self, value: &Js) -> Result<JsString, Thrown> {
        Ok(js(Host::to_string(self, value)?))
    }
}

impl SpotAccountHost for Host {
    fn null(&self) -> Js {
        Js::Null
    }
    fn boolean(&self, v: bool) -> Js {
        Js::Bool(v)
    }
    fn number(&self, v: f64) -> Js {
        Js::Number(v)
    }
    fn is_nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn is_array(&mut self, v: &Js) -> Result<bool, Thrown> {
        Ok(v.is_array())
    }
    fn candle_binding(&mut self, v: &Js) -> Result<[Js; 6], Thrown> {
        let values = self.object_pattern(v, &["t", "o", "h", "l", "c", "v"])?;
        Ok(std::array::from_fn(|index| values[index].clone()))
    }
    fn is_undefined(&self, v: &Js) -> bool {
        matches!(v, Js::Undefined)
    }
    fn strict_equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn imported(&mut self, name: &str) -> Result<Js, Thrown> {
        self.import(name)
    }
    fn array(&mut self, values: &[Js]) -> Result<Js, Thrown> {
        Ok(self.new_array(values.to_vec()))
    }
    fn spread_array(&mut self, value: &Js) -> Result<Js, Thrown> {
        self.spread_to_array(value)
    }
    fn spread_into(&mut self, target: &Js, source: &Js) -> Result<(), Thrown> {
        self.copy_data_properties(target, source)
    }
    fn template_string(&mut self, value: &Js) -> Result<JsString, Thrown> {
        Ok(js(self.template(value)?))
    }
    fn binary(&mut self, op: &str, a: &Js, b: &Js) -> Result<Js, Thrown> {
        Host::binary(self, op, a, b)
    }
    fn interval_binding(&mut self, value: &Js) -> Result<[Js; 3], Thrown> {
        let s = self.array_pattern(value, &[false, true, true])?;
        Ok([Js::Undefined, s[0].clone(), s[1].clone()])
    }
    fn pair_binding(&mut self, value: &Js) -> Result<[Js; 2], Thrown> {
        let s = self.array_slots(value, 2)?;
        Ok([s[0].clone(), s[1].clone()])
    }
    fn index(&mut self, value: &Js, key: &Js) -> Result<Js, Thrown> {
        Host::index(self, value, key)
    }
    fn digit_regex(&mut self) -> Result<Js, Thrown> {
        self.regexp(r"/(\d+)/")
    }
    fn response_data(&mut self, value: &Js) -> Result<Js, Thrown> {
        self.data_binding(value)
    }
    fn construct(&mut self, constructor: &Js, args: &[Js]) -> Result<Js, Thrown> {
        Host::construct(self, constructor, args, None)
    }
    fn throw_value(&self, value: Js) -> Thrown {
        Thrown::new(value)
    }
    fn error_value(&self, error: &Thrown) -> Js {
        error.value.clone()
    }
    fn callback(&mut self, capture: AccountCallback<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 0, move |r, _, args| {
            spot_account_reads::run_account_callback(r, &capture, args)
        }))
    }
    fn await_value(&mut self, value: Js) -> AccountFuture<Js, Thrown> {
        Host::await_value(self, value)
    }
    fn spawn_native(&self, body: AccountFuture<Js, Thrown>) -> Js {
        self.spawn_value(body)
    }
}

impl StartupTimeoutHost for Host {
    type Value = Js;
    type Error = Thrown;
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn null(&self) -> Js {
        Js::Null
    }
    fn literal(&self, v: &str) -> Js {
        Js::str(v)
    }
    fn number(&self, v: f64) -> Js {
        Js::Number(v)
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn global(&self, k: &str) -> Result<Js, Thrown> {
        self.clone().global_binding(k)
    }
    fn get(&self, owner: &Js, k: &str) -> Result<Js, Thrown> {
        self.clone().get_str(owner, k)
    }
    fn call(&self, callee: Js, receiver: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        Host::call(&mut self.clone(), &callee, &receiver, &args)
    }
    fn construct(&self, callee: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        Host::construct(&mut self.clone(), &callee, &args, None)
    }
    fn array(&self, values: Vec<Js>) -> Js {
        self.new_array(values)
    }
    fn callback(&self, body: TimeoutCallback<Js, Thrown>) -> Js {
        self.native("", 0, move |_, _, args| body(args.to_vec()))
    }
    fn divide(&self, a: Js, b: Js) -> Result<Js, Thrown> {
        Host::binary(&mut self.clone(), "/", &a, &b)
    }
    fn template(&self, parts: Vec<Js>) -> Result<Js, Thrown> {
        let mut realm = self.clone();
        let mut out = vec![];
        for p in &parts {
            out.extend_from_slice(&Host::template(&mut realm, p)?);
        }
        Ok(Js::String(out.into()))
    }
    fn await_value(&self, value: Js) -> JsFuture<'static> {
        Host::await_value(self, value)
    }
}

impl CrossCacheHost for Host {
    fn null(&self) -> Js {
        Js::Null
    }
    fn is_undefined(&self, value: &Js) -> bool {
        matches!(value, Js::Undefined)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    /// `typeof sessionStorage` lookup: absent storage reads as undefined.
    fn session_storage(&mut self) -> Result<Js, Thrown> {
        let globals = self.shared.globals.clone();
        let key = Js::str("sessionStorage");
        if self.has_property(&globals, &key)? {
            self.get(&globals, &key)
        } else {
            Ok(Js::Undefined)
        }
    }
    fn literal(&self, text: &JsString) -> Js {
        Js::utf16(&text.0)
    }
    fn object(&mut self) -> Result<Js, Thrown> {
        Ok(self.new_object())
    }
    fn set(&mut self, object: &Js, key: &Js, value: &Js) -> Result<(), Thrown> {
        self.set_index(object, key, value.clone())
    }
    fn define_data(&mut self, object: &Js, key: &JsString, value: &Js) -> Result<(), Thrown> {
        self.define(object, Js::String(key.0.as_slice().into()), value.clone())
    }
    fn call(&mut self, function: &Js, receiver: &Js, args: &[Js]) -> Result<Js, Thrown> {
        Host::call(self, function, receiver, args)
    }
    fn parse_json_value(&mut self, value: &Js) -> Result<Js, Thrown> {
        let text = Host::to_string(self, value)?;
        Host::parse_json(self, &text)
    }
    fn property_key(&mut self, value: &Js) -> Result<Js, Thrown> {
        Ok(self.to_property_key(value)?.clone())
    }
    fn entries(&mut self, map: &Js) -> Result<Js, Thrown> {
        self.get_iterator(map)
    }
    fn iterator_step(&mut self, iterator: &Js) -> Result<Option<Js>, Thrown> {
        Host::iterator_step(self, iterator)
    }
    fn entry_pair(&mut self, entry: &Js) -> Result<[Js; 2], Thrown> {
        let s = self.array_slots(entry, 2)?;
        Ok([s[0].clone(), s[1].clone()])
    }
    fn close_after_throw(&mut self, iterator: &Js) {
        self.iterator_close_after_throw(iterator)
    }
    fn spread(&mut self, value: &Js) -> Result<Js, Thrown> {
        self.spread_to_array(value)
    }
    fn greater_than_500(&mut self, value: &Js) -> Result<bool, Thrown> {
        Ok(Host::binary(self, ">", value, &Js::Number(500.0))?.truthy())
    }
}

impl CacheOrdersHost for Host {
    fn now(&mut self) -> Result<Js, Thrown> {
        Ok(Js::Number(self.runtime().now_ms()))
    }
    fn object_keys(&mut self, value: &Js) -> Result<Js, Thrown> {
        let object = self.global_binding("Object")?;
        self.call_method(&object, "keys", &[value.clone()])
    }
    fn get_index(&mut self, value: &Js, key: &Js) -> Result<Js, Thrown> {
        Host::index(self, value, key)
    }
    fn delete(&mut self, value: &Js, key: &Js) -> Result<(), Thrown> {
        let key = self.to_property_key(key)?;
        Host::delete(self, value, &key).map(|_| ())
    }
    fn older_than(&mut self, now: &Js, timestamp: &Js, milliseconds: f64) -> Result<bool, Thrown> {
        let elapsed = Host::binary(self, "-", now, timestamp)?;
        Ok(Host::binary(self, ">", &elapsed, &Js::Number(milliseconds))?.truthy())
    }
    fn singleton(&mut self) -> Result<Js, Thrown> {
        let get = self.import("getCrossCache")?;
        Host::call(self, &get, &Js::Undefined, &[])
    }
    fn imported(&mut self, name: &str) -> Result<Js, Thrown> {
        self.import(name)
    }
}
