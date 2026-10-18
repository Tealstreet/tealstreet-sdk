#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn post_timeout_error(post: js_sys::Function) -> Result<(), JsValue> {
    use safe_cex_exchange::runtime::ExchangeError;
    use safe_cex_webworker::error_post::{WorkerErrorPost, WorkerErrorReport};
    use std::rc::Rc;
    let host = WorkerErrorPost::for_browser(Rc::new(move |payload| {
        post.call1(&JsValue::UNDEFINED, payload)
            .map(|_| ())
            .map_err(|value| ExchangeError::new(format!("host post failed: {value:?}")))
    }));
    let message = "Markets still not loaded after 30s, dropping 1 pending subscriptions";
    let error = host
        .create("MarketsNotLoadedError", message)
        .map_err(|error| js_sys::Error::new(&error.message))?;
    host.send(WorkerErrorReport {
        message: message.into(),
        error,
        tags: serde_json::json!({"worker_error_family":"markets_not_loaded"}),
        extra: serde_json::json!({"pendingCount":1}),
    })
    .map_err(|error| js_sys::Error::new(&error.message).into())
}
