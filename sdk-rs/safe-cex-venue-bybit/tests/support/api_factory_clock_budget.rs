use super::*;

const HMAC_BEFORE_CLOCK: [&str; 5] = [
    "d7679158b705d7843f97a42f20daf61e69e0a0508c27764103c814be66e10d24",
    "ba3a0f7ba95d8b58f6fded292ee626c757f5827ff24f3335c0a3a96c5a35b7a6",
    "6a4ff7378c0b39a80073f77e8ee98249c9d5f3debd3282dd70a4940ae259fa08",
    "2b0a1956c1c3b96d841d9f1cb0b6d613850771f73432b0bb45737b777a0b2a17",
    "53fc55caa03812fe9786686097c22a76e0c8e237ef9cd10e5cdb18911bbab0bb",
];
const HMAC_AFTER_CLOCK: [&str; 5] = [
    "c63c251773a4e82ffbd539bbc869adccabeff0990c3335344c230c6872e2d124",
    "feb17385eaef00b10d0c9a933dd7e95cd239624c84e4493777812cab22877805",
    "6732a6d4da5a3755946b506c3aa249d8c16e4f8f2b1be0f6f2b1419385c82abe",
    "d01f22b0e828eca09aa55c9cdeba075227a93a52d961e0297c37e2d10b2cb6dc",
    "34959e172a9c952a23f381cbf92aa483c8b63aeef02c048838d8b8b7dc33c7cf",
];

