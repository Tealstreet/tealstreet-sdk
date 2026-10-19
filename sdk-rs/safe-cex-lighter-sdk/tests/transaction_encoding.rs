use num_bigint::BigInt;
use safe_cex_lighter_sdk::{amount_encoding::*, transaction_encoding::*};
use serde_json::Value;
use std::sync::OnceLock;
fn corpus() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/transaction_encoding_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "SDK-ENC actual source: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn units(v: &Value) -> Vec<u16> {
    v["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u16)
        .collect()
}
fn number(v: &Value) -> f64 {
    let bits = v["number"].as_str().unwrap();
    if bits == "NaN" {
        f64::NAN
    } else {
        f64::from_bits(u64::from_str_radix(bits, 16).unwrap())
    }
}
fn big(v: &Value) -> BigInt {
    v["bigint"].as_str().unwrap().parse().unwrap()
}
fn compare<T>(case: &Value, actual: Result<T, EncodingError>, decode: impl Fn(&Value) -> T)
where
    T: std::fmt::Debug + PartialEq,
{
    let method = case["method"].as_str().unwrap();
    let label = case["label"].as_u64().unwrap();
    if let Some(expected) = case["output"].get("ok") {
        assert_eq!(
            actual,
            Ok(decode(expected)),
            "SDK-ENC native {method} {label}"
        );
    } else {
        let error = actual
            .err()
            .unwrap_or_else(|| panic!("SDK-ENC expected error {method} {label}"));
        assert_eq!(
            error.name,
            case["output"]["error"]["name"].as_str().unwrap(),
            "SDK-ENC error name {method} {label}"
        );
        let message: Vec<u16> = case["output"]["error"]["message"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u.as_u64().unwrap() as u16)
            .collect();
        assert_eq!(
            error.message, message,
            "SDK-ENC error units {method} {label}"
        );
    }
}
struct PrimitiveBindings;
impl TextIntrinsics for PrimitiveBindings {
    fn pow10(&self, exponent: &BigInt) -> Result<BigInt, EncodingError> {
        let exponent = u32::try_from(exponent).expect("finite oracle exponent");
        Ok(BigInt::from(10).pow(exponent))
    }
    fn pad_zeros(&self, text: &[u16], target: f64, start: bool) -> Result<Vec<u16>, EncodingError> {
        if target == f64::INFINITY {
            return Err(EncodingError {
                name: "RangeError",
                message: "Invalid string length".encode_utf16().collect(),
            });
        }
        let target = if target.is_nan() || target < 0.0 {
            0
        } else {
            target.trunc() as usize
        };
        if target <= text.len() {
            return Ok(text.to_vec());
        }
        let mut output = Vec::new();
        if !start {
            output.extend_from_slice(text);
        }
        output.extend(std::iter::repeat_n(48, target - text.len()));
        if start {
            output.extend_from_slice(text);
        }
        Ok(output)
    }
}
#[test]
fn amount_utilities_match_maintained_typescript() {
    let cases = corpus()["cases"].as_array().unwrap();
    let runtime = PrimitiveBindings;
    for method in ["bigIntToHex", "formatAmount", "parseAmount"] {
        let rows: Vec<_> = cases.iter().filter(|c| c["method"] == method).collect();
        assert_eq!(
            rows.len(),
            if method == "parseAmount" { 16 } else { 12 },
            "SDK-ENC independent utility floor {method}"
        );
        for case in rows {
            let args = case["args"].as_array().unwrap();
            if method == "parseAmount" {
                compare(
                    case,
                    parse_amount(&runtime, &units(&args[0]), number(&args[1])),
                    big,
                );
            } else if method == "formatAmount" {
                compare(
                    case,
                    format_amount(&runtime, &big(&args[0]), number(&args[1])),
                    units,
                );
            } else {
                compare(
                    case,
                    big_int_to_hex(&runtime, &big(&args[0]), args.get(1).map(number)),
                    units,
                );
            }
        }
    }
}
#[test]
fn utility_bodies_forward_exact_intrinsic_inputs_and_failures() {
    struct Recording {
        calls: std::cell::RefCell<Vec<String>>,
    }
    impl TextIntrinsics for Recording {
        fn pow10(&self, exponent: &BigInt) -> Result<BigInt, EncodingError> {
            self.calls.borrow_mut().push(format!("pow:{exponent}"));
            PrimitiveBindings.pow10(exponent)
        }
        fn pad_zeros(
            &self,
            text: &[u16],
            target: f64,
            start: bool,
        ) -> Result<Vec<u16>, EncodingError> {
            self.calls
                .borrow_mut()
                .push(format!("pad:{text:?}:{:x}:{start}", target.to_bits()));
            PrimitiveBindings.pad_zeros(text, target, start)
        }
    }
    let runtime = Recording {
        calls: Default::default(),
    };
    assert_eq!(
        big_int_to_hex(&runtime, &BigInt::from(-255), Some(3.75)).unwrap(),
        "0x0000-ff".encode_utf16().collect::<Vec<_>>(),
        "SDK-ENC signed hex padding"
    );
    assert_eq!(
        runtime.calls.borrow()[0],
        format!(
            "pad:{:?}:{:x}:true",
            vec![45u16, 102, 102],
            7.5f64.to_bits()
        ),
        "SDK-ENC raw fractional pad target"
    );
    runtime.calls.borrow_mut().clear();
    format_amount(&runtime, &BigInt::from(-5), 3.0).unwrap();
    assert_eq!(
        runtime.calls.borrow()[0],
        "pow:3",
        "SDK-ENC raw BigInt exponent"
    );
    assert_eq!(
        runtime.calls.borrow()[1],
        format!("pad:{:?}:{:x}:true", vec![45u16, 53], 3.0f64.to_bits()),
        "SDK-ENC signed fractional primitive input"
    );
    struct Failing;
    impl TextIntrinsics for Failing {
        fn pow10(&self, _: &BigInt) -> Result<BigInt, EncodingError> {
            Err(EncodingError {
                name: "HostFault",
                message: vec![0xd800],
            })
        }
        fn pad_zeros(&self, _: &[u16], _: f64, _: bool) -> Result<Vec<u16>, EncodingError> {
            Err(EncodingError {
                name: "HostFault",
                message: vec![0xdfff],
            })
        }
    }
    assert_eq!(
        format_amount(&Failing, &BigInt::from(1), 3.0)
            .err()
            .unwrap()
            .message,
        vec![0xd800],
        "SDK-ENC primitive power error units retained"
    );
    assert_eq!(
        parse_amount(&Failing, &"1".encode_utf16().collect::<Vec<_>>(), 3.0)
            .err()
            .unwrap()
            .message,
        vec![0xdfff],
        "SDK-ENC primitive padding error units retained"
    );
}
