//! Corrected weight values and explicit remaining parser findings.
use safe_cex_venue_hyperliquid::rate_limit::get_hyperliquid_rest_request_weight;
use serde_json::{Value, json};
#[test]
fn weight_ingress_native_errors_batches_and_remaining_parser_findings() {
    let mut inputs = vec![];
    for raw in [
        r#"{"toString":0}"#,
        r#"{"valueOf":0,"toString":0}"#,
        r#"[{"toString":0}]"#,
        r#"{"toString":"x"}"#,
        r#"{}"#,
        r#"[60]"#,
    ] {
        inputs.push((
            "/info",
            json!({"type":"candleSnapshot","limit":serde_json::from_str::<Value>(raw).unwrap()}),
        ));
    }
    for kind in [Some(json!("unexpected")), None, Some(Value::Null)] {
        let mut action = json!({"":vec![Value::Null;40]});
        if let Some(kind) = kind {
            action["type"] = kind
        }
        inputs.push(("/exchange", json!({"action":action})));
    }
    inputs.push((
        "/exchange",
        json!({"action":{"type":"order","orders":vec![Value::Null;40]}}),
    ));
    inputs.push((
        "/info",
        Value::String(r#"{"type":"userRole","unused":"\ud800"}"#.into()),
    ));
    inputs.push((
        "/info",
        Value::String(format!(
            "{{\"type\":\"userRole\",\"unused\":{}0{}}}",
            "[".repeat(140),
            "]".repeat(140)
        )),
    ));
    inputs.push((
        "/info",
        Value::String(r#"{"type":"userRole","unused":"\ud83d\ude00"}"#.into()),
    ));
    inputs.push((
        "/info",
        Value::String(r#"{"type":"userRole","unused":"\\ud800"}"#.into()),
    ));
    let expected: [f64; 14] = [
        20., 20., 20., 20., 20., 21., 1., 1., 1., 2., 60., 60., 60., 60.,
    ];
    assert_eq!(inputs.len(), 14, "independent weight ingress case floor");
    let mut completed = 0;
    for (index, ((url, data), expected)) in inputs.iter().zip(expected).enumerate() {
        let actual = get_hyperliquid_rest_request_weight(url, data);
        if index < 4 {
            let error = actual.expect_err("native poisoned limit must retain Number failure");
            assert_eq!(error.name, "TypeError", "native Number error name");
            assert_eq!(
                error.message, "Cannot convert object to primitive value",
                "native Number error message"
            );
        } else {
            let actual = actual.expect("native weight");
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "native weight at case {index}"
            );
        }
        completed += 1;
    }
    assert_eq!(completed, 14, "independent completed weight ingress count");
    let object = json!({"limit":17});
    let parsed = safe_cex_venue_hyperliquid::rate_limit::parse_request_data(&object);
    assert!(
        std::ptr::eq(parsed.as_ref(), &object),
        "native non-string parser retains original object"
    );
    // Cases10/11 now project consumed weight fields before native string decoding.
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/weight-ingress-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual source weight ingress: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let wire: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(wire["count"], 14, "independent source weight floor");
    assert_eq!(wire["last"], 13, "source weight last case");
}

#[test]
fn direct_weight_helpers_keep_native_shapes_and_original_parser_borrow() {
    use safe_cex_venue_hyperliquid::rate_limit::*;
    assert!(!is_endpoint(None, "/info"), "native missing endpoint");
    let mut url_refusals = 0;
    for raw in [r#""\ud800/info""#, r#""\udc00/info""#] {
        assert!(
            serde_json::from_str::<String>(raw).is_err(),
            "native shared URL ingress refusal"
        );
        url_refusals += 1;
    }
    assert_eq!(url_refusals, 2, "independent native URL refusal floor");
    assert!(
        is_endpoint(Some("https://example.test/😀/info"), "/info"),
        "native astral endpoint suffix"
    );
    assert!(
        !is_endpoint(Some("/info?x=1"), "/info"),
        "native query suffix mismatch"
    );
    let mut shapes = 0;
    for (input, expected) in [
        (None, 0.0),
        (Some(Value::Null), 0.0),
        (Some(json!({"length":40})), 0.0),
        (Some(json!([])), 0.0),
        (Some(json!([1, null])), 2.0),
    ] {
        assert_eq!(
            get_array_length(input.as_ref()),
            expected,
            "native array classifier length"
        );
        shapes += 1;
    }
    assert_eq!(shapes, 5, "independent native array classifier floor");
    assert_eq!(
        get_info_base_weight(Some(&json!("userRole"))),
        60.0,
        "native role base weight"
    );
    assert_eq!(
        get_info_base_weight(Some(&json!("allMids"))),
        2.0,
        "native low base weight"
    );
    assert_eq!(
        get_info_base_weight(Some(&json!({"type":"userRole"}))),
        20.0,
        "native strict base discriminator"
    );
    assert_eq!(
        get_info_extra_weight(Some(&json!("candleSnapshot")), Some(&json!({}))).unwrap(),
        0.0,
        "native JSON-only object conversion"
    );
    let original = json!({"nested":{"marker":73}});
    let parsed = parse_request_data(&original);
    assert!(
        std::ptr::eq(parsed.as_ref(), &original),
        "native parser original object allocation"
    );
}

#[derive(Default)]
struct GuardClock(std::sync::atomic::AtomicUsize);
impl safe_cex_transport::rate_limit::RateLimitClock for GuardClock {
    fn now_ms(&self) -> u64 {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        0
    }
    fn sleep_ms(&self, _: u64) -> safe_cex_transport::TransportFuture<'_, ()> {
        panic!("weight error must not reach pacing sleep")
    }
}
#[derive(Default)]
struct GuardAdapter(std::sync::atomic::AtomicUsize);
impl safe_cex_transport::RestAdapter for GuardAdapter {
    fn execute(
        &self,
        _: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        Box::pin(async move {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(safe_cex_transport::RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(b"{}".to_vec()),
            })
        })
    }
}
fn ready<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
    {
        std::task::Poll::Ready(v) => v,
        std::task::Poll::Pending => panic!("weight guard unexpectedly suspended"),
    }
}
#[test]
fn api_weight_errors_precede_pacing_and_transport_while_capture_bypasses() {
    use safe_cex_venue_hyperliquid::{api::create_info_api, rate_limit::RateLimiter};
    use std::sync::{Arc, atomic::Ordering};
    let clock = Arc::new(GuardClock::default());
    let adapter = Arc::new(GuardAdapter::default());
    let options = serde_json::from_value(json!({"key":"","secret":""})).unwrap();
    let client = create_info_api(
        options,
        adapter.clone(),
        std::rc::Rc::new(RateLimiter::new(clock.clone(), 1200., 60000.)),
        Arc::new(|_| panic!("pre-request weight failure must not emit response error")),
    );
    let body = json!({"type":"candleSnapshot","limit":{"toString":0}});
    let error = ready(client.prepare_request(
        client.build_request("/info", &body).unwrap(),
        false,
        &|_| Ok(()),
    ))
    .expect_err("native API Number failure before pacing");
    assert!(
        matches!(error,safe_cex_transport::TransportError::JavaScript(ref cause) if cause.name=="TypeError" && cause.message=="Cannot convert object to primitive value"),
        "WEIGHT native API retains admitted TypeError class before pacing"
    );
    assert_eq!(
        clock.0.load(Ordering::SeqCst),
        0,
        "native weight failure before limiter clock"
    );
    let failure = ready(client.execute(
        client.build_request("/info", &body).unwrap(),
        false,
        &|_| Ok(()),
    ))
    .expect_err("native API weight error must stop actual transport");
    assert!(
        matches!(failure.transport_error.as_deref(),Some(safe_cex_transport::TransportError::JavaScript(cause)) if cause.name=="TypeError" && cause.message=="Cannot convert object to primitive value"),
        "native request failure retains weight projection"
    );
    assert_eq!(
        failure.message, "Cannot convert object to primitive value",
        "WEIGHT native request failure keeps source message without an added name prefix"
    );
    assert!(
        failure.response.is_none(),
        "native weight failure has no response"
    );
    assert_eq!(
        adapter.0.load(Ordering::SeqCst),
        0,
        "native weight failure before adapter call"
    );
    let captured = ready(client.execute(
        client.build_request("/info", &body).unwrap(),
        true,
        &|_| Ok(()),
    ))
    .expect("native capture bypasses weight conversion");
    assert_eq!(captured.status, 200, "native capture actual response");
    assert_eq!(
        adapter.0.load(Ordering::SeqCst),
        1,
        "native capture positive transport reachability"
    );
}

