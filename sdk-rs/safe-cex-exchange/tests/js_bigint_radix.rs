//! Compare the shared BigInt member against the actual Node engine.
use safe_cex_exchange::js_runtime::value::Js;
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};

fn radix(spec: &Value) -> Vec<Js> {
    let value = &spec["value"];
    match spec["kind"].as_str().unwrap() {
        "omitted" => vec![],
        "undefined" => vec![Js::Undefined],
        "null" => vec![Js::Null],
        "bool" => vec![Js::Bool(value.as_bool().unwrap())],
        "number" => vec![Js::Number(match value.as_str().unwrap() {
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "NaN" => f64::NAN,
            text => text.parse().unwrap(),
        })],
        "string" => vec![Js::str(value.as_str().unwrap())],
        "bigint" => vec![Js::BigInt(Rc::new(
            value.as_str().unwrap().parse().unwrap(),
        ))],
        "array" => vec![Js::array(
            value
                .as_array()
                .unwrap()
                .iter()
                .map(|v| Js::Number(v.as_f64().unwrap()))
                .collect(),
        )],
        _ => panic!("unknown radix descriptor"),
    }
}

#[test]
fn bigint_radix_matches_node_in_every_base_and_coercion_boundary() {
    let mut radices: Vec<Value> = (2..=36)
        .map(|n| json!({"kind":"number", "value": n.to_string()}))
        .collect();
    radices.extend([
        json!({"kind":"omitted"}),
        json!({"kind":"undefined"}),
        json!({"kind":"null"}),
        json!({"kind":"bool", "value":false}),
        json!({"kind":"bool", "value":true}),
        json!({"kind":"number", "value":"0"}),
        json!({"kind":"number", "value":"-0"}),
        json!({"kind":"number", "value":"NaN"}),
        json!({"kind":"number", "value":"Infinity"}),
        json!({"kind":"number", "value":"-Infinity"}),
        json!({"kind":"number", "value":"1"}),
        json!({"kind":"number", "value":"37"}),
        json!({"kind":"number", "value":"-2"}),
        json!({"kind":"number", "value":"2.9"}),
        json!({"kind":"number", "value":"36.9"}),
        json!({"kind":"number", "value":"1.9"}),
        json!({"kind":"string", "value":"16"}),
        json!({"kind":"string", "value":"0x10"}),
        json!({"kind":"string", "value":" "}),
        json!({"kind":"string", "value":"bad"}),
        json!({"kind":"string", "value":"36.9"}),
        json!({"kind":"bigint", "value":"16"}),
        json!({"kind":"array", "value":[16]}),
        json!({"kind":"array", "value":[]}),
    ]);
    assert_eq!(radices.len(), 59, "BigInt radix native census");
    let values = [
        "0",
        "1",
        "-1",
        "255",
        "-255",
        "9007199254740993",
        "115792089237316195423570985008687907853269984665640564039457584007913129639935",
        "-115792089237316195423570985008687907853269984665640564039457584007913129639935",
    ];
    let cases: Vec<Value> = values
        .iter()
        .flat_map(|value| {
            radices
                .iter()
                .map(move |radix| json!({"value":value,"radix":radix}))
        })
        .collect();
    assert_eq!(cases.len(), 472, "BigInt radix native census");
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_bigint_radix_oracle.cjs"
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
        .write_all(serde_json::to_string(&cases).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Node: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!("BigInt radix Node {} / {}", oracle["node"], oracle["arch"]);
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 472, "BigInt radix source census");
    for index in 0..472 {
        let case = &cases[index];
        let number = Js::BigInt(Rc::new(case["value"].as_str().unwrap().parse().unwrap()));
        let observed = match number.call_method("toString", radix(&case["radix"])) {
            Ok(value) => {
                assert!(
                    matches!(value, Js::String(_)),
                    "BigInt radix result type {index}"
                );
                json!({"ok":value.to_display()})
            }
            Err(error) => json!({"error":error.value.get("name").unwrap().to_display()}),
        };
        assert_eq!(
            observed, rows[index],
            "BigInt radix source case {index}: {case}"
        );
    }
}

#[test]
fn bigint_to_string_uses_the_actual_call_receiver() {
    let first = Js::BigInt(Rc::new(255.into()));
    let method = first.get("toString").unwrap();
    let second = Js::BigInt(Rc::new((-256).into()));
    assert_eq!(
        method
            .call(second, vec![Js::Number(16.)])
            .unwrap()
            .to_display(),
        "-100"
    );
    for receiver in [
        Js::Undefined,
        Js::Null,
        Js::Number(255.),
        Js::str("255"),
        Js::object(vec![]),
    ] {
        let error = method.call(receiver, vec![Js::Number(16.)]).unwrap_err();
        assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
    }
}
