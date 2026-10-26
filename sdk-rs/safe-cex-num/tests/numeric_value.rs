use safe_cex_num::{numeric_ingress::js_from_str, numeric_value_to_number};
use safe_cex_types::{MarketLimitsAmount, NumericValue};

#[test]
fn runtime_numeric_text_stays_text_until_arithmetic() {
    let texts = [
        "1200",
        "001200",
        "-0",
        "  \u{feff}0x10 ",
        "Infinity",
        "3x",
        "",
        "1.25",
    ];
    let output = std::process::Command::new("node")
        .args(["-e", "const values=JSON.parse(process.argv[1]);console.log(JSON.stringify(values.map(value=>{const n=Number(value),b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'NaN':b.toString('hex')})))", &serde_json::to_string(&texts).unwrap()])
        .output().unwrap();
    assert!(output.status.success());
    let expected: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), texts.len());
    for (text, bits) in texts.into_iter().zip(expected) {
        let input = format!(
            r#"{{"min":0,"max":{}}}"#,
            serde_json::to_string(text).unwrap()
        );
        let amount: MarketLimitsAmount = js_from_str(&input).unwrap();
        assert_eq!(amount.max, NumericValue::Text(text.into()));
        assert_eq!(serde_json::to_value(&amount).unwrap()["max"], text);
        let number = numeric_value_to_number(&amount.max);
        if bits == "NaN" {
            assert!(number.is_nan(), "{text:?}");
        } else {
            assert_eq!(format!("{:016x}", number.to_bits()), bits, "{text:?}");
        }
    }
}

#[test]
fn ordinary_numeric_limits_keep_their_native_bits() {
    for (token, expected) in [("-0", -0.0f64), ("1e400", f64::INFINITY), ("12.5", 12.5)] {
        let amount: MarketLimitsAmount =
            js_from_str(&format!(r#"{{"min":0,"max":{token}}}"#)).unwrap();
        let NumericValue::Number(value) = amount.max else {
            panic!("numeric limit")
        };
        assert_eq!(value.to_bits(), expected.to_bits());
    }
    for token in ["null", "true", "[]", "{}", r#"{"Number":1200}"#] {
        assert!(
            js_from_str::<MarketLimitsAmount>(&format!(r#"{{"min":0,"max":{token}}}"#)).is_err()
        );
    }
}
