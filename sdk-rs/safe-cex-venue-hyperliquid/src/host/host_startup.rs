//! Error-class, tuple and market-startup host traits for the imported startup
//! helpers (`fetchCoreMetaAndAssetCtxsWithFallback` and its HIP-3 sibling).
use safe_cex_exchange::js_runtime::{value::{Js, Thrown}};
use super::{
    Host,
    host_core::js,
};
use crate::{
    api_errors::{ApiErrorClass, ApiErrorHost},
    income::IncomeFuture,
    market_helpers::MetaTupleHost,
    market_startup::StartupHost,
    spot_json::JsString,
};

impl ApiErrorHost for Host {
    /// Error(message); the derived class name is assigned by the caller.
    fn new_error(&mut self, _class: ApiErrorClass, message: &Js) -> Result<Js, Thrown> {
        let e = safe_cex_exchange::js_runtime::value::error_object("Error", "");
        if !matches!(message, Js::Undefined) {
            let m = Host::to_string(self, message)?;
            self.define_hidden(&e, Js::str("message"), Js::String(m));
        }
        Ok(e)
    }
    fn define_field(&mut self, target: &Js, key: &str, value: &Js) -> Result<(), Thrown> {
        self.define_str(target, key, value.clone())
    }
    fn set_property(&mut self, target: &Js, key: &str, value: &Js) -> Result<(), Thrown> {
        self.set_str(target, key, value.clone())
    }
    fn literal(&self, text: &str) -> Js {
        Js::str(text)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn exception_from_value(&self, value: Js) -> Thrown {
        Thrown::new(value)
    }
}

impl MetaTupleHost for Host {
    fn first_two(&mut self, data: &Js) -> Result<[Js; 2], Thrown> {
        Ok([self.get_str(data, "0")?, self.get_str(data, "1")?])
    }
    fn type_of(&self, value: &Js) -> &'static str {
        value.type_of()
    }
    fn implicit_string(&mut self, value: &Js) -> Result<JsString, Thrown> {
        Ok(js(Host::to_string(self, value)?))
    }
    fn log_warning(&mut self, message: &JsString) -> Result<(), Thrown> {
        self.runtime()
            .console("warning", &String::from_utf16_lossy(&message.0));
        Ok(())
    }
}

impl StartupHost for Host {
    fn object(&mut self) -> Result<Js, Thrown> {
        Ok(self.new_object())
    }
    fn null(&self) -> Js {
        Js::Null
    }
    fn tuple(&mut self, values: &[Js; 2]) -> Result<Js, Thrown> {
        Ok(self.new_array(values.to_vec()))
    }
    fn string_value(&self, text: &JsString) -> Js {
        Js::utf16(&text.0)
    }
    fn call(&mut self, function: &Js, receiver: &Js, args: &[Js]) -> Result<Js, Thrown> {
        Host::call(self, function, receiver, args)
    }
    fn await_value(&mut self, value: Js) -> IncomeFuture<'_, Js, Thrown> {
        Host::await_value(self, value)
    }
    fn error_value(&self, error: &Thrown) -> Js {
        error.value.clone()
    }
    fn helper_promise(&mut self, result: Result<Js, Thrown>) -> Js {
        self.settled_promise(result)
    }
    fn info_endpoint(&mut self) -> Result<Js, Thrown> {
        let endpoints = self.import("ENDPOINTS")?;
        self.get_str(&endpoints, "INFO")
    }
    fn meta_info_type(&mut self) -> Result<Js, Thrown> {
        let types = self.import("INFO_TYPES")?;
        self.get_str(&types, "PERPS_META_AND_ASSET_CTXS")
    }
    fn request_config(&self) -> Js {
        let headers = self.object_from(vec![("Content-Type", Js::str("application/json"))]);
        self.object_from(vec![("headers", headers)])
    }
    fn warning_severity(&mut self) -> Result<Js, Thrown> {
        let severity = self.import("LogSeverity")?;
        self.get_str(&severity, "Warning")
    }
}

impl crate::browser_approval_flows::BrowserApprovalHost for Host {
    /// `typeof name` on a global: an unbound name is "undefined", not a throw.
    fn global_typeof(&mut self, name: &str) -> Result<Js, Thrown> {
        let globals = self.shared.globals.clone();
        let key = Js::str(name);
        if self.has_binding(name) || self.has_property(&globals, &key)? {
            let v = self.global_binding(name)?;
            return Ok(Js::str(v.type_of()));
        }
        Ok(Js::str("undefined"))
    }
    fn construct_value(&mut self, c: &Js, args: &[Js]) -> Result<Js, Thrown> {
        Host::construct(self, c, args, None)
    }
    fn error_value(&self, error: &Thrown) -> Js {
        error.value.clone()
    }
}
