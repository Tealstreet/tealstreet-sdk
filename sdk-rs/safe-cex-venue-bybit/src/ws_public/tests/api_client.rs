use super::*;
use crate::api_request::client::RequestClient;
use safe_cex_crypto::{PayloadSigner, PayloadSignerRequest, PayloadSignerResult};
use safe_cex_exchange::http::{HttpClientOptions, HttpFailure, HttpFuture, HttpHost};
use safe_cex_transport::{
    RestAdapter, RestBody, RestFuture, RestRequest, RestResponse, TransportRegistry,
};

#[derive(Default)]
struct WireAdapter(RefCell<Vec<Value>>);
impl RestAdapter for WireAdapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let data = request.body.as_ref().map(|b| match b {
            RestBody::Encoded(bytes) => String::from_utf8(bytes.clone()).unwrap(),
            _ => panic!("wire body must be encoded by core after signing"),
        });
        self.0.borrow_mut().push(json!({"method":request.method,"url":request.url,"headers":request.headers,"data":data}));
        Box::pin(async {
            Ok(RestResponse {
                status: 200,
                reason: Some("OK".into()),
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(br#"{"retCode":0}"#.to_vec()),
            })
        })
    }
}
#[derive(Default)]
struct Host {
    starts: Cell<usize>,
    failures: Cell<usize>,
}
impl HttpHost for Host {
    fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { adapter.execute_with_receipt(request).await })
    }
    fn enhance_error(&self, error: HttpFailure, _: &safe_cex_types::ExchangeName) -> HttpFailure {
        error
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn note_success(&self, _: &safe_cex_types::ExchangeName, _: &RestResponse) {}
    fn note_failure(&self, _: &safe_cex_types::ExchangeName, _: &HttpFailure) {
        self.failures.set(self.failures.get() + 1);
    }
    fn mark_start(&self, _: &mut RestRequest) {
        self.starts.set(self.starts.get() + 1);
    }
    fn signing_capture(&self, _: &RestRequest) -> bool {
        false
    }
    fn is_development(&self) -> bool {
        false
    }
    fn is_main_window(&self) -> bool {
        false
    }
}
struct RejectSigner;
impl PayloadSigner for RejectSigner {
    fn sign<'a>(
        &'a self,
        _: PayloadSignerRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<PayloadSignerResult, String>> + 'a>> {
        Box::pin(async { Err("signer refused".into()) })
    }
}
#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "browser-local recording adapter"
)]
fn request_client_signs_then_transforms_and_keeps_kline_unsigned() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/client-request-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 48);
    for (i, case) in cases.into_iter().enumerate() {
        let wire = Arc::new(WireAdapter::default());
        let host = Rc::new(Host::default());
        let client = RequestClient::new(
            HttpClientOptions {
                response_transform: None,
                params_serializer: None,
                exchange: safe_cex_types::ExchangeName::Bybit,
                options: serde_json::from_value(case["options"].clone()).unwrap(),
                request: RestRequest {
                    base_url: Some("https://api.bybit.com".into()),
                    ..Default::default()
                },
                provider: None,
                signer: None,
                account_name: None,
                adapter: wire.clone(),
                host: host.clone(),
                runtime: Rc::new(Runtime::default()),
                store: None,
                stats: Some(Default::default()),
                logs: Some(Default::default()),
                every: vec![],
                periodic: vec![],
                next_id: Arc::new(|| "fixture-id".into()),
                signing_now: Arc::new(|| 1000),
            },
            &TransportRegistry::default(),
            case["siteHeaders"].as_object().unwrap().clone(),
            (case["mode"] == "reject").then(|| Rc::new(RejectSigner) as Rc<dyn PayloadSigner>),
            {
                let mut clock = safe_cex_store::virtual_clock::VirtualClock::default();
                clock.now = Arc::new(|| 1000.);
                clock
            },
        )
        .unwrap();
        let request = &case["request"];
        let result = ready(client.execute(RestRequest {
            method: request["method"].as_str().unwrap().into(),
            url: request["url"].as_str().unwrap().into(),
            params: Some(request["params"].clone()),
            body: request.get("data").cloned().map(|value| {
                RestBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(value).unwrap(),
                )
            }),
            ..Default::default()
        }));
        // Core's callback runs after defaults: KLINE clearing must survive to wire.
        let actual = json!(*wire.0.borrow());
        let expected = case["wire"].clone();
        assert_eq!(actual, expected, "signed wire case {i}");
        assert_eq!(
            result.err().map(|e| e.message),
            case["error"].as_str().map(str::to_owned),
            "signer failure case {i}"
        );
        assert_eq!(
            host.starts.get(),
            case["wire"].as_array().unwrap().len(),
            "preflight must precede common stats case {i}"
        );
        assert_eq!(
            host.failures.get(),
            usize::from(!case["error"].is_null()),
            "preflight must reach common failure path case {i}"
        );
    }
}
