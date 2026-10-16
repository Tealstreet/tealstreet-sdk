use safe_cex_venue_kucoin::order_helpers::{
    parse_margin_type, parse_position_mode, parse_row_position_mode,
};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
#[test]
fn live_mode_parsers_preserve_js_coercion() {
    let inputs = vec![
        json!(null),
        json!(false),
        json!(true),
        json!(0),
        json!(1),
        json!(2),
        json!(""),
        json!("ISOLATED"),
        json!("fixed"),
        json!("cross"),
        json!("crossed"),
        json!("HEDGE_MODE"),
        json!("hedge"),
        json!("double_hold"),
        json!("1"),
        json!("1.0"),
        json!(" ISOLATED"),
        json!("\u{feff}ISOLATED"),
        json!([]),
        json!(["fixed"]),
        json!(["HEDGE"]),
        json!({}),
        json!({"toString":null}),
        json!("ıSOLATED"),
        json!("ﬁxed"),
    ];
    let mut cases = vec![
        json!({"name":"parseMarginType","args":[]}),
        json!({"name":"parsePositionMode","args":[]}),
    ];
    for value in &inputs {
        cases.push(json!({"name":"parsePositionMode","args":[value]}));
        for cross in [
            Value::Null,
            json!(false),
            json!(true),
            json!(0),
            json!("false"),
        ] {
            cases.push(json!({"name":"parseMarginType","args":[value,cross]}));
        }
        for side in [
            Value::Null,
            json!("BOTH"),
            json!("both"),
            json!("LONG"),
            json!(""),
            json!(false),
            json!(1),
            json!([]),
        ] {
            cases.push(json!({"name":"parseRowPositionMode","args":[{"positionSide":side,"positionMode":value,"holdMode":"hedge"}]}));
        }
    }
    let mut child = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/parsers.cjs"))
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
    assert_eq!(cases.len(), 352, "KUCOIN-CARDINALITY parsers input total");
    assert_eq!(
        expected.len(),
        352,
        "KUCOIN-CARDINALITY parsers source total"
    );
    let mut compared_cases = 0;
    for index in 0..352 {
        let case = &cases[index];
        let expected = &expected[index];
        compared_cases += 1;
        let args = case["args"].as_array().unwrap();
        let result = match case["name"].as_str().unwrap() {
            "parseMarginType" => parse_margin_type(args.first(), args.get(1)).map(|v| json!(v)),
            "parsePositionMode" => parse_position_mode(args.first()).map(|v| json!(v)),
            _ => parse_row_position_mode(args[0].as_object().unwrap()).map(|v| json!(v)),
        };
        let actual = match result {
            Ok(v) => json!({"ok":v}),
            Err(e) => json!({"error":e.to_string()}),
        };
        assert_eq!(actual, *expected, "{case}");
    }
    assert_eq!(
        compared_cases, 352,
        "KUCOIN-CARDINALITY parsers compared cases"
    );
}
