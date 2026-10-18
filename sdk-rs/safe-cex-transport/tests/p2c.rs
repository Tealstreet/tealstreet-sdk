use safe_cex_transport::{
    DirectTransport, RestAdapter, RestRequest, RestResponse, TransportError, TransportFuture,
};
use safe_cex_transport::{polling::*, rate_limit::*, retry::*, secondary::*, signing::*};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

fn run<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::noop();
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(waker)) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending fake"),
    }
}
#[derive(Default)]
struct Venue {
    accepted: Mutex<Vec<RestRequest>>,
    replies: Mutex<VecDeque<Result<RestResponse, safe_cex_transport::RestFailure>>>,
}
fn ok() -> RestResponse {
    RestResponse {
        status: 200,
        reason: None,
        headers: BTreeMap::new(),
        body: safe_cex_transport::RestResponseBody::Encoded(b"{}".to_vec()),
    }
}
fn reset() -> safe_cex_transport::RestFailure {
    safe_cex_transport::RestFailure::from_transport(
        TransportError::Network(safe_cex_transport::NetworkFailure::from_code(
            "ECONNRESET after acceptance".into(),
            Some("ECONNRESET".into()),
        )),
        RestRequest::default(),
    )
}
impl RestAdapter for Venue {
    fn execute(&self, request: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.accepted.lock().unwrap().push(request.clone());
            self.replies
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Ok(ok()))
                .map_err(|mut error| {
                    // Network fixture failures carry the actual attempt config,
                    // as Axios does; explicit mutation witnesses retain theirs.
                    if *error.request == RestRequest::default() {
                        error.request = Box::new(request);
                    }
                    error
                })
        })
    }
}
fn req(method: &str, url: &str) -> RestRequest {
    RestRequest {
        method: method.into(),
        url: url.into(),
        ..Default::default()
    }
}
fn read(classifier: ReadClassification, request: RestRequest) -> ReadRequest {
    match classifier.classify(request) {
        ClassifiedRequest::Read(read) => read,
        _ => panic!("expected read"),
    }
}