pub fn collect_native() -> Vec<Value> {
    let inputs = [
        ("clock-only", 0, 0, 2, true, 1),
        (
            "clock-with-three-following-network-retries",
            0,
            3,
            5,
            true,
            4,
        ),
        ("one-used-before-clock-only-two-left", 1, 3, 5, false, 0),
        ("one-before-clock-two-after-clock-succeed", 1, 2, 5, true, 4),
        ("three-used-before-clock-no-retries-left", 3, 1, 5, false, 0),
    ];
    assert_eq!(
        inputs.len(),
        5,
        "factory_clock_budget_native_declared_cases"
    );
    let mut cases = Vec::new();
    for (id, before, after, expected_attempts, resolved, auth_calls) in inputs {
        let network = || Reply::Network {
            url: None,
            method: None,
        };
        let mut replies = Vec::new();
        replies.extend((0..before).map(|_| network()));
        replies.push(response(
            r#"{"retCode":10002,"retMsg":"recv window rejected"}"#,
        ));
        replies.extend((0..after).map(|_| network()));
        replies.push(response(r#"{"retCode":0,"retMsg":"OK"}"#));
        let f = fixture(replies);
        let result = ready(f.api.execute_with_receipt(get("/private")));
        let requests = f.wire.requests.borrow();
        assert_eq!(
            requests.len(),
            expected_attempts,
            "factory_clock_budget_native_attempts {id}"
        );
        assert_eq!(
            result.is_ok(),
            resolved,
            "factory_clock_budget_native_outcome {id}"
        );
        assert_eq!(
            f.clock_fetches.load(Ordering::SeqCst),
            1,
            "factory_clock_budget_native_single_clock_sync {id}"
        );
        assert_eq!(
            f.clock.get_current_time_ms(),
            2000.0,
            "factory_clock_budget_native_shared_clock {id}"
        );
        assert_eq!(
            f.owner.accesses.borrow().len(),
            auth_calls,
            "factory_clock_budget_native_response_unwind {id}"
        );
        assert_eq!(
            f.owner.base.auth_failure_streak(),
            0,
            "factory_clock_budget_native_no_auth_failure {id}"
        );
        assert_eq!(
            f.host.failures.get(),
            expected_attempts - 1 - usize::from(resolved),
            "factory_clock_budget_native_common_failure_count {id}"
        );
        assert_eq!(
            f.host.responses.borrow().len(),
            1 + usize::from(resolved),
            "factory_clock_budget_native_common_response_count {id}"
        );
        assert_eq!(
            f.wire.replies.borrow().len(),
            usize::from(!resolved),
            "factory_clock_budget_native_terminal_budget_stops_before_success {id}"
        );
        for (index, request) in requests.iter().enumerate() {
            assert_eq!(
                request.url,
                format!("/private?requestId=id-{}", index + 1),
                "factory_clock_budget_native_fresh_wire_id {id}/{index}"
            );
            assert_eq!(
                request.method, "get",
                "factory_clock_budget_native_method {id}/{index}"
            );
            assert!(
                request.params.is_none() && request.query_params.is_none(),
                "factory_clock_budget_native_query_consumed {id}/{index}"
            );
            let after_clock = index > before;
            assert_eq!(
                request.headers.get("X-BAPI-TIMESTAMP").map(String::as_str),
                Some(if after_clock { "2000" } else { "1000" }),
                "factory_clock_budget_native_timestamp {id}/{index}"
            );
            assert_eq!(
                signature(request),
                if after_clock {
                    HMAC_AFTER_CLOCK[index]
                } else {
                    HMAC_BEFORE_CLOCK[index]
                },
                "factory_clock_budget_native_hmac {id}/{index}"
            );
        }
        match result {
            Ok(execution) => {
                assert_native_json(
                    &execution.response.body.clone().into_data(),
                    &json!({"retCode":0,"retMsg":"OK"}),
                    &format!("factory_clock_budget_native_final_success {id}"),
                );
                assert_eq!(
                    execution.config.url, "/private",
                    "factory_clock_budget_native_effective_url {id}"
                );
                assert_eq!(
                    execution.config.params.as_ref().unwrap()["requestId"],
                    format!("id-{expected_attempts}"),
                    "factory_clock_budget_native_final_config_id {id}"
                );
            }
            Err(error) => {
                assert_eq!(
                    error.code.as_deref(),
                    Some("ERR_NETWORK"),
                    "factory_clock_budget_native_original_error_code {id}"
                );
                assert_eq!(
                    error.message, "scripted network",
                    "factory_clock_budget_native_original_error_message {id}"
                );
                let Some(TransportError::Network(cause)) = error.transport_error.as_deref() else {
                    panic!("factory_clock_budget_native_original_error_kind {id}");
                };
                assert_eq!(
                    cause.code(),
                    Some("ERR_NETWORK"),
                    "factory_clock_budget_native_original_transport_code {id}"
                );
                assert!(
                    error.response.is_none() && error.context.is_none(),
                    "factory_clock_budget_native_original_error_shape {id}"
                );
                assert_eq!(
                    error.request.url, "/private",
                    "factory_clock_budget_native_original_error_url {id}"
                );
                assert_eq!(
                    error.request.params.as_ref().unwrap()["requestId"],
                    format!("id-{expected_attempts}"),
                    "factory_clock_budget_native_terminal_error_config_id {id}"
                );
            }
        }
        assert_eq!(
            f.emissions.get(),
            0,
            "factory_clock_budget_native_no_emit {id}"
        );
        assert!(
            f.runtime.console.borrow().is_empty(),
            "factory_clock_budget_native_no_console {id}"
        );
        cases.push(json!({"id":id,"attempts":requests.len(),"resolved":resolved,
            "syncs":f.clock_fetches.load(Ordering::SeqCst),"authSuccesses":f.owner.accesses.borrow().len(),
            "ids":requests.iter().map(|request|request.url.split("requestId=").nth(1).unwrap()).collect::<Vec<_>>()}));
    }
    assert_eq!(cases.len(), 5, "factory_clock_budget_native_executed_cases");
    assert_eq!(
        cases
            .iter()
            .map(|value| value["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "clock-only",
            "clock-with-three-following-network-retries",
            "one-used-before-clock-only-two-left",
            "one-before-clock-two-after-clock-succeed",
            "three-used-before-clock-no-retries-left"
        ],
        "factory_clock_budget_native_ids"
    );
    cases
}

pub fn compare_source(native: &[Value]) {
    assert_eq!(native.len(), 5, "factory_clock_budget_native_before_source");
    let source = source("create-api-clock-retry-budget-boundaries.cjs", 5);
    for (index, actual) in native.iter().enumerate() {
        let expected = &source["cases"][index];
        assert_eq!(
            actual["id"], expected["id"],
            "factory_clock_budget_source_id {index}"
        );
        for field in ["attempts", "resolved", "syncs", "authSuccesses"] {
            assert_eq!(
                actual[field], expected["source"][field],
                "factory_clock_budget_source_value {index}/{field}"
            );
        }
        let source_ids = expected["source"]["physical"]
            .as_array()
            .unwrap()
            .iter()
            .map(|attempt| attempt["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            actual["ids"],
            json!(source_ids),
            "factory_clock_budget_source_fresh_ids {index}"
        );
    }
}
