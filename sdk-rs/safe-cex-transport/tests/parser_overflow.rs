use serde_json::{Value, json};

#[test]
fn parser_overflow_reaches_venue_string_and_boolean_branches() {
    let script = r#"
const fs = require('node:fs');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const source = fs.readFileSync('packages/json-parse/src/jsonParse.ts', 'utf8');
const module = {exports:{}};
new Function('module', 'exports', ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText)(module,module.exports);
const text = '{"seqId":1e400,"negative":-1e400,"nested":[1e400],"rounded":9007199254740993,"decimal":1.50,"exponent":1e3}';
process.stdout.write(JSON.stringify(Object.values(module.exports).map(parse => {
 const v=parse(text);
 return [String(v.seqId), Boolean(v.seqId), String(v.negative), String(v.nested[0]), String(v.rounded), v.decimal, v.exponent];
})));
"#;
    let output = std::process::Command::new("node")
        .args(["-e", script])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        expected.as_array().unwrap().len(),
        2,
        "OVERFLOW-CARDINALITY source parsers"
    );
    let text = r#"{"seqId":1e400,"negative":-1e400,"nested":[1e400],"rounded":9007199254740993,"decimal":1.50,"exponent":1e3}"#;
    for (i, parse) in [json_parse, big_int_as_string_json_parse]
        .into_iter()
        .enumerate()
    {
        let v = parse(text);
        let string = |value: &Value| safe_cex_num::js_string(Some(value)).unwrap();
        let actual = json!([
            string(&v["seqId"]),
            safe_cex_num::js_truthy(Some(&v["seqId"])),
            string(&v["negative"]),
            string(&v["nested"][0]),
            string(&v["rounded"]),
            v["decimal"],
            v["exponent"]
        ]);
        assert_eq!(
            actual, expected[i],
            "parser {i}: overflow must reach venue interpretation"
        );
        assert!(v["seqId"].is_number());
    }
}

fn big_int_as_string_json_parse(text: &str) -> serde_json::Value {
    safe_cex_transport::RestResponseBody::Transformed(
        safe_cex_transport::how::big_int_as_string_json_parse(text),
    )
    .json_value()
    .unwrap()
}

fn json_parse(text: &str) -> serde_json::Value {
    safe_cex_transport::RestResponseBody::Transformed(safe_cex_transport::how::json_parse(text))
        .json_value()
        .unwrap()
}
