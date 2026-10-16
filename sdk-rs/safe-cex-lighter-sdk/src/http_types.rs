//! HTTP declarations retain generic opaque values, callback identity and UTF-16.
use crate::key_runtime::KeyFuture;
use std::{cell::RefCell, rc::Rc};

pub enum CallbackReturn<V, E> {
    Value(V),
    Promise(KeyFuture<V, E>),
}
impl<V, E> CallbackReturn<V, E> {
    pub fn value(self) -> Result<V, KeyFuture<V, E>> {
        match self {
            Self::Value(value) => Ok(value),
            Self::Promise(future) => Err(future),
        }
    }
}
pub type RequestInterceptor<V, E> = Rc<dyn Fn(V) -> Result<CallbackReturn<V, E>, E>>;
pub type ResponseInterceptor<V, E> = Rc<dyn Fn(V) -> Result<CallbackReturn<V, E>, E>>;
pub type ErrorInterceptor<V, E> = Rc<dyn Fn(V) -> Result<V, E>>;
pub struct Interceptors<V, E> {
    pub request: Option<Rc<RefCell<Vec<RequestInterceptor<V, E>>>>>,
    pub response: Option<Rc<RefCell<Vec<ResponseInterceptor<V, E>>>>>,
    pub error: Option<Rc<RefCell<Vec<ErrorInterceptor<V, E>>>>>,
}
impl<V, E> Interceptors<V, E> {
    pub fn new(
        request: Option<Rc<RefCell<Vec<RequestInterceptor<V, E>>>>>,
        response: Option<Rc<RefCell<Vec<ResponseInterceptor<V, E>>>>>,
        error: Option<Rc<RefCell<Vec<ErrorInterceptor<V, E>>>>>,
    ) -> Self {
        Self {
            request,
            response,
            error,
        }
    }
    pub fn first_request(&self) -> Option<RequestInterceptor<V, E>> {
        self.request.as_ref()?.borrow().first().cloned()
    }
    pub fn first_response(&self) -> Option<ResponseInterceptor<V, E>> {
        self.response.as_ref()?.borrow().first().cloned()
    }
    pub fn first_error(&self) -> Option<ErrorInterceptor<V, E>> {
        self.error.as_ref()?.borrow().first().cloned()
    }
}
pub struct LighterApiError<D> {
    pub name: &'static str,
    pub message: Vec<u16>,
    pub code: Vec<u16>,
    pub details: Option<D>,
    pub request_id: Option<Vec<u16>>,
    pub status_code: Option<f64>,
}
impl<D> LighterApiError<D> {
    pub fn new(
        message: Vec<u16>,
        code: Option<Vec<u16>>,
        details: Option<D>,
        request_id: Option<Vec<u16>>,
        status_code: Option<f64>,
    ) -> Self {
        Self {
            name: "LighterApiError",
            message,
            code: code.unwrap_or_else(|| "UNKNOWN_ERROR".encode_utf16().collect()),
            details,
            request_id,
            status_code,
        }
    }
}
