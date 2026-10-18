//! GRVT SDK signing over the shared EIP-712 implementation.
use super::runtime::{Host, value};
use crate::{
    account_runtime::AccountHost, builder_workflow::JsResult, signing_runtime::SigningHost,
};
use num_bigint::{BigInt, Sign};
use safe_cex_exchange::js_runtime::value::{Js, Thrown, arg, from_json, to_json};
use serde_json::{Value, json};

fn fields(entries: &[(&str, &str)]) -> Value {
    Value::Array(
        entries
            .iter()
            .map(|(name, kind)| json!({"name":name,"type":kind}))
            .collect(),
    )
}

/// thirdparty/grvt-ts-sdk/src/signing/types.ts::OrderWithBuilderFee.
pub(super) fn sdk_template() -> Value {
    json!({
        "primaryType":"OrderWithBuilderFee",
        "domain":{"name":"GRVT Exchange","version":"0","chainId":1},
        "types":{
            "EIP712Domain":fields(&[("name","string"),("version","string"),("chainId","uint256")]),
            "OrderWithBuilderFee":fields(&[("subAccountID","uint64"),("isMarket","bool"),("timeInForce","uint8"),("postOnly","bool"),("reduceOnly","bool"),("legs","OrderLeg[]"),("builder","address"),("builderFee","uint32"),("nonce","uint32"),("expiration","int64")]),
            "OrderLeg":fields(&[("assetID","uint256"),("contractSize","uint64"),("limitPrice","uint64"),("isBuyingContract","bool")]),
        }
    })
}

// Alloy's typed-data JSON integer carrier accepts integer strings, while Js
// numbers serialize through an f64 Number (including `325.0`). Preserve the
// exact binary64 integer here: shortest JS text can spell a different nearby
// integer above 2^53. GRVT's Number-valued integer fields are at most uint64.
fn typed_numbers(value: &mut Value) {
    match value {
        Value::Number(number) => {
            if let Some(n) = number
                .as_f64()
                .filter(|n| n.is_finite() && n.fract() == 0.0)
            {
                *value = Value::String(if n == 0.0 {
                    "0".into()
                } else {
                    format!("{n:.0}")
                });
            }
        }
        Value::Array(values) => values.iter_mut().for_each(typed_numbers),
        Value::Object(values) => values.values_mut().for_each(typed_numbers),
        _ => {}
    }
}

// MetaMask eth-sig-util encodeField(address) and abi-utils getAddress. Its
// unprefixed/nonhex string path deliberately retains the legacy base-10 loop
// over UTF-16 units, then takes the first 20 bytes of an oversized result.
fn sdk_builder_address(address: &Value) -> JsResult<Js, Value> {
    let bytes = match address {
        Value::Number(_) => {
            let unsafe_number = || {
                Thrown::error(
                    "Error",
                    "Value is not a safe integer. Use `bigIntToBytes` instead.",
                )
                .value
            };
            let number = safe_cex_num::numeric_ingress::js_value_as_f64(address)
                .ok_or_else(unsafe_number)?;
            if number < 0.0 {
                return Err(Thrown::error("Error", "Value must be a non-negative number.").value);
            }
            if !number.is_finite() || number.fract() != 0.0 || number > 9007199254740991.0 {
                return Err(unsafe_number());
            }
            (number as u64).to_be_bytes().to_vec()
        }
        Value::String(address) => {
            if let Some(digits) = address
                .strip_prefix("0x")
                .or_else(|| address.strip_prefix("0X"))
                && !digits.is_empty()
                && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                let byte_count = digits.len().div_ceil(2);
                if byte_count > 20 {
                    return Err(Thrown::error("ParserError", &format!(
                        "Unable to encode value: Invalid address value. Expected address to be 20 bytes long, but received {byte_count} bytes."
                    )).value);
                }
                return Ok(Value::String(format!(
                    "0x{:0>40}",
                    digits.to_ascii_lowercase()
                )));
            }
            let mut integer = BigInt::from(0);
            for unit in address.encode_utf16() {
                let character = i32::from(unit) - 48;
                let digit = if character >= 49 {
                    character - 49 + 10
                } else if character >= 17 {
                    character - 17 + 10
                } else {
                    character
                };
                integer = integer * 10 + digit;
            }
            if integer.sign() == Sign::Minus {
                return Err(Thrown::error("Error", "Value must be a non-negative bigint.").value);
            }
            let (_, mut bytes) = integer.to_bytes_be();
            bytes.truncate(20);
            bytes
        }
        address => {
            let text = safe_cex_num::js_string(Some(address))
                .map_err(|e| Thrown::error(e.name, &e.message).value)?;
            return Err(Thrown::error(
                "ParserError",
                &format!(
                    "An unexpected error occurred: Expected a bytes-like value, got \"{text}\"."
                ),
            )
            .value);
        }
    };
    Ok(Value::String(format!(
        "0x{}{}",
        "00".repeat(20 - bytes.len()),
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )))
}

