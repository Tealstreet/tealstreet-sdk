include!("public_reads.rs");

#[test]
fn class_cost_http_native() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/class_cost_http.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "HTTP source execution: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        data["cases"].as_array().unwrap().len(),
        512,
        "HTTP independent case floor"
    );
    for index in 0..512 {
        let case = &data["cases"][index];
        let n = index as f64;
        let value = [
            n * 123.25,
            -n * 0.125,
            -0.,
            n / 1024.,
            1e21 + n * 1048576.,
            1e-300 * (n + 1.),
            1.7e308,
            0.,
        ][index % 8];
        let failure = RestFailure {
            code: Some("ERR_NETWORK".into()),
            context: Some(Box::new(json!({"marker":index}))),
            message: "generated network failure".into(),
            transport_error: None,
            response: None,
            request: Box::new(RestRequest::default()),
        };
        let calls = Arc::new(Mutex::new(vec![]));
        let result = if index % 8 == 7 {
            Err(failure.clone())
        } else {
            Ok(response(RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!(value)).unwrap(),
            )))
        };
        let xhr = client(result, calls.clone());
        let future = get_server_time(&xhr);
        // Known scheduling difference is recorded separately, never an agreement.
        assert_eq!(
            calls.lock().unwrap().len(),
            0,
            "HTTP native lazy prefix (excluded difference)"
        );
        let actual = run(future);
        if index % 8 == 7 {
            assert_eq!(
                actual.unwrap_err(),
                failure,
                "class_cost_http_failure:{index}"
            );
            assert_eq!(case["preserved"], true, "HTTP source failure identity");
        } else {
            assert_eq!(
                format!("{:016x}", actual.unwrap().to_bits()),
                case["outputBits"],
                "class_cost_http_native:{index}"
            );
        }
        assert_eq!(
            calls.lock().unwrap().len(),
            1,
            "class_cost_http_calls:{index}"
        );
        assert_eq!(
            calls.lock().unwrap()[0].url,
            "/time",
            "class_cost_http_url:{index}"
        );
        assert_eq!(
            calls.lock().unwrap()[0].method,
            "get",
            "class_cost_http_method:{index}"
        );
        assert_eq!(
            case["inputBits"],
            format!("{:016x}", value.to_bits()),
            "HTTP independent generated input"
        );
        assert_eq!(case["index"], index, "HTTP iteration identity");
        assert_eq!(
            case["immediateCalls"], 1,
            "HTTP source eager prefix (excluded difference)"
        );
        assert_eq!(case["calls"], json!(["/time"]), "HTTP source request");
    }
    eprintln!(
        "512 awaited outcomes; 512 known eager/lazy prefix differences excluded from agreement"
    );
}