#[test]
fn lost_order_response_is_never_resent_but_a_read_is() {
    let venue = Venue::default();
    venue.replies.lock().unwrap().push_back(Err(reset()));
    let ClassifiedRequest::Write(write) =
        ReadClassification::Methods.classify(req("POST", "/orders"))
    else {
        panic!("write classified as read")
    };
    assert!(run(write.execute(&venue)).is_err());
    assert_eq!(
        venue.accepted.lock().unwrap().len(),
        1,
        "venue already accepted the order"
    );
    venue.replies.lock().unwrap().push_back(Err(reset()));
    assert!(
        run(
            network_error_on_read(read(ReadClassification::Methods, req("GET", "/orders")), 3)
                .execute(&venue, &RetryClock::default())
        )
        .is_ok()
    );
    assert_eq!(venue.accepted.lock().unwrap().len(), 3);
}
#[test]
fn retry_predicates_preserve_post_reads_and_reject_writes() {
    for method in ["GET", "get", "HEAD", "head"] {
        assert!(is_read_only_method(&req(method, "/orders")));
    }
    for method in ["POST", "PUT", "DELETE", "PATCH", ""] {
        assert!(!is_read_only_method(&req(method, "/orders")));
    }
    let posted = reads_and_posted(vec![
        "/private/Balance".into(),
        "/bullet-private".into(),
        "/listenKey".into(),
    ]);
    for path in ["/private/Balance?a=1", "/bullet-private", "/listenKey"] {
        let _ = read(posted.clone(), req("POST", path));
    }
    assert!(matches!(
        posted.classify(req("POST", "/orders")),
        ClassifiedRequest::Write(_)
    ));
    assert!(!is_hyperliquid_read(&req("POST", "/exchange?x=1")));
    assert!(is_hyperliquid_read(&req("POST", "/info")));
    for path in [
        "/full/v1/open_orders",
        "/lite/v1/open_orders?x=1",
        "/auth/api_key/login",
    ] {
        assert!(is_grvt_read(&req("POST", path)));
    }
    for path in [
        "/full/v1/create_order",
        "/lite/v1/cancel_all_orders",
        "/full/v1/authorize_builder",
    ] {
        assert!(!is_grvt_read(&req("POST", path)));
    }
}
#[test]
fn network_retry_excludes_timeout_response_errors_and_exhausts_budget() {
    let venue = Venue::default();
    venue
        .replies
        .lock()
        .unwrap()
        .push_back(Err(safe_cex_transport::RestFailure::from_transport(
            TransportError::Network(safe_cex_transport::NetworkFailure::from_code(
                "timeout".into(),
                Some("ECONNABORTED".into()),
            )),
            RestRequest::default(),
        )));
    assert!(
        run(
            network_error_on_read(read(ReadClassification::Methods, req("GET", "/data")), 3)
                .execute(&venue, &RetryClock::default())
        )
        .is_err()
    );
    assert_eq!(venue.accepted.lock().unwrap().len(), 1);
    venue
        .replies
        .lock()
        .unwrap()
        .extend([Err(reset()), Err(reset()), Err(reset())]);
    assert!(
        run(
            network_error_on_read(read(ReadClassification::Methods, req("HEAD", "/data")), 2)
                .execute(&venue, &RetryClock::default())
        )
        .is_err()
    );
    assert_eq!(venue.accepted.lock().unwrap().len(), 4);
}
#[test]
fn asterdex_status_retry_uses_the_read_policy() {
    let venue = Venue::default();
    venue.replies.lock().unwrap().push_back(Ok(RestResponse {
        status: 503,
        ..ok()
    }));
    let policy = only_reads(
        read(ReadClassification::Methods, req("GET", "/positions")),
        1,
        |_, result| matches!(result, Ok(response) if matches!(response.status, 503 | 504)),
    );
    assert_eq!(
        run(policy.execute(&venue, &RetryClock::default()))
            .unwrap()
            .status,
        200
    );
    assert_eq!(venue.accepted.lock().unwrap().len(), 2);
}
struct Signer {
    calls: Mutex<Vec<RemoteSignerRequest>>,
    response: Mutex<Option<RemoteSignerResponse>>,
}
impl RemoteSigner for Signer {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(request);
            Ok(self.response.lock().unwrap().take().unwrap())
        })
    }
}
fn signer(config: SignedConfig) -> Arc<Signer> {
    Arc::new(Signer {
        calls: Mutex::new(vec![]),
        response: Mutex::new(Some(RemoteSignerResponse::Http(
            safe_cex_transport::signing::HttpSigningResponse {
                id: "id".into(),
                config: Some(config),
                error: None,
            },
        ))),
    })
}
fn signing_config() -> SigningConfig {
    SigningConfig {
        method: "post".into(),
        url: "/orders".into(),
        base_url: "https://venue".into(),
        params: Some(json!("{\"symbol\":\"BTCUSDT\",\"side\":\"Buy\"}")),
        data: Some(retained(json!("{\"symbol\":\"BTCUSDT\",\"side\":\"Buy\"}"))),
        headers: indexmap::IndexMap::from([
            ("Authorization".into(), "Bearer ours".into()),
            ("X-Old-Signature".into(), "stale".into()),
        ]),
    }
}
#[test]
fn remote_signature_omissions_are_authoritative_and_transport_auth_survives() {
    let venue = Arc::new(Venue::default());
    let signer = signer(SignedConfig {
        url: Some("/orders?signature=good".into()),
        headers: indexmap::IndexMap::from([("authorization".into(), "Bearer".into())]),
        ..Default::default()
    });
    let adapter = create_remote_signing_adapter(
        signer.clone(),
        ExchangeName::Binance,
        Some("account".into()),
        Arc::new(|| "id".into()),
        Arc::new(|| 0),
        venue.clone(),
    );
    run(adapter.execute_signed(SigningRequest::new(signing_config()))).unwrap();
    let calls = signer.calls.lock().unwrap();
    let RemoteSignerRequest::Http(call) = &calls[0] else {
        panic!("HTTP must not dispatch WS auth")
    };
    assert_eq!(call.config.method, "POST");
    for value in [
        safe_cex_num::js_retained_json_stringify(call.config.data.as_ref().unwrap())
            .unwrap()
            .unwrap(),
        serde_json::to_string(call.config.params.as_ref().unwrap()).unwrap(),
    ] {
        assert_eq!(value, r#"{"symbol":"BTCUSDT","side":"Buy"}"#);
    }
    assert_eq!(call.account_name.as_deref(), Some("account"));
    let accepted = venue.accepted.lock().unwrap();
    assert_eq!(accepted[0].url, "/orders?signature=good");
    assert!(accepted[0].body.is_none());
    assert!(accepted[0].params.is_none());
    assert_eq!(
        accepted[0].headers.get("Authorization").unwrap(),
        "Bearer ours"
    );
    assert!(!accepted[0].headers.contains_key("X-Old-Signature"));
}
#[test]
fn public_tag_skips_only_remote_signing_and_inner_adapter_is_lazy() {
    let old = Arc::new(Venue::default());
    let new = Arc::new(Venue::default());
    let signer = signer(SignedConfig::default());
    let adapter = create_remote_signing_adapter(
        signer.clone(),
        ExchangeName::Bybit,
        None,
        Arc::new(|| "id".into()),
        Arc::new(|| 0),
        old.clone(),
    );
    let mut request = SigningRequest::new(signing_config());
    mark_skip_remote_signing(&mut request);
    // A local signing interceptor can still add a signature after the tag.
    request
        .config
        .headers
        .insert("X-Local-Signature".into(), "good".into());
    set_inner_adapter_for_remote_signing(&adapter, new.clone());
    run(adapter.execute_signed(request)).unwrap();
    assert!(signer.calls.lock().unwrap().is_empty());
    assert!(old.accepted.lock().unwrap().is_empty());
    assert_eq!(
        new.accepted.lock().unwrap()[0]
            .headers
            .get("X-Local-Signature")
            .unwrap(),
        "good"
    );
}
#[test]
fn signing_failures_never_reach_the_venue_and_fresh_browser_auth_wins() {
    let venue = Arc::new(Venue::default());
    let signer = signer(SignedConfig {
        headers: indexmap::IndexMap::from([("authorization".into(), "Bearer browser".into())]),
        ..Default::default()
    });
    let adapter = create_remote_signing_adapter(
        signer.clone(),
        ExchangeName::Bybit,
        None,
        Arc::new(|| "id".into()),
        Arc::new(|| 0),
        venue.clone(),
    );
    run(adapter.execute_signed(SigningRequest::new(signing_config()))).unwrap();
    assert_eq!(
        venue.accepted.lock().unwrap()[0]
            .headers
            .get("authorization")
            .unwrap(),
        "Bearer browser"
    );
    *signer.response.lock().unwrap() = Some(RemoteSignerResponse::Http(
        safe_cex_transport::signing::HttpSigningResponse {
            error: Some("disconnected".into()),
            ..Default::default()
        },
    ));
    assert!(run(adapter.execute_signed(SigningRequest::new(signing_config()))).is_err());
    assert_eq!(venue.accepted.lock().unwrap().len(), 1);
    for value in [
        json!("null"),
        json!("1"),
        json!("true"),
        json!("side=Buy"),
        json!(null),
    ] {
        assert_eq!(try_parse_json(value.clone()), value);
    }
    assert_eq!(try_parse_json(json!("[1,2]")), json!([1, 2]));
}
#[test]
fn secondary_is_default_off_and_only_get_uses_secondary_credentials() {
    let created = Mutex::new(vec![]);
    let factory = |options: &ExchangeOptions| {
        created.lock().unwrap().push(options.clone());
        Ok(Arc::new(Venue::default()) as Arc<dyn RestAdapter>)
    };
    let options: ExchangeOptions =
        serde_json::from_value(json!({"key":"primary", "secret":"secret"})).unwrap();
    let clients =
        create_api_with_secondary_key(&options, &DirectTransport, None, factory, None, None)
            .unwrap();
    assert!(clients.shares_primary_adapter());
    assert_eq!(created.lock().unwrap().len(), 1);
    let options: ExchangeOptions = serde_json::from_value(json!({"key":"primary", "secret":"secret", "secondary_key":"read", "secondary_secret":"", "secondary_passphrase":"pass", "secondaryProxyUrl":"https://read"})).unwrap();
    let primary = Arc::new(Venue::default());
    let secondary = Arc::new(Venue::default());
    let factory = |options: &ExchangeOptions| {
        created.lock().unwrap().push(options.clone());
        Ok(if options.key == "read" {
            secondary.clone()
        } else {
            primary.clone()
        } as Arc<dyn RestAdapter>)
    };
    let clients = create_api_with_secondary_key(
        &options,
        &DirectTransport,
        Some("https://other"),
        factory,
        None,
        None,
    )
    .unwrap();
    run(clients.get(req("POST", "/balance"))).unwrap();
    assert_eq!(secondary.accepted.lock().unwrap()[0].method, "get");
    for method in ["POST", "DELETE", "HEAD", "GET"] {
        run(clients.request(req(method, "/orders"))).unwrap();
    }
    assert_eq!(secondary.accepted.lock().unwrap().len(), 1);
    assert_eq!(primary.accepted.lock().unwrap().len(), 4);
    let created = created.lock().unwrap();
    assert_eq!(created[2].key, "read");
    assert_eq!(created[2].secret, "");
    assert_eq!(
        created[2].override_proxy_url.as_deref(),
        Some("https://read")
    );
}
#[test]
fn limiter_counts_failed_requests_and_secondary_limits_are_independent() {
    let venue = Arc::new(Venue::default());
    let clock = Arc::new(HeldClock(Mutex::new(0)));
    let limited = RateLimitedAdapter::new(venue.clone(), clock.clone(), 2, 1000);
    venue.replies.lock().unwrap().push_back(Err(reset()));
    assert!(run(limited.execute(req("POST", "/orders"))).is_err());
    run(limited.execute(req("GET", "/positions"))).unwrap();
    assert_eq!(clock.now_ms(), 0);
    let mut balance = limited.execute(req("GET", "/balance"));
    assert!(
        balance
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    *clock.0.lock().unwrap() = 1000;
    run(balance).unwrap();
    assert_eq!(clock.now_ms(), 1000);
    // Exhaust the primary's current window before using the fresh secondary.
    run(limited.execute(req("GET", "/positions"))).unwrap();
    assert_eq!(clock.now_ms(), 1000);
    let secondary = RateLimitedAdapter::new(venue.clone(), clock.clone(), 2, 1000);
    run(secondary.execute(req("GET", "/positions"))).unwrap();
    assert_eq!(clock.now_ms(), 1000);
    assert_eq!(venue.accepted.lock().unwrap().len(), 5);
}
#[test]
fn polling_override_replaces_clamps_and_preserves_wildcard_fallthrough() {
    set_tick_interval_multipliers(Some(&BTreeMap::from([
        ("*".into(), 3.0),
        ("bybit".into(), 100.0),
        ("okx".into(), 0.1),
        ("binance".into(), f64::NAN),
    ])));
    assert_eq!(get_tick_interval_multiplier("bybit"), 60.0);
    assert_eq!(get_tick_interval("bybit", Some(2000.0), 1), Some(60_000.0));
    assert_eq!(get_tick_interval("bybit", Some(2000.0), 0), Some(120_000.0));
    assert_eq!(get_tick_interval("bybit", None, 1), None);
    assert_eq!(get_tick_interval("bybit", Some(0.0), 1), Some(0.0));
    assert_eq!(get_tick_interval_multiplier("okx"), 3.0);
    assert_eq!(get_tick_interval_multiplier("binance"), 3.0);
    assert_eq!(get_tick_interval_multipliers_for_tests().len(), 2);
    set_tick_interval_multipliers(Some(&BTreeMap::from([("bybit".into(), 2.0)])));
    assert_eq!(get_tick_interval_multiplier("okx"), 1.0);
    set_tick_interval_multipliers(None);
    assert_eq!(get_tick_interval_multiplier("bybit"), 1.0);
}

struct HeldClock(Mutex<u64>);
impl RateLimitClock for HeldClock {
    fn now_ms(&self) -> u64 {
        *self.0.lock().unwrap()
    }
    fn sleep_ms(&self, milliseconds: u64) -> TransportFuture<'_, ()> {
        let until = self.now_ms() + milliseconds;
        Box::pin(std::future::poll_fn(move |_| {
            if self.now_ms() >= until {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
}
#[test]
fn limiter_fifo_and_cancelled_waiters_do_not_consume_capacity() {
    let venue = Arc::new(Venue::default());
    let clock = Arc::new(HeldClock(Mutex::new(0)));
    let limited = RateLimitedAdapter::new(venue.clone(), clock.clone(), 1, 1000);
    run(limited.execute(req("GET", "/first"))).unwrap();
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut cancelled = limited.execute(req("GET", "/cancelled"));
    let mut second = limited.execute(req("GET", "/second"));
    let mut third = limited.execute(req("GET", "/third"));
    assert!(cancelled.as_mut().poll(&mut cx).is_pending());
    assert!(second.as_mut().poll(&mut cx).is_pending());
    assert!(third.as_mut().poll(&mut cx).is_pending());
    drop(cancelled);
    *clock.0.lock().unwrap() = 1000;
    assert!(third.as_mut().poll(&mut cx).is_pending());
    assert!(second.as_mut().poll(&mut cx).is_ready());
    assert!(third.as_mut().poll(&mut cx).is_pending());
    *clock.0.lock().unwrap() = 2000;
    assert!(third.as_mut().poll(&mut cx).is_ready());
    let accepted = venue.accepted.lock().unwrap();
    assert_eq!(
        accepted.iter().map(|r| r.url.as_str()).collect::<Vec<_>>(),
        ["/first", "/second", "/third"]
    );
}

#[test]
fn secondary_proxy_fallbacks_and_wrapper_calls_match_typescript() {
    for (secondary_proxy, other_proxy, expected) in [
        (
            Some("https://secondary"),
            Some("https://other"),
            Some("https://secondary"),
        ),
        (Some(""), Some("https://other"), Some("https://other")),
        (None, None, None),
    ] {
        let mut value = json!({"key":"primary", "secret":"secret", "secondary_key":"read", "overrideProxyUrl":"https://primary"});
        if let Some(proxy) = secondary_proxy {
            value["secondaryProxyUrl"] = proxy.into();
        }
        let options: ExchangeOptions = serde_json::from_value(value).unwrap();
        let captured = Mutex::new(vec![]);
        let counts = Mutex::new([0, 0]);
        let factory = |options: &ExchangeOptions| {
            captured.lock().unwrap().push(options.clone());
            Ok(Arc::new(Venue::default()) as Arc<dyn RestAdapter>)
        };
        let primary_limit = |client| {
            counts.lock().unwrap()[0] += 1;
            client
        };
        let secondary_limit = |client| {
            counts.lock().unwrap()[1] += 1;
            client
        };
        let clients = create_api_with_secondary_key(
            &options,
            &DirectTransport,
            other_proxy,
            factory,
            Some(&primary_limit),
            Some(&secondary_limit),
        )
        .unwrap();
        assert!(!clients.shares_primary_adapter());
        assert_eq!(*counts.lock().unwrap(), [1, 1]);
        let captured = captured.lock().unwrap();
        assert_eq!(
            captured[0].override_proxy_url.as_deref(),
            Some("https://primary")
        );
        assert_eq!(captured[1].override_proxy_url.as_deref(), expected);
    }
}
#[test]
fn tag_survives_a_retry_policy_and_signed_params_and_latency_reach_inner() {
    let venue = Arc::new(Venue::default());
    venue.replies.lock().unwrap().push_back(Err(reset()));
    let signer = signer(SignedConfig {
        params: Some(json!({"signature":"good"})),
        data: Some(retained(json!({"side":"Buy"}))),
        ..Default::default()
    });
    let time = Arc::new(Mutex::new(1000));
    let now = time.clone();
    let adapter = create_remote_signing_adapter(
        signer.clone(),
        ExchangeName::Bybit,
        None,
        Arc::new(|| "id".into()),
        Arc::new(move || {
            let mut now = now.lock().unwrap();
            *now += 17;
            *now
        }),
        venue.clone(),
    );
    let public = RestRequest {
        skip_remote_signing: true,
        headers: indexmap::IndexMap::from([("Local-Signature".into(), "good".into())]),
        ..req("GET", "/public")
    };
    let policy = network_error_on_read(read(ReadClassification::Methods, public), 1);
    run(policy.execute(&adapter, &RetryClock::default())).unwrap();
    assert!(signer.calls.lock().unwrap().is_empty());
    assert!(venue.accepted.lock().unwrap().iter().all(|request| {
        request
            .headers
            .get("Local-Signature")
            .is_some_and(|s| s == "good")
            && request.sign_ms.is_none()
    }));
    run(adapter.execute(req("POST", "/orders"))).unwrap();
    let accepted = venue.accepted.lock().unwrap();
    assert_eq!(accepted[2].params, Some(json!({"signature":"good"})));
    assert_eq!(
        accepted[2]
            .body
            .as_ref()
            .map(|body| body.wire_bytes().unwrap())
            .as_deref(),
        Some(b"{\"side\":\"Buy\"}".as_slice())
    );
    assert_eq!(accepted[2].sign_ms, Some(17));
}

#[test]
fn browser_signed_body_and_params_keep_wire_insertion_order() {
    let response: HttpSigningResponse = serde_json::from_str(
        r#"{"id":"id","config":{"data":{"symbol":"BTCUSDT","side":"Buy"},"params":{"symbol":"BTCUSDT","side":"Buy","qty":"1","timestamp":"123","signature":"signed-last"},"headers":{}}}"#,
    ).unwrap();
    let signer = Arc::new(Signer {
        calls: Mutex::new(vec![]),
        response: Mutex::new(Some(RemoteSignerResponse::Http(response))),
    });
    let venue = Arc::new(Venue::default());
    let adapter = create_remote_signing_adapter(
        signer,
        ExchangeName::Binance,
        None,
        Arc::new(|| "id".into()),
        Arc::new(|| 0),
        venue.clone(),
    );
    run(adapter.execute(req("POST", "/orders"))).unwrap();
    let accepted = venue.accepted.lock().unwrap();
    assert_eq!(
        accepted[0]
            .body
            .as_ref()
            .map(|body| body.wire_bytes().unwrap())
            .as_deref(),
        Some(br#"{"symbol":"BTCUSDT","side":"Buy"}"#.as_slice())
    );
    let params = accepted[0].params.as_ref().unwrap().as_object().unwrap();
    let query = params
        .iter()
        .map(|(key, value)| format!("{key}={}", value.as_str().unwrap()))
        .collect::<Vec<_>>()
        .join("&");
    assert_eq!(
        query,
        "symbol=BTCUSDT&side=Buy&qty=1&timestamp=123&signature=signed-last"
    );
}

fn asterdex_classifier() -> ReadClassification {
    ReadClassification::Custom(Arc::new(|request| {
        is_read_only_method(request)
            || request
                .url
                .split('?')
                .next()
                .unwrap_or("")
                .ends_with("/listenKey")
    }))
}
fn asterdex_condition(
    _request: &RestRequest,
    result: &Result<RestResponse, safe_cex_transport::RestFailure>,
) -> bool {
    result.as_ref().err().is_some_and(|error| {
        is_network_or_idempotent_request_error(error)
            || error
                .response
                .as_ref()
                .is_some_and(|response| matches!(response.status, 503 | 504))
    })
}

#[test]
fn asterdex_suffix_classification_and_method_dependent_5xx_retry() {
    for version in ["v1", "v3", "future-version"] {
        let path = format!("/fapi/{version}/listenKey?existing=1");
        for method in ["POST", "PUT", "DELETE"] {
            let _ = read(asterdex_classifier(), req(method, &path));
        }
    }
    for method in ["POST", "PUT", "DELETE", "OPTIONS"] {
        assert!(matches!(
            asterdex_classifier().classify(req(method, "/fapi/v3/order")),
            ClassifiedRequest::Write(_)
        ));
    }
    assert!(matches!(
        asterdex_classifier().classify(req("POST", "/fapi/v3/listenKey/extra")),
        ClassifiedRequest::Write(_)
    ));
    for (method, path, status, expected_attempts) in [
        ("GET", "/fapi/v3/balance", 500, 2),
        ("HEAD", "/fapi/v3/balance", 502, 2),
        ("POST", "/fapi/v3/listenKey", 500, 1),
        ("POST", "/fapi/v3/listenKey", 502, 1),
        ("POST", "/fapi/v3/listenKey", 503, 2),
        ("POST", "/fapi/v3/listenKey", 504, 2),
        ("GET", "/fapi/v3/balance", 429, 2),
    ] {
        let venue = Venue::default();
        venue
            .replies
            .lock()
            .unwrap()
            .push_back(Err(safe_cex_transport::RestFailure {
                code: None,
                transport_error: None,
                message: "status rejection".into(),
                response: Some(Box::new(RestResponse { status, ..ok() })),
                request: Box::new(req(&method.to_lowercase(), path)),
                context: None,
            }));
        let policy = only_reads(
            read(asterdex_classifier(), req(method, path)),
            1,
            asterdex_condition,
        );
        assert_eq!(
            run(policy.execute(&venue, &RetryClock::default())).is_ok(),
            expected_attempts > 1
        );
        assert_eq!(
            venue.accepted.lock().unwrap().len(),
            expected_attempts,
            "{method} {path} {status}"
        );
    }
}
#[derive(Default)]
struct RetryClock {
    now: Mutex<u64>,
    waits: Mutex<Vec<u64>>,
}
impl RateLimitClock for RetryClock {
    fn now_ms(&self) -> u64 {
        *self.now.lock().unwrap()
    }
    fn sleep_ms(&self, milliseconds: u64) -> TransportFuture<'_, ()> {
        Box::pin(async move {
            self.waits.lock().unwrap().push(milliseconds);
            *self.now.lock().unwrap() += milliseconds;
            Ok(())
        })
    }
}
struct TimedVenue {
    venue: Venue,
    clock: Arc<RetryClock>,
    sent_at: Mutex<Vec<u64>>,
}
impl RestAdapter for TimedVenue {
    fn execute(&self, request: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        self.sent_at.lock().unwrap().push(self.clock.now_ms());
        self.venue.execute(request)
    }
}
#[test]
fn asterdex_linear_delay_precedes_each_retry_and_stops_at_budget() {
    let clock = Arc::new(RetryClock::default());
    let venue = TimedVenue {
        venue: Venue::default(),
        clock: clock.clone(),
        sent_at: Mutex::new(vec![]),
    };
    venue.venue.replies.lock().unwrap().extend([
        Err(reset()),
        Err(reset()),
        Err(reset()),
        Err(reset()),
    ]);
    let policy = only_reads(
        read(asterdex_classifier(), req("POST", "/fapi/v3/listenKey")),
        3,
        asterdex_condition,
    )
    .with_delay(|retry_count| retry_count as u64 * 1000);
    assert!(run(policy.execute(&venue, clock.as_ref())).is_err());
    assert_eq!(*clock.waits.lock().unwrap(), [1000, 2000, 3000]);
    assert_eq!(*venue.sent_at.lock().unwrap(), [0, 1000, 3000, 6000]);
    assert_eq!(venue.venue.accepted.lock().unwrap().len(), 4);
    run(policy.execute(&venue, clock.as_ref())).unwrap();
    assert_eq!(
        *clock.waits.lock().unwrap(),
        [1000, 2000, 3000],
        "success must not sleep"
    );
}
#[test]
fn other_venues_default_to_no_delay() {
    let clock = Arc::new(RetryClock::default());
    let venue = TimedVenue {
        venue: Venue::default(),
        clock: clock.clone(),
        sent_at: Mutex::new(vec![]),
    };
    venue
        .venue
        .replies
        .lock()
        .unwrap()
        .extend([Err(reset()), Err(reset())]);
    let policy = network_error_on_read(
        read(ReadClassification::Methods, req("GET", "/positions")),
        3,
    );
    run(policy.execute(&venue, clock.as_ref())).unwrap();
    assert_eq!(*venue.sent_at.lock().unwrap(), [0, 0, 0]);
    assert_eq!(
        *clock.waits.lock().unwrap(),
        [0, 0],
        "zero-delay retry scheduler calls on the actual execution clock"
    );
}

#[test]
fn zero_delay_retry_waits_for_host_scheduler_before_next_attempt() {
    use std::cell::{Cell, RefCell};
    #[derive(Default)]
    struct Scheduler {
        released: Cell<bool>,
        calls: RefCell<Vec<u64>>,
    }
    impl RateLimitClock for Scheduler {
        fn now_ms(&self) -> u64 {
            1
        }
        fn sleep_ms(&self, delay: u64) -> TransportFuture<'_, ()> {
            self.calls.borrow_mut().push(delay);
            Box::pin(std::future::poll_fn(|_| {
                if self.released.get() {
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Pending
                }
            }))
        }
    }
    let clock = Scheduler::default();
    let venue = Venue::default();
    venue.replies.lock().unwrap().push_back(Err(reset()));
    let policy = network_error_on_read(
        read(ReadClassification::Methods, req("GET", "/positions")),
        1,
    );
    let mut future = std::pin::pin!(policy.execute(&venue, &clock));
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        future.as_mut().poll(&mut context).is_pending(),
        "retry waits for zero-delay host scheduler"
    );
    assert_eq!(
        venue.accepted.lock().unwrap().len(),
        1,
        "retry native adapter count before scheduler release"
    );
    assert_eq!(
        *clock.calls.borrow(),
        [0],
        "retry native scheduler call before release"
    );
    clock.released.set(true);
    let Poll::Ready(Ok(response)) = future.as_mut().poll(&mut context) else {
        panic!("retry resumes after scheduler release")
    };
    assert_eq!(
        response.status, 200,
        "retry native decoded status after scheduler release"
    );
    assert_eq!(
        venue.accepted.lock().unwrap().len(),
        2,
        "retry native adapter count after scheduler release"
    );
    assert_eq!(
        *clock.calls.borrow(),
        [0],
        "retry native scheduler count after success"
    );
}

#[test]
fn encoded_signing_body_is_rejected_before_signer_or_venue() {
    let venue = Arc::new(Venue::default());
    let signer = signer(SignedConfig::default());
    let adapter = create_remote_signing_adapter(
        signer.clone(),
        ExchangeName::Binance,
        None,
        Arc::new(|| "id".into()),
        Arc::new(|| 0),
        venue.clone(),
    );
    let error = run(adapter.execute(RestRequest {
        body: Some(safe_cex_transport::RestBody::Encoded(vec![
            123, 34, 97, 34, 58, 255, 254, 125,
        ])),
        ..req("POST", "/orders")
    }))
    .unwrap_err();
    assert!(
        matches!(error.transport_error.as_deref(), Some(TransportError::Host(message)) if message == "Signing requires untransformed request data")
    );
    assert!(signer.calls.lock().unwrap().is_empty());
    assert!(venue.accepted.lock().unwrap().is_empty());
}

#[test]
fn direct_http_reason_is_optional_and_survives_adapter_return() {
    let venue = Venue::default();
    venue.replies.lock().unwrap().push_back(Ok(RestResponse {
        status: 500,
        reason: Some("Internal Server Error".into()),
        ..ok()
    }));
    let response = run(venue.execute(req("GET", "/data"))).unwrap();
    assert_eq!(response.reason.as_deref(), Some("Internal Server Error"));
    assert_eq!(ok().reason, None);
}

#[test]
fn body_wire_transform_uses_js_numeric_spelling_and_property_order() {
    use safe_cex_transport::RestBody;
    let raw: serde_json::Value =
        serde_json::from_str(r#"{"z":1e-7,"10":2,"2":10001.0,"a":1e400}"#).unwrap();
    assert_eq!(
        RestBody::Untransformed(retained(raw)).wire_bytes().unwrap(),
        br#"{"2":10001,"10":2,"z":1e-7,"a":null}"#,
        "request transformation must use canonical JS number and own-key semantics"
    );
}

#[test]
fn skip_remote_signing_still_transforms_raw_data_without_inferencing_encoded_origin() {
    let venue = Arc::new(Venue::default());
    let signer = signer(SignedConfig::default());
    let adapter = create_remote_signing_adapter(
        signer.clone(),
        ExchangeName::Binance,
        None,
        Arc::new(|| "id".into()),
        Arc::new(|| 0),
        venue.clone(),
    );
    for body in [
        safe_cex_transport::RestBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"x":1})).unwrap(),
        ),
        safe_cex_transport::RestBody::Encoded(vec![255]),
    ] {
        let expected = body.transform(None).unwrap();
        run(adapter.execute(RestRequest {
            body: Some(body),
            skip_remote_signing: true,
            ..req("POST", "/public")
        }))
        .unwrap();
        assert_eq!(
            venue.accepted.lock().unwrap().last().unwrap().body,
            Some(expected)
        );
    }
    assert!(signer.calls.lock().unwrap().is_empty());
}

#[test]
fn ws_signer_union_preserves_bybit_envelope_and_correlated_error_kind() {
    let source = r#"{"type":"ws-auth","id":"auth-1","exchangeName":"bybit","accountName":"desk","wsAuth":{"toSign":"GET/realtime1234","outputFormat":"hex","extraData":{"expires":1234,"overflow":1e400,"nullable":null}}}"#;
    let request: RemoteSignerRequest = serde_json::from_str(source).unwrap();
    let via_value: RemoteSignerRequest =
        serde_json::from_value(serde_json::from_str::<serde_json::Value>(source).unwrap()).unwrap();
    assert!(matches!(via_value, RemoteSignerRequest::WsAuth { .. }));
    assert!(
        serde_json::from_str::<RemoteSignerRequest>(
            r#"{"type":"future-kind","exchangeName":"bybit"}"#
        )
        .is_err()
    );

    let RemoteSignerRequest::WsAuth {
        request: payload, ..
    } = &request
    else {
        panic!("WS auth must never decode as HTTP signing");
    };
    for decoded in [&request, &via_value] {
        let RemoteSignerRequest::WsAuth {
            exchange_name,
            account_name,
            request: native,
        } = decoded
        else {
            panic!("native WS variant identity");
        };
        assert_eq!(
            *exchange_name,
            ExchangeName::Bybit,
            "native WS exchange identity"
        );
        assert_eq!(
            account_name.as_deref(),
            Some("desk"),
            "native WS account identity"
        );
        assert_eq!(
            native.id,
            Some(json!("auth-1")),
            "native WS request id identity"
        );
        let auth = native.ws_auth.as_ref().unwrap();
        assert_eq!(
            auth.to_sign.as_deref(),
            Some("GET/realtime1234"),
            "native WS toSign identity"
        );
        assert_eq!(
            auth.output_format,
            Some(json!("hex")),
            "native WS outputFormat identity"
        );
    }
    // This is the same payload type consumed by local crypto, not a transport copy.
    let _: &safe_cex_types::ws_auth::WsAuthRequest = payload;
    let serialized = serde_json::to_string(&request).unwrap();
    assert!(serialized.contains("\"type\":\"ws-auth\""));
    assert!(
        serialized
            .contains("\"extraData\":{\"expires\":1234,\"overflow\":1e400,\"nullable\":null}"),
        "remote envelope must preserve canonical WS metadata without a Value projection"
    );
    assert!(
        !serialized.contains("config"),
        "WS requests must not pretend to be HTTP"
    );
    let response = request
        .decode_response(r#"{"id":"auth-1","wsAuthSignature":"fixture"}"#)
        .unwrap();
    let RemoteSignerResponse::WsAuth(native) = &response else {
        panic!("native WS response variant");
    };
    assert_eq!(
        native.id,
        Some(json!("auth-1")),
        "native WS response id identity"
    );
    assert_eq!(
        native.ws_auth_signature.as_deref(),
        Some("fixture"),
        "native WS signature identity"
    );
    assert!(native.error.is_none(), "native WS error identity");
    assert_eq!(
        serde_json::to_string(&response).unwrap(),
        r#"{"id":"auth-1","wsAuthSignature":"fixture"}"#
    );
    assert!(
        matches!(
            request
                .decode_response(r#"{"id":"auth-1","error":"unsupported"}"#)
                .unwrap(),
            RemoteSignerResponse::WsAuth(_)
        ),
        "error-only replies must retain the originating operation"
    );
    let nested = format!(
        r#"{{"type":"ws-auth","exchangeName":"bybit","wsAuth":{{"toSign":"x","extraData":{}0{}}}}}"#,
        "[".repeat(200),
        "]".repeat(200)
    );
    let nested: RemoteSignerRequest = serde_json::from_str(&nested).unwrap();
    assert!(
        serde_json::to_string(&nested)
            .unwrap()
            .contains(&"[".repeat(200))
    );
}

#[test]
fn http_adapter_refuses_ws_response_before_transmission() {
    let venue = Arc::new(Venue::default());
    let signer = signer(SignedConfig::default());
    *signer.response.lock().unwrap() = Some(RemoteSignerResponse::WsAuth(
        safe_cex_types::ws_auth::WsAuthResponse {
            id: Some(json!("id")),
            ws_auth_signature: Some("signature".into()),
            error: None,
        },
    ));
    let adapter = create_remote_signing_adapter(
        signer,
        ExchangeName::Bybit,
        None,
        Arc::new(|| "id".into()),
        Arc::new(|| 0),
        venue.clone(),
    );
    let result = run(adapter.execute(req("POST", "/order")));
    assert!(
        venue.accepted.lock().unwrap().is_empty(),
        "wrong signer variant must never transmit"
    );
    let failure = result.unwrap_err();
    assert_eq!(
        failure.message,
        "Remote signer returned WS auth for an HTTP request"
    );
}

#[test]
fn signer_union_http_records_refuse_positional_arrays() {
    assert!(serde_json::from_str::<HttpSigningRequest>(r#"["id","bybit",null,{}]"#).is_err());
    assert!(serde_json::from_str::<HttpSigningResponse>(r#"["id",null,null]"#).is_err());
    assert!(serde_json::from_str::<SigningConfig>(r#"["GET","/","",null,null,{}]"#).is_err());
    assert!(serde_json::from_str::<SignedConfig>(r#"[null,null,null,{}]"#).is_err());
}

#[test]
fn remote_http_union_keeps_explicit_null_distinct_from_signed_omission() {
    let response: HttpSigningResponse =
        serde_json::from_str(r#"{"id":"id","config":{"data":null,"params":null,"headers":{}}}"#)
            .unwrap();
    assert_eq!(
        response.config.as_ref().unwrap().data,
        Some(retained(json!(null))),
        "signed envelope must retain explicit null before transformation"
    );
    let signer = Arc::new(Signer {
        calls: Mutex::new(vec![]),
        response: Mutex::new(Some(RemoteSignerResponse::Http(response))),
    });
    let wire = Arc::new(Venue::default());
    let adapter = create_remote_signing_adapter(
        signer,
        ExchangeName::Binance,
        None,
        Arc::new(|| "id".into()),
        Arc::new(|| 0),
        wire.clone(),
    );
    run(adapter.execute(req("POST", "/order"))).unwrap();
    let sent = wire.accepted.lock().unwrap();
    assert_eq!(
        sent[0].body.as_ref().unwrap().untransformed().unwrap(),
        &retained(json!(null)),
        "explicit signed null reaches the inner adapter as null, not omission"
    );
    assert_eq!(sent[0].params, Some(serde_json::Value::Null));
}

#[test]
fn remote_signed_data_is_authoritative_at_inner_adapter_boundary() {
    // The live whole-adapter oracle proves no post-return transform occurs.
    for data in ["raw text", "", "  {\"x\":1} \n", "\u{feff}{\"x\":1}"] {
        let venue = Arc::new(Venue::default());
        let signer = signer(SignedConfig {
            data: Some(retained(json!(data))),
            headers: indexmap::IndexMap::from([("Content-Type".into(), "application/json".into())]),
            ..Default::default()
        });
        let adapter = create_remote_signing_adapter(
            signer.clone(),
            ExchangeName::Binance,
            None,
            Arc::new(|| "id".into()),
            Arc::new(|| 0),
            venue.clone(),
        );
        let mut config = signing_config();
        config.data = Some(retained(json!("unsigned raw text")));
        run(adapter.execute_signed(SigningRequest::new(config))).unwrap();
        assert_eq!(
            venue.accepted.lock().unwrap()[0]
                .body
                .as_ref()
                .unwrap()
                .untransformed()
                .unwrap(),
            &retained(json!(data)),
            "remote returned data must reach inner adapter without another transform"
        );
        let capture = serde_json::to_value(&signer.calls.lock().unwrap()[0]).unwrap();
        assert_eq!(
            capture["config"]["data"],
            json!("unsigned raw text"),
            "server non-JSON string remains raw before remote signing"
        );
    }
}

#[derive(Default)]
struct RetryTransformWire(Mutex<Vec<RestRequest>>);
impl RestAdapter for RetryTransformWire {
    fn execute(&self, request: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.0.lock().unwrap().push(request.clone());
            Err(safe_cex_transport::RestFailure::from_transport(
                TransportError::Host("wire refused".into()),
                request,
            ))
        })
    }
}
#[test]
fn remote_retry_keeps_transform_phase_and_signed_failure_headers() {
    let wire = Arc::new(RetryTransformWire::default());
    let signer = signer(SignedConfig {
        data: Some(retained(json!("raw text"))),
        headers: [
            ("Content-Type".into(), "application/json".into()),
            ("X-Time".into(), "first".into()),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    });
    let adapter = create_remote_signing_adapter(
        signer.clone(),
        ExchangeName::Bybit,
        None,
        Arc::new(|| "id".into()),
        Arc::new(|| 0),
        wire.clone(),
    );
    let first = run(adapter.execute(RestRequest {
        body: Some(safe_cex_transport::RestBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(json!("raw text"))
                .unwrap(),
        )),
        ..req("GET", "/data")
    }))
    .unwrap_err();
    assert_eq!(
        first
            .request
            .body
            .as_ref()
            .unwrap()
            .untransformed()
            .unwrap(),
        &retained(json!("raw text")),
        "remote failure must retain actual returned adapter data"
    );
    *signer.response.lock().unwrap() = Some(RemoteSignerResponse::Http(HttpSigningResponse {
        config: Some(SignedConfig {
            data: Some(retained(json!("replacement raw text"))),
            headers: first.request.headers.clone(),
            ..Default::default()
        }),
        ..Default::default()
    }));
    run(adapter.execute(*first.request)).unwrap_err();
    let calls = signer.calls.lock().unwrap();
    let RemoteSignerRequest::Http(second) = &calls[1] else {
        panic!("HTTP")
    };
    assert_eq!(
        second.config.data,
        Some(retained(json!("raw text"))),
        "remote retry signs actual returned adapter data"
    );
    assert_eq!(second.config.headers["X-Time"], "first");
    let sent = wire.0.lock().unwrap();
    assert_eq!(
        sent[0].body.as_ref().unwrap().wire_bytes().unwrap(),
        b"raw text"
    );
    assert_eq!(
        sent[1].body.as_ref().unwrap().wire_bytes().unwrap(),
        b"replacement raw text",
        "remote retry must retain identity transform"
    );
}
#[test]
fn skipped_remote_signer_preserves_retry_transform_provenance() {
    let wire = Arc::new(RetryTransformWire::default());
    let signer = signer(SignedConfig::default());
    let adapter = create_remote_signing_adapter(
        signer.clone(),
        ExchangeName::Bybit,
        None,
        Arc::new(|| "id".into()),
        Arc::new(|| 0),
        wire,
    );
    let failure = run(adapter.execute(RestRequest {
        body: Some(safe_cex_transport::RestBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(json!("raw text"))
                .unwrap(),
        )),
        headers: [("Content-Type".into(), "application/json".into())]
            .into_iter()
            .collect(),
        skip_remote_signing: true,
        ..req("GET", "/data")
    }))
    .unwrap_err();
    assert_eq!(
        failure
            .request
            .body
            .as_ref()
            .unwrap()
            .untransformed()
            .unwrap(),
        &retained(json!("\"raw text\"")),
        "skip-signing failure must retain transform provenance"
    );
    assert!(signer.calls.lock().unwrap().is_empty());
}

#[test]
fn remote_server_headers_generate_before_client_signing_and_preserve_null_retry_data() {
    for data in [json!(null), json!({"x":1}), json!([1])] {
        let wire = Arc::new(RetryTransformWire::default());
        let signer = signer(SignedConfig {
            data: Some(retained(data.clone())),
            headers: [
                ("apikey".into(), "old".into()),
                ("ApiKey".into(), "signed".into()),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        });
        let adapter = create_remote_signing_adapter(
            signer.clone(),
            ExchangeName::Bybit,
            None,
            Arc::new(|| "id".into()),
            Arc::new(|| 0),
            wire.clone(),
        );
        let failure = run(adapter.execute(RestRequest {
            body: Some(safe_cex_transport::RestBody::Untransformed(retained(
                data.clone(),
            ))),
            headers: [
                ("apikey".into(), "old".into()),
                ("ApiKey".into(), "input".into()),
            ]
            .into_iter()
            .collect(),
            ..req("POST", "/data")
        }))
        .unwrap_err();
        let calls = signer.calls.lock().unwrap();
        let RemoteSignerRequest::Http(call) = &calls[0] else {
            panic!("HTTP")
        };
        assert_eq!(
            call.config.headers.len(),
            2,
            "remote signer headers include server-generated content type before client signing"
        );
        assert_eq!(call.config.headers["apikey"], "input");
        let sent = wire.0.lock().unwrap();
        assert_eq!(
            sent[0].headers.len(),
            1,
            "remote signer output must normalize before transmission"
        );
        assert_eq!(sent[0].headers["apikey"], "signed");
        assert!(
            !sent[0].headers.contains_key("Content-Type"),
            "remote returned header deletion must survive"
        );
        assert_eq!(
            sent[0].body.as_ref().unwrap().untransformed().unwrap(),
            &retained(data.clone()),
            "remote returned object/null identity must survive"
        );
        if data.is_null() {
            assert_eq!(
                failure
                    .request
                    .body
                    .as_ref()
                    .unwrap()
                    .untransformed()
                    .unwrap(),
                &retained(json!(null)),
                "null retry data must retain null rather than string origin"
            );
        }
    }
}

#[test]
fn serialize_headers_retains_plain_object_case_duplicates() {
    let headers = [
        ("apikey".into(), "old".into()),
        ("ApiKey".into(), "new".into()),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        serialize_headers(&headers),
        headers,
        "plain-object serialization must not acquire AxiosHeaders normalization"
    );
}

#[test]
fn remote_signer_wire_config_uses_exact_base_url_protocol_spelling() {
    let config = SigningConfig {
        method: "GET".into(),
        url: "/account".into(),
        base_url: "https://venue.example".into(),
        ..Default::default()
    };
    let wire = serde_json::to_value(RemoteSignerRequest::Http(HttpSigningRequest {
        id: "fixture".into(),
        exchange_name: ExchangeName::Kucoin,
        account_name: None,
        config,
    }))
    .unwrap();
    assert_eq!(
        wire["config"]["baseURL"], "https://venue.example",
        "remote signer must receive baseURL with exact protocol spelling"
    );
    assert!(
        wire["config"].get("baseUrl").is_none(),
        "remote signer must not receive the camelCase typo"
    );
    let source: SigningConfig = serde_json::from_value(
        json!({"method":"GET","url":"/account","baseURL":"https://source.example","headers":{}}),
    )
    .unwrap();
    assert_eq!(
        source.base_url, "https://source.example",
        "remote signer decoder must accept the source baseURL field"
    );
}

#[test]
fn http_signer_decoding_preserves_native_field_identity() {
    let source = r#"{"type":"http","id":"request-id","exchangeName":"bybit","accountName":"desk-name","config":{"method":"post","url":"/order","baseURL":"https://fixture","params":{"query":"param-value"},"data":{"body":"data-value"},"headers":{"key-header":"key-value","sign-header":"sign-value"}}}"#;
    let request: RemoteSignerRequest = serde_json::from_str(source).unwrap();
    let RemoteSignerRequest::Http(native) = &request else {
        panic!("native HTTP request variant");
    };
    assert_eq!(native.id, "request-id", "native HTTP id identity");
    assert_eq!(native.exchange_name, ExchangeName::Bybit);
    assert_eq!(native.account_name.as_deref(), Some("desk-name"));
    assert_eq!(native.config.method, "post", "native HTTP method identity");
    assert_eq!(native.config.url, "/order", "native HTTP URL identity");
    assert_eq!(
        native.config.base_url, "https://fixture",
        "native HTTP baseURL identity"
    );
    assert_eq!(
        native.config.params,
        Some(json!({"query":"param-value"})),
        "native HTTP params identity"
    );
    assert_eq!(
        native.config.data,
        Some(retained(json!({"body":"data-value"}))),
        "native HTTP data identity"
    );
    assert_eq!(native.config.headers["key-header"], "key-value");
    assert_eq!(native.config.headers["sign-header"], "sign-value");
    let response = request.decode_response(r#"{"id":"response-id","config":{"url":"/signed","params":{"query":"signed-param"},"data":{"body":"signed-data"},"headers":{"key-header":"signed-key","sign-header":"signature"}},"error":"response-error"}"#).unwrap();
    let RemoteSignerResponse::Http(native) = response else {
        panic!("native HTTP response variant");
    };
    assert_eq!(native.id, "response-id", "native HTTP response id identity");
    assert_eq!(
        native.error.as_deref(),
        Some("response-error"),
        "native HTTP response error identity"
    );
    let config = native.config.unwrap();
    assert_eq!(config.url.as_deref(), Some("/signed"));
    assert_eq!(
        config.params,
        Some(json!({"query":"signed-param"})),
        "native signed params identity"
    );
    assert_eq!(
        config.data,
        Some(retained(json!({"body":"signed-data"}))),
        "native signed data identity"
    );
    assert_eq!(config.headers["key-header"], "signed-key");
    assert_eq!(config.headers["sign-header"], "signature");
}

struct ReceiptOnlyVenue {
    calls: Mutex<usize>,
    requests: Mutex<Vec<RestRequest>>,
    fail_first: bool,
}
impl RestAdapter for ReceiptOnlyVenue {
    fn execute(&self, _: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        panic!("middleware must preserve an effective config receipt")
    }
    fn execute_with_receipt(
        &self,
        mut request: RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestExecution> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request.clone());
            let call = {
                let mut calls = self.calls.lock().unwrap();
                *calls += 1;
                *calls
            };
            if self.fail_first && call == 1 {
                let mut error = reset();
                request.url = "/retry-returned".into();
                error.request = Box::new(request);
                return Err(error);
            }
            request.url = "/effective-returned".into();
            request.headers.clear();
            request
                .headers
                .insert("Receipt-Only".into(), "returned".into());
            request.body = None;
            Ok(safe_cex_transport::RestExecution {
                response: ok(),
                config: request,
            })
        })
    }
}
#[test]
fn limiter_preserves_inner_effective_receipt_and_admission() {
    let venue = Arc::new(ReceiptOnlyVenue {
        calls: Mutex::new(0),
        requests: Mutex::new(Vec::new()),
        fail_first: false,
    });
    let clock = Arc::new(HeldClock(Mutex::new(0)));
    let limiter = RateLimitedAdapter::new(venue.clone(), clock.clone(), 1, 1000);
    for expected_time in [0, 1000] {
        let mut request = limiter.execute_with_receipt(req("GET", "/input"));
        if expected_time > 0 {
            assert!(
                request
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            *clock.0.lock().unwrap() = expected_time;
        }
        let execution = run(request).unwrap();
        assert_eq!(
            execution.config.url, "/effective-returned",
            "receipt_limiter_native_config"
        );
        assert_eq!(
            clock.now_ms(),
            expected_time,
            "receipt_limiter_native_admission"
        );
    }
    assert_eq!(
        *venue.calls.lock().unwrap(),
        2,
        "receipt_limiter_native_count"
    );
}
#[test]
fn retry_preserves_final_effective_config_after_failed_config_reentry() {
    let venue = ReceiptOnlyVenue {
        calls: Mutex::new(0),
        requests: Mutex::new(Vec::new()),
        fail_first: true,
    };
    let execution = run(network_error_on_read(
        read(ReadClassification::Methods, req("GET", "/input")),
        1,
    )
    .execute_with_receipt(&venue, &RetryClock::default()))
    .unwrap();
    assert_eq!(
        *venue.calls.lock().unwrap(),
        2,
        "receipt_retry_native_count"
    );
    let requests = venue.requests.lock().unwrap();
    assert_eq!(
        requests[0].url, "/input",
        "receipt_retry_native_original_config"
    );
    assert_eq!(
        requests[1].url, "/retry-returned",
        "receipt_retry_native_reentry_config"
    );
    assert_eq!(
        execution.config.url, "/effective-returned",
        "receipt_retry_native_final_config"
    );
    assert_eq!(
        execution
            .config
            .headers
            .get("Receipt-Only")
            .map(String::as_str),
        Some("returned"),
        "receipt_retry_native_final_headers"
    );
    assert!(
        execution.config.body.is_none(),
        "receipt_retry_native_body_omission"
    );
}
#[test]
fn remote_signing_signed_and_bypass_paths_preserve_inner_receipt() {
    for skip in [false, true] {
        let venue = Arc::new(ReceiptOnlyVenue {
            calls: Mutex::new(0),
            requests: Mutex::new(Vec::new()),
            fail_first: false,
        });
        let signer = signer(SignedConfig {
            url: Some("/signed-before-terminal".into()),
            headers: [("Signed-Only".into(), "signed".into())]
                .into_iter()
                .collect(),
            ..Default::default()
        });
        let adapter = create_remote_signing_adapter(
            signer.clone(),
            ExchangeName::Bybit,
            None,
            Arc::new(|| "receipt-id".into()),
            Arc::new(|| 0),
            venue.clone(),
        );
        let execution = run(adapter.execute_with_receipt(RestRequest {
            method: "POST".into(),
            url: "/remote-input".into(),
            skip_remote_signing: skip,
            config_extensions: [
                (
                    "grvtCookie".into(),
                    safe_cex_types::retained_json::RetainedJsonInput::Text(
                        "gravity=retained".encode_utf16().collect(),
                    ),
                ),
                (
                    "nativeConfig".into(),
                    safe_cex_types::retained_json::RetainedJsonInput::Object(vec![
                        (
                            "missing".encode_utf16().collect(),
                            safe_cex_types::retained_json::RetainedJsonInput::Undefined,
                        ),
                        (
                            "nan".encode_utf16().collect(),
                            safe_cex_types::retained_json::RetainedJsonInput::Float(
                                f64::from_bits(0x7ff8000000000042),
                            ),
                        ),
                        (
                            "negativeZero".encode_utf16().collect(),
                            safe_cex_types::retained_json::RetainedJsonInput::Float(-0.0),
                        ),
                    ]),
                ),
            ]
            .into(),
            body: Some(safe_cex_transport::RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"input":79}))
                    .unwrap(),
            )),
            ..Default::default()
        }))
        .unwrap();
        assert_eq!(
            *venue.calls.lock().unwrap(),
            1,
            "receipt_remote_native_count {skip}"
        );
        assert_eq!(
            venue.requests.lock().unwrap()[0].url,
            if skip {
                "/remote-input"
            } else {
                "/signed-before-terminal"
            },
            "receipt_remote_native_signed_config {skip}"
        );
        assert_eq!(
            venue.requests.lock().unwrap()[0].config_extensions["grvtCookie"]
                .clone()
                .into_value()
                .unwrap(),
            json!("gravity=retained"),
            "REST CONFIG signed/bypass terminal extensions {skip}"
        );
        {
            use safe_cex_types::retained_json::RetainedJsonInput as Input;
            let requests = venue.requests.lock().unwrap();
            let Input::Object(fields) = &requests[0].config_extensions["nativeConfig"] else {
                panic!("native config object");
            };
            assert_eq!(fields.len(), 3, "hosted config retains every data field");
            assert!(
                matches!(&fields[0], (key, Input::Undefined) if key == &"missing".encode_utf16().collect::<Vec<_>>())
            );
            assert!(
                matches!(&fields[1], (key, Input::Float(value)) if key == &"nan".encode_utf16().collect::<Vec<_>>() && value.to_bits() == 0x7ff8000000000042)
            );
            assert!(
                matches!(&fields[2], (key, Input::Float(value)) if key == &"negativeZero".encode_utf16().collect::<Vec<_>>() && value.to_bits() == (-0.0_f64).to_bits())
            );
        }
        assert_eq!(
            signer.calls.lock().unwrap().len(),
            usize::from(!skip),
            "receipt_remote_native_signer_count {skip}"
        );
        assert_eq!(
            execution.config.url, "/effective-returned",
            "receipt_remote_native_config {skip}"
        );
        assert_eq!(
            execution.config.headers.len(),
            1,
            "receipt_remote_native_header_omission {skip}"
        );
        assert_eq!(
            execution
                .config
                .headers
                .get("Receipt-Only")
                .map(String::as_str),
            Some("returned"),
            "receipt_remote_native_headers {skip}"
        );
        assert!(
            execution.config.body.is_none(),
            "receipt_remote_native_body_omission {skip}"
        );
    }
}
struct NativeReceiptSigner(Mutex<Vec<safe_cex_transport::query_signing::HttpSigningRequest>>);
impl RemoteSigner for NativeReceiptSigner {
    fn sign(&self, _: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        panic!("native profile must use the native signer")
    }
    fn sign_query(
        &self,
        request: safe_cex_transport::query_signing::HttpSigningRequest,
    ) -> TransportFuture<'_, safe_cex_transport::query_signing::HttpSigningResponse> {
        self.0.lock().unwrap().push(request);
        Box::pin(async {
            Ok(safe_cex_transport::query_signing::HttpSigningResponse {
                id: "native-receipt-id".into(),
                config: Some(safe_cex_transport::query_signing::SignedConfig {
                    url: Some("/native-signed-before-terminal".into()),
                    ..Default::default()
                }),
                error: None,
            })
        })
    }
}
#[test]
fn remote_native_query_path_preserves_inner_effective_config() {
    let venue = Arc::new(ReceiptOnlyVenue {
        calls: Mutex::new(0),
        requests: Mutex::new(Vec::new()),
        fail_first: false,
    });
    let signer = Arc::new(NativeReceiptSigner(Mutex::new(Vec::new())));
    let adapter = create_remote_signing_adapter(
        signer.clone(),
        ExchangeName::Bybit,
        None,
        Arc::new(|| "native-receipt-id".into()),
        Arc::new(|| 0),
        venue.clone(),
    );
    let execution = run(adapter.execute_with_receipt(RestRequest {
        method: "GET".into(),
        url: "/native-input".into(),
        query_params: Some(safe_cex_types::QueryValue::Object(Default::default())),
        query_profile: Some(safe_cex_transport::query::QueryProfile {
            key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
            arrays: safe_cex_transport::query::QueryArrayFormat::Indexed,
            encode: true,
        }),
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        *venue.calls.lock().unwrap(),
        1,
        "receipt_native_query_native_count"
    );
    assert_eq!(
        signer.0.lock().unwrap().len(),
        1,
        "receipt_native_query_native_signer_count"
    );
    assert_eq!(
        signer.0.lock().unwrap()[0].config.url,
        "/native-input",
        "receipt_native_query_native_signer_input"
    );
    assert_eq!(
        venue.requests.lock().unwrap()[0].url,
        "/native-signed-before-terminal",
        "receipt_native_query_native_signed_target"
    );
    assert_eq!(
        execution.config.url, "/effective-returned",
        "receipt_native_query_native_config"
    );
    assert!(
        execution.config.body.is_none(),
        "receipt_native_query_native_body_omission"
    );
}

#[test]
fn zero_delay_retry_invokes_scheduler_for_each_retried_attempt() {
    let venue = Venue::default();
    venue
        .replies
        .lock()
        .unwrap()
        .extend([Err(reset()), Err(reset())]);
    let clock = RetryClock {
        now: Mutex::new(1),
        waits: Mutex::new(Vec::new()),
    };
    let response = run(network_error_on_read(
        read(
            ReadClassification::Methods,
            req("GET", "/native-zero-delay"),
        ),
        3,
    )
    .execute(&venue, &clock))
    .unwrap();
    assert_eq!(
        response.status, 200,
        "retry native decoded successful status"
    );
    assert_eq!(
        venue.accepted.lock().unwrap().len(),
        3,
        "retry native independent adapter calls"
    );
    assert_eq!(
        *clock.waits.lock().unwrap(),
        [0, 0],
        "retry native zero-delay scheduler calls"
    );
}

#[test]
fn signing_rate_slots_release_cancelled_waiters_without_spending_capacity() {
    let clock = Arc::new(HeldClock(Mutex::new(0)));
    let limiter = RateLimiter::new(clock.clone(), 1, 1000);
    drop(limiter.acquire()); // An unpolled reservation has no queue side effects.
    run(limiter.acquire()).unwrap();
    let mut cx = Context::from_waker(Waker::noop());
    let mut cancelled = Box::pin(limiter.acquire());
    let mut next = Box::pin(limiter.acquire());
    assert!(cancelled.as_mut().poll(&mut cx).is_pending());
    assert!(next.as_mut().poll(&mut cx).is_pending());
    drop(cancelled);
    *clock.0.lock().unwrap() = 1000;
    assert!(matches!(next.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
    let mut later = Box::pin(limiter.acquire());
    assert!(later.as_mut().poll(&mut cx).is_pending());
    *clock.0.lock().unwrap() = 2000;
    assert!(matches!(later.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
}

fn retained(value: Value) -> safe_cex_types::retained_json::RetainedJsonInput {
    safe_cex_types::retained_json::RetainedJsonInput::from_value(value).unwrap()
}
fn try_parse_json(value: Value) -> Value {
    safe_cex_transport::RestResponseBody::Transformed(safe_cex_transport::signing::try_parse_json(
        retained(value),
    ))
    .json_value()
    .unwrap()
}

#[test]
fn limiter_resets_from_timer_completion_with_frozen_now_and_idle_windows() {
    struct Clock {
        elapsed: Mutex<u64>,
    }
    impl RateLimitClock for Clock {
        fn now_ms(&self) -> u64 {
            0
        }
        fn sleep_ms(&self, ms: u64) -> TransportFuture<'_, ()> {
            let deadline = *self.elapsed.lock().unwrap() + ms;
            Box::pin(std::future::poll_fn(move |_| {
                if *self.elapsed.lock().unwrap() >= deadline {
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Pending
                }
            }))
        }
    }
    let clock = Arc::new(Clock {
        elapsed: Mutex::new(0),
    });
    let limiter = RateLimiter::new(clock.clone(), 3, 1000);
    for _ in 0..3 {
        run(limiter.acquire()).unwrap();
    }
    let mut cancelled = Box::pin(limiter.acquire());
    let mut pending = Box::pin(limiter.acquire());
    let mut cx = Context::from_waker(Waker::noop());
    assert!(cancelled.as_mut().poll(&mut cx).is_pending());
    assert!(pending.as_mut().poll(&mut cx).is_pending());
    drop(cancelled);
    *clock.elapsed.lock().unwrap() = 1000;
    run(pending).unwrap();
    run(limiter.acquire()).unwrap();
    run(limiter.acquire()).unwrap();
    let mut blocked = Box::pin(limiter.acquire());
    assert!(blocked.as_mut().poll(&mut cx).is_pending());
    drop(blocked);
    *clock.elapsed.lock().unwrap() = 4000;
    for _ in 0..3 {
        run(limiter.acquire()).unwrap();
    }
    assert_eq!(
        clock.now_ms(),
        0,
        "timer completion does not advance Date/performance"
    );
}
