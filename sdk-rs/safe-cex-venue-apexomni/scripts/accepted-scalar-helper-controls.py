#!/usr/bin/env python3
"""One reviewed native body at a time; compiler failures never satisfy controls."""
from pathlib import Path
import os
import subprocess

# These literal full bodies are reviewed source, not a runtime proposal input.
# Exact uniqueness preflight prevents suffix collisions or sibling mutation.
CONTROLS = [('signer_utils',
  'assert_type_array',
  'pub fn assert_type_array(value: &JsJsonValue, message: &str) -> Result<(), String> {\n'
  '    assert(matches!(value, JsJsonValue::Array(_)), message)\n'
  '}',
  'pub fn assert_type_array(value: &JsJsonValue, message: &str) -> Result<(), String> {\n'
  '    assert(matches!(value, JsJsonValue::Bool(_)), message)\n'
  '}',
  True),
 ('signer_utils',
  'assert_type_boolean',
  'pub fn assert_type_boolean(value: &JsJsonValue, message: &str) -> Result<(), String> {\n'
  '    assert(matches!(value, JsJsonValue::Bool(_)), message)\n'
  '}',
  'pub fn assert_type_boolean(value: &JsJsonValue, message: &str) -> Result<(), String> {\n'
  '    assert(matches!(value, JsJsonValue::Null), message)\n'
  '}',
  True),
 ('signer_utils',
  'assert_type_number',
  'pub fn assert_type_number(value: &JsJsonValue, message: &str) -> Result<(), String> {\n'
  '    assert(matches!(value, JsJsonValue::Number(_)), message)\n'
  '}',
  'pub fn assert_type_number(value: &JsJsonValue, message: &str) -> Result<(), String> {\n'
  '    assert(matches!(value, JsJsonValue::Bool(_)), message)\n'
  '}',
  True),
 ('signer_utils',
  'assert_type_string',
  'pub fn assert_type_string(value: &JsJsonValue, message: &str) -> Result<(), String> {\n'
  '    assert(matches!(value, JsJsonValue::String(_)), message)\n'
  '}',
  'pub fn assert_type_string(value: &JsJsonValue, message: &str) -> Result<(), String> {\n'
  '    assert(matches!(value, JsJsonValue::Bool(_)), message)\n'
  '}',
  True),
 ('signer_utils',
  'assert_address',
  'pub fn assert_address(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let valid = if let JsJsonValue::String(value) = value {\n'
  '        safe_cex_crypto::ethereum::is_address(&serde_json::Value::String(value.clone()))\n'
  '    } else {\n'
  '        false\n'
  '    };\n'
  '    assert(\n'
  '        valid,\n'
  '        &format!(\n'
  '            "Invalid address in {msg}, address: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  'pub fn assert_address(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let valid = if let JsJsonValue::String(value) = value {\n'
  '        safe_cex_crypto::ethereum::is_address(&serde_json::Value::String(value.clone()))\n'
  '    } else {\n'
  '        true\n'
  '    };\n'
  '    assert(\n'
  '        valid,\n'
  '        &format!(\n'
  '            "Invalid address in {msg}, address: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  True),
 ('signer_utils',
  'assert_chain_id',
  'pub fn assert_chain_id(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition = matches!(value, JsJsonValue::Number(value) if *value > 0.0);\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid chainId in {msg}, chainId: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  'pub fn assert_chain_id(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition = matches!(value, JsJsonValue::Number(value) if *value >= 0.0);\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid chainId in {msg}, chainId: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  True),
 ('signer_utils',
  'assert_account_id',
  'pub fn assert_account_id(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition = matches!(value, JsJsonValue::Number(value) if *value > 0.0);\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid accountId in {msg}, accountId: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  'pub fn assert_account_id(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition = matches!(value, JsJsonValue::Number(value) if *value >= 0.0);\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid accountId in {msg}, accountId: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  True),
 ('signer_utils',
  'assert_nonce',
  'pub fn assert_nonce(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition = matches!(value, JsJsonValue::Number(value) if *value >= 0.0);\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!("Invalid nonce in {msg}, nonce: {}", assertion_text(value)?),\n'
  '    )\n'
  '}',
  'pub fn assert_nonce(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition = matches!(value, JsJsonValue::Number(value) if *value > 0.0);\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!("Invalid nonce in {msg}, nonce: {}", assertion_text(value)?),\n'
  '    )\n'
  '}',
  True),
 ('signer_utils',
  'assert_token_id',
  'pub fn assert_token_id(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition = matches!(value, JsJsonValue::Number(value) if *value > 0.0);\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid tokenId in {msg}, tokenId: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  'pub fn assert_token_id(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition = matches!(value, JsJsonValue::Number(value) if *value >= 0.0);\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid tokenId in {msg}, tokenId: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  True),
 ('signer_utils',
  'assert_pub_key_hash',
  'pub fn assert_pub_key_hash(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition = matches!(value, JsJsonValue::String(value) if value.encode_utf16().count() '
  '== 42 && value.starts_with("0x"));\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid pubkey hash in {msg}, pubkey hash: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  'pub fn assert_pub_key_hash(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition = matches!(value, JsJsonValue::String(value) if value.encode_utf16().count() '
  '== 41 && value.starts_with("0x"));\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid pubkey hash in {msg}, pubkey hash: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  True),
 ('signer_utils',
  'assert_ts',
  'pub fn assert_ts(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition = matches!(value, JsJsonValue::Number(value) if *value > 1_700_000_000.0)\n'
  '        && safe_cex_num::js_number_string(1_700_000_000.0).len() == 10;\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid timestamp in {msg}, timestamp: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  'pub fn assert_ts(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition = matches!(value, JsJsonValue::Number(value) if *value > 1_700_000_001.0)\n'
  '        && safe_cex_num::js_number_string(1_700_000_000.0).len() == 10;\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid timestamp in {msg}, timestamp: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  True),
 ('signer_utils',
  'assert_withdraw_fee_ratio',
  'pub fn assert_withdraw_fee_ratio(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition =\n'
  '        matches!(value, JsJsonValue::Number(value) if *value >= 0.0 && *value <= 10000.0);\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid withdraw fee ratio in {msg}, fee ratio: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  'pub fn assert_withdraw_fee_ratio(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition =\n'
  '        matches!(value, JsJsonValue::Number(value) if *value >= 0.0 && *value <= 9999.0);\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid withdraw fee ratio in {msg}, fee ratio: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  True),
 ('signer_utils',
  'assert_trading_fee_ratio',
  'pub fn assert_trading_fee_ratio(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition =\n'
  '        matches!(value, JsJsonValue::Number(value) if *value >= -255.0 && *value <= 255.0);\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid trading fee ratio in {msg}, fee ratio: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  'pub fn assert_trading_fee_ratio(value: &JsJsonValue, msg: &str) -> Result<(), String> {\n'
  '    let condition =\n'
  '        matches!(value, JsJsonValue::Number(value) if *value >= -255.0 && *value <= 254.0);\n'
  '    assert(\n'
  '        condition,\n'
  '        &format!(\n'
  '            "Invalid trading fee ratio in {msg}, fee ratio: {}",\n'
  '            assertion_text(value)?\n'
  '        ),\n'
  '    )\n'
  '}',
  True),
 ('signer_utils',
  'assert',
  'pub fn assert(condition: bool, message: &str) -> Result<(), String> {\n'
  '    if !condition {\n'
  '        return Err(message.to_owned());\n'
  '    }\n'
  '    Ok(())\n'
  '}',
  'pub fn assert(condition: bool, message: &str) -> Result<(), String> {\n'
  '    if condition {\n'
  '        return Err(message.to_owned());\n'
  '    }\n'
  '    Ok(())\n'
  '}',
  False),
 ('signer_utils',
  'assert_sub_account_id',
  'pub fn assert_sub_account_id(_value: &JsJsonValue, _message: &str) -> Result<(), String> {\n'
  '    // Source validation is commented out: this is deliberately a no-op.\n'
  '    Ok(())\n'
  '}',
  'pub fn assert_sub_account_id(_value: &JsJsonValue, _message: &str) -> Result<(), String> {\n'
  '    // Source validation is commented out: this is deliberately a no-op.\n'
  '    Err("changed source no-op".to_owned())\n'
  '}',
  False),
 ('signer_utils',
  'display_value',
  'fn display_value(value: &JsJsonValue) -> serde_json::Value {\n'
  '    match value {\n'
  '        JsJsonValue::Undefined | JsJsonValue::Null => serde_json::Value::Null,\n'
  '        JsJsonValue::Bool(value) => (*value).into(),\n'
  '        JsJsonValue::Number(value) => safe_cex_num::js_number_string(*value).into(),\n'
  '        JsJsonValue::String(value) => value.clone().into(),\n'
  '        JsJsonValue::BigInt(value) => value.to_string().into(),\n'
  '        JsJsonValue::Array(values) => values.iter().map(display_value).collect(),\n'
  '        // Shared coercion checks an own non-callable toString. Other property\n'
  '        // values are not read by Object.prototype.toString on prepared objects.\n'
  '        JsJsonValue::Object(entries) => serde_json::Value::Object(\n'
  '            entries\n'
  '                .iter()\n'
  '                .map(|(key, _)| (key.clone(), serde_json::Value::Null))\n'
  '                .collect(),\n'
  '        ),\n'
  '    }\n'
  '}',
  'fn display_value(value: &JsJsonValue) -> serde_json::Value {\n'
  '    match value {\n'
  '        JsJsonValue::Undefined | JsJsonValue::Null => serde_json::Value::Null,\n'
  '        JsJsonValue::Bool(value) => (*value).into(),\n'
  '        JsJsonValue::Number(value) => format!("{}!", '
  'safe_cex_num::js_number_string(*value)).into(),\n'
  '        JsJsonValue::String(value) => value.clone().into(),\n'
  '        JsJsonValue::BigInt(value) => value.to_string().into(),\n'
  '        JsJsonValue::Array(values) => values.iter().map(display_value).collect(),\n'
  '        // Shared coercion checks an own non-callable toString. Other property\n'
  '        // values are not read by Object.prototype.toString on prepared objects.\n'
  '        JsJsonValue::Object(entries) => serde_json::Value::Object(\n'
  '            entries\n'
  '                .iter()\n'
  '                .map(|(key, _)| (key.clone(), serde_json::Value::Null))\n'
  '                .collect(),\n'
  '        ),\n'
  '    }\n'
  '}',
  True),
 ('exchange_utils',
  'iso_timestamp_to_epoch_hours',
  'pub fn iso_timestamp_to_epoch_hours(iso_timestamp: &str) -> f64 {\n'
  '    (js_time_clip(number_from_string(iso_timestamp)) / ONE_HOUR_MS).ceil()\n'
  '}',
  'pub fn iso_timestamp_to_epoch_hours(iso_timestamp: &str) -> f64 {\n'
  '    (js_time_clip(number_from_string(iso_timestamp)) / ONE_HOUR_MS).floor()\n'
  '}',
  True),
 ('exchange_utils',
  'character_at',
  'fn character_at(alphabet: &str, index: f64) -> Option<char> {\n'
  '    if index >= 0.0 && index < alphabet.len() as f64 {\n'
  '        alphabet.as_bytes().get(index as usize).map(|c| *c as char)\n'
  '    } else {\n'
  '        None\n'
  '    }\n'
  '}',
  'fn character_at(alphabet: &str, index: f64) -> Option<char> {\n'
  '    if index >= 0.0 && index < alphabet.len() as f64 {\n'
  '        alphabet.as_bytes().get(0).map(|c| *c as char)\n'
  '    } else {\n'
  '        None\n'
  '    }\n'
  '}',
  False),
 ('exchange_utils',
  'd8',
  'pub fn d8(e: Option<f64>, t: Option<&str>, mut random: impl FnMut() -> f64) -> RandomId {\n'
  '    let e = e.unwrap_or(16.0);\n'
  '    let t = t.unwrap_or("D");\n'
  '    let n = "123456789";\n'
  '    let r = "abcdef";\n'
  '    let i = if t == "hex" {\n'
  '        format!("{n}{r}")\n'
  '    } else {\n'
  '        n.to_owned()\n'
  '    };\n'
  '    let s = if t == "hex" {\n'
  '        format!("0{n}{r}")\n'
  '    } else {\n'
  '        format!("0{n}")\n'
  '    };\n'
  '    let c = if t == "hex" { 15.0 } else { 9.0 };\n'
  '    let mut l = character_at(&i, (random() * c).floor())\n'
  '        .map(|c| RandomId::String(c.to_string()))\n'
  '        .unwrap_or(RandomId::Undefined);\n'
  '    let mut d = 1.0;\n'
  '    while d < e {\n'
  '        let next = character_at(&s, (random() * (c + 1.0)).floor());\n'
  '        l = match (&l, next) {\n'
  '            (RandomId::Undefined | RandomId::Nan, None) => RandomId::Nan,\n'
  '            (_, Some(next)) => RandomId::String(format!("{}{next}", l.as_js_string())),\n'
  '            (_, None) => RandomId::String(format!("{}undefined", l.as_js_string())),\n'
  '        };\n'
  '        d += 1.0;\n'
  '    }\n'
  '    l\n'
  '}',
  'pub fn d8(e: Option<f64>, t: Option<&str>, mut random: impl FnMut() -> f64) -> RandomId {\n'
  '    let e = e.unwrap_or(16.0);\n'
  '    let t = t.unwrap_or("D");\n'
  '    let n = "987654321";\n'
  '    let r = "abcdef";\n'
  '    let i = if t == "hex" {\n'
  '        format!("{n}{r}")\n'
  '    } else {\n'
  '        n.to_owned()\n'
  '    };\n'
  '    let s = if t == "hex" {\n'
  '        format!("0{n}{r}")\n'
  '    } else {\n'
  '        format!("0{n}")\n'
  '    };\n'
  '    let c = if t == "hex" { 15.0 } else { 9.0 };\n'
  '    let mut l = character_at(&i, (random() * c).floor())\n'
  '        .map(|c| RandomId::String(c.to_string()))\n'
  '        .unwrap_or(RandomId::Undefined);\n'
  '    let mut d = 1.0;\n'
  '    while d < e {\n'
  '        let next = character_at(&s, (random() * (c + 1.0)).floor());\n'
  '        l = match (&l, next) {\n'
  '            (RandomId::Undefined | RandomId::Nan, None) => RandomId::Nan,\n'
  '            (_, Some(next)) => RandomId::String(format!("{}{next}", l.as_js_string())),\n'
  '            (_, None) => RandomId::String(format!("{}undefined", l.as_js_string())),\n'
  '        };\n'
  '        d += 1.0;\n'
  '    }\n'
  '    l\n'
  '}',
  False),
 ('exchange_utils',
  'generate_apex_random_client_id',
  'pub fn generate_apex_random_client_id(\n'
  '    account_id: &str,\n'
  '    now: impl FnOnce() -> f64,\n'
  '    random: impl FnMut() -> f64,\n'
  ') -> String {\n'
  '    let timestamp = safe_cex_num::js_number_string(now());\n'
  '    let suffix = d8(Some(6.0), None, random);\n'
  '    format!(\n'
  '        "apexomni-{account_id}-{timestamp}-{}",\n'
  '        suffix.as_js_string()\n'
  '    )\n'
  '}',
  'pub fn generate_apex_random_client_id(\n'
  '    account_id: &str,\n'
  '    now: impl FnOnce() -> f64,\n'
  '    random: impl FnMut() -> f64,\n'
  ') -> String {\n'
  '    let timestamp = safe_cex_num::js_number_string(now());\n'
  '    let suffix = d8(Some(6.0), None, random);\n'
  '    format!(\n'
  '        "apexomni-changed-{account_id}-{timestamp}-{}",\n'
  '        suffix.as_js_string()\n'
  '    )\n'
  '}',
  False),
 ('exchange_utils',
  'calculate_tp_sl_price',
  'pub fn calculate_tp_sl_price(side: &str, market_price: f64, price_step: f64) -> f64 {\n'
  '    let slippage_multiplier = if side == "BUY" { 1.005 } else { 0.995 };\n'
  '    let execution_price = market_price * slippage_multiplier;\n'
  '    safe_cex_num::adjust(execution_price, price_step)\n'
  '}',
  'pub fn calculate_tp_sl_price(side: &str, market_price: f64, price_step: f64) -> f64 {\n'
  '    let slippage_multiplier = if side == "BUY" { 1.015 } else { 0.995 };\n'
  '    let execution_price = market_price * slippage_multiplier;\n'
  '    safe_cex_num::adjust(execution_price, price_step)\n'
  '}',
  False),
 ('exchange_utils',
  'fee_helper',
  'pub fn fee_helper(\n'
  '    input: &crate::order_records::Iso8601,\n'
  '    digits: Option<f64>,\n'
  '    math: impl FnOnce(f64) -> Result<f64, String>,\n'
  ') -> Result<String, String> {\n'
  '    let number = match input {\n'
  '        crate::order_records::Iso8601::Number(value) => *value,\n'
  '        crate::order_records::Iso8601::String(value) => '
  'safe_cex_num::number_from_string(value),\n'
  '    };\n'
  '    let digits = digits.unwrap_or(2.0);\n'
  '    if number.is_nan() {\n'
  '        return safe_cex_num::js_to_fixed(0.0, digits).map_err(str::to_owned);\n'
  '    }\n'
  '    let scale = 10.0_f64.powf(digits);\n'
  '    safe_cex_num::js_to_fixed(math(decimal_multiply(number, scale))? / scale, digits)\n'
  '        .map_err(str::to_owned)\n'
  '}',
  'pub fn fee_helper(\n'
  '    input: &crate::order_records::Iso8601,\n'
  '    digits: Option<f64>,\n'
  '    math: impl FnOnce(f64) -> Result<f64, String>,\n'
  ') -> Result<String, String> {\n'
  '    let number = match input {\n'
  '        crate::order_records::Iso8601::Number(value) => *value,\n'
  '        crate::order_records::Iso8601::String(value) => '
  'safe_cex_num::number_from_string(value),\n'
  '    };\n'
  '    let digits = digits.unwrap_or(3.0);\n'
  '    if number.is_nan() {\n'
  '        return safe_cex_num::js_to_fixed(0.0, digits).map_err(str::to_owned);\n'
  '    }\n'
  '    let scale = 10.0_f64.powf(digits);\n'
  '    safe_cex_num::js_to_fixed(math(decimal_multiply(number, scale))? / scale, digits)\n'
  '        .map_err(str::to_owned)\n'
  '}',
  True),
 ('utils',
  'drop_prefix',
  'pub fn drop_prefix(hash: &str) -> &str {\n    hash.strip_prefix("0x").unwrap_or(hash)\n}',
  'pub fn drop_prefix(hash: &str) -> &str {\n    hash.strip_prefix("0X").unwrap_or(hash)\n}',
  False)]

def main():
 workspace=Path(__file__).resolve().parents[2]
 crate=workspace/'safe-cex-venue-apexomni'
 assert len(CONTROLS)==23 and sum(row[4] for row in CONTROLS)==16
 originals={module:(crate/'src'/(module+'.rs')).read_bytes() for module,*_ in CONTROLS}
 for module,name,original,mutant,_ in CONTROLS:
  assert originals[module].count(original.encode())==1,(name,'reviewed original body changed')
  assert original!=mutant and ('fn '+name+'(') in original
 def restore():
  for module,data in originals.items():(crate/'src'/(module+'.rs')).write_bytes(data)
 def run(args):
  r=subprocess.run(args,cwd=workspace,env={**os.environ,'RUSTC_BOOTSTRAP':'1','CARGO_BUILD_JOBS':'1'},capture_output=True,text=True,timeout=900)
  return r,r.stdout+r.stderr
 def native(test=None):
  args=['cargo','test','--locked','-p','safe-cex-venue-apexomni','--lib','--jobs','1',test or 'accepted_scalar_helper_tests','--','--nocapture']
  if test:args+=['--exact']
  return run(args)
 def production(control=None):
  args=['node','safe-cex-venue-apexomni/scripts/accepted-scalar-helper-contracts.mjs']
  if control:args+=['--expect-output-failure='+control]
  return run(args)
 def positive(label,with_production):
  r,output=native();assert r.returncode==0 and '23 passed; 0 failed; 0 ignored' in output,output[-18000:]
  assert all((crate/'src'/(m+'.rs')).read_bytes()==b for m,b in originals.items())
  if with_production:
   r,output=production();assert r.returncode==0 and '16 actual runtime decoders/7 explicit classifications/zero failures' in output,output[-18000:]
  print(label+': exact source restored; native23 PASS'+('; production16runtime/7classification PASS' if with_production else ''),flush=True)
 positive('BASELINE',True)
 try:
  for module,name,original,mutant,decoder in CONTROLS:
   test=module+'::accepted_scalar_helper_tests::'+name+'_native_output'
   owner='safe_cex_venue_apexomni::'+module+'::'+name
   try:
    path=crate/'src'/(module+'.rs');path.write_bytes(originals[module].replace(original.encode(),mutant.encode(),1))
    r,output=native(test)
    assert r.returncode==101 and ('test '+test+' ... FAILED') in output and 'test result: FAILED' in output and ('independent native helper output input=' in output or 'literal borrowed slice identity' in output) and 'could not compile' not in output,output[-18000:]
    print('NATIVE CONTROL PASS: '+owner+'; isolated compiled named native output failure',flush=True)
    if decoder:
     r,output=production(owner)
     assert r.returncode==0 and ('NATIVE_OUTPUT_FAILURE: '+owner) in output,output[-18000:]
     print('PRODUCTION CONTROL PASS: '+owner+'; actual selected native output mismatch',flush=True)
   finally:restore()
   positive('RESTORED '+owner,decoder)
 finally:restore()
 positive('FINAL',True)
 print('ALL23 isolated native controls and16 actual production output controls passed; exact restorations verified',flush=True)

if __name__=='__main__':main()
