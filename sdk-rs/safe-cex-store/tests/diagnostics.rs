use safe_cex_store::ws_diagnostics::*;
use serde_json::json;

#[test]
fn ws_redaction_native_fields_and_fallbacks() {
    assert!(!is_ws_diagnostics_enabled());
    set_ws_diagnostics_enabled(true);
    assert!(is_ws_diagnostics_enabled());
    set_ws_diagnostics_enabled(false);
    assert_eq!(normalize_secret_key("AπPI-Key!"), "apikey");
    let native = redact_json_value(
        &json!({"op":"login", "args":["credential", {"ordinary":"visible", "params":["private"]}], "API-Key":"sensitive", "market":"BTC"}),
        0,
        false,
    );
    assert_eq!(native["args"][0], "[redacted]");
    assert_eq!(native["args"][1]["ordinary"], "visible");
    assert_eq!(native["args"][1]["params"][0], "[redacted]");
    assert_eq!(native["API-Key"], "[redacted]");
    assert_eq!(native["market"], "BTC");
    assert_eq!(
        redact_json_value(&json!({"secret":"kept"}), 13, false)["secret"],
        "kept"
    );
    assert_eq!(stringify_ws_diagnostic_payload(&json!("raw")), "raw");
    assert!(redact_structured_json("not json").is_none());
    assert_eq!(
        redact_ws_diagnostic_payload("oops {\"token\":\"secret\"} ?apiKey=abc&symbol=BTC"),
        "oops {\"token\":\"[redacted]\"} ?apiKey=[redacted]&symbol=BTC"
    );
    assert_eq!(
        redact_ws_diagnostic_payload("é0123456789abcdef0123456789abcdef0123456789é"),
        "é0123...6789é"
    );
    assert_eq!(
        redact_ws_diagnostic_payload("?token=abc\u{feff}keep"),
        "?token=[redacted]\u{feff}keep"
    );
    let wire: serde_json::Value = serde_json::from_str(&redact_ws_diagnostic_payload(
        " {\"op\":\"AUTH\",\"params\":[\"x\"],\"id\":42} ",
    ))
    .unwrap();
    assert_eq!(wire["params"][0], "[redacted]");
    assert_eq!(wire["id"], 42);
}

use safe_cex_store::venue_rest_diagnostics::*;
use std::rc::Rc;

