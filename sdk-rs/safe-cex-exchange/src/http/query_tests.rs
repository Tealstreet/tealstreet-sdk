// Arc is required by the production signature under test, not chosen here: the
// transport provider surface is `Arc<dyn ...>`, so a host fixture that satisfies
// it holds an Arc even though its interior is RefCell/Cell and so not Send+Sync.
#![allow(clippy::arc_with_non_send_sync)]
use super::*;
use safe_cex_transport::query::{QueryArrayFormat, QueryProfile};
use safe_cex_transport::signing::{
    HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig,
};
use safe_cex_types::QueryValue;
use std::task::{Context, Poll, Waker};

fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("QUERY fixture unexpectedly suspended"),
    }
}
fn params() -> QueryValue {
    QueryValue::Object(
        [
            ("number".into(), QueryValue::Number(f64::NAN)),
            (
                "list".into(),
                QueryValue::Array(vec![
                    QueryValue::Number(f64::NEG_INFINITY),
                    QueryValue::String("-0".into()),
                ]),
            ),
        ]
        .into_iter()
        .collect(),
    )
}
#[derive(Default)]
struct Capture(Mutex<Vec<RestRequest>>);
impl RestAdapter for Capture {
    fn execute(&self, request: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        self.0.lock().unwrap().push(request.clone());
        Box::pin(async move { Err(HttpFailure::pre_transmit("fixture failure".into(), request)) })
    }
}
#[derive(Default)]
struct EchoSigner(Mutex<Vec<RemoteSignerRequest>>);
impl RemoteSigner for EchoSigner {
    fn sign(
        &self,
        request: RemoteSignerRequest,
    ) -> safe_cex_transport::TransportFuture<'_, RemoteSignerResponse> {
        let RemoteSignerRequest::Http(http) = &request else {
            panic!("QUERY expected HTTP")
        };
        let response = SignedConfig {
            params: http.config.params.clone(),
            ..Default::default()
        };
        self.0.lock().unwrap().push(request);
        Box::pin(async move {
            Ok(RemoteSignerResponse::Http(HttpSigningResponse {
                id: "fixture".into(),
                config: Some(response),
                error: None,
            }))
        })
    }
}
#[derive(Default)]
struct RetryClock(Mutex<Vec<u64>>);
impl safe_cex_transport::rate_limit::RateLimitClock for RetryClock {
    fn now_ms(&self) -> u64 {
        1
    }
    fn sleep_ms(&self, delay: u64) -> safe_cex_transport::TransportFuture<'_, ()> {
        self.0.lock().unwrap().push(delay);
        Box::pin(async { Ok(()) })
    }
}
#[test]
fn query_profiles_restore_retry_values_without_value_conversion() {
    for (arrays, expected) in [
        (
            QueryArrayFormat::Indexed,
            "/fixture?number=NaN&list%5B0%5D=-Infinity&list%5B1%5D=-0",
        ),
        (
            QueryArrayFormat::Repeat,
            "/fixture?number=NaN&list=-Infinity&list=-0",
        ),
    ] {
        let profile = QueryProfile {
            key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
            arrays,
            encode: true,
        };
        let native = params();
        let inner = Arc::new(Capture::default());
        let adapter = Arc::new(SerializedParamsAdapter {
            input: AdapterInput::RemoteOutput,
            inner: inner.clone(),
            serialize: Some(Arc::new(|_| {
                panic!("QUERY request profile precedes factory")
            })),
        });
        let request = RestRequest {
            method: "GET".into(),
            url: "/fixture".into(),
            query_params: Some(native.clone()),
            query_profile: Some(profile),
            ..Default::default()
        };
        use safe_cex_transport::retry::{ClassifiedRequest, ReadClassification, only_reads};
        let ClassifiedRequest::Read(read) = ReadClassification::Methods.classify(request) else {
            panic!("QUERY fixture must classify as read")
        };
        let clock = RetryClock::default();
        let error =
            run(only_reads(read, 1, |_, result| result.is_err()).execute(adapter.as_ref(), &clock))
                .unwrap_err();
        assert_eq!(
            *clock.0.lock().unwrap(),
            [0],
            "QUERY retry schedules its zero configured delay"
        );

        assert!(
            query_identity_eq(
                error
                    .request
                    .query_params
                    .as_ref()
                    .expect("QUERY retry params"),
                &native
            ),
            "QUERY retry native number identity"
        );
        assert_eq!(
            error.request.query_profile,
            Some(profile),
            "QUERY retry serializer profile"
        );
        assert_eq!(error.request.url, "/fixture", "QUERY retry original URL");
        let sent = inner.0.lock().unwrap();
        assert_eq!(sent.len(), 2, "QUERY adapter native call count");
        for index in 0..2 {
            assert_eq!(
                sent[index].query_profile,
                Some(profile),
                "QUERY adapter serializer profile"
            );
            assert!(
                sent[index].query_params.is_none(),
                "QUERY wire params consumed once"
            );
            assert_eq!(sent[index].url, expected, "QUERY exact post-signing URL");
        }
    }
}

