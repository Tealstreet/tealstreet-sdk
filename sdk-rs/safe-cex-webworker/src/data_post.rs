//! Native worker DATA is carried to postMessage before any JSON projection.
use crate::payload::StoreWorkerOp;
use safe_cex_exchange::{
    js_runtime::value::{Js, from_typed},
    runtime::ExchangeError,
};
use std::rc::Rc;

/// An outgoing data envelope. Deliberately has no Serialize implementation.
pub struct WorkerDataPayload {
    pub op: StoreWorkerOp,
    pub data: Js,
}
impl WorkerDataPayload {
    pub fn into_message(self) -> Result<Js, ExchangeError> {
        Ok(Js::object(vec![
            (
                "op",
                from_typed(&self.op).map_err(|e| e.to_exchange_error())?,
            ),
            ("data", self.data),
        ]))
    }
}
pub type SendWorkerData = Rc<dyn Fn(WorkerDataPayload) -> Result<(), ExchangeError>>;
/// Hosts must materialize native values and perform structured clone at send.
#[derive(Clone)]
pub struct WorkerDataPost(SendWorkerData);
impl WorkerDataPost {
    pub fn new(send: SendWorkerData) -> Self {
        Self(send)
    }
    pub fn send(&self, payload: WorkerDataPayload) -> Result<(), ExchangeError> {
        (self.0)(payload)
    }
    #[cfg(target_arch = "wasm32")]
    pub fn for_browser(post: BrowserDataPost) -> Self {
        browser::create(post)
    }
}
#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(target_arch = "wasm32")]
pub use browser::BrowserDataPost;