pub(super) fn sign(key: Js, typed: Js) -> JsResult<Js> {
    let key = value(key.to_js_string())?;
    let mut typed = value(to_json(&typed))?;
    // Normalize only this SDK field in the local signing copy. Number addresses
    // must retain their type until the SDK byte conversion is complete.
    if typed["primaryType"].as_str() == Some("OrderWithBuilderFee")
        && let Some(builder) = typed.get_mut("message").and_then(|v| v.get_mut("builder"))
    {
        *builder = sdk_builder_address(builder)?;
    }
    typed_numbers(&mut typed);
    let raw = typed.to_string();
    safe_cex_crypto::eip712::sign_typed_data(&raw, &String::from_utf16_lossy(&key))
        .map(|signature| Js::str(&signature))
        .map_err(|error| Thrown::error("Error", &error).value)
}

/// The SDK's pairwise parseInt loop deliberately skips the initial `0x` pair.
pub(super) fn decode(signature: Js) -> JsResult<Js> {
    let text = value(signature.to_js_string())?;
    let bytes: Vec<u8> = text
        .chunks(2)
        .filter_map(|chunk| {
            let parsed = safe_cex_num::parse_int(&String::from_utf16_lossy(chunk), Some(16));
            (!parsed.is_nan()).then_some(parsed as u8)
        })
        .collect();
    let hex = |data: &[u8]| {
        Js::str(&format!(
            "0x{}",
            data.iter().map(|v| format!("{v:02x}")).collect::<String>()
        ))
    };
    Ok(Js::object(vec![
        ("r", hex(&bytes[..bytes.len().min(32)])),
        ("s", hex(&bytes[bytes.len().min(32)..bytes.len().min(64)])),
        (
            "v",
            bytes
                .last()
                .map_or(Js::Undefined, |v| Js::Number(f64::from(*v))),
        ),
    ]))
}

fn legacy_types() -> Value {
    json!({
        "Order":fields(&[("subAccountId","string"),("isMarket","bool"),("timeInForce","string"),("postOnly","bool"),("reduceOnly","bool"),("legs","OrderLeg[]"),("nonce","uint64"),("expiration","uint64")]),
        "OrderLeg":fields(&[("instrument","string"),("size","string"),("limitPrice","string"),("isBuyingAsset","bool")]),
    })
}

impl SigningHost for Host {
    fn undefined_argument(&self, value: &Js) -> bool {
        value.is_undefined()
    }
    fn new_wallet(&self, key: Js) -> JsResult<Js> {
        let units = value(key.to_js_string())?;
        let key = String::from_utf16_lossy(&units);
        let wallet = safe_cex_crypto::wallet_utils::create_wallet_from_private_key(&key)
            .map_err(|error| Thrown::error("Error", &error).value)?;
        let address = wallet.address().to_checksum(None);
        Ok(Js::object(vec![
            ("address", Js::str(&address)),
            (
                "signTypedData",
                Js::function("signTypedData", move |_, args| {
                    let domain = arg(&args, 0);
                    let types = arg(&args, 1);
                    let message = arg(&args, 2);
                    let primary = types
                        .own_keys()
                        .into_iter()
                        .find(|key| {
                            key.as_ref()
                                != &*safe_cex_exchange::js_runtime::value::utf16("EIP712Domain")
                        })
                        .map_or(Js::Undefined, Js::String);
                    let typed = Js::object(vec![
                        ("domain", domain),
                        ("types", types),
                        ("message", message),
                        ("primaryType", primary),
                    ]);
                    super::runtime::thrown(sign(Js::str(&key), typed))
                }),
            ),
        ]))
    }
    fn legacy_domain(&self, testnet: bool) -> Js {
        from_json(&json!({"name":"Grvt","version":"0","chainId":if testnet {"325"} else {"324"}}))
    }
    fn legacy_order_types(&self) -> Js {
        from_json(&legacy_types())
    }
    fn signature_from(&self, signature: Js) -> JsResult<Js> {
        decode(signature)
    }
}

