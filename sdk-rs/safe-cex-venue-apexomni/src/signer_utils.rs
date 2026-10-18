//! Correspondence: zklink/signer/utils.ts; host clock and prepared data boundary.
use safe_cex_num::JsJsonValue;

// ts: timestamp
pub fn timestamp(now: impl FnOnce() -> f64) -> f64 {
    (now() / 1000.0).floor()
}

// ts: assert
pub fn assert(condition: bool, message: &str) -> Result<(), String> {
    if !condition {
        return Err(message.to_owned());
    }
    Ok(())
}

// ts: assertTypeArray
pub fn assert_type_array(value: &JsJsonValue, message: &str) -> Result<(), String> {
    assert(matches!(value, JsJsonValue::Array(_)), message)
}

// ts: assertTypeBoolean
pub fn assert_type_boolean(value: &JsJsonValue, message: &str) -> Result<(), String> {
    assert(matches!(value, JsJsonValue::Bool(_)), message)
}

// ts: assertTypeNumber
pub fn assert_type_number(value: &JsJsonValue, message: &str) -> Result<(), String> {
    assert(matches!(value, JsJsonValue::Number(_)), message)
}

// ts: assertTypeString
pub fn assert_type_string(value: &JsJsonValue, message: &str) -> Result<(), String> {
    assert(matches!(value, JsJsonValue::String(_)), message)
}

// ts: assertSubAccountId
pub fn assert_sub_account_id(_value: &JsJsonValue, _message: &str) -> Result<(), String> {
    // Source validation is commented out: this is deliberately a no-op.
    Ok(())
}

// Prepared-data adapter for shared String(any). Only the display projection
// converts numbers/BigInts to strings; predicates inspect the original value.
// Undefined array entries become null solely to preserve shared array joining.
fn display_value(value: &JsJsonValue) -> serde_json::Value {
    match value {
        JsJsonValue::Undefined | JsJsonValue::Null => serde_json::Value::Null,
        JsJsonValue::Bool(value) => (*value).into(),
        JsJsonValue::Number(value) => safe_cex_num::js_number_string(*value).into(),
        JsJsonValue::String(value) => value.clone().into(),
        JsJsonValue::BigInt(value) => value.to_string().into(),
        JsJsonValue::Array(values) => values.iter().map(display_value).collect(),
        // Shared coercion checks an own non-callable toString. Other property
        // values are not read by Object.prototype.toString on prepared objects.
        JsJsonValue::Object(entries) => serde_json::Value::Object(
            entries
                .iter()
                .map(|(key, _)| (key.clone(), serde_json::Value::Null))
                .collect(),
        ),
    }
}
pub(crate) fn assertion_text(value: &JsJsonValue) -> Result<String, String> {
    let display = display_value(value);
    safe_cex_types::js_coercion::js_string(if matches!(value, JsJsonValue::Undefined) {
        None
    } else {
        Some(&display)
    })
    .map_err(|error| error.message.to_owned())
}

// ts: assertAddress
pub fn assert_address(value: &JsJsonValue, msg: &str) -> Result<(), String> {
    let valid = if let JsJsonValue::String(value) = value {
        safe_cex_crypto::ethereum::is_address(&serde_json::Value::String(value.clone()))
    } else {
        false
    };
    assert(
        valid,
        &format!(
            "Invalid address in {msg}, address: {}",
            assertion_text(value)?
        ),
    )
}

// ts: assertChainId
pub fn assert_chain_id(value: &JsJsonValue, msg: &str) -> Result<(), String> {
    let condition = matches!(value, JsJsonValue::Number(value) if *value > 0.0);
    assert(
        condition,
        &format!(
            "Invalid chainId in {msg}, chainId: {}",
            assertion_text(value)?
        ),
    )
}

// ts: assertAccountId
pub fn assert_account_id(value: &JsJsonValue, msg: &str) -> Result<(), String> {
    let condition = matches!(value, JsJsonValue::Number(value) if *value > 0.0);
    assert(
        condition,
        &format!(
            "Invalid accountId in {msg}, accountId: {}",
            assertion_text(value)?
        ),
    )
}

// ts: assertNonce
pub fn assert_nonce(value: &JsJsonValue, msg: &str) -> Result<(), String> {
    let condition = matches!(value, JsJsonValue::Number(value) if *value >= 0.0);
    assert(
        condition,
        &format!("Invalid nonce in {msg}, nonce: {}", assertion_text(value)?),
    )
}

