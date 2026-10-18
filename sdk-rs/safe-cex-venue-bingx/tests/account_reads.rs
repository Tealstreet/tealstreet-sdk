use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_venue_bingx::account_reads::{fetch_primary_balance_data, get_account};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    task::{Context, Poll, Waker},
};
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let waker = Waker::noop();
    match future.as_mut().poll(&mut Context::from_waker(waker)) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test transport must resolve immediately"),
    }
}
#[test]
fn account_reads_match_actual_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/account_reads_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    println!("runtime {}", corpus["runtime"]);
    // Pin inputs independently of live outputs and runtime/architecture metadata.
    // This fixture is never used as the expected result: TypeScript still executes.
    let pinned: Value =
        serde_json::from_str(include_str!("fixtures/account-reads-inputs.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 68, "BINGX-READ source cases");
    assert_eq!(
        cases
            .iter()
            .map(|case| json!([case["method"], case["input"]]))
            .collect::<Vec<_>>(),
        *pinned["cases"].as_array().unwrap(),
        "BINGX-READ cases input content"
    );
    let mut counts = [0, 0];
    for (index, case) in cases.iter().enumerate() {
        let calls = RefCell::new(Vec::new());
        let callback = |request: safe_cex_transport::RestRequest| {
            calls.borrow_mut().push(request);
            let response = if case["input"]["transportError"] == true {
                let mut error = ExchangeError::new("network preserved");
                error.name = "NetworkError".into();
                error.store_subscriber_ids = vec![71, 83];
                Err(error)
            } else {
                Ok(serde_json::from_value::<JsNumberInput>(case["input"].clone()).unwrap())
            };
            std::future::ready(response)
        };
        let result = match case["method"].as_str().unwrap() {
            "fetchPrimaryBalanceData" => {
                counts[0] += 1;
                ready(fetch_primary_balance_data(callback))
            }
            "getAccount" => {
                counts[1] += 1;
                ready(get_account(callback)).map(Input::from)
            }
            _ => panic!("unknown method"),
        };
        assert_eq!(
            case["calls"].as_array().unwrap().len(),
            1,
            "BINGX-READ source calls {index}"
        );
        assert_eq!(
            calls.borrow().len(),
            1,
            "BINGX-READ native call count {index}"
        );
        let calls = calls.borrow();
        assert_eq!(
            calls[0],
            safe_cex_transport::RestRequest {
                method: "GET".into(),
                url: "/openApi/swap/v3/user/balance".into(),
                ..Default::default()
            },
            "BINGX-READ native config {index}"
        );
        assert_eq!(calls[0].method, "GET", "BINGX-READ native method {index}");
        assert_eq!(
            calls[0].url, "/openApi/swap/v3/user/balance",
            "BINGX-READ native URL {index}"
        );
        assert_eq!(
            case["calls"],
            json!([[calls[0].url]]),
            "BINGX-READ source/native call {index}"
        );
        assert!(
            calls[0].params.is_none() && calls[0].body.is_none(),
            "BINGX-READ empty config {index}"
        );
        match result {
            Ok(value) => {
                assert!(
                    case["expected"].get("ok").is_some(),
                    "BINGX-READ success outcome {index}"
                );
                assert_eq!(
                    value,
                    Input::from(case["expected"]["ok"].clone()),
                    "BINGX-READ native value {index}"
                );
            }
            Err(error) => {
                assert!(
                    case["expected"].get("error").is_some(),
                    "BINGX-READ error outcome {index}"
                );
                assert_eq!(
                    error.message, case["expected"]["error"]["message"],
                    "BINGX-READ error message {index}"
                );
                assert_eq!(
                    error.name, case["expected"]["error"]["name"],
                    "BINGX-READ error name {index}"
                );
                if case["input"]["transportError"] == true {
                    assert_eq!(
                        error.store_subscriber_ids,
                        vec![71, 83],
                        "BINGX-READ propagated identity"
                    );
                }
            }
        }
    }
    assert_eq!(counts, [34, 34], "BINGX-READ executed method counts");
    let numeric = corpus["numeric"].as_array().unwrap();
    assert_eq!(numeric.len(), 10, "BINGX-READ numeric source total");
    assert_eq!(
        numeric
            .iter()
            .map(|case| json!([case["method"], case["raw"]]))
            .collect::<Vec<_>>(),
        *pinned["numeric"].as_array().unwrap(),
        "BINGX-READ numeric input content"
    );
    let mut numeric_counts = [0, 0];
    for (index, case) in numeric.iter().enumerate() {
        let calls = RefCell::new(Vec::new());
        let request = |config| {
            calls.borrow_mut().push(config);
            std::future::ready(Ok(serde_json::from_str::<JsNumberInput>(
                case["raw"].as_str().unwrap(),
            )
            .unwrap()))
        };
        let mut native = match case["method"].as_str().unwrap() {
            "fetchPrimaryBalanceData" => {
                numeric_counts[0] += 1;
                ready(fetch_primary_balance_data(request)).unwrap()
            }
            "getAccount" => {
                numeric_counts[1] += 1;
                Input::from(ready(get_account(request)).unwrap())
            }
            _ => panic!("unknown numeric method"),
        };
        assert_eq!(
            case["calls"],
            json!([["/openApi/swap/v3/user/balance"]]),
            "BINGX-READ numeric source request"
        );
        assert_eq!(
            *calls.borrow(),
            vec![safe_cex_transport::RestRequest {
                method: "GET".into(),
                url: "/openApi/swap/v3/user/balance".into(),
                ..Default::default()
            }],
            "BINGX-READ numeric native request"
        );
        let mut expected = case["output"].clone();
        if case["method"] == "fetchPrimaryBalanceData" {
            assert_eq!(
                native.as_object().unwrap().len(),
                9,
                "BINGX-READ numeric native fields"
            );
            let number = native.get("userId").and_then(Input::as_f64).unwrap();
            assert_eq!(
                format!("{:016x}", number.to_bits()),
                case["bits"].as_str().unwrap(),
                "BINGX-READ numeric native bits {index}"
            );
            native
                .as_object_mut()
                .unwrap()
                .retain(|(key, _)| key != &"userId".encode_utf16().collect::<Vec<_>>());
            expected.as_object_mut().unwrap().remove("userId");
        } else {
            assert_eq!(
                native.as_object().unwrap().len(),
                1,
                "BINGX-READ numeric account fields"
            );
        }
        assert_eq!(
            native,
            Input::from(expected),
            "BINGX-READ numeric native value {index}"
        );
    }
    assert_eq!(numeric_counts, [5, 5], "BINGX-READ numeric comparisons");
    let boundary = corpus["boundary"].as_array().unwrap();
    assert_eq!(boundary.len(), 2, "BINGX-READ UTF8 census");
    assert_eq!(
        boundary
            .iter()
            .map(|case| json!([case["method"], case["raw"]]))
            .collect::<Vec<_>>(),
        *pinned["boundary"].as_array().unwrap(),
        "BINGX-READ boundary input content"
    );
    for case in boundary {
        assert_eq!(
            case["units"],
            json!([97, 55296, 98]),
            "BINGX-READ source surrogate"
        );
        assert!(
            serde_json::from_str::<JsNumberInput>(case["raw"].as_str().unwrap()).is_err(),
            "BINGX-READ UTF8 refusal"
        );
    }
}