impl crate::trading_runtime::TradingHost for Host {
    fn parse_int_decimal(&self, value: Js) -> JsResult<Js, f64> {
        self.global_call("parseInt", vec![value, Js::Number(10.0)])?
            .as_f64()
            .ok_or_else(|| Thrown::type_error("parseInt returned a non-number").value)
    }
    fn random(&self) -> JsResult<Js, f64> {
        Ok(self.0.shared.runtime.random())
    }
    fn adjust(&self, number: Js, step: Js) -> JsResult<Js, f64> {
        Ok(safe_cex_num::adjust(
            self.number_value(number)?,
            self.number_value(step)?,
        ))
    }
    fn pow(&self, base: f64, exponent: Js) -> JsResult<Js, f64> {
        Ok(safe_cex_num::math_pow(base, self.number_value(exponent)?))
    }
    fn sdk_domain(&self) -> Js {
        from_json(&sdk_template()["domain"])
    }
    fn sdk_order_types(&self) -> Js {
        from_json(&sdk_template())
    }
    fn sdk_sign(&self, key: Js, typed: Js) -> JsResult<Js> {
        sign(key, typed)
    }
    fn sdk_decode(&self, signature: Js) -> JsResult<Js> {
        decode(signature)
    }
    fn is_builder_error(&self, error: &Js) -> bool {
        let mut current = error.clone();
        while let Js::Object(object) = current {
            let Some(proto) = object.proto.borrow().clone() else {
                return false;
            };
            if proto.same_reference(&self.0.shared.extra.builder_error_proto) {
                return true;
            }
            current = proto;
        }
        false
    }
    fn new_map(&self) -> JsResult<Js> {
        value(value(self.0.global("Map"))?.construct(vec![], None))
    }
    fn new_set(&self, values: Js) -> JsResult<Js> {
        value(value(self.0.global("Set"))?.construct(vec![values], None))
    }
    fn has_own(&self, object: &Js, key: &str) -> JsResult<Js, bool> {
        Ok(object.has_own(key))
    }
    fn is_object(&self, value: &Js) -> bool {
        value.type_of() == "object"
    }
    fn is_number(&self, value: &Js) -> bool {
        matches!(value, Js::Number(_))
    }
    fn decimal_pattern(&self, text: &Js) -> JsResult<Js, bool> {
        let units = value(text.to_js_string())?;
        let mut parts = units.split(|v| *v == b'.' as u16);
        let first = parts.next().unwrap_or_default();
        let last = parts.next();
        let digits = |part: &[u16]| part.iter().all(|v| (b'0' as u16..=b'9' as u16).contains(v));
        Ok((!first.is_empty() || last.is_some())
            && digits(first)
            && last.is_none_or(|part| !part.is_empty() && digits(part))
            && parts.next().is_none())
    }
    fn first_set_value(&self, values: Js) -> JsResult<Js> {
        match values {
            Js::Set(values) => Ok(values.borrow().first().cloned().unwrap_or(Js::Undefined)),
            _ => Err(Thrown::type_error("set required").value),
        }
    }
    fn minimum(&self, values: Js) -> JsResult<Js, f64> {
        let values = values
            .array_values()
            .ok_or_else(|| Thrown::type_error("value is not iterable").value)?;
        let values = values
            .into_iter()
            .map(|v| self.number_value(v))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(values
            .into_iter()
            .fold(f64::INFINITY, safe_cex_num::math_min))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_sdk_signature_matches_actual_sdk_source() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/binding-signing-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            sdk_template(),
            corpus["template"],
            "actual SDK signing schema"
        );
        let cases = corpus["cases"].as_array().unwrap();
        assert_eq!(
            cases.len(),
            24,
            "independent network/order-kind/integer-boundary/address-casing floor"
        );
        for case in cases {
            let typed = from_json(&case["typed"]);
            let result = sign(from_json(&case["key"]), typed.clone());
            assert!(
                result.is_ok(),
                "GRVT SDK builder casing {:?}: native signer refused actual SDK input",
                case["typed"]["message"]["builder"]
            );
            let signature = result.unwrap();
            assert!(
                typed
                    .get("message")
                    .unwrap()
                    .get("builder")
                    .unwrap()
                    .strict_equals(&from_json(&case["typed"]["message"]["builder"])),
                "SDK signing must not mutate the caller's address spelling"
            );
            assert_eq!(
                to_json(&signature).unwrap(),
                case["signature"],
                "actual SDK signature"
            );
            let decoded = decode(signature).unwrap();
            for field in ["r", "s"] {
                let actual = decoded.get(field).unwrap();
                assert!(
                    actual.strict_equals(&from_json(&case["decoded"][field])),
                    "actual SDK {field}"
                );
            }
            assert_eq!(
                decoded.get("v").unwrap().as_f64(),
                case["decoded"]["v"].as_f64(),
                "actual SDK v"
            );
        }
    }
}
