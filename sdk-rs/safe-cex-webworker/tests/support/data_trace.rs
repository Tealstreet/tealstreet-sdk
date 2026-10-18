//! Explicitly lossy JSON adapter for legacy finite trace comparisons only.
//! Native DATA regressions and the actual WASM host bypass this adapter.
use safe_cex_exchange::js_runtime::value::to_js_json;
use safe_cex_webworker::{data_post::WorkerDataPost, payload::StoreWorkerPayload};
use std::rc::Rc;
pub fn post(trace: Rc<dyn Fn(StoreWorkerPayload)>) -> WorkerDataPost {
    WorkerDataPost::new(Rc::new(move |payload| {
        trace(StoreWorkerPayload {
            op: payload.op,
            instance_id: None,
            data: Some(
                serde_json::from_str(
                    &safe_cex_num::js_json_stringify(
                        &to_js_json(&payload.data).map_err(|e| e.to_exchange_error())?,
                    )
                    .map_err(|e| safe_cex_exchange::runtime::ExchangeError::new(e.to_string()))?
                    .expect("worker trace object"),
                )
                .unwrap(),
            ),
        });
        Ok(())
    }))
}
