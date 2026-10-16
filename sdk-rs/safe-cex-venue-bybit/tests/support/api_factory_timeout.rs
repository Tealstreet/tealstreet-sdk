use super::api_factory_runtime::*;
use safe_cex_transport::RestAdapter;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
pub fn collect_native() -> Vec<Value> {
    let inputs = [
        (
            "async-sign-80-wire-10",
            80,
            10,
            vec![100, 10],
            vec![1080, 1170],
            true,
        ),
        (
            "async-sign-120-wire-10",
            120,
            10,
            vec![100],
            vec![1120],
            false,
        ),
        (
            "async-sign-0-wire-10",
            0,
            10,
            vec![100, 90],
            vec![1000, 1010],
            true,
        ),
        (
            "async-sign-80-wire-100",
            80,
            100,
            vec![100],
            vec![1080],
            false,
        ),
    ];
    let mut cases = Vec::new();
    for (id, signer_ms, physical_ms, timeouts, starts, resolved) in inputs {
        let f = fixture(Mode::Timeout { physical_ms }, Some(signer_ms));
        let mut request = request();
        request.timeout_ms = Some(100);
        let result = drive(f.api.execute_with_receipt(request), &f.runtime);
        let native_resolved = result.is_ok();
        assert_eq!(
            result.is_ok(),
            resolved,
            "factory_timeout_native_outcome {id}"
        );
        let requests = f.wire.requests.borrow();
        assert_eq!(
            requests.len(),
            timeouts.len(),
            "factory_timeout_native_attempts {id}"
        );
        assert_eq!(
            requests
                .iter()
                .map(|value| value.timeout_ms.unwrap())
                .collect::<Vec<_>>(),
            timeouts,
            "factory_timeout_native_whole_attempt_budget {id}"
        );
        assert_eq!(
            &*f.wire.starts.borrow(),
            &starts,
            "factory_timeout_native_physical_start {id}"
        );
        assert_eq!(
            f.signer.as_ref().unwrap().calls.borrow().len(),
            timeouts.len(),
            "factory_timeout_native_custom_signer_count {id}"
        );
        assert_eq!(
            requests
                .iter()
                .map(|value| value.headers.get("X-BAPI-SIGN").unwrap().as_str())
                .collect::<Vec<_>>(),
            vec!["signature"; timeouts.len()],
            "factory_timeout_native_custom_signature {id}"
        );
        assert_eq!(
            f.owner.accesses.borrow().len(),
            if resolved { 2 } else { 0 },
            "factory_timeout_native_auth_unwind {id}"
        );
        assert_eq!(f.owner.base.auth_failure_streak(), 0);
        assert_eq!(f.clock_fetches.load(Ordering::SeqCst), 0);
        assert!(
            f.wakes.0.load(Ordering::SeqCst) > 0,
            "factory_timeout_native_signer_really_suspended {id}"
        );
        assert_eq!(
            f.host.failures.get(),
            1,
            "factory_timeout_native_original_common_failure {id}"
        );
        if let Err(error) = result {
            assert_eq!(
                error.code.as_deref(),
                Some("ECONNRESET"),
                "factory_timeout_native_original_network_code {id}"
            );
            assert!(error.response.is_none());
        }
        let payloads = f
            .signer
            .as_ref()
            .unwrap()
            .calls
            .borrow()
            .iter()
            .map(|(_, payload)| payload.clone())
            .collect::<Vec<_>>();
        let expected = if resolved {
            vec![
                "1000key20000requestId=id-1".to_owned(),
                format!("{}key20000requestId=id-2", 1000 + signer_ms + physical_ms),
            ]
        } else {
            vec!["1000key20000requestId=id-1".to_owned()]
        };
        assert_eq!(
            payloads, expected,
            "factory_timeout_native_live_signing_clock {id}"
        );
        cases.push(json!({"id":id,"native":{"attempts":requests.len(),"timeouts":requests.iter().map(|value|value.timeout_ms.unwrap()).collect::<Vec<_>>(),"starts":*f.wire.starts.borrow(),"resolved":native_resolved,"authSuccesses":f.owner.accesses.borrow().len(),"authFailures":f.owner.base.auth_failure_streak()}}));
    }
    assert_eq!(cases.len(), 4, "factory_timeout_native_inventory");
    assert_eq!(
        cases
            .iter()
            .map(|value| value["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "async-sign-80-wire-10",
            "async-sign-120-wire-10",
            "async-sign-0-wire-10",
            "async-sign-80-wire-100"
        ],
        "factory_timeout_native_ids"
    );
    cases
}
pub fn compare_source(native: &[Value]) {
    assert_eq!(native.len(), 4, "factory_timeout_native_before_source");
    let source = source("create-api-signing-timeout-boundaries.cjs", 4);
    let source_timeouts = [
        json!([100, 90]),
        json!([100, 90]),
        json!([100, 90]),
        json!([100]),
    ];
    let source_starts = [
        json!([1080, 1170]),
        json!([1120, 1250]),
        json!([1000, 1010]),
        json!([1080]),
    ];
    for i in 0..4 {
        assert_eq!(native[i]["id"], source["cases"][i]["id"]);
        let calls = source["cases"][i]["source"]["physical"].as_array().unwrap();
        assert_eq!(
            json!(
                calls
                    .iter()
                    .map(|call| call["timeout"].clone())
                    .collect::<Vec<_>>()
            ),
            source_timeouts[i],
            "source excludes signer time {i}"
        );
        assert_eq!(
            json!(
                calls
                    .iter()
                    .map(|call| call["time"].clone())
                    .collect::<Vec<_>>()
            ),
            source_starts[i]
        );
        if i == 0 {
            assert_eq!(
                native[i]["native"]["timeouts"],
                json!([100, 10]),
                "CHOSEN shared total-attempt budget includes signer"
            );
            assert_eq!(source_timeouts[i], json!([100, 90]));
        } else if i == 1 {
            assert_eq!(
                native[i]["native"]["attempts"], 1,
                "CHOSEN signer consumes native budget"
            );
            assert_eq!(calls.len(), 2);
            assert_eq!(native[i]["native"]["authSuccesses"], 0);
            assert_eq!(source["cases"][i]["source"]["authSuccesses"], 2);
        } else {
            assert_eq!(native[i]["native"]["timeouts"], source_timeouts[i]);
            assert_eq!(
                native[i]["native"]["authSuccesses"],
                source["cases"][i]["source"]["authSuccesses"]
            );
        }
    }
}
