use safe_cex_webworker::payload::{StoreWorkerOp, StoreWorkerPayload};
use serde_json::json;
use std::rc::Rc;
// Test-only projection for legacy telemetry comparisons; this is no native Error proof.

pub fn post(
    post: Rc<dyn Fn(StoreWorkerPayload)>,
) -> safe_cex_webworker::error_post::WorkerErrorPost {
    use safe_cex_webworker::error_post::{WorkerErrorHandle, WorkerErrorPost};
    WorkerErrorPost::new(
        Rc::new(|name, message| {
            Ok(WorkerErrorHandle::new((
                name.to_owned(),
                message.to_owned(),
            )))
        }),
        Rc::new(move |report| {
            let (name, message) = report.error.downcast_ref::<(String, String)>().unwrap();
            post(StoreWorkerPayload {
                op: StoreWorkerOp::Error,
                instance_id: None,
                data: Some(json!([report.message, {"sendToSentry":true,"sentryData":{
                    "error":{"name":name,"message":message}, "tags":report.tags,"extra":report.extra
                }}])),
            });
            Ok(())
        }),
    )
}
