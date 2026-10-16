use indexmap::IndexMap;
use safe_cex_types::ExchangeOptions;
use safe_cex_venue_kucoin::broker::*;
use serde_json::{Value, json};
use std::{
    future::Future,
    io::Write,
    process::{Command, Stdio},
    task::{Context, Poll, Waker},
};
#[test]
fn live_broker_attribution() {
    let mut cases = vec![];
    for trim in [
        json!(null),
        json!(false),
        json!(42),
        json!([]),
        json!({}),
        json!(""),
        json!(" \u{feff}"),
        json!("\u{feff}broker\u{feff}"),
        json!("\u{85}broker\u{85}"),
    ] {
        for verify in [None, Some(false), Some(true)] {
            for partner in [
                "partner",
                "",
                " \u{feff}",
                "\u{feff}partner ",
                "\u{85}partner",
            ] {
                let mut options = json!({"key":"fixture-key","secret":"fixture-secret","extra":{"kucoinBroker":{"partner":partner,"brokerKey":" broker-key ","brokerName":" broker-name "}}});
                if let Some(verify) = verify {
                    options["extra"]["kucoinBroker"]["verify"] = json!(verify);
                }
                cases.push(json!({"options":options,"headers":{"kc-api-timestamp":"123","X-Custom":"value","kc-api-partner":"old"},"timestamp":"456","trim":trim}));
            }
        }
    }
    for opts in [
        json!({"key":"k","secret":"s"}),
        json!({"key":"","secret":"s","extra":{"kucoinBroker":{"partner":"p","brokerKey":"b","brokerName":"n"}}}),
    ] {
        cases.push(json!({"options":opts,"headers":{},"timestamp":"0","trim":"x"}));
    }
    let mut child = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/broker.cjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 137, "KUCOIN-CARDINALITY broker input total");
    assert_eq!(
        expected.len(),
        137,
        "KUCOIN-CARDINALITY broker source total"
    );
    let mut compared_cases = 0;
    for index in 0..137 {
        let case = &cases[index];
        let expected = &expected[index];
        compared_cases += 1;
        let options: ExchangeOptions = serde_json::from_value(case["options"].clone()).unwrap();
        let mut headers: IndexMap<String, String> =
            serde_json::from_value(case["headers"].clone()).unwrap();
        let timestamp = get_kucoin_request_timestamp(Some(&headers));
        {
            let mut future = Box::pin(add_kucoin_broker_headers(
                &mut headers,
                &options,
                case["timestamp"].as_str().unwrap(),
            ));
            assert!(matches!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop())),
                Poll::Ready(Ok(()))
            ));
        }
        assert_eq!(
            json!({"trim":trim_string(case.get("trim")),"config":get_kucoin_broker_config(&options),"timestamp":timestamp,"headers":headers}),
            *expected,
            "{case}"
        );
    }
    assert_eq!(
        compared_cases, 137,
        "KUCOIN-CARDINALITY broker compared cases"
    );
}
