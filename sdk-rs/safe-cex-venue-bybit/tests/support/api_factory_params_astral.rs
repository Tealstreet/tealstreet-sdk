use super::*;
use safe_cex_types::QueryValue;

pub fn collect_native() -> Vec<Value> {
    let mut cases = Vec::new();
    for carrier in ["legacy", "native"] {
        for signed in [false, true] {
            let options: ExchangeOptions = serde_json::from_value(if signed {
                json!({"key":"key","secret":"secret"})
            } else {
                json!({"key":"","secret":""})
            })
            .unwrap();
            let f = fixture_with_options(
                vec![response(r#"{"retCode":0,"retMsg":"OK"}"#)],
                true,
                options,
            );
            let mut request = get("/private");
            if carrier == "legacy" {
                request.params = Some(json!("😀"));
            } else {
                request.query_params = Some(QueryValue::String("😀".into()));
            }
            assert_eq!("😀".encode_utf16().collect::<Vec<_>>(), [55357, 56832]);
            let error = ready(f.api.execute_with_receipt(request.clone())).unwrap_err();
            assert_eq!(
                error.message, "Bybit params spread cannot represent a lone UTF-16 code unit",
                "factory_params_astral_native_refusal {carrier}/{signed}"
            );
            assert!(
                error.code.is_none(),
                "factory_params_astral_native_no_code {carrier}/{signed}"
            );
            assert!(
                error.response.is_none(),
                "factory_params_astral_native_no_response {carrier}/{signed}"
            );
            assert!(
                error.transport_error.is_none(),
                "factory_params_astral_native_no_network_origin {carrier}/{signed}"
            );
            assert!(
                !error.is_retryable_network(),
                "factory_params_astral_native_no_retry {carrier}/{signed}"
            );
            assert!(
                error.context.is_none(),
                "factory_params_astral_native_no_invented_context {carrier}/{signed}"
            );
            assert_eq!(
                error.request.method, "get",
                "factory_params_astral_native_original_method {carrier}/{signed}"
            );
            assert_eq!(
                error.request.url, "/private",
                "factory_params_astral_native_original_url {carrier}/{signed}"
            );
            assert_eq!(
                error.request.params, request.params,
                "factory_params_astral_native_original_legacy_params {carrier}/{signed}"
            );
            assert_eq!(
                error.request.query_params, request.query_params,
                "factory_params_astral_native_original_query_params {carrier}/{signed}"
            );
            assert!(
                error.request.body.is_none(),
                "factory_params_astral_native_original_body {carrier}/{signed}"
            );
            assert!(
                !error.request.headers.contains_key("X-BAPI-SIGN"),
                "factory_params_astral_native_no_signature {carrier}/{signed}"
            );
            assert_eq!(
                f.wire.requests.borrow().len(),
                0,
                "factory_params_astral_native_no_dispatch {carrier}/{signed}"
            );
            assert_eq!(
                f.wire.replies.borrow().len(),
                1,
                "factory_params_astral_native_response_unconsumed {carrier}/{signed}"
            );
            assert_eq!(
                f.host.failures.get(),
                1,
                "factory_params_astral_native_visible_failure_hook {carrier}/{signed}"
            );
            assert!(
                f.host.responses.borrow().is_empty(),
                "factory_params_astral_native_no_response_hook {carrier}/{signed}"
            );
            assert!(
                f.owner.accesses.borrow().is_empty(),
                "factory_params_astral_native_no_auth_callback {carrier}/{signed}"
            );
            assert_eq!(
                f.owner.base.auth_failure_streak(),
                0,
                "factory_params_astral_native_no_auth_streak {carrier}/{signed}"
            );
            assert_eq!(
                f.clock_fetches.load(Ordering::SeqCst),
                0,
                "factory_params_astral_native_no_clock_sync {carrier}/{signed}"
            );
            assert_eq!(
                f.emissions.get(),
                0,
                "factory_params_astral_native_no_exchange_emit {carrier}/{signed}"
            );
            assert!(
                f.runtime.console.borrow().is_empty(),
                "factory_params_astral_native_no_console {carrier}/{signed}"
            );
            assert_eq!(
                f.trace
                    .borrow()
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["common-failure"],
                "factory_params_astral_native_failure_precedes_request_hooks {carrier}/{signed}"
            );
            cases.push(
                json!({"id":if signed {"signed-astral-string"} else {"unsigned-astral-string"},
                "carrier":carrier,"inputUnits":[55357,56832],"outcome":"representation-refusal",
                "message":error.message,"physical":f.wire.requests.borrow().len()}),
            );
        }
    }
    assert_eq!(
        cases.len(),
        4,
        "factory_params_astral_native_separate_floor"
    );
    cases
}

pub fn compare_source(native: &[Value]) {
    assert_eq!(
        native.len(),
        4,
        "factory_params_astral_native_before_source"
    );
    let source = super::source("create-api-params-astral-boundaries.cjs", 2);
    for (index, actual) in native.iter().enumerate() {
        let expected = &source["cases"][index % 2];
        assert_eq!(actual["id"], expected["id"]);
        assert_eq!(actual["inputUnits"], expected["inputUnits"]);
        assert_eq!(actual["outcome"], "representation-refusal");
        assert_eq!(actual["physical"], 0);
        assert_eq!(
            expected["physical"], 1,
            "factory_params_astral_source_dispatches {index}"
        );
        assert_eq!(
            expected["ownParams"],
            json!([
                {"key":"0","units":[55357]},{"key":"1","units":[56832]},
                {"key":"requestId","units":[105,100,45,49]},
            ]),
            "factory_params_astral_source_materialized_units {index}"
        );
        assert_eq!(
            expected["wireQuery"],
            "0=%F0%9F%90%80&1=%F2%90%80%80&requestId=id-1"
        );
    }
}