// ts: assertTokenId
pub fn assert_token_id(value: &JsJsonValue, msg: &str) -> Result<(), String> {
    let condition = matches!(value, JsJsonValue::Number(value) if *value > 0.0);
    assert(
        condition,
        &format!(
            "Invalid tokenId in {msg}, tokenId: {}",
            assertion_text(value)?
        ),
    )
}

// ts: assertPubKeyHash
pub fn assert_pub_key_hash(value: &JsJsonValue, msg: &str) -> Result<(), String> {
    let condition = matches!(value, JsJsonValue::String(value) if value.encode_utf16().count() == 42 && value.starts_with("0x"));
    assert(
        condition,
        &format!(
            "Invalid pubkey hash in {msg}, pubkey hash: {}",
            assertion_text(value)?
        ),
    )
}

// ts: assertTs
pub fn assert_ts(value: &JsJsonValue, msg: &str) -> Result<(), String> {
    let condition = matches!(value, JsJsonValue::Number(value) if *value > 1_700_000_000.0)
        && safe_cex_num::js_number_string(1_700_000_000.0).len() == 10;
    assert(
        condition,
        &format!(
            "Invalid timestamp in {msg}, timestamp: {}",
            assertion_text(value)?
        ),
    )
}

// ts: assertWithdrawFeeRatio
pub fn assert_withdraw_fee_ratio(value: &JsJsonValue, msg: &str) -> Result<(), String> {
    let condition =
        matches!(value, JsJsonValue::Number(value) if *value >= 0.0 && *value <= 10000.0);
    assert(
        condition,
        &format!(
            "Invalid withdraw fee ratio in {msg}, fee ratio: {}",
            assertion_text(value)?
        ),
    )
}

// ts: assertTradingFeeRatio
pub fn assert_trading_fee_ratio(value: &JsJsonValue, msg: &str) -> Result<(), String> {
    let condition =
        matches!(value, JsJsonValue::Number(value) if *value >= -255.0 && *value <= 255.0);
    assert(
        condition,
        &format!(
            "Invalid trading fee ratio in {msg}, fee ratio: {}",
            assertion_text(value)?
        ),
    )
}

#[cfg(test)]
mod accepted_scalar_helper_tests {
    use super::*;
    use crate::accepted_scalar_helper_support::*;
    #[test]
    fn assert_type_array_native_output() {
        check("signer_utils::assert_type_array", 37, |value| {
            outcome(assert_type_array(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn assert_type_boolean_native_output() {
        check("signer_utils::assert_type_boolean", 37, |value| {
            outcome(assert_type_boolean(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn assert_type_number_native_output() {
        check("signer_utils::assert_type_number", 37, |value| {
            outcome(assert_type_number(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn assert_type_string_native_output() {
        check("signer_utils::assert_type_string", 37, |value| {
            outcome(assert_type_string(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn assert_address_native_output() {
        check("signer_utils::assert_address", 37, |value| {
            outcome(assert_address(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn assert_chain_id_native_output() {
        check("signer_utils::assert_chain_id", 37, |value| {
            outcome(assert_chain_id(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn assert_account_id_native_output() {
        check("signer_utils::assert_account_id", 37, |value| {
            outcome(assert_account_id(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn assert_nonce_native_output() {
        check("signer_utils::assert_nonce", 37, |value| {
            outcome(assert_nonce(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn assert_token_id_native_output() {
        check("signer_utils::assert_token_id", 37, |value| {
            outcome(assert_token_id(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn assert_pub_key_hash_native_output() {
        check("signer_utils::assert_pub_key_hash", 37, |value| {
            outcome(assert_pub_key_hash(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn assert_ts_native_output() {
        check("signer_utils::assert_ts", 37, |value| {
            outcome(assert_ts(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn assert_withdraw_fee_ratio_native_output() {
        check("signer_utils::assert_withdraw_fee_ratio", 37, |value| {
            outcome(assert_withdraw_fee_ratio(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn assert_trading_fee_ratio_native_output() {
        check("signer_utils::assert_trading_fee_ratio", 37, |value| {
            outcome(assert_trading_fee_ratio(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn assert_native_output() {
        check("signer_utils::assert", 8, |value| {
            outcome(assert(
                value["condition"].as_bool().unwrap(),
                value["message"].as_str().unwrap(),
            ))
        });
    }
    #[test]
    fn assert_sub_account_id_native_output() {
        check("signer_utils::assert_sub_account_id", 37, |value| {
            outcome(assert_sub_account_id(&prepared(value), "review é😀"))
        });
    }
    #[test]
    fn display_value_native_output() {
        check("signer_utils::display_value", 37, |value| {
            display_value(&prepared(value))
        });
    }
}
