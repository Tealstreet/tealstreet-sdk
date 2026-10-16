//! Bound JavaScript primitives for GRVT session callbacks; values retain identity.
use crate::builder_workflow::{JsPromise, JsResult};
use std::rc::Rc;
pub type Callback<V> = Rc<dyn Fn(V) -> JsResult<V>>;

/// call preserves the receiver; set binds ordinary assignment, spread binds CopyDataProperties.
/// start owns eager tasks; promise adopts returned thenables after the task's return.
/// Primitive hooks preserve JavaScript coercion and abrupt completion.
pub trait SessionHost: 'static {
    type Value: Clone + 'static;
    fn get(&self, value: &Self::Value, key: &str) -> JsResult<Self::Value>;
    fn set(&self, value: &Self::Value, key: &str, field: Self::Value) -> JsResult<Self::Value, ()>;
    fn call(
        &self,
        function: Self::Value,
        receiver: Self::Value,
        args: Vec<Self::Value>,
    ) -> JsResult<Self::Value>;
    fn function(&self, callback: Callback<Self::Value>) -> Self::Value;
    fn object(&self, fields: Vec<(&str, Self::Value)>) -> Self::Value;
    fn define(
        &self,
        value: &Self::Value,
        key: &str,
        field: Self::Value,
    ) -> JsResult<Self::Value, ()>;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn spread(&self, target: &Self::Value, source: Self::Value) -> JsResult<Self::Value, ()>;
    fn string(&self, value: &str) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn undefined(&self) -> Self::Value;
    fn null(&self) -> Self::Value;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn is_string(&self, value: &Self::Value) -> bool;
    fn is_function(&self, value: &Self::Value) -> bool;
    fn equals_number(&self, value: &Self::Value, number: f64) -> JsResult<Self::Value, bool>;
    fn template(&self, values: Vec<Self::Value>) -> JsResult<Self::Value>;
    fn error(&self, message: Self::Value) -> JsResult<Self::Value>;
    fn reject(&self, error: Self::Value) -> Self::Value;
    fn start(&self, task: JsPromise<Self::Value>) -> JsPromise<Self::Value>;
    fn promise(&self, task: JsPromise<Self::Value>) -> Self::Value;
    fn await_value(&self, value: Self::Value) -> JsPromise<Self::Value>;
}
pub(crate) fn optional<H: SessionHost>(h: &H, value: H::Value, key: &str) -> JsResult<H::Value> {
    if h.nullish(&value) {
        Ok(h.undefined())
    } else {
        h.get(&value, key)
    }
}
pub(crate) fn method<H: SessionHost>(
    h: &H,
    value: H::Value,
    key: &str,
    args: Vec<H::Value>,
) -> JsResult<H::Value> {
    let function = h.get(&value, key)?;
    h.call(function, value, args)
}
macro_rules! session_template {
    ($host:expr; $($part:expr),* $(,)?) => {{
        (|| -> crate::builder_workflow::JsResult<_> {
            let mut parts = Vec::new();
            $(let value = $part; parts.push($host.template(vec![value])?);)*
            $host.template(parts)
        })()
    }};
}
pub(crate) use session_template;
