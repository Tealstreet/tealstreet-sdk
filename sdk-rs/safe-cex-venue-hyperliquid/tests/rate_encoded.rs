use safe_cex_transport::{
    RestAdapter, RestBody, RestFuture, RestRequest, RestResponse, RestResponseBody,
    TransportFuture, rate_limit::RateLimitClock,
};
use safe_cex_venue_hyperliquid::{api::create_info_api, rate_limit::RateLimiter};
use serde_json::{Value, json};
use std::{
    future::Future,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll, Waker},
};
#[derive(Default)]
struct Clock {
    now: AtomicU64,
    sleeps: Mutex<Vec<u64>>,
}
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
    fn sleep_ms(&self, ms: u64) -> TransportFuture<'_, ()> {
        Box::pin(async move {
            self.sleeps.lock().unwrap().push(ms);
            self.now.fetch_add(ms, Ordering::SeqCst);
            Ok(())
        })
    }
}
struct NoSend;
impl RestAdapter for NoSend {
    fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
        panic!("ENCODED no transport during prepare/response observation")
    }
}
fn ready<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("ENCODED manual clock must finish"),
    }
}
fn client(limiter: Rc<RateLimiter>) -> safe_cex_venue_hyperliquid::api::ApiClient {
    create_info_api(
        serde_json::from_value(json!({"key":"","secret":"","publicDataOnly":true})).unwrap(),
        Arc::new(NoSend),
        limiter,
        Arc::new(|_| {}),
    )
}
fn encoded_rate_classifier_parses_once_at_the_actual_prepare_callsite() {
    let role = r#"{"type":"userRole"}"#;
    let batch = json!({"action":{"type":"order","orders":vec![Value::Null;40]}}).to_string();
    let inputs = [
        role.to_owned(),
        serde_json::to_string(role).unwrap(),
        serde_json::to_string(&batch).unwrap(),
        r#"{"type":"userRole","unused":"\ud800"}"#.into(),
        format!(
            "{{\"type\":\"userRole\",\"unused\":{}0{}}}",
            "[".repeat(140),
            "]".repeat(140)
        ),
        r#"{"type":"userRole","unused":"\ud83d\ude00"}"#.into(),
        "null".into(),
        "not json".into(),
        role.into(),
    ];
    // Raw field projection supersedes the earlier source60/native20 whole-parser cost.
    let expected_delays = [3000, 1000, 50, 3000, 3000, 3000, 1000, 1000, 0];
    let mut completed = 0;
    assert_eq!(inputs.len(), 9, "ENCODED independent native input floor");
    for (i, (raw, delay)) in inputs.iter().zip(expected_delays).enumerate() {
        let clock = Arc::new(Clock::default());
        let limiter = Rc::new(RateLimiter::new(clock.clone(), 1200., 60000.));
        let api = client(limiter.clone());
        let mut request = api
            .build_request(if i == 2 { "/exchange" } else { "/info" }, &Value::Null)
            .unwrap();
        request.body = Some(RestBody::Encoded(raw.as_bytes().to_vec()));
        let prepared = ready(api.prepare_request(request, i == 8)).unwrap();
        ready(limiter.wait_for_token(1.)).unwrap();
        assert_eq!(
            clock.now.load(Ordering::SeqCst),
            delay,
            "ENCODED native reserved weight at {i}"
        );
        let sleeps = clock.sleeps.lock().unwrap();
        assert_eq!(
            sleeps.as_slice(),
            if i == 8 {
                &[][..]
            } else {
                std::slice::from_ref(&delay)
            },
            "ENCODED native reservation sleep at {i}"
        );
        assert!(
            matches!(prepared.body, Some(RestBody::Encoded(_))),
            "ENCODED native body provenance retained"
        );
        completed += 1;
    }
    assert_eq!(completed, 9, "ENCODED independent native completed floor");
}
fn telemetry_receives_the_original_native_request_data_variant() {
    let role = r#"{"type":"userRole"}"#;
    let inputs = [
        json!({"action":{"type":"order"},"nonce":1}),
        Value::String(role.into()),
        Value::String(serde_json::to_string(role).unwrap()),
        Value::String(r#"{"action":{"type":"order"},"unused":"\ud800"}"#.into()),
        Value::String("not json".into()),
    ];
    let mut completed = 0;
    assert_eq!(inputs.len(), 5, "TELEMETRY independent native input floor");
    for (i, input) in inputs.into_iter().enumerate() {
        let clock = Arc::new(Clock::default());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let emitted = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        let e = emitted.clone();
        let api = client(Rc::new(RateLimiter::new(clock, 1200., 60000.)))
            .with_order_error_telemetry(
                Arc::new(move |data, message| {
                    s.lock().unwrap().push((data.clone(), message.to_owned()));
                    Some(json!({"native":17}))
                }),
                Arc::new(move |message, context| {
                    e.lock()
                        .unwrap()
                        .push((message.to_owned(), context.cloned()))
                }),
            );
        let mut request = api.build_request("/exchange", &input).unwrap();
        if i >= 2 {
            request.body = Some(RestBody::Encoded(
                input.as_str().unwrap().as_bytes().to_vec(),
            ))
        }
        let error = api
            .check_response(
                &request,
                RestResponse {
                    status: 200,
                    reason: None,
                    headers: Default::default(),
                    body: RestResponseBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(
                            json!({"status":"err","response":"venue rejected"}),
                        )
                        .unwrap(),
                    ),
                },
                false,
            )
            .unwrap_err();
        assert!(
            matches!(error,safe_cex_transport::TransportError::Host(ref s)if s=="venue rejected"),
            "TELEMETRY native terminal error"
        );
        let observed = seen.lock().unwrap();
        assert_eq!(observed.len(), 1, "TELEMETRY native one provider");
        assert_eq!(
            observed[0].0, input,
            "TELEMETRY native original data variant at {i}"
        );
        assert_eq!(observed[0].1, "venue rejected", "TELEMETRY native message");
        let emitted = emitted.lock().unwrap();
        assert_eq!(emitted.len(), 1, "TELEMETRY native one emission");
        assert_eq!(
            emitted[0].1.as_ref().unwrap()["native"].as_u64(),
            Some(17),
            "TELEMETRY native returned context field"
        );
        completed += 1;
    }
    assert_eq!(completed, 5, "TELEMETRY independent native completed floor");
}
fn source() {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rate-encoded-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ENCODED actual source: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["encoded"], 9);
    assert_eq!(v["telemetry"], 5);
    assert_eq!(v["defaults"], 9);
}

