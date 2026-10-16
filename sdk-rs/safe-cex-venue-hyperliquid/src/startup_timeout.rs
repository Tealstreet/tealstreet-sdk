//! Source startup timeout runs independently of observers of its returned Promise.
use crate::helper_tasks::{HelperObserver, HelperTaskHost};
use std::{cell::RefCell, rc::Rc};
pub type TimeoutCallback<V, E> = Rc<dyn Fn(Vec<V>) -> Result<V, E>>;
pub trait StartupTimeoutHost: HelperTaskHost<Self::Value, Self::Error> {
    type Value: Clone + 'static;
    type Error: Clone + 'static;
    fn undefined(&self) -> Self::Value;
    fn null(&self) -> Self::Value;
    fn literal(&self, v: &str) -> Self::Value;
    fn number(&self, v: f64) -> Self::Value;
    fn truthy(&self, v: &Self::Value) -> bool;
    fn global(&self, k: &str) -> Result<Self::Value, Self::Error>;
    fn get(&self, owner: &Self::Value, k: &str) -> Result<Self::Value, Self::Error>;
    fn call(
        &self,
        callee: Self::Value,
        receiver: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn construct(
        &self,
        callee: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn callback(&self, body: TimeoutCallback<Self::Value, Self::Error>) -> Self::Value;
    fn divide(&self, a: Self::Value, b: Self::Value) -> Result<Self::Value, Self::Error>;
    fn template(&self, parts: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn await_value(
        &self,
        value: Self::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Value, Self::Error>>>>;
}
// ts: HyperliquidExchange.withStartupTimeout
pub fn with_startup_timeout<H: StartupTimeoutHost>(
    host: &H,
    owner: H::Value,
    operation: H::Value,
    timeout_ms: H::Value,
    operation_name: H::Value,
    fallback: H::Value,
) -> HelperObserver<H::Value, H::Error> {
    let h = host.clone();
    crate::helper_tasks::start(host, async move {
        let timer = Rc::new(RefCell::new(h.null()));
        let promise = h.global("Promise")?;
        let executor_host = h.clone();
        let timer_slot = timer.clone();
        let executor = h.callback(Rc::new(move |args| {
            let h = &executor_host;
            let resolve = args.first().cloned().unwrap_or_else(|| h.undefined());
            let set_timeout = h.global("setTimeout")?;
            let callback_host = h.clone();
            let owner = owner.clone();
            let name = operation_name.clone();
            let ms = timeout_ms.clone();
            let fallback = fallback.clone();
            let callback = h.callback(Rc::new(move |_| {
                let h = &callback_host;
                let logger = h.get(&owner, "log")?;
                let name = h.template(vec![name.clone()])?;
                let math = h.global("Math")?;
                let round = h.get(&math, "round")?;
                let seconds = h.divide(ms.clone(), h.number(1000.))?;
                let seconds = h.call(round, math, vec![seconds])?;
                let message = h.template(vec![
                    name,
                    h.literal(" timed out after "),
                    seconds,
                    h.literal("s; continuing with fallback"),
                ])?;
                let severity = h.get(&h.global("LogSeverity")?, "Warning")?;
                h.call(logger, owner.clone(), vec![message, severity])?;
                h.call(resolve.clone(), h.undefined(), vec![fallback.clone()])?;
                Ok(h.undefined())
            }));
            let id = h.call(
                set_timeout,
                h.undefined(),
                vec![callback, timeout_ms.clone()],
            )?;
            *timer_slot.borrow_mut() = id;
            Ok(h.undefined())
        }));
        let timeout_promise = h.construct(promise, vec![executor])?;
        let result = async {
            let promise = h.global("Promise")?;
            let race = h.get(&promise, "race")?;
            let pending = h.call(
                race,
                promise,
                vec![h.array(vec![operation, timeout_promise])],
            )?;
            h.await_value(pending).await
        }
        .await;
        // The runtime retains this task if an observer is dropped. Cleanup stays with finally.
        let id = timer.borrow().clone();
        if h.truthy(&id) {
            let clear = h.global("clearTimeout")?;
            let id = timer.borrow().clone();
            h.call(clear, h.undefined(), vec![id])?;
        }
        result
    })
}
