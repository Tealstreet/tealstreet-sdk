//! Base JS host traits over the production realm.
use super::{
    Realm,
    promise::ValueFuture,
    value::{PropKey, Value},
};
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
use std::{future::Future, pin::Pin, rc::Rc};

pub(super) fn js(units: Rc<[u16]>) -> JsString {
    JsString(units.to_vec())
}

impl WsPingHost for Realm {
    type Value = Value;
    type Error = Value;
    fn undefined(&self) -> Value {
        Value::Undefined
    }
    fn literal(&self, text: &JsString) -> Value {
        Value::js(text)
    }
    fn truthy(&self, value: &Value) -> bool {
        value.truthy()
    }
    fn global(&mut self, name: &str) -> Result<Value, Value> {
        self.global_binding(name)
    }
    fn get(&mut self, object: &Value, name: &str) -> Result<Value, Value> {
        self.get_str(object, name)
    }
    fn set(&mut self, object: &Value, name: &str, value: &Value) -> Result<(), Value> {
        self.set_str(object, name, value.clone())
    }
    fn call(&mut self, function: &Value, receiver: &Value, args: &[Value]) -> Result<Value, Value> {
        Realm::call(self, function, receiver, args)
    }
    fn object(&mut self) -> Result<Value, Value> {
        Ok(self.new_object())
    }
    fn define_data(&mut self, object: &Value, name: &str, value: &Value) -> Result<(), Value> {
        self.define_str(object, name, value.clone())
    }
    fn callback(&mut self, capture: PingCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 0, move |r, _, _| {
            ws_ping_lifecycle::run_ping_timeout(r, &capture)?;
            Ok(Value::Undefined)
        }))
    }
}

impl HelperTaskHost<Value, Value> for Realm {
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>) {
        self.start(task)
    }
    fn settle_return(&self, result: Result<Value, Value>) -> ValueFuture {
        match result {
            Ok(v) => self.await_value(v),
            Err(e) => Box::pin(std::future::ready(Err(e))),
        }
    }
}

impl JsPayloadHost for Realm {
    type Value = Value;
    type Error = Value;
    fn string_units<'a>(&self, value: &'a Value) -> Option<&'a [u16]> {
        value.as_units()
    }
    fn is_object(&self, value: &Value) -> bool {
        matches!(value, Value::Object(_)) && value.type_of() == "object"
    }
    fn is_array(&mut self, value: &Value) -> Result<bool, Value> {
        Ok(value.is_array())
    }
    fn is_nullish(&self, value: &Value) -> bool {
        value.is_nullish()
    }
    fn undefined(&self) -> Value {
        Value::Undefined
    }
    fn parse_json(&mut self, text: &[u16]) -> Result<Value, Value> {
        Realm::parse_json(self, text)
    }
    fn get(&mut self, value: &Value, property: &str) -> Result<Value, Value> {
        self.get_str(value, property)
    }
    fn json_stringify(&mut self, value: &Value) -> Result<Option<JsString>, Value> {
        Ok(self.stringify(value)?.map(JsString))
    }
    fn to_string(&mut self, value: &Value) -> Result<JsString, Value> {
        Ok(js(Realm::to_string(self, value)?))
    }
}

