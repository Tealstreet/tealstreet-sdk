use safe_cex_transport::{
    RestAdapter, RestExecution, RestFailure, RestFuture, RestRequest, RestResponse,
    RestResponseBody, TransportError, TransportFuture,
    rate_limit::RateLimitClock,
    retry::ReadClassification,
    signature_rejection::{
        IsSignatureRejection, SignatureRetry, is_binance_style_signature_rejection,
        resend_on_signature_rejection, retry_network_reads_and_signature_rejection,
        signature_rejected_response,
    },
};
use serde_json::json;
use std::{
    cell::Cell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn request(url: &str) -> RestRequest {
    RestRequest {
        method: "POST".into(),
        url: url.into(),
        timeout_ms: Some(27),
        ..Default::default()
    }
}
fn response() -> RestResponse {
    RestResponse {
        status: 401,
        reason: None,
        headers: Default::default(),
        body: RestResponseBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(
                json!({"code":{"toString":0}}),
            )
            .unwrap(),
        ),
    }
}
fn predicate() -> IsSignatureRejection {
    Rc::new(is_binance_style_signature_rejection)
}
fn assert_type_error(error: &RestFailure, url: &str) {
    let Some(TransportError::JavaScript(cause)) = error.transport_error.as_deref() else {
        panic!(
            "signature predicate TypeError must retain its native JavaScript error kind: {:?}",
            error.transport_error
        );
    };
    assert_eq!(cause.name, "TypeError");
    assert_eq!(cause.message, "Cannot convert object to primitive value");
    assert_eq!(error.message, cause.message);
    assert_eq!(error.request.url, url);
    assert_eq!(error.request.timeout_ms, Some(27));
    assert!(error.response.is_none());
    assert!(error.code.is_none());
    assert!(!error.is_retryable_network());
}
#[test]
fn rejected_error_keeps_predicate_type_error_and_failed_request() {
    let mut original = RestFailure::pre_transmit("HTTP rejection".into(), request("/failed"));
    original.response = Some(Box::new(response()));
    let retry = SignatureRetry::default();
    let error = signature_rejected_response(&original, &retry, &predicate()).unwrap_err();
    assert_type_error(&error, "/failed");
    assert!(!retry.has_retried());
}
#[test]
fn resolved_resend_keeps_predicate_type_error_and_effective_config() {
    let execution = RestExecution {
        response: response(),
        config: request("/effective"),
    };
    let retry = SignatureRetry::default();
    let error = resend_on_signature_rejection(&execution, &retry, &predicate()).unwrap_err();
    assert_type_error(&error, "/effective");
    assert!(!retry.has_retried());
}
struct Adapter(Cell<usize>);
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.0.set(self.0.get() + 1);
        Box::pin(async move {
            let mut error = RestFailure::pre_transmit("HTTP rejection".into(), request);
            error.response = Some(Box::new(response()));
            Err(error)
        })
    }
}
struct Clock;
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        0
    }
    fn sleep_ms(&self, _: u64) -> TransportFuture<'_, ()> {
        panic!("predicate failure must not schedule a retry")
    }
}
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("synchronous fixture must finish"),
    }
}
#[test]
fn retry_engine_keeps_predicate_type_error_for_read_and_write() {
    for method in ["GET", "POST"] {
        let mut request = request("/engine");
        request.method = method.into();
        let adapter = Adapter(Cell::new(0));
        let retry = SignatureRetry::default();
        let result = run(retry_network_reads_and_signature_rejection(
            ReadClassification::Methods.classify(request),
            1,
            &adapter,
            &Clock,
            &retry,
            &predicate(),
        ));
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("predicate must fail"),
        };
        assert_type_error(&error, "/engine");
        assert_eq!(adapter.0.get(), 1);
        assert!(!retry.has_retried());
    }
}
#[test]
#[ignore = "requires isolated TypeScript compiler; run with --include-ignored"]
fn maintained_source_throws_type_error_before_stamping_or_resending() {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/signature-error-kind-source.cjs"
        ))
        .output()
        .expect("Node source oracle");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let actual: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let expected = json!({"name":"TypeError","message":"Cannot convert object to primitive value","stamped":false,"requests":0});
    assert_eq!(actual, json!([expected.clone(), expected]));
}
