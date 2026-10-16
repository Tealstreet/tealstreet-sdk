use safe_cex_num::JsPrimitive;
use safe_cex_venue_bingx::precision::parse_precision;
use serde_json::Value;

#[test]
fn precision_matches_actual_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/precision_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    println!("capture {}", corpus["runtime"]);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 45, "BINGX-PARSE source total");
    let mut counts = [0, 0];
    for case in cases {
        let (input, group) = match case["kind"].as_str().unwrap() {
            "string" => (
                JsPrimitive::String(case["text"].as_str().unwrap().to_owned()),
                0,
            ),
            "number" => (
                JsPrimitive::Number(f64::from_bits(
                    u64::from_str_radix(case["bits"].as_str().unwrap(), 16).unwrap(),
                )),
                1,
            ),
            _ => panic!("unexpected kind"),
        };
        assert_eq!(
            parse_precision(&input).as_deref(),
            case["expected"].as_str(),
            "BINGX-PARSE native precision {case}"
        );
        counts[group] += 1;
    }
    assert_eq!(counts, [27, 18], "BINGX-PARSE executed subcorpora");
    let boundary = corpus["boundary"].as_array().unwrap();
    assert_eq!(boundary.len(), 2, "BINGX-PARSE boundary count");
    for (case, expected) in boundary.iter().zip(["0.001", "0.1"]) {
        let units: Vec<u16> = serde_json::from_value(case["units"].clone()).unwrap();
        assert!(String::from_utf16(&units).is_err());
        assert_eq!(
            case["expected"], expected,
            "BINGX-PARSE source UTF8 boundary"
        );
    }
    assert_eq!(parse_precision(&JsPrimitive::Null), None);
}