impl SpotAccountHost for Realm {
    fn null(&self) -> Value {
        Value::Null
    }
    fn boolean(&self, v: bool) -> Value {
        Value::Bool(v)
    }
    fn number(&self, v: f64) -> Value {
        Value::Number(v)
    }
    fn is_nullish(&self, v: &Value) -> bool {
        v.is_nullish()
    }
    fn is_array(&mut self, v: &Value) -> Result<bool, Value> {
        Ok(v.is_array())
    }
    fn candle_binding(&mut self, v: &Value) -> Result<[Value; 6], Value> {
        let values = self.object_pattern(v, &["t", "o", "h", "l", "c", "v"])?;
        Ok(std::array::from_fn(|index| values[index].clone()))
    }
    fn is_undefined(&self, v: &Value) -> bool {
        matches!(v, Value::Undefined)
    }
    fn strict_equal(&self, a: &Value, b: &Value) -> bool {
        a.strict_equal(b)
    }
    fn imported(&mut self, name: &str) -> Result<Value, Value> {
        self.import(name)
    }
    fn array(&mut self, values: &[Value]) -> Result<Value, Value> {
        Ok(self.new_array(values.to_vec()))
    }
    fn spread_array(&mut self, value: &Value) -> Result<Value, Value> {
        self.spread_to_array(value)
    }
    fn spread_into(&mut self, target: &Value, source: &Value) -> Result<(), Value> {
        self.copy_data_properties(target, source)
    }
    fn template_string(&mut self, value: &Value) -> Result<JsString, Value> {
        Ok(js(self.template(value)?))
    }
    fn binary(&mut self, op: &str, a: &Value, b: &Value) -> Result<Value, Value> {
        Realm::binary(self, op, a, b)
    }
    fn interval_binding(&mut self, value: &Value) -> Result<[Value; 3], Value> {
        let s = self.array_pattern(value, &[false, true, true])?;
        Ok([Value::Undefined, s[0].clone(), s[1].clone()])
    }
    fn pair_binding(&mut self, value: &Value) -> Result<[Value; 2], Value> {
        let s = self.array_slots(value, 2)?;
        Ok([s[0].clone(), s[1].clone()])
    }
    fn index(&mut self, value: &Value, key: &Value) -> Result<Value, Value> {
        Realm::index(self, value, key)
    }
    fn digit_regex(&mut self) -> Result<Value, Value> {
        self.regexp(r"/(\d+)/")
    }
    fn response_data(&mut self, value: &Value) -> Result<Value, Value> {
        self.data_binding(value)
    }
    fn construct(&mut self, constructor: &Value, args: &[Value]) -> Result<Value, Value> {
        Realm::construct(self, constructor, args, None)
    }
    fn throw_value(&self, value: Value) -> Value {
        value
    }
    fn error_value(&self, error: &Value) -> Value {
        error.clone()
    }
    fn callback(&mut self, capture: AccountCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 0, move |r, _, args| {
            spot_account_reads::run_account_callback(r, &capture, args)
        }))
    }
    fn await_value(&mut self, value: Value) -> AccountFuture<Value, Value> {
        Realm::await_value(self, value)
    }
    fn spawn_native(&self, body: AccountFuture<Value, Value>) -> Value {
        self.spawn_value(body)
    }
}

impl StartupTimeoutHost for Realm {
    type Value = Value;
    type Error = Value;
    fn undefined(&self) -> Value {
        Value::Undefined
    }
    fn null(&self) -> Value {
        Value::Null
    }
    fn literal(&self, v: &str) -> Value {
        Value::str(v)
    }
    fn number(&self, v: f64) -> Value {
        Value::Number(v)
    }
    fn truthy(&self, v: &Value) -> bool {
        v.truthy()
    }
    fn global(&self, k: &str) -> Result<Value, Value> {
        self.clone().global_binding(k)
    }
    fn get(&self, owner: &Value, k: &str) -> Result<Value, Value> {
        self.clone().get_str(owner, k)
    }
    fn call(&self, callee: Value, receiver: Value, args: Vec<Value>) -> Result<Value, Value> {
        Realm::call(&mut self.clone(), &callee, &receiver, &args)
    }
    fn construct(&self, callee: Value, args: Vec<Value>) -> Result<Value, Value> {
        Realm::construct(&mut self.clone(), &callee, &args, None)
    }
    fn array(&self, values: Vec<Value>) -> Value {
        self.new_array(values)
    }
    fn callback(&self, body: TimeoutCallback<Value, Value>) -> Value {
        self.native("", 0, move |_, _, args| body(args.to_vec()))
    }
    fn divide(&self, a: Value, b: Value) -> Result<Value, Value> {
        Realm::binary(&mut self.clone(), "/", &a, &b)
    }
    fn template(&self, parts: Vec<Value>) -> Result<Value, Value> {
        let mut realm = self.clone();
        let mut out = vec![];
        for p in &parts {
            out.extend_from_slice(&Realm::template(&mut realm, p)?);
        }
        Ok(Value::String(out.into()))
    }
    fn await_value(&self, value: Value) -> ValueFuture {
        Realm::await_value(self, value)
    }
}