#[test]
fn legacy_remote_signing_retains_value_params_and_profiles_refuse_shims() {
    let inner = Arc::new(Capture::default());
    let signer = Arc::new(EchoSigner::default());
    let remote = create_remote_signing_adapter(
        signer.clone(),
        ExchangeName::Binance,
        None,
        Arc::new(|| "fixture".into()),
        Arc::new(|| 0),
        inner.clone(),
    );
    let error = run(remote.execute(RestRequest {
        method: "GET".into(),
        url: "/fixture".into(),
        params: Some(json!({"qty":"-0"})),
        ..Default::default()
    }))
    .unwrap_err();
    assert_eq!(
        error.request.params,
        Some(json!({"qty":"-0"})),
        "QUERY legacy remote native Value identity"
    );
    assert_eq!(
        signer.0.lock().unwrap().len(),
        1,
        "QUERY legacy remote call count"
    );
    let error = run(remote.execute(RestRequest {
        method: "GET".into(),
        url: "/fixture".into(),
        query_params: Some(params()),
        query_profile: Some(QueryProfile {
            key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
            arrays: QueryArrayFormat::Repeat,
            encode: true,
        }),
        ..Default::default()
    }))
    .unwrap_err();
    assert_eq!(
        error.message, "Native query parameters require a native-profile signing host",
        "QUERY remote refuses Value shim"
    );
    assert_eq!(
        signer.0.lock().unwrap().len(),
        1,
        "QUERY native refusal does not invoke legacy signer"
    );
    assert_eq!(
        inner.0.lock().unwrap().len(),
        1,
        "QUERY native refusal does not transmit"
    );
}

#[test]
fn frame_adapter_retains_profile_params_and_falsy_legacy_values() {
    struct Frame(Capture);
    impl RestAdapter for Frame {
        fn query_in_url(&self) -> bool {
            false
        }
        fn execute(
            &self,
            request: RestRequest,
        ) -> safe_cex_transport::RestFuture<'_, RestResponse> {
            self.0.execute(request)
        }
    }
    let inner = Arc::new(Frame(Capture::default()));
    let adapter = SerializedParamsAdapter {
        input: AdapterInput::BeforeTransform,
        inner: inner.clone(),
        serialize: Some(Arc::new(|_| panic!("QUERY HOW must bypass URL serializer"))),
    };
    let expected = params();
    let error = run(adapter.execute(RestRequest {
        url: "/frame".into(),
        query_params: Some(expected.clone()),
        query_profile: Some(QueryProfile {
            key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
            arrays: QueryArrayFormat::Indexed,
            encode: true,
        }),
        ..Default::default()
    }))
    .unwrap_err();
    assert!(
        query_identity_eq(error.request.query_params.as_ref().unwrap(), &expected),
        "QUERY HOW native profile retained"
    );
    assert!(
        query_identity_eq(
            inner.0.0.lock().unwrap()[0].query_params.as_ref().unwrap(),
            &expected
        ),
        "QUERY HOW inner native params retained"
    );
    assert_eq!(
        inner.0.0.lock().unwrap()[0].url,
        "/frame",
        "QUERY HOW URL unchanged"
    );
    let direct = Arc::new(Capture::default());
    let adapter = SerializedParamsAdapter {
        input: AdapterInput::BeforeTransform,
        inner: direct.clone(),
        serialize: Some(Arc::new(|_| panic!("QUERY falsy serializer must not run"))),
    };
    for value in [Value::Null, Value::Bool(false), json!(0), json!("")] {
        let _error = run(adapter.execute(RestRequest {
            url: "/falsy".into(),
            params: Some(value.clone()),
            ..Default::default()
        }))
        .unwrap_err();
        assert_eq!(
            direct.0.lock().unwrap().last().unwrap().params,
            Some(value),
            "QUERY direct falsy params retained"
        );
    }
}

