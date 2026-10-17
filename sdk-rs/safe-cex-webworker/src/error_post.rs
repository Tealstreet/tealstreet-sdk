//! Native Error objects stay opaque until the host's structured-clone post.
use safe_cex_exchange::runtime::ExchangeError;
use serde_json::Value;
use std::{any::Any, rc::Rc};

#[derive(Clone)]
pub struct WorkerErrorHandle(Rc<dyn Any>);
impl WorkerErrorHandle {
    pub fn new<T: 'static>(value: T) -> Self {
        Self(Rc::new(value))
    }
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }
    pub fn same_object(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
pub struct WorkerErrorReport {
    pub message: String,
    pub error: WorkerErrorHandle,
    pub tags: Value,
    pub extra: Value,
}
pub type CreateWorkerError = Rc<dyn Fn(&str, &str) -> Result<WorkerErrorHandle, ExchangeError>>;
pub type SendWorkerError = Rc<dyn Fn(WorkerErrorReport) -> Result<(), ExchangeError>>;
#[derive(Clone)]
pub struct WorkerErrorPost {
    create: CreateWorkerError,
    send: SendWorkerError,
}
impl WorkerErrorPost {
    #[cfg(target_arch = "wasm32")]
    pub fn for_browser(post: BrowserErrorPost) -> Self {
        browser::create(post)
    }
    pub fn new(create: CreateWorkerError, send: SendWorkerError) -> Self {
        Self { create, send }
    }
    pub fn create(&self, name: &str, message: &str) -> Result<WorkerErrorHandle, ExchangeError> {
        (self.create)(name, message)
    }
    pub fn send(&self, report: WorkerErrorReport) -> Result<(), ExchangeError> {
        (self.send)(report)
    }
}

#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(target_arch = "wasm32")]
pub use browser::BrowserErrorPost;