impl CrossCacheHost for Realm {
    fn null(&self) -> Value {
        Value::Null
    }
    fn is_undefined(&self, value: &Value) -> bool {
        matches!(value, Value::Undefined)
    }
    fn truthy(&self, value: &Value) -> bool {
        value.truthy()
    }
    /// `typeof sessionStorage` lookup: absent storage reads as undefined.
    fn session_storage(&mut self) -> Result<Value, Value> {
        let globals = Value::Object(self.state.globals.clone());
        let key = PropKey::str("sessionStorage");
        if self.has_property(&globals, &key)? {
            self.get(&globals, &key)
        } else {
            Ok(Value::Undefined)
        }
    }
    fn literal(&self, text: &JsString) -> Value {
        Value::js(text)
    }
    fn object(&mut self) -> Result<Value, Value> {
        Ok(self.new_object())
    }
    fn set(&mut self, object: &Value, key: &Value, value: &Value) -> Result<(), Value> {
        self.set_index(object, key, value.clone())
    }
    fn define_data(&mut self, object: &Value, key: &JsString, value: &Value) -> Result<(), Value> {
        self.define(
            object,
            PropKey::String(key.0.as_slice().into()),
            value.clone(),
        )
    }
    fn call(&mut self, function: &Value, receiver: &Value, args: &[Value]) -> Result<Value, Value> {
        Realm::call(self, function, receiver, args)
    }
    fn parse_json_value(&mut self, value: &Value) -> Result<Value, Value> {
        let text = Realm::to_string(self, value)?;
        Realm::parse_json(self, &text)
    }
    fn property_key(&mut self, value: &Value) -> Result<Value, Value> {
        Ok(self.to_property_key(value)?.to_value())
    }
    fn entries(&mut self, map: &Value) -> Result<Value, Value> {
        self.get_iterator(map)
    }
    fn iterator_step(&mut self, iterator: &Value) -> Result<Option<Value>, Value> {
        Realm::iterator_step(self, iterator)
    }
    fn entry_pair(&mut self, entry: &Value) -> Result<[Value; 2], Value> {
        let s = self.array_slots(entry, 2)?;
        Ok([s[0].clone(), s[1].clone()])
    }
    fn close_after_throw(&mut self, iterator: &Value) {
        self.iterator_close_after_throw(iterator)
    }
    fn spread(&mut self, value: &Value) -> Result<Value, Value> {
        self.spread_to_array(value)
    }
    fn greater_than_500(&mut self, value: &Value) -> Result<bool, Value> {
        Ok(Realm::binary(self, ">", value, &Value::Number(500.0))?.truthy())
    }
}

impl CacheOrdersHost for Realm {
    fn now(&mut self) -> Result<Value, Value> {
        Ok(Value::Number(self.runtime().now_ms()))
    }
    fn object_keys(&mut self, value: &Value) -> Result<Value, Value> {
        let object = self.global_binding("Object")?;
        self.call_method(&object, "keys", &[value.clone()])
    }
    fn get_index(&mut self, value: &Value, key: &Value) -> Result<Value, Value> {
        Realm::index(self, value, key)
    }
    fn delete(&mut self, value: &Value, key: &Value) -> Result<(), Value> {
        let key = self.to_property_key(key)?;
        Realm::delete(self, value, &key).map(|_| ())
    }
    fn older_than(
        &mut self,
        now: &Value,
        timestamp: &Value,
        milliseconds: f64,
    ) -> Result<bool, Value> {
        let elapsed = Realm::binary(self, "-", now, timestamp)?;
        Ok(Realm::binary(self, ">", &elapsed, &Value::Number(milliseconds))?.truthy())
    }
    fn singleton(&mut self) -> Result<Value, Value> {
        let get = self.import("getCrossCache")?;
        Realm::call(self, &get, &Value::Undefined, &[])
    }
    fn imported(&mut self, name: &str) -> Result<Value, Value> {
        self.import(name)
    }
}
