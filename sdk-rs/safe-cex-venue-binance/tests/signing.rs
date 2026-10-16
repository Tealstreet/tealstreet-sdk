use safe_cex_crypto::{PayloadSigner, PayloadSignerRequest, PayloadSignerResult, SigningOptions};
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_types::ExchangeName;
use safe_cex_venue_binance::signing::{serialize_query, sign_binance_request};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    io::Write,
    pin::Pin,
    process::{Command, Stdio},
    sync::Arc,
    task::{Context, Poll, Waker},
};
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture must complete"),
    }
}
struct Signer {
    headers: std::collections::BTreeMap<String, String>,
    fail: bool,
    calls: RefCell<Vec<Value>>,
}
impl PayloadSigner for Signer {
    fn sign<'a>(
        &'a self,
        request: PayloadSignerRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<PayloadSignerResult, String>> + 'a>> {
        self.calls.borrow_mut().push(json!({"exchangeName":"binance","key":request.key,"scheme":"hmac-sha256","encoding":"hex","payload":request.payload}));
        Box::pin(async move {
            if self.fail {
                Err("fixture signer refusal".into())
            } else {
                Ok(PayloadSignerResult {
                    signature: "fixture-signature".into(),
                    headers: Some(self.headers.clone()),
                })
            }
        })
    }
}
fn actual(v: &Value) -> Value {
    let signer = Signer {
        headers: v
            .get("signerHeaders")
            .map(|h| serde_json::from_value(h.clone()).unwrap())
            .unwrap_or_else(|| {
                [
                    ("X-Proof".into(), "signer".into()),
                    ("X-Shared".into(), "signer".into()),
                ]
                .into()
            }),
        fail: v["mode"] == "failure",
        calls: RefCell::default(),
    };
    let options = SigningOptions {
        key: v["options"]["key"].as_str().unwrap(),
        secret: v["options"]["secret"].as_str().unwrap(),
        payload_signer: if v["mode"] == "callback" || v["mode"] == "failure" {
            Some(&signer)
        } else {
            None
        },
    };
    let now = v["now"].as_f64().unwrap();
    let mut clock = VirtualClock::default();
    clock.now = Arc::new(move || now);
    let result = run(sign_binance_request(
        v["config"].as_object().unwrap(),
        &options,
        ExchangeName::Binance,
        v["options"]["remoteSigner"].as_bool().unwrap_or(false),
        v["window"].as_f64().unwrap(),
        &clock,
    ));
    match result {
        Ok(config) => {
            json!({"query":serialize_query(config.get("params").unwrap_or(&Value::Null)),"inputQuery":serialize_query(&v["config"]["params"]),"config":config,"calls":*signer.calls.borrow()})
        }
        Err(error) => json!({"error":error,"calls":*signer.calls.borrow()}),
    }
}
fn normalize(value: Value) -> Value {
    match value {
        Value::Number(n) => {
            let v = n.as_f64().unwrap();
            json!(if v == 0. { 0. } else { v })
        }
        Value::Array(v) => Value::Array(v.into_iter().map(normalize).collect()),
        Value::Object(v) => Value::Object(v.into_iter().map(|(k, v)| (k, normalize(v))).collect()),
        v => v,
    }
}
#[test]
fn signed_numbers_use_js_exponents_and_signature_is_last() {
    let case = json!({"config":{"data":{"amount":1e21,"tiny":1e-7,"signature":"old"}},"options":{"key":"k","secret":"s"},"mode":"callback","now":1234,"window":20000});
    let output = actual(&case);
    assert_eq!(
        output["calls"][0]["payload"],
        "amount=1e%2B21&tiny=1e-7&timestamp=1234&recvWindow=20000"
    );
    assert!(
        output["query"]
            .as_str()
            .unwrap()
            .ends_with("&signature=fixture-signature")
    );
    assert!(output["config"].get("data").is_none());
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn signer_and_query_codec_match_live_typescript() {
    let mut values = vec![
        Value::Null,
        json!(false),
        json!(0),
        json!(""),
        json!({}),
        json!([]),
        json!(["A B", "C+雪"]),
        json!({"symbols":["A B","C"],"nested":{"a":null,"b":true}}),
        json!({"9":"nine","2":"two","01":"one","signature":"stale","timestamp":0,"recvWindow":1}),
        json!("plain"),
        json!({"x":1}),
        json!("{\"x\":1}"),
    ];
    for n in [
        0.,
        -0.,
        1e-8,
        1e-7,
        1e-6,
        1e-5,
        1e20,
        1e21,
        1e22,
        -1e21,
        9007199254740991.,
        0.000000123456789,
    ] {
        values.push(json!({"amount":n}));
    }
    for text in [
        "'()!*~ -._",
        "雪😀",
        "\r\n\u{2028}",
        "a&b=c#d%",
        "\u{feff}\u{85}",
    ] {
        values.push(json!({"symbol":text,"list":[null,text,""],"nested":[{"x":text},{"y":2}]}));
    }
    let mut cases = vec![];
    for value in values {
        for mode in [
            "local",
            "callback",
            "failure",
            "remote",
            "no-key",
            "no-secret",
        ] {
            for body in [false, true] {
                let mut config = json!({"method":"POST","url":"/order","params":value,"headers":{"x-shared":"caller","X-API":"key"}});
                if body {
                    config["data"] = value.clone();
                    config["params"] = json!({"fallback":true});
                }
                cases.push(json!({"config":config,"options":{"key":if mode=="no-key" {""}else{"k"},"secret":if mode=="no-secret" {""}else{"fixture-only-secret"},"remoteSigner":mode=="remote"},"mode":mode,"now":1712345678901_f64,"window":20000}));
            }
        }
    }
    for raw in ["1e400", "-1e400"] {
        for mode in ["local", "callback", "failure"] {
            let data: Value = serde_json::from_str(raw).unwrap();
            cases.push(json!({"config":{"data":data,"params":{"fallback":true}},"options":{"key":"k","secret":"s"},"mode":mode,"now":1,"window":2}));
        }
    }
    for headers in [
        json!({"X-Array":["a","b"],"X-Value":"  a\r\nb\t "}),
        json!({"X-Array":[null,false,["  a ","b\r"]],"X-Shared":false,"x-shared":"blocked"}),
        json!({"  ":"ignored"," X-Name ":"space name","x-name":"distinct","X-Bom":"\u{feff}a\u{85}"}),
    ] {
        cases.push(json!({"config":{"params":{"symbol":"BTC"},"headers":headers},"options":{"key":"k","secret":"s"},"mode":"callback","now":1,"window":2}));
    }
    for code in 0..=127 {
        let value = format!(" \tA{}B\t ", char::from_u32(code).unwrap());
        cases.push(json!({"config":{"params":{},"headers":{"X-Caller":value}},"signerHeaders":{"X-Signer":value},"options":{"key":"k","secret":"s"},"mode":"callback","now":1,"window":2}));
    }
    let mut child = Command::new("node")
        .arg(format!(
            "{}/scripts/signing.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
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
    assert!(
        !cases.is_empty(),
        "signing source input corpus must execute"
    );
    assert_eq!(
        expected.len(),
        cases.len(),
        "signing oracle output cardinality"
    );
    for (i, (v, expected)) in cases.iter().zip(expected).enumerate() {
        assert_eq!(
            normalize(actual(v)),
            normalize(expected),
            "live signing case {i}: {v}"
        );
    }
    eprintln!("{} live Binance signing/codec cases", cases.len());
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn utf16_string_body_spread_divergence_is_pinned() {
    let case = json!({"config":{"data":"😀"},"options":{"key":"k","secret":"s"},"mode":"callback","now":1,"window":2,"lossy":true});
    let mut child = Command::new("node")
        .arg(format!(
            "{}/scripts/signing.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&vec![case.clone()]).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        expected[0]["calls"][0]["payload"],
        "0=%F0%9F%90%80&1=%F2%90%80%80&timestamp=1&recvWindow=2"
    );
    assert_eq!(
        actual(&case)["calls"][0]["payload"],
        "0=%EF%BF%BD&1=%EF%BF%BD&timestamp=1&recvWindow=2"
    );
}

#[test]
fn pre_transform_body_identity_changes_signed_bytes() {
    let object = json!({"x":1});
    let text = serde_json::to_string(&object).unwrap();
    assert_eq!(serde_json::to_vec(&object).unwrap(), text.as_bytes());
    let case = |data| json!({"mode":"callback","config":{"data":data},"options":{"key":"k","secret":"s"},"now":1,"window":2});
    let object_signed = actual(&case(object));
    let string_signed = actual(&case(Value::String(text)));
    assert_eq!(
        object_signed["calls"][0]["payload"],
        "x=1&timestamp=1&recvWindow=2"
    );
    assert_eq!(
        string_signed["calls"][0]["payload"],
        "0=%7B&1=%22&2=x&3=%22&4=%3A&5=1&6=%7D&timestamp=1&recvWindow=2"
    );
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn signer_oracle_detects_source_mutation_and_missing_packages() {
    let output = Command::new("node")
        .arg(format!(
            "{}/scripts/signing-controls.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "mandatory live alias witness: yarn venue-oracle:binance"]
fn signing_nested_params_live_alias_differs_from_owned_snapshot() {
    let mut vectors = vec![];
    for remote in [false, true] {
        for key in ["", "key"] {
            for secret in ["", "secret"] {
                vectors.push(json!({"aliasWitness":true,"config":{"params":{"nested":{"label":"initial"}}},"options":{"key":key,"secret":secret,"remoteSigner":remote},"mode":"local","now":1,"window":2}));
            }
        }
    }
    let mut child = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/signing.mjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&vectors).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "alias source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), vectors.len());
    for (i, v) in vectors.into_iter().enumerate() {
        let mut input = v["config"].clone();
        let options = SigningOptions {
            key: v["options"]["key"].as_str().unwrap(),
            secret: v["options"]["secret"].as_str().unwrap(),
            payload_signer: None,
        };
        let mut clock = VirtualClock::default();
        clock.now = Arc::new(|| 1.0);
        let mut result = Value::Object(
            run(sign_binance_request(
                input.as_object().unwrap(),
                &options,
                ExchangeName::Binance,
                v["options"]["remoteSigner"] == true,
                2.0,
                &clock,
            ))
            .unwrap(),
        );
        input["params"]["nested"]["label"] = json!("input-mutated");
        let from_input = json!({"input":input["params"]["nested"]["label"],"output":result["params"]["nested"]["label"]});
        result["params"]["nested"]["label"] = json!("output-mutated");
        let from_output = json!({"input":input["params"]["nested"]["label"],"output":result["params"]["nested"]["label"]});
        assert_eq!(
            source[i],
            json!({"sameNested":true,"fromInput":{"input":"input-mutated","output":"input-mutated"},"fromOutput":{"input":"output-mutated","output":"output-mutated"}}),
            "TS live alias case {i}"
        );
        assert_eq!(
            from_input,
            json!({"input":"input-mutated","output":"initial"}),
            "Rust owned snapshot input direction {i}"
        );
        assert_eq!(
            from_output,
            json!({"input":"input-mutated","output":"output-mutated"}),
            "Rust owned snapshot output direction {i}"
        );
        assert_ne!(
            source[i]["fromInput"], from_input,
            "declared alias difference input {i}"
        );
        assert_ne!(
            source[i]["fromOutput"], from_output,
            "declared alias difference output {i}"
        );
    }
}
