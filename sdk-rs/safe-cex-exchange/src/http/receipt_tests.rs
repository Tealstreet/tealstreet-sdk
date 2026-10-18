#![allow(clippy::arc_with_non_send_sync)]
use super::*;
use safe_cex_transport::query::{QueryArrayFormat, QueryProfile};
use safe_cex_transport::{RestBody, RestExecution, RestFuture, RestResponseBody};
use std::task::{Context, Poll, Waker};

fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("receipt fixture unexpectedly suspended"),
    }
}
fn response() -> RestResponse {
    RestResponse {
        status: 207,
        reason: Some("fixture receipt".into()),
        headers: BTreeMap::new(),
        body: RestResponseBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"receipt": 19}))
                .unwrap(),
        ),
    }
}
struct Terminal(RefCell<Vec<RestRequest>>);
impl RestAdapter for Terminal {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.0.borrow_mut().push(request);
        Box::pin(async { Ok(response()) })
    }
}
struct ChangedConfig {
    sent: RefCell<Vec<RestRequest>>,
    returned: RestRequest,
}
impl RestAdapter for ChangedConfig {
    fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
        panic!("receipt middleware must retain the inner receipt")
    }
    fn execute_with_receipt(&self, request: RestRequest) -> RestFuture<'_, RestExecution> {
        self.sent.borrow_mut().push(request);
        let config = self.returned.clone();
        Box::pin(async move {
            Ok(RestExecution {
                response: response(),
                config,
            })
        })
    }
}
#[test]
fn terminal_receipt_calls_adapter_once_and_retains_input() {
    let adapter = Terminal(RefCell::new(Vec::new()));
    let input = RestRequest {
        method: "post".into(),
        url: "/terminal-input".into(),
        body: Some(RestBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"raw": 37}))
                .unwrap(),
        )),
        timeout_ms: Some(159),
        ..Default::default()
    };
    let execution = run(adapter.execute_with_receipt(input.clone())).unwrap();
    assert_eq!(adapter.0.borrow().len(), 1, "receipt_terminal_native_count");
    assert_eq!(execution.config, input, "receipt_terminal_native_config");
    assert_eq!(
        execution.response.status, 207,
        "receipt_terminal_native_status"
    );
}
#[test]
fn codec_success_restores_only_its_wire_projections() {
    let terminal = Arc::new(Terminal(RefCell::new(Vec::new())));
    let codec = SerializedParamsAdapter {
        input: AdapterInput::BeforeTransform,
        inner: terminal.clone(),
        serialize: None,
    };
    let profile = QueryProfile {
        key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
        arrays: QueryArrayFormat::Indexed,
        encode: true,
    };
    let params = safe_cex_types::QueryValue::Object(
        [(
            "query-id".into(),
            safe_cex_types::QueryValue::String("two words".into()),
        )]
        .into_iter()
        .collect(),
    );
    let input = RestRequest {
        method: "post".into(),
        url: "/codec-input#fragment".into(),
        query_params: Some(params.clone()),
        query_profile: Some(profile),
        body: Some(RestBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"field": 43}))
                .unwrap(),
        )),
        ..Default::default()
    };
    let execution = run(codec.execute_with_receipt(input)).unwrap();
    assert_eq!(terminal.0.borrow().len(), 1, "receipt_codec_native_count");
    let wire = terminal.0.borrow();
    assert_eq!(
        wire[0].url, "/codec-input?query-id=two%20words",
        "receipt_codec_native_wire_url"
    );
    assert!(
        wire[0].query_params.is_none(),
        "receipt_codec_native_wire_query_consumed"
    );
    assert!(
        matches!(wire[0].body, Some(RestBody::Encoded(_))),
        "receipt_codec_native_wire_body_phase"
    );
    assert_eq!(
        execution.config.url, "/codec-input#fragment",
        "receipt_codec_native_retry_url"
    );
    assert!(
        query_identity_eq(execution.config.query_params.as_ref().unwrap(), &params),
        "receipt_codec_native_retry_query"
    );
    assert!(
        matches!(execution.config.body, Some(RestBody::Transformed(_))),
        "receipt_codec_native_retry_body_phase"
    );
    assert_eq!(
        execution
            .config
            .body
            .as_ref()
            .unwrap()
            .untransformed()
            .unwrap(),
        &safe_cex_types::retained_json::RetainedJsonInput::from_value(json!("{\"field\":43}"))
            .unwrap(),
        "receipt_codec_native_retry_body_value"
    );
}
#[test]
fn codec_success_retains_effective_inner_changes_and_omissions() {
    let returned = RestRequest {
        method: "head".into(),
        url: "/changed-target?inner=1".into(),
        params: Some(json!({"inner-query": 47})),
        body: Some(RestBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"inner-body": 53}))
                .unwrap(),
        )),
        headers: [("Inner-Only".into(), "present".into())]
            .into_iter()
            .collect(),
        timeout_ms: Some(173),
        ..Default::default()
    };
    let inner = Arc::new(ChangedConfig {
        sent: RefCell::new(Vec::new()),
        returned: returned.clone(),
    });
    let codec = SerializedParamsAdapter {
        input: AdapterInput::BeforeTransform,
        inner: inner.clone(),
        serialize: Some(Arc::new(|_| Ok("outer-query=31".into()))),
    };
    let execution = run(codec.execute_with_receipt(RestRequest {
        method: "post".into(),
        url: "/outer-target".into(),
        params: Some(json!({"outer-query": 31})),
        headers: [("Outer-Only".into(), "remove".into())]
            .into_iter()
            .collect(),
        body: Some(RestBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"outer-body": 59}))
                .unwrap(),
        )),
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(inner.sent.borrow().len(), 1, "receipt_changed_native_count");
    assert_eq!(
        execution.config.method, "head",
        "receipt_changed_native_method"
    );
    assert_eq!(
        execution.config.url, "/changed-target?inner=1",
        "receipt_changed_native_url"
    );
    assert_eq!(
        execution.config.body, returned.body,
        "receipt_changed_native_body"
    );
    assert_eq!(
        execution.config.params, returned.params,
        "receipt_changed_native_params"
    );
    assert_eq!(
        execution.config.headers, returned.headers,
        "receipt_changed_native_headers"
    );
    assert_eq!(
        execution.config.timeout_ms,
        Some(173),
        "receipt_changed_native_timeout"
    );
}
