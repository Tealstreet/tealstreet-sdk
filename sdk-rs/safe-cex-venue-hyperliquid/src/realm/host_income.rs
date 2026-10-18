//! Primitive host bindings for the existing income mappers and history pager.
use super::{Realm, Value};
use crate::{
    income::{IncomeFuture, IncomeHost},
    spot_json::JsString,
};

impl IncomeHost for Realm {
    fn number(&mut self, value: &Value) -> Result<f64, Value> {
        self.to_number(value)
    }
    fn number_value(&self, value: f64) -> Value {
        Value::Number(value)
    }
    fn equal_number(&self, value: &Value, number: f64) -> bool {
        matches!(value, Value::Number(value) if *value == number)
    }
    fn parse_float(&mut self, text: &JsString) -> Result<f64, Value> {
        let parser = self.global_binding("parseFloat")?;
        let result = self.call(&parser, &Value::Undefined, &[Value::js(text)])?;
        self.to_number(&result)
    }
    fn array(&mut self, values: &[Value]) -> Result<Value, Value> {
        Ok(self.new_array(values.to_vec()))
    }
    fn new_set(&mut self) -> Result<Value, Value> {
        Ok(Realm::new_set(self))
    }
    fn math_max(&mut self, left: f64, right: f64) -> Result<f64, Value> {
        let math = self.global_binding("Math")?;
        let result =
            self.call_method(&math, "max", &[Value::Number(left), Value::Number(right)])?;
        self.to_number(&result)
    }
    fn await_value(&mut self, value: Value) -> IncomeFuture<'_, Value, Value> {
        Realm::await_value(self, value)
    }
}
