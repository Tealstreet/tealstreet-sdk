use super::{WorkerErrorHandle, WorkerErrorPost};
use js_sys::{Array, Error, Object};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeErrorOrigin};
use std::rc::Rc;
use wasm_bindgen::JsValue;

pub type BrowserErrorPost = Rc<dyn Fn(&JsValue) -> Result<(), ExchangeError>>;
#[derive(Debug)]
struct BrowserErrorCause(JsValue);
impl std::fmt::Display for BrowserErrorCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for BrowserErrorCause {}
fn failure(value: JsValue) -> ExchangeError {
    let mut error = ExchangeError::new(format!("{value:?}"));
    error.origin = Some(ExchangeErrorOrigin::new(BrowserErrorCause(value)));
    error
}
fn object(entries: &[(&str, JsValue)]) -> Result<JsValue, ExchangeError> {
    let rows = Array::new();
    for (key, value) in entries {
        let row = Array::new();
        row.push(&JsValue::from_str(key));
        row.push(value);
        rows.push(&row);
    }
    Object::from_entries(&rows)
        .map(JsValue::from)
        .map_err(failure)
}
fn metadata(value: &serde_json::Value) -> Result<JsValue, ExchangeError> {
    let json = serde_json::to_string(value).map_err(|e| ExchangeError::new(e.to_string()))?;
    js_sys::JSON::parse(&json).map_err(failure)
}
pub(super) fn create(post: BrowserErrorPost) -> WorkerErrorPost {
    WorkerErrorPost::new(
        Rc::new(|name, message| {
            let error = Error::new(message);
            error.set_name(name);
            Ok(WorkerErrorHandle::new(error))
        }),
        Rc::new(move |report| {
            let native = report.error.downcast_ref::<Error>().ok_or_else(|| {
                ExchangeError::new("Worker Error belongs to a different post host")
            })?;
            let sentry = object(&[
                ("error", native.clone().into()),
                ("tags", metadata(&report.tags)?),
                ("extra", metadata(&report.extra)?),
            ])?;
            let options = object(&[("sendToSentry", JsValue::TRUE), ("sentryData", sentry)])?;
            let data = Array::new();
            data.push(&JsValue::from_str(&report.message));
            data.push(&options);
            let payload = object(&[("op", JsValue::from_str("error")), ("data", data.into())])?;
            post(&payload)
        }),
    )
}
