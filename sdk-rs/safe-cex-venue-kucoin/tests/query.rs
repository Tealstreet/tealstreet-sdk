use safe_cex_num::JsJsonValue;
use safe_cex_venue_kucoin::api_helpers::{is_public_endpoint, stringify_query};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
#[test]
fn live_query_serialization_and_public_prefixes() {
    let values = vec![
        json!({}),
        json!(null),
        json!(false),
        json!("text"),
        json!([1, null, "x"]),
        json!({"a":null,"b":false,"c":0,"d":"","e":[1,null,"x"],"f":{"x":"a b","y":"!()*~"}}),
        json!({"10":"ten","2":"two","01":"one","4294967294":"last-index","4294967295":"not-index","arr":[[],{},[1,2]]}),
        json!({"unicode":"😀 / + & = # \u{feff}","a[b]":{"x.y":"å"}}),
        json!({"toString":"x","__proto__":"y","constructor":"z"}),
        json!({"small":1e-7,"large":1e21,"negative":-2.5}),
    ];
    let mut urls = vec![None, Some(String::new()), Some("/api/v1/accounts".into())];
    for endpoint in safe_cex_venue_kucoin::spot_types::KUCOIN_SPOT_PUBLIC_ENDPOINTS {
        for url in [
            endpoint.to_string(),
            format!("{endpoint}?a=1"),
            format!("{endpoint}suffix"),
            format!("https://api.kucoin.com{endpoint}"),
            endpoint.to_uppercase(),
        ] {
            urls.push(Some(url));
        }
    }
    let mut cases = vec![];
    for value in values {
        for url in &urls {
            cases.push(json!({"value":value,"url":url}));
        }
    }
    cases.push(json!({"special":true}));
    let mut child = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/query.cjs"))
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
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(cases.len(), 281, "KUCOIN-CARDINALITY query input total");
    assert_eq!(expected.len(), 281, "KUCOIN-CARDINALITY query source total");
    let mut compared_cases = 0;
    for index in 0..281 {
        let c = &cases[index];
        let expected = &expected[index];
        compared_cases += 1;
        let value = if c["special"] == true {
            JsJsonValue::Object(vec![
                ("missing".into(), JsJsonValue::Undefined),
                ("nil".into(), JsJsonValue::Null),
                (
                    "array".into(),
                    JsJsonValue::Array(vec![
                        JsJsonValue::Undefined,
                        JsJsonValue::Null,
                        JsJsonValue::Number(f64::NAN),
                        JsJsonValue::Number(f64::INFINITY),
                        JsJsonValue::Number(f64::NEG_INFINITY),
                        JsJsonValue::Number(-0.0),
                    ]),
                ),
                (
                    "big".into(),
                    JsJsonValue::BigInt("900719925474099312345".parse().unwrap()),
                ),
            ])
        } else {
            JsJsonValue::from(c["value"].clone())
        };
        assert_eq!(
            json!({"encoded":stringify_query(&value,true),"raw":stringify_query(&value,false),"public":is_public_endpoint(c["url"].as_str())}),
            *expected,
            "case {c}"
        );
    }
    assert_eq!(
        compared_cases, 281,
        "KUCOIN-CARDINALITY query compared cases"
    );
}
