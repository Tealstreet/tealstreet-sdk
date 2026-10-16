//! Error-class, tuple and market-startup host traits for the imported startup
//! helpers (`fetchCoreMetaAndAssetCtxsWithFallback` and its HIP-3 sibling).
use super::{
    Realm,
    host_core::js,
    value::{Kind, PropKey, Value},
};
use crate::{
    api_errors::{ApiErrorClass, ApiErrorHost},
    income::IncomeFuture,
    market_helpers::MetaTupleHost,
    market_startup::StartupHost,
    spot_json::JsString,
};

impl ApiErrorHost for Realm {
    /// Error(message); the derived class name is assigned by the caller.
    fn new_error(&mut self, _class: ApiErrorClass, message: &Value) -> Result<Value, Value> {
        let e = self.alloc(Kind::Error, Some(&self.intrinsics().error_proto.clone()));
        if !matches!(message, Value::Undefined) {
            let m = Realm::to_string(self, message)?;
            self.define_hidden(&e, PropKey::str("message"), Value::String(m));
        }
        Ok(e)
    }
    fn define_field(&mut self, target: &Value, key: &str, value: &Value) -> Result<(), Value> {
        self.define_str(target, key, value.clone())
    }
    fn set_property(&mut self, target: &Value, key: &str, value: &Value) -> Result<(), Value> {
        self.set_str(target, key, value.clone())
    }
    fn literal(&self, text: &str) -> Value {
        Value::str(text)
    }
    fn truthy(&self, value: &Value) -> bool {
        value.truthy()
    }
    fn exception_from_value(&self, value: Value) -> Value {
        value
    }
}

impl MetaTupleHost for Realm {
    fn first_two(&mut self, data: &Value) -> Result<[Value; 2], Value> {
        Ok([self.get_str(data, "0")?, self.get_str(data, "1")?])
    }
    fn type_of(&self, value: &Value) -> &'static str {
        value.type_of()
    }
    fn implicit_string(&mut self, value: &Value) -> Result<JsString, Value> {
        Ok(js(Realm::to_string(self, value)?))
    }
    fn log_warning(&mut self, message: &JsString) -> Result<(), Value> {
        self.runtime()
            .console("warning", &String::from_utf16_lossy(&message.0));
        Ok(())
    }
}

impl StartupHost for Realm {
    fn object(&mut self) -> Result<Value, Value> {
        Ok(self.new_object())
    }
    fn null(&self) -> Value {
        Value::Null
    }
    fn tuple(&mut self, values: &[Value; 2]) -> Result<Value, Value> {
        Ok(self.new_array(values.to_vec()))
    }
    fn string_value(&self, text: &JsString) -> Value {
        Value::js(text)
    }
    fn call(&mut self, function: &Value, receiver: &Value, args: &[Value]) -> Result<Value, Value> {
        Realm::call(self, function, receiver, args)
    }
    fn await_value(&mut self, value: Value) -> IncomeFuture<'_, Value, Value> {
        Realm::await_value(self, value)
    }
    fn error_value(&self, error: &Value) -> Value {
        error.clone()
    }
    fn helper_promise(&mut self, result: Result<Value, Value>) -> Value {
        self.settled_promise(result)
    }
    fn info_endpoint(&mut self) -> Result<Value, Value> {
        let endpoints = self.import("ENDPOINTS")?;
        self.get_str(&endpoints, "INFO")
    }
    fn meta_info_type(&mut self) -> Result<Value, Value> {
        let types = self.import("INFO_TYPES")?;
        self.get_str(&types, "PERPS_META_AND_ASSET_CTXS")
    }
    fn request_config(&self) -> Value {
        let headers = self.object_from(vec![("Content-Type", Value::str("application/json"))]);
        self.object_from(vec![("headers", headers)])
    }
    fn warning_severity(&mut self) -> Result<Value, Value> {
        let severity = self.import("LogSeverity")?;
        self.get_str(&severity, "Warning")
    }
}

impl crate::browser_approval_flows::BrowserApprovalHost for Realm {
    /// `typeof name` on a global: an unbound name is "undefined", not a throw.
    fn global_typeof(&mut self, name: &str) -> Result<Value, Value> {
        let globals = Value::Object(self.state.globals.clone());
        let key = PropKey::str(name);
        if self.state.imports.borrow().contains_key(name) || self.has_property(&globals, &key)? {
            let v = self.global_binding(name)?;
            return Ok(Value::str(v.type_of()));
        }
        Ok(Value::str("undefined"))
    }
    fn construct_value(&mut self, c: &Value, args: &[Value]) -> Result<Value, Value> {
        Realm::construct(self, c, args, None)
    }
    fn error_value(&self, error: &Value) -> Value {
        error.clone()
    }
}