#[test]
fn rest_native_stamps_aggregates_and_bounds() {
    reset_venue_rest_diagnostics();
    let mut config = RestDiagnosticConfig {
        url: Some("/wallet-balance?signature=private".into()),
        base_url: Some("https://proxy/https://venue?private".into()),
        ..Default::default()
    };
    config.headers.get = Some(Rc::new(|name| {
        Ok((name == "x-bapi-timestamp").then(|| json!("900")))
    }));
    let ptr = &config as *const _;
    assert_eq!(
        mark_rest_request_start(&mut config, 1000.0) as *const _,
        ptr
    );
    mark_rest_signed(&mut config, 17.0, 1100.0);
    mark_rest_proxy_header(&mut config, "present");
    assert_eq!(config.numbers["safeCexRestDiagStartedAt"], 1000.0);
    assert_eq!(config.numbers["safeCexRestDiagSignMs"], 17.0);
    assert_eq!(config.numbers["safeCexRestDiagSignedAt"], 1100.0);
    assert_eq!(config.numbers["safeCexRestDiagSignatureAgeMs"], 200.0);
    assert_eq!(
        proxy_header_of(Some(&config)).unwrap().as_deref(),
        Some("absent")
    );
    config.headers.get = None;
    config
        .headers
        .values
        .insert("Authorization".into(), json!("Bearer private"));
    assert_eq!(
        proxy_header_of(Some(&config)).unwrap().as_deref(),
        Some("bearer")
    );
    let mut response = RestDiagnosticResponse {
        config: Some(config.clone()),
        status: Some(403.0),
        data: Some(json!("<html>CloudFront denied ABC123456789xyz</html>")),
        ..Default::default()
    };
    response
        .headers
        .values
        .insert("X-BAPI-LIMIT-STATUS".into(), json!("12"));
    response
        .headers
        .values
        .insert("server".into(), json!("edge"));
    response
        .headers
        .values
        .insert("Authorization".into(), json!("do not expose"));
    note_rest_success("bybit", Some(&response), 1120.0);
    response
        .headers
        .values
        .insert("X-BAPI-LIMIT-STATUS".into(), json!("7"));
    note_rest_success("bybit", Some(&response), 1140.0);
    note_rest_failure(
        "bybit",
        Some(&RestDiagnosticError {
            config: Some(config),
            response: Some(response),
            code: Some("ERR_BAD_RESPONSE".into()),
            message: Some("Request failed 403".into()),
            name: Some("AxiosError".into()),
        }),
        1200.0,
    );
    let stats = get_venue_rest_stats(1200.0);
    let stats = &stats["bybit"];
    assert_eq!((stats.requests, stats.ok, stats.failures), (3.0, 2.0, 1.0));
    assert_eq!(stats.total_p50_ms, Some(140.0));
    assert_eq!(stats.total_p95_ms, Some(200.0));
    assert_eq!(stats.sign_p50_ms, Some(17.0));
    assert_eq!(stats.min_quota_remaining, Some(7.0));
    assert_eq!(stats.max_signature_age_ms, Some(200.0));
    assert_eq!(stats.status_counts["403"], 1.0);
    assert_eq!(stats.code_counts["ERR_BAD_RESPONSE"], 1.0);
    assert_eq!(stats.per_minute, 0.6);
    let samples = drain_venue_rest_failure_samples();
    assert_eq!(samples.len(), 1);
    let sample = &samples[0];
    assert_eq!(sample.path.as_deref(), Some("/wallet-balance"));
    assert_eq!(
        sample.endpoint.as_deref(),
        Some("https://proxy/https://venue")
    );
    assert_eq!(sample.body_kind, "cloudfront");
    assert_eq!(
        sample.body_snippet.as_deref(),
        Some("CloudFront denied <redacted>")
    );
    assert_eq!(sample.headers.len(), 2);
    assert_eq!(sample.headers["server"], "edge");
    assert_eq!(sample.headers["x-bapi-limit-status"], "7");
    assert_eq!(sample.total_ms, Some(200.0));
    assert_eq!(sample.signature_age_ms, Some(200.0));
    assert!(drain_venue_rest_failure_samples().is_empty());
    reset_venue_rest_diagnostics();
    let signing = |response| RestDiagnosticError {
        response,
        message: Some("Signing request abc timed out".into()),
        ..Default::default()
    };
    note_rest_failure("signer", Some(&signing(None)), 5.0);
    let unsent = &get_venue_rest_stats(5.0)["signer"];
    assert_eq!(
        (unsent.requests, unsent.failures, unsent.unsent_signing),
        (0.0, 0.0, 1.0),
        "REST unsent signing is not a venue failure"
    );
    assert!(unsent.status_counts.is_empty());
    note_rest_failure(
        "signer",
        Some(&signing(Some(RestDiagnosticResponse::default()))),
        6.0,
    );
    let answered = &get_venue_rest_stats(6.0)["signer"];
    assert_eq!(
        (
            answered.requests,
            answered.failures,
            answered.unsent_signing
        ),
        (1.0, 1.0, 1.0),
        "REST a venue response means the request was sent"
    );
    let flags: Vec<_> = drain_venue_rest_failure_samples()
        .iter()
        .map(|sample| sample.unsent_signing)
        .collect();
    assert_eq!(flags, [true, false]);
    assert_eq!(
        visible_text(
            "<html><head><style>p{}</style><SCRIPT>x</script></head><body> Reason \n here </body></html>"
        ),
        "Reason here"
    );
    assert_eq!(visible_text("<html><style>a<p>b</p>"), "a b");
    assert_eq!(
        scrub("WebSocket disconnected — signing aborted"),
        "WebSocket disconnected — signing aborted"
    );
    assert_eq!(
        normalize_error_key("Timeout 123\u{feff}after 45"),
        "Timeout # after #"
    );
    assert_eq!(classify_body(Some(&json!({"retCode":0}))).0, "venue-json");
    assert_eq!(percentile(&[-1.5], 0.5), Some(-1.0));
    assert!(percentile(&[f64::NAN], 0.5).unwrap().is_nan());
    let mut sealed = RestDiagnosticConfig {
        sealed: true,
        ..Default::default()
    };
    mark_rest_request_start(&mut sealed, 1.0);
    mark_rest_signed(&mut sealed, 2.0, 3.0);
    assert!(sealed.numbers.is_empty());
    let mut partial = RestDiagnosticConfig::default();
    partial.headers.get = Some(Rc::new(|_| Err("getter failed".into())));
    mark_rest_signed(&mut partial, 9.0, 10.0);
    assert_eq!(partial.numbers["safeCexRestDiagSignedAt"], 10.0);
    assert!(
        !partial
            .numbers
            .contains_key("safeCexRestDiagSignatureAgeMs")
    );
    reset_venue_rest_diagnostics();
    for i in 0..202 {
        note_rest_success("bounds", None, i as f64);
    }
    note_rest_success("bounds", None, 400_000.0);
    assert_eq!(get_venue_rest_stats(400_000.0)["bounds"].tracked_samples, 1);
    for i in 0..27 {
        note_rest_failure(
            "bounds",
            Some(&RestDiagnosticError {
                message: Some(format!("failure {}", (b'a' + i as u8) as char)),
                ..Default::default()
            }),
            400_000.0 + i as f64,
        );
    }
    let bounded = get_venue_rest_stats(400_027.0);
    assert_eq!(bounded["bounds"].error_counts.len(), 21);
    assert_eq!(bounded["bounds"].error_counts["other"], 7.0);
    let samples = drain_venue_rest_failure_samples();
    assert_eq!(samples.len(), 25);
    assert_eq!(samples[0].at, 400_002.0);
    let mut values = vec![];
    for i in 0..202 {
        push(&mut values, i as f64);
    }
    assert_eq!(values.len(), 200);
    assert_eq!(values[0], 2.0);
    reset_venue_rest_diagnostics();
    assert!(get_venue_rest_stats(0.0).is_empty());
}

