use super::*;
use safe_cex_transport::{
    NetworkFailure, RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse,
    RestResponseBody, TransportError,
};
use std::sync::{Arc, Mutex};
struct ApiHttp {
    sent: Mutex<Vec<RestRequest>>,
}
impl RestAdapter for ApiHttp {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let path = request.url.split('?').next().unwrap().to_owned();
        let first = {
            let mut sent = self.sent.lock().unwrap();
            let first = !sent
                .iter()
                .any(|r| r.url.split('?').next() == Some(path.as_str()));
            sent.push(request.clone());
            first
        };
        Box::pin(async move {
            if path == "/fapi/v1/listenKey" && first {
                return Err(RestFailure {
                    code: Some("ECONNRESET".into()),
                    message: "transient socket reset".into(),
                    transport_error: Some(Box::new(TransportError::Network(
                        NetworkFailure::from_code(
                            "transient socket reset".into(),
                            Some("ECONNRESET".into()),
                        ),
                    ))),
                    response: None,
                    request: Box::new(request),
                    context: None,
                });
            }
            assert!(
                path == "/fapi/v1/listenKey" || path == "/fapi/v1/income",
                "unexpected production route {path}"
            );
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(if path == "/fapi/v1/listenKey" {
                    b"{\"listenKey\":\"factory-key\"}".to_vec()
                } else {
                    b"[]".to_vec()
                }),
            })
        })
    }
}
#[test]
fn native_factory_retries_unsigned_listen_post_and_encodes_signed_query() {
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(ApiHttp {
        sent: Mutex::new(vec![]),
    });
    let deps = safe_cex_venue_binance::native_host::native_dependencies(
        runtime.clone(),
        http.clone(),
        Rc::new(|_, _| panic!("no external provider")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    let store = Rc::new(safe_cex_exchange::native_store::DefaultStore::new());
    let owner = ExchangeOwner::new(
        Venue::Futures,
        safe_cex_types::ExchangeName::Binance,
        serde_json::from_value(json!({"key":"factory-key","secret":"factory-secret"})).unwrap(),
        store.clone(),
        deps,
    )
    .unwrap();
    let key = run(&runtime, owner.private.fetch_listen_key()).unwrap();
    assert!(key.strict_equals(&EventValue::str("factory-key")));
    let counts = store.request_stats().unwrap().unwrap();
    assert_eq!(counts.ok, 1.0);
    assert_eq!(counts.network, 1.0, "the failed attempt is counted once");
    let calls = http.sent.lock().unwrap();
    assert_eq!(
        calls.len(),
        2,
        "actual native POST is retried after network reset"
    );
    for r in calls.iter() {
        assert!(r.method.eq_ignore_ascii_case("POST"));
        assert_eq!(
            r.url, "/fapi/v1/listenKey",
            "listen-key bypass stays unsigned"
        );
    }
    drop(calls);
    run(
        &runtime,
        owner.invoke(
            safe_cex_venue_binance::exchange_classes::Method::IncomePage,
            vec![EventValue::from_json(
                json!({"limit":17,"symbol":"BTC USDT"}),
            )],
        ),
    )
    .unwrap();
    let calls = http.sent.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert!(calls[2].url.starts_with("/fapi/v1/income?"));
    assert!(
        calls[2].url.contains("symbol=BTC%20USDT"),
        "actual serializer percent-encodes query"
    );
    assert!(calls[2].url.contains("limit=17"));
    assert!(calls[2].url.contains("signature="));
    drop(calls);
    let counts = store.request_stats().unwrap().unwrap();
    assert_eq!(counts.ok, 2.0, "the signed request advances the same Store");
    assert_eq!(counts.network, 1.0);
    owner.dispose().unwrap();
}
