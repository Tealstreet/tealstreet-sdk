//! Primitive host bindings for the existing income mappers and history pager.
use super::Host;
use crate::{
    income::{IncomeFuture, IncomeHost},
    spot_json::JsString,
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};

impl IncomeHost for Host {
    fn number(&mut self, value: &Js) -> Result<f64, Thrown> {
        self.to_number(value)
    }
    fn number_value(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn equal_number(&self, value: &Js, number: f64) -> bool {
        matches!(value, Js::Number(value) if *value == number)
    }
    fn parse_float(&mut self, text: &JsString) -> Result<f64, Thrown> {
        let parser = self.global_binding("parseFloat")?;
        let result = self.call(&parser, &Js::Undefined, &[Js::utf16(&text.0)])?;
        self.to_number(&result)
    }
    fn array(&mut self, values: &[Js]) -> Result<Js, Thrown> {
        Ok(self.new_array(values.to_vec()))
    }
    fn new_set(&mut self) -> Result<Js, Thrown> {
        Ok(Host::new_set(self))
    }
    fn math_max(&mut self, left: f64, right: f64) -> Result<f64, Thrown> {
        let math = self.global_binding("Math")?;
        let result = self.call_method(&math, "max", &[Js::Number(left), Js::Number(right)])?;
        self.to_number(&result)
    }
    fn await_value(&mut self, value: Js) -> IncomeFuture<'_, Js, Thrown> {
        Host::await_value(self, value)
    }
}
