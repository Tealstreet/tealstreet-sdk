use safe_cex_num::JsCoercionError;
use safe_cex_venue_okx::finite::{parse_finite, sum_finite};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn finite_helpers_native_values_before_source() {
    let parse_cases = [
        (None, 0.0_f64),
        (Some(json!(null)), 0.0),
        (Some(json!(false)), 0.0),
        (Some(json!(true)), 0.0),
        (Some(json!("")), 0.0),
        (Some(json!(" \t")), 0.0),
        (Some(json!("12.5tail")), 12.5),
        (Some(json!("0x10")), 0.0),
        (Some(json!("-0")), -0.0),
        (Some(json!("-1.25")), -1.25),
        (Some(json!("1e2tail")), 100.0),
        (Some(json!("1e+tail")), 1.0),
        (Some(json!("Infinity")), 0.0),
        (Some(json!("-Infinity")), 0.0),
        (Some(json!("1e309")), 0.0),
        (Some(json!("NaN")), 0.0),
        (Some(json!("\u{feff}4.5")), 4.5),
        (Some(json!("\u{85}4.5")), 0.0),
        (Some(json!([])), 0.0),
        (Some(json!(["3.5"])), 3.5),
        (Some(json!([2, 3])), 2.0),
        (Some(json!([["4.25"], null])), 4.25),
        (Some(json!({})), 0.0),
        (Some(json!(-2.75)), -2.75),
    ];
    let sum_cases = [
        (json!([]), 0.0_f64),
        (json!(["-0"]), 0.0),
        (json!(["2.5", "3tail", null, false, "NaN"]), 5.5),
        (json!([1e16, 1, -1e16]), 0.0),
        (json!([1e16, -1e16, 1]), 1.0),
        (json!(["1e308", "1e308"]), f64::INFINITY),
        (json!(["-1e308", "-1e308"]), f64::NEG_INFINITY),
        (json!([["4"], [2, 3], {}, "Infinity"]), 6.0),
    ];
    let mut cases = vec![];
    let mut native = vec![];
    for (index, (value, expected)) in parse_cases.into_iter().enumerate() {
        let actual = parse_finite(value.as_ref()).unwrap();
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "OKX-FINITE native parse {index}"
        );
        cases.push(value.map_or(
            json!({"method":"parse"}),
            |value| json!({"method":"parse","value":value}),
        ));
        native.push(json!({"bits":format!("{:016x}",actual.to_bits())}));
    }
    for (index, (values, expected)) in sum_cases.into_iter().enumerate() {
        let actual = sum_finite(values.as_array().unwrap()).unwrap();
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "OKX-FINITE native sum {index}"
        );
        cases.push(json!({"method":"sum", "values":values}));
        native.push(json!({"bits":format!("{:016x}",actual.to_bits())}));
    }
    let expected = JsCoercionError {
        name: "TypeError",
        message: "Cannot convert object to primitive value",
    };
    for value in [json!({"toString":null}), json!([{"toString":false}])] {
        let actual = parse_finite(Some(&value)).unwrap_err();
        assert_eq!(actual, expected, "OKX-FINITE native coercion error");
        cases.push(json!({"method":"parse", "value":value}));
        native.push(json!({"error":{"name":actual.name,"message":actual.message}}));
    }
    let values = json!([1,{"toString":null},"2"]);
    let actual = sum_finite(values.as_array().unwrap()).unwrap_err();
    assert_eq!(actual, expected, "OKX-FINITE native sum error");
    cases.push(json!({"method":"sum", "values":values}));
    native.push(json!({"error":{"name":actual.name,"message":actual.message}}));
    assert_eq!(
        parse_finite(Some(&json!({}))).unwrap(),
        0.0,
        "OKX-FINITE native callable JSON-view boundary"
    );
    assert_eq!(
        sum_finite(&[json!({})]).unwrap(),
        0.0,
        "OKX-FINITE native callable sum JSON-view boundary"
    );
    assert_eq!(cases.len(), 35, "declared finite helper profile floor");
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/finite-oracle.mjs"
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
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 35, "source finite helper profile floor");
    assert_eq!(
        native, source,
        "maintained finite helpers after native assertions"
    );
    let witness = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/finite-oracle.mjs"
        ))
        .arg("--runtime-witness")
        .output()
        .unwrap();
    assert!(
        witness.status.success(),
        "{}",
        String::from_utf8_lossy(&witness.stderr)
    );
    let witness: Value = serde_json::from_slice(&witness.stdout).unwrap();
    assert_eq!(
        witness["parsed"], 7.5,
        "OKX-FINITE source callable parse witness"
    );
    assert_eq!(
        witness["summed"], 7.5,
        "OKX-FINITE source callable sum witness"
    );
    assert_eq!(
        witness["jsonView"],
        json!({}),
        "OKX-FINITE source chosen JSON view"
    );
}