#[test]
fn native_remote_signing_uses_retained_wire_domain_without_legacy_projection() {
    struct NativeSigner(Mutex<Vec<safe_cex_transport::query_signing::SigningConfig>>);
    impl RemoteSigner for NativeSigner {
        fn sign(
            &self,
            _: RemoteSignerRequest,
        ) -> safe_cex_transport::TransportFuture<'_, RemoteSignerResponse> {
            panic!("QUERY native request must not use legacy signing")
        }
        fn sign_query(
            &self,
            request: safe_cex_transport::query_signing::HttpSigningRequest,
        ) -> safe_cex_transport::TransportFuture<
            '_,
            safe_cex_transport::query_signing::HttpSigningResponse,
        > {
            let params = request.config.params.clone();
            self.0.lock().unwrap().push(request.config);
            Box::pin(async move {
                Ok(safe_cex_transport::query_signing::HttpSigningResponse {
                    id: request.id,
                    config: Some(safe_cex_transport::query_signing::SignedConfig {
                        url: Some(String::new()),
                        params,
                        headers: indexmap::IndexMap::from([
                            ("authorization".into(), "bad signed auth".into()),
                            ("tealstreet-authorization".into(), "bad signed proxy".into()),
                        ]),
                        ..Default::default()
                    }),
                    error: None,
                })
            })
        }
    }
    let signer = Arc::new(NativeSigner(Mutex::new(vec![])));
    let inner = Arc::new(Capture::default());
    let serialized = Arc::new(SerializedParamsAdapter {
        input: AdapterInput::RemoteOutput,
        inner: inner.clone(),
        serialize: None,
    });
    let remote = create_remote_signing_adapter(
        signer.clone(),
        ExchangeName::Binance,
        None,
        Arc::new(|| "native".into()),
        Arc::new(|| 0),
        serialized,
    );
    let error = run(remote.execute(RestRequest {
        method: "GET".into(),
        url: "/native".into(),
        headers: indexmap::IndexMap::from([
            ("Authorization".into(), "Bearer original".into()),
            (
                "Tealstreet-Authorization".into(),
                "bad original proxy".into(),
            ),
        ]),
        query_params: Some(params()),
        query_profile: Some(QueryProfile {
            key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
            arrays: QueryArrayFormat::Repeat,
            encode: true,
        }),
        ..Default::default()
    }))
    .unwrap_err();
    let calls = signer.0.lock().unwrap();
    assert_eq!(calls.len(), 1, "QUERY native remote call count");
    assert!(
        query_identity_eq(calls[0].params.as_ref().unwrap(), &params()),
        "QUERY native remote NaN identity"
    );
    assert!(
        query_identity_eq(error.request.query_params.as_ref().unwrap(), &params()),
        "QUERY native remote retry identity"
    );
    let wire = inner.0.lock().unwrap();
    assert_eq!(wire.len(), 1, "QUERY native remote wire call count");
    assert_eq!(
        wire[0].headers.get("Authorization").map(String::as_str),
        Some("Bearer original"),
        "QUERY native usable auth restored"
    );
    assert!(
        !wire[0].headers.contains_key("authorization"),
        "QUERY native unusable duplicate auth removed"
    );
    assert!(
        !wire[0]
            .headers
            .keys()
            .any(|k| k.eq_ignore_ascii_case("Tealstreet-Authorization")),
        "QUERY native unusable source auth omitted"
    );
    assert_eq!(
        wire[0].url, "/native?number=NaN&list=-Infinity&list=-0",
        "QUERY native remote exact query URL"
    );
}
