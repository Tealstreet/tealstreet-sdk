//! Bound native primitives for Aster exchange and Axios bodies.
use std::{future::Future, pin::Pin, rc::Rc};
pub type ResultValue<V, T = V> = Result<T, V>;
pub type Task<V> = Pin<Box<dyn Future<Output = ResultValue<V>>>>;
pub type Function<V> = Rc<dyn Fn(Vec<V>) -> ResultValue<V>>;
/// Imports bind existing mapped helpers and shared owners, never venue method bodies.
/// Calls retain receivers; objects retain JS descriptors, key order and thrown values.
/// start eagerly owns tasks; promise adopts thenables; await_value queues reactions.
pub trait RuntimeHost: 'static {
    type Value: Clone + 'static;
    fn get(&self, object: &Self::Value, key: &str) -> ResultValue<Self::Value>;
    fn destructure(
        &self,
        value: Self::Value,
        keys: &[&str],
        binding: &str,
    ) -> ResultValue<Self::Value, Vec<Self::Value>>;
    fn set(
        &self,
        object: &Self::Value,
        key: &str,
        value: Self::Value,
    ) -> ResultValue<Self::Value, ()>;
    fn define(
        &self,
        object: &Self::Value,
        key: &str,
        value: Self::Value,
    ) -> ResultValue<Self::Value, ()>;
    fn delete(&self, object: &Self::Value, key: &str) -> ResultValue<Self::Value, ()>;
    fn call(
        &self,
        function: Self::Value,
        receiver: Self::Value,
        args: Vec<Self::Value>,
    ) -> ResultValue<Self::Value>;
    fn function(&self, function: Function<Self::Value>) -> Self::Value;
    fn object(&self, fields: Vec<(&str, Self::Value)>) -> Self::Value;
    fn spread(&self, target: &Self::Value, source: Self::Value) -> ResultValue<Self::Value, ()>;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn text(&self, text: &str) -> Self::Value;
    fn number(&self, number: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn undefined(&self) -> Self::Value;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn strict_equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn less(&self, a: Self::Value, b: Self::Value) -> ResultValue<Self::Value, bool>;
    fn greater_equal(&self, a: Self::Value, b: Self::Value) -> ResultValue<Self::Value, bool>;
    fn subtract(&self, a: Self::Value, b: Self::Value) -> ResultValue<Self::Value>;
    fn template(&self, parts: Vec<Self::Value>) -> ResultValue<Self::Value>;
    fn error(&self, message: Self::Value) -> Self::Value;
    fn import(&self, name: &str, args: Vec<Self::Value>) -> ResultValue<Self::Value>;
    fn constant(&self, name: &str) -> Self::Value;
    fn start(&self, task: Task<Self::Value>) -> Task<Self::Value>;
    fn promise(&self, task: Task<Self::Value>) -> Self::Value;
    fn await_value(&self, value: Self::Value) -> Task<Self::Value>;
}
pub(crate) fn opt<H: RuntimeHost>(h: &H, value: H::Value, key: &str) -> ResultValue<H::Value> {
    if h.nullish(&value) {
        Ok(h.undefined())
    } else {
        h.get(&value, key)
    }
}
pub(crate) fn or<H: RuntimeHost>(
    h: &H,
    value: H::Value,
    other: impl FnOnce() -> ResultValue<H::Value>,
) -> ResultValue<H::Value> {
    if h.truthy(&value) { Ok(value) } else { other() }
}
pub(crate) fn copy<H: RuntimeHost>(h: &H, source: H::Value) -> ResultValue<H::Value> {
    let target = h.object(vec![]);
    h.spread(&target, source)?;
    Ok(target)
}
pub(crate) async fn awaited<H: RuntimeHost>(
    h: &H,
    value: ResultValue<H::Value>,
) -> ResultValue<H::Value> {
    h.await_value(value?).await
}

macro_rules! invoke {
    ($host:expr, $owner:expr, $key:expr, $args:expr $(,)?) => {{
        let host = $host;
        let owner = $owner;
        let function = host.get(&owner, $key)?;
        host.call(function, owner, $args)
    }};
}
pub(crate) use invoke;
macro_rules! emit_log {
    ($host:expr, $owner:expr, $message:expr, $severity:expr $(,)?) => {{
        let host = $host;
        let owner = $owner;
        let function = host.get(&owner, "log")?;
        host.call(function, owner, vec![$message, host.constant($severity)])?;
        Ok::<(), <H as crate::runtime_host::RuntimeHost>::Value>(())
    }};
}
pub(crate) use emit_log;