#[test]
fn request_action_type_parser_findings_are_entry_specific() {
    use safe_cex_venue_hyperliquid::api::get_request_action_type;
    let surrogate = Value::String(r#"{"action":{"type":"cancel"},"unused":"\ud800"}"#.into());
    let deep = Value::String(format!(
        "{{\"action\":{{\"type\":\"cancel\"}},\"unused\":{}0{}}}",
        "[".repeat(140),
        "]".repeat(140)
    ));
    assert_eq!(
        get_request_action_type(&surrogate),
        Some("cancel".into()),
        "ACTION native ignored surrogate does not suppress type"
    );
    assert_eq!(
        get_request_action_type(&deep),
        None,
        "ACTION native explicit depth budget refusal"
    );
    assert_eq!(
        get_request_action_type(&Value::String(r#"{"action":{"type":"\ud800"}}"#.into())),
        None,
        "ACTION native materialized returned D800 cannot cross String"
    );
    assert_eq!(
        get_request_action_type(&Value::String(r#"{"action":{"type":"\\ud800"}}"#.into())),
        Some(r"\ud800".into()),
        "ACTION native literal ASCII returned backslash sequence"
    );
    assert_eq!(
        get_request_action_type(&Value::String(
            r#"{"action":{"type":"cancel"},"unused":"\ud83d\ude00"}"#.into()
        )),
        Some("cancel".into()),
        "native action-type astral positive"
    );
}

#[test]
fn parser_required_value_fallback_keeps_original_handle_and_prices_returned_unit_loss() {
    use safe_cex_venue_hyperliquid::rate_limit::parse_request_data;
    let inputs = [
        Value::String(r#"{"type":"userRole","unused":"\ud800"}"#.into()),
        Value::String(format!(
            "{{\"type\":\"userRole\",\"unused\":{}0{}}}",
            "[".repeat(140),
            "]".repeat(140)
        )),
    ];
    let mut completed = 0;
    for input in &inputs {
        let parsed = parse_request_data(input);
        assert!(
            matches!(parsed, std::borrow::Cow::Borrowed(_)),
            "PARSER native original-string fallback variant"
        );
        assert!(
            std::ptr::eq(parsed.as_ref(), input),
            "PARSER native fallback original allocation"
        );
        assert!(
            matches!(parsed.as_ref(), Value::String(_)),
            "PARSER native fallback loses parsed record shape"
        );
        assert_eq!(
            get_hyperliquid_rest_request_weight("/info", input)
                .unwrap()
                .to_bits(),
            60f64.to_bits(),
            "PARSER native consumer projects weight60 independently of whole-return fallback"
        );
        completed += 1;
    }
    assert_eq!(completed, 2, "PARSER independent native fallback floor");
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/finding-parser-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "PARSER actual source whole result: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let receipt: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(receipt["source_cases"], 13);
    assert_eq!(receipt["stop"], "literal ASCII returned action type");
    assert_eq!(receipt["rate_returned_lone_unit"], "D800");
}