fn omitted_weight_retains_the_source_default_and_numeric_normalization() {
    let cases: [(Option<f64>, u64); 9] = [
        (None, 50),
        (None, 50),
        (Some(0.), 50),
        (Some(-0.), 50),
        (Some(f64::NAN), 50),
        (Some(-2.), 50),
        (Some(f64::INFINITY), 50),
        (Some(2.), 100),
        (Some(1.5), 75),
    ];
    assert_eq!(cases.len(), 9, "DEFAULT independent native input floor");
    assert_eq!(
        cases[3].0.unwrap().to_bits(),
        1_u64 << 63,
        "DEFAULT native negative-zero input bits"
    );
    assert!(cases[4].0.unwrap().is_nan(), "DEFAULT native NaN input");
    let mut completed = 0;
    for (i, (weight, delay)) in cases.into_iter().enumerate() {
        let clock = Arc::new(Clock::default());
        let limiter = RateLimiter::with_defaults(clock.clone(), None, None);
        ready(limiter.wait_for_token(weight)).unwrap();
        ready(limiter.wait_for_token(1.)).unwrap();
        assert_eq!(
            clock.now.load(Ordering::SeqCst),
            delay,
            "DEFAULT native reservation at {i}"
        );
        assert_eq!(
            clock.sleeps.lock().unwrap().as_slice(),
            &[delay],
            "DEFAULT native one reserved sleep at {i}"
        );
        completed += 1;
    }
    assert_eq!(completed, 9, "DEFAULT independent native completed floor");
}
#[test]
fn encoded_pacing_telemetry_and_optional_weight_native_values_precede_source() {
    encoded_rate_classifier_parses_once_at_the_actual_prepare_callsite();
    telemetry_receives_the_original_native_request_data_variant();
    omitted_weight_retains_the_source_default_and_numeric_normalization();
    source();
}

#[test]
fn cancel_by_cloid_weight_reaches_real_http_prepare_for_raw_and_value_bodies() {
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rate-encoded-source.cjs"
        ))
        .arg("--cloid-weights")
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(source.len(), 6);
    for (index, (mode, count, delay)) in [
        ("raw", 39, 50),
        ("raw", 40, 100),
        ("raw", 80, 150),
        ("value", 39, 50),
        ("value", 40, 100),
        ("value", 80, 150),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(source[index]["mode"], mode);
        assert_eq!(source[index]["count"].as_u64(), Some(count as u64));
        assert_eq!(source[index]["weight"].as_f64(), Some((delay / 50) as f64));
        let clock = Arc::new(Clock::default());
        let limiter = Rc::new(RateLimiter::new(clock.clone(), 1200.0, 60000.0));
        let api = client(limiter.clone());
        let action = json!({"action":{"type":"cancelByCloid","cancels":vec![Value::Null;count]}});
        let body = if mode == "raw" {
            Value::String(action.to_string())
        } else {
            action
        };
        ready(api.prepare_request(api.build_request("/exchange", &body).unwrap(), false)).unwrap();
        ready(limiter.wait_for_token(1.0)).unwrap();
        assert_eq!(
            clock.now.load(Ordering::SeqCst),
            delay,
            "cancelByCloid batch pacing at HTTP prepare: {mode}/{count}"
        );
        assert_eq!(clock.sleeps.lock().unwrap().as_slice(), &[delay]);
    }
}