#[test]
fn rest_native_signed_zero_extrema() {
    for arrivals in [["0", "-0"], ["-0", "0"]] {
        reset_venue_rest_diagnostics();
        for value in arrivals {
            let mut response = RestDiagnosticResponse::default();
            response
                .headers
                .values
                .insert("x-bapi-limit-status".into(), json!(value));
            note_rest_success("zero", Some(&response), 1.0);
        }
        let native = get_venue_rest_stats(1.0);
        assert_eq!(
            native["zero"].min_quota_remaining.unwrap().to_bits(),
            (-0.0_f64).to_bits(),
            "native quota Math.min signed zero"
        );
        for age in [-0.0, 0.0] {
            let mut config = RestDiagnosticConfig::default();
            config
                .numbers
                .insert("safeCexRestDiagSignatureAgeMs".into(), age);
            note_rest_failure(
                "zero",
                Some(&RestDiagnosticError {
                    config: Some(config),
                    ..Default::default()
                }),
                1.0,
            );
            assert_eq!(
                get_venue_rest_stats(1.0)["zero"]
                    .max_signature_age_ms
                    .unwrap()
                    .to_bits(),
                0.0_f64.to_bits(),
                "native age Math.max signed zero"
            );
        }
    }
    reset_venue_rest_diagnostics();
}
#[test]
fn rest_and_ws_literals_hold_in_actual_typescript_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/diagnostics_oracle.cjs"
        ))
        .output()
        .expect("Node required; the diagnostics source oracle never skips");
    assert!(
        output.status.success(),
        "diagnostics source oracle failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
