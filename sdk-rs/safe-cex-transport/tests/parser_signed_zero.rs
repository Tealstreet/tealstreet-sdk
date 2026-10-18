use serde_json::Value;

fn compare(parser: fn(&str) -> Value, export: &str) {
    let script = r#"
const load = require('./sdk-rs/safe-cex-transport/scripts/ts-source.cjs');
const parse = load('./packages/json-parse/src/jsonParse.ts')[process.argv[1]];
const inputs=['-0','-0.0','-0e10','-1e-400','0','0.0','1e-400','-5e-324','1.50','9007199254740993'];
const observe = n => { const b=Buffer.alloc(8); b.writeDoubleBE(n); return {bits:b.toString('hex'),text:String(n),truthy:Boolean(n),wire:JSON.stringify(n)}; };
process.stdout.write(JSON.stringify(inputs.map(input=>({input,root:observe(parse(input)),nested:observe(parse('{"data":'+input+'}').data)}))));
"#;
    let output = std::process::Command::new("node")
        .args(["-e", script, export])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 10, "ZERO-CARDINALITY source cases");
    for case in cases {
        let input = case["input"].as_str().unwrap();
        for (location, value) in [
            ("root", parser(input)),
            (
                "nested",
                parser(&format!("{{\"data\":{input}}}"))["data"].clone(),
            ),
        ] {
            let number = safe_cex_num::js_number(Some(&value)).unwrap();
            assert_eq!(
                format!("{:016x}", number.to_bits()),
                case[location]["bits"],
                "{export} {location} {input}: binary64 sign and value"
            );
            assert_eq!(
                safe_cex_num::js_string(Some(&value)).unwrap(),
                case[location]["text"]
            );
            assert_eq!(
                safe_cex_num::js_truthy(Some(&value)),
                case[location]["truthy"]
            );
            assert_eq!(
                safe_cex_num::js_json_stringify(&value.into())
                    .unwrap()
                    .unwrap(),
                case[location]["wire"]
            );
        }
    }
}
#[test]
fn plain_parser_preserves_signed_zero_before_serialization() {
    compare(json_parse, "jsonParse");
}
#[test]
fn bigint_parser_preserves_signed_zero_before_serialization() {
    compare(big_int_as_string_json_parse, "bigIntAsStringJsonParse");
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
