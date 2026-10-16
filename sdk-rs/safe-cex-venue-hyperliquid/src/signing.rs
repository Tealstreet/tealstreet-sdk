//! Hyperliquid's action composition over the independently vector-bound primitives.
use crate::types::Signature;
use alloy_dyn_abi::DynSolValue;
use safe_cex_num::JsJsonValue;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::borrow::Cow;

/// Per-request views: JS undefined object members are nil in MessagePack, absent in JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct SigningPayload {
    wire: Value,
    encoding: JsJsonValue,
}

#[cfg(test)]
#[path = "../tests/support/accepted_signing_clone.rs"]
mod accepted_clone;

pub trait SigningInput {
    fn encoding_view(&self) -> Cow<'_, JsJsonValue>;
}
impl SigningInput for Value {
    fn encoding_view(&self) -> Cow<'_, JsJsonValue> {
        Cow::Owned(self.clone().into())
    }
}
impl SigningInput for SigningPayload {
    fn encoding_view(&self) -> Cow<'_, JsJsonValue> {
        Cow::Borrowed(&self.encoding)
    }
}
impl From<Value> for SigningPayload {
    fn from(value: Value) -> Self {
        Self {
            wire: value.clone(),
            encoding: value.into(),
        }
    }
}
impl From<Vec<SigningPayload>> for SigningPayload {
    fn from(values: Vec<SigningPayload>) -> Self {
        let wire = Value::Array(values.iter().map(|v| v.wire.clone()).collect());
        let encoding = JsJsonValue::Array(values.into_iter().map(|v| v.encoding).collect());
        Self { wire, encoding }
    }
}
impl serde::Serialize for SigningPayload {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let canonical = self.canonical_wire().map_err(serde::ser::Error::custom)?;
        serde::Serialize::serialize(&canonical, serializer)
    }
}
impl SigningPayload {
    fn canonical_wire(&self) -> Result<Value, serde_json::Error> {
        serde_json::from_str::<safe_cex_types::retained_json::RetainedJsonInput>(
            &crate::order_response::describe_response(&self.wire),
        )
        .and_then(safe_cex_types::retained_json::RetainedJsonInput::into_value)
    }
}
pub(crate) fn object(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
}
pub(crate) fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Number(v) => {
            v.as_f64()
                .unwrap_or_else(|| v.to_string().parse().unwrap_or(f64::NAN))
                != 0.0
        }
        _ => true,
    }
}
pub(crate) fn number(v: &Value) -> f64 {
    match v {
        Value::Null => 0.0,
        Value::Bool(v) => u8::from(*v) as f64,
        Value::Number(v) => v
            .as_f64()
            .unwrap_or_else(|| v.to_string().parse().unwrap_or(f64::NAN)),
        Value::Array(_) => number(&Value::String(crate::order_response::js_string(v))),
        Value::String(v) => {
            let v = safe_cex_num::js_trim(v);
            if v.is_empty() {
                return 0.0;
            }
            for (prefix, radix) in [
                ("0x", 16u32),
                ("0X", 16),
                ("0o", 8),
                ("0O", 8),
                ("0b", 2),
                ("0B", 2),
            ] {
                if let Some(digits) = v.strip_prefix(prefix) {
                    if digits.is_empty() {
                        return f64::NAN;
                    }
                    let mut bits = String::new();
                    let width = radix.ilog2() as usize;
                    for c in digits.chars() {
                        let Some(d) = c.to_digit(radix) else {
                            return f64::NAN;
                        };
                        bits.push_str(&format!("{d:0width$b}"));
                    }
                    let bits = bits.trim_start_matches('0');
                    if bits.is_empty() {
                        return 0.0;
                    }
                    if bits.len() <= 53 {
                        return u64::from_str_radix(bits, 2).unwrap() as f64;
                    }
                    let mut mantissa = u64::from_str_radix(&bits[..53], 2).unwrap();
                    let tail = &bits[53..];
                    if tail.starts_with('1') && (tail[1..].contains('1') || mantissa % 2 == 1) {
                        mantissa += 1;
                    }
                    return mantissa as f64 * 2.0f64.powf((bits.len() - 53) as f64);
                }
            }
            if matches!(
                v.to_ascii_lowercase().as_str(),
                "inf" | "+inf" | "-inf" | "infinity" | "+infinity" | "-infinity"
            ) && !matches!(v, "Infinity" | "+Infinity" | "-Infinity")
            {
                return f64::NAN;
            }
            v.parse().unwrap_or(f64::NAN)
        }
        Value::Object(_) => f64::NAN,
    }
}
fn fixed_eight(x: f64) -> String {
    if !x.is_finite() || x.abs() >= 1e21 {
        return safe_cex_num::js_number_string(x);
    }
    let b = x.abs().to_bits();
    let exp = ((b >> 52) & 0x7ff) as i32 - 1023 - 52;
    let mantissa = (b & ((1_u64 << 52) - 1)) | if b >> 52 == 0 { 0 } else { 1_u64 << 52 };
    let numerator = u128::from(mantissa) * 100_000_000;
    let rounded = if exp >= 0 {
        numerator << (exp as u32)
    } else {
        let shift = (-exp) as u32;
        if shift >= 128 {
            0
        } else {
            let q = numerator >> shift;
            let remainder = numerator - (q << shift);
            q + u128::from(shift > 0 && remainder >= (1_u128 << (shift - 1)))
        }
    };
    format!(
        "{}{}.{:08}",
        if x < 0.0 { "-" } else { "" },
        rounded / 100_000_000,
        rounded % 100_000_000
    )
}
pub fn float_to_wire(x: f64) -> Result<String, String> {
    let rounded = fixed_eight(x);
    let parsed = rounded.parse::<f64>().unwrap_or(f64::NAN);
    if (parsed - x).abs() >= 1e-9 {
        return Err(format!(
            "floatToWire causes rounding: {}",
            safe_cex_num::js_number_string(x)
        ));
    }
    // ts-bug: floatToWire also strips trailing zeros from scientific exponents (1e30 becomes 1e+3).
    let mut normalized = rounded.trim_end_matches('0').to_owned();
    if normalized.ends_with('.') {
        normalized.pop();
    }
    if normalized == "-0" {
        normalized = "0".into()
    }
    Ok(normalized)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireError {
    pub name: &'static str,
    pub message: String,
}
impl WireError {
    fn type_error(message: impl Into<String>) -> Self {
        Self {
            name: "TypeError",
            message: message.into(),
        }
    }
    fn error(message: impl Into<String>) -> Self {
        Self {
            name: "Error",
            message: message.into(),
        }
    }
}
impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for WireError {}
fn wire_float(value: Option<&Value>) -> Result<String, WireError> {
    match value {
        Some(value @ Value::Number(_)) => float_to_wire(number(value)).map_err(WireError::error),
        None => Err(WireError::type_error(
            "Cannot read properties of undefined (reading 'toFixed')",
        )),
        Some(Value::Null) => Err(WireError::type_error(
            "Cannot read properties of null (reading 'toFixed')",
        )),
        _ => Err(WireError::type_error("x.toFixed is not a function")),
    }
}
pub fn order_type_to_wire(order: &Value) -> Result<Value, WireError> {
    if order.is_null() {
        return Err(WireError::type_error(
            "Cannot read properties of null (reading 'limit')",
        ));
    }
    if let Some(limit) = order.get("limit").filter(|v| truthy(v)) {
        return Ok(object([("limit", limit.clone())]));
    }
    if let Some(trigger) = order.get("trigger").filter(|v| truthy(v)) {
        let mut fields = Map::new();
        if let Some(v) = trigger.get("isMarket") {
            fields.insert("isMarket".into(), v.clone());
        }
        fields.insert(
            "triggerPx".into(),
            wire_float(trigger.get("triggerPx"))?.into(),
        );
        if let Some(v) = trigger.get("tpsl") {
            fields.insert("tpsl".into(), v.clone());
        }
        return Ok(object([("trigger", Value::Object(fields))]));
    }
    Err(WireError::error("Invalid order type"))
}
pub fn order_request_to_order_wire(order: &Value, asset: f64) -> Result<SigningPayload, WireError> {
    if order.is_null() {
        return Err(WireError::type_error(
            "Cannot read properties of null (reading 'is_buy')",
        ));
    }
    let mut fields = Map::new();
    fields.insert("a".into(), Value::from(asset));
    for (wire, key) in [
        ("b", "is_buy"),
        ("p", "limit_px"),
        ("s", "sz"),
        ("r", "reduce_only"),
        ("t", "order_type"),
    ] {
        let v = match wire {
            "p" | "s" => wire_float(order.get(key))?.into(),
            "t" => order_type_to_wire(order.get(key).ok_or_else(|| {
                WireError::type_error("Cannot read properties of undefined (reading 'limit')")
            })?)?,
            _ => order.get(key).cloned().unwrap_or(Value::Null),
        };
        fields.insert(wire.into(), v);
    }
    if let Some(v) = order.get("cloid") {
        fields.insert("c".into(), v.clone());
    }
    let encoding = JsJsonValue::Object(
        fields
            .iter()
            .map(|(key, value)| {
                let value = if key == "a" {
                    JsJsonValue::Number(asset)
                } else {
                    value.clone().into()
                };
                (key.clone(), value)
            })
            .collect(),
    );
    // ts-bug: orderRequestToOrderWire retains undefined members for msgpack nil but JSON omits them.
    for (wire, key) in [("b", "is_buy"), ("r", "reduce_only")] {
        if order.get(key).is_none() {
            fields.shift_remove(wire);
        }
    }
    Ok(SigningPayload {
        wire: Value::Object(fields),
        encoding,
    })
}
fn order_action(orders: SigningPayload, builder: Option<Value>, grouping: &str) -> SigningPayload {
    let build = |orders| {
        let mut action = object([
            ("type", "order".into()),
            ("orders", orders),
            ("grouping", grouping.into()),
        ]);
        if let Some(builder) = builder.as_ref().filter(|v| truthy(v)) {
            action
                .as_object_mut()
                .unwrap()
                .insert("builder".into(), builder.clone());
        }
        action
    };
    let mut encoding = vec![
        ("type".into(), JsJsonValue::String("order".into())),
        ("orders".into(), orders.encoding),
        ("grouping".into(), JsJsonValue::String(grouping.into())),
    ];
    if let Some(builder) = builder.as_ref().filter(|value| truthy(value)) {
        encoding.push(("builder".into(), builder.clone().into()));
    }
    SigningPayload {
        wire: build(orders.wire),
        encoding: JsJsonValue::Object(encoding),
    }
}
pub fn order_wires_to_order_action(
    orders: impl Into<SigningPayload>,
    builder: Option<Value>,
) -> SigningPayload {
    order_action(orders.into(), builder, "na")
}
pub fn order_wires_to_normal_tpsl_action(
    orders: impl Into<SigningPayload>,
    builder: Option<Value>,
) -> SigningPayload {
    order_action(orders.into(), builder, "normalTpsl")
}
pub fn order_wires_to_positional_tpsl_action(
    orders: impl Into<SigningPayload>,
    builder: Option<Value>,
) -> SigningPayload {
    order_action(orders.into(), builder, "positionTpsl")
}
pub fn cancel_order_to_action(cancel: Value) -> Value {
    object([
        ("type", "cancel".into()),
        ("cancels", Value::Array(vec![cancel])),
    ])
}
pub fn address_to_bytes(address: &str) -> Result<Vec<u8>, String> {
    let hex = address
        .strip_prefix("0x")
        .ok_or("invalid BytesLike value")?;
    if hex.len() % 2 != 0 {
        return Err("invalid BytesLike value".into());
    }
    hex.as_bytes()
        .chunks(2)
        .map(|pair| {
            let a = (pair[0] as char)
                .to_digit(16)
                .ok_or("invalid BytesLike value")?;
            let b = (pair[1] as char)
                .to_digit(16)
                .ok_or("invalid BytesLike value")?;
            Ok((a * 16 + b) as u8)
        })
        .collect()
}
fn number_nonce(n: f64) -> Result<u64, String> {
    if !n.is_finite() || n.fract() != 0.0 {
        return Err(format!(
            "The number {} cannot be converted to a BigInt because it is not an integer",
            safe_cex_num::js_number_string(n)
        ));
    }
    if n == 0.0 {
        return Ok(0);
    }
    let bits = n.abs().to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023 - 52;
    let mantissa = (bits & ((1_u64 << 52) - 1)) | (1_u64 << 52);
    let value = if exponent >= 64 {
        0
    } else if exponent >= 0 {
        mantissa.wrapping_shl(exponent as u32)
    } else {
        mantissa >> ((-exponent) as u32)
    };
    Ok(if n < 0.0 { value.wrapping_neg() } else { value })
}
pub(crate) fn nonce_u64(n: &Value) -> Result<u64, String> {
    if n.is_null() {
        return Err("Cannot convert null to a BigInt".into());
    }
    if n.is_array() || n.is_object() {
        return nonce_u64(&Value::String(crate::order_response::js_string(n)));
    }
    if let Value::String(s) = n {
        let original = s;
        let s = safe_cex_num::js_trim(s);
        let (negative, s) = if let Some(s) = s.strip_prefix('-') {
            (true, s)
        } else {
            (false, s.strip_prefix('+').unwrap_or(s))
        };
        let (radix, digits) = if let Some(d) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))
        {
            (16, d)
        } else if let Some(d) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
            (8, d)
        } else if let Some(d) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
            (2, d)
        } else {
            (10, s)
        };
        if radix != 10 && (negative || safe_cex_num::js_trim(original).starts_with('+')) {
            return Err(format!("Cannot convert {original} to a BigInt"));
        }
        if digits.is_empty() {
            return if safe_cex_num::js_trim(original).is_empty() {
                Ok(0)
            } else {
                Err(format!("Cannot convert {original} to a BigInt"))
            };
        }
        let value = digits.chars().try_fold(0u64, |acc, c| {
            c.to_digit(radix)
                .map(|d| {
                    acc.wrapping_mul(u64::from(radix))
                        .wrapping_add(u64::from(d))
                })
                .ok_or_else(|| format!("Cannot convert {original} to a BigInt"))
        })?;
        return Ok(if negative {
            value.wrapping_neg()
        } else {
            value
        });
    }
    number_nonce(number(n))
}
pub fn signing_preimage(
    action: &(impl SigningInput + ?Sized),
    vault: Option<&str>,
    nonce: &Value,
) -> Result<Vec<u8>, String> {
    let mut data = safe_cex_crypto::msgpack::encode_js_value(&action.encoding_view())?;
    data.extend_from_slice(&nonce_u64(nonce)?.to_be_bytes());
    data.push(u8::from(vault.is_some()));
    // ts-bug: actionHash accepts short hex vaults and pads trailing zeros instead of enforcing 20 bytes.
    if let Some(vault) = vault {
        let bytes = address_to_bytes(vault)?;
        if bytes.len() > 20 {
            return Err("offset is out of bounds".into());
        }
        data.extend_from_slice(&bytes);
        data.resize(data.len() + 20 - bytes.len(), 0);
    }
    Ok(data)
}
pub fn action_hash(
    action: &(impl SigningInput + ?Sized),
    vault: Option<&str>,
    nonce: &Value,
) -> Result<String, String> {
    Ok(format!(
        "{:#x}",
        DynSolValue::Bytes(signing_preimage(action, vault, nonce)?).encode_topic()
    ))
}
pub fn construct_phantom_agent(hash: &str, mainnet: bool) -> Value {
    object([
        ("source", if mainnet { "a" } else { "b" }.into()),
        ("connectionId", hash.into()),
    ])
}
pub(crate) fn split_sig(signature: &str) -> Result<Signature, String> {
    let bytes = address_to_bytes(signature)?;
    if !matches!(bytes.len(), 64 | 65) {
        return Err("invalid raw signature length".into());
    }
    let mut scalar = bytes[32..64].to_vec();
    let v = if bytes.len() == 64 {
        let parity = scalar[0] >> 7;
        scalar[0] &= 0x7f;
        27 + parity
    } else {
        match bytes[64] {
            0 | 1 => 27 + bytes[64],
            27 | 28 => bytes[64],
            value if value >= 35 => 27 + (value - 35) % 2,
            _ => return Err("invalid v".into()),
        }
    };
    // Match Signature.from's s getter, not a stronger curve-order validator:
    // it refuses the high bit but accepts 0 and other values below 2^255.
    if scalar[0] & 0x80 != 0 {
        return Err("non-canonical s; use ._s".into());
    }
    let hex = |bytes: &[u8]| {
        format!(
            "0x{}",
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        )
    };
    Ok(Signature {
        r: hex(&bytes[..32]),
        s: hex(&scalar),
        v: f64::from(v),
    })
}
/// Native raw-key construction boundary, before any action/vault hashing.
/// Ethers Wallet also accepts an unprefixed 32-byte hex key.
pub(crate) fn validate_wallet_key(key: &str) -> Result<(), String> {
    let prefixed = if key.starts_with("0x") {
        key.to_owned()
    } else {
        format!("0x{key}")
    };
    let bytes = address_to_bytes(&prefixed).map_err(|_| "invalid private key")?;
    let order =
        address_to_bytes("0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141")
            .expect("constant secp256k1 order");
    if bytes.len() != 32 || bytes.iter().all(|b| *b == 0) || bytes.as_slice() >= order.as_slice() {
        return Err("invalid private key".into());
    }
    Ok(())
}
pub fn sign_l1_action(
    key: &str,
    action: &(impl SigningInput + ?Sized),
    vault: Option<&str>,
    nonce: &Value,
    mainnet: bool,
) -> Result<Signature, String> {
    let hash = action_hash(action, vault, nonce)?;
    let domain:Value=serde_json::from_str(r#"{"name":"Exchange","version":"1","chainId":1337,"verifyingContract":"0x0000000000000000000000000000000000000000"}"#).unwrap();
    let types: Value = serde_json::from_str(
        r#"{"Agent":[{"name":"source","type":"string"},{"name":"connectionId","type":"bytes32"}]}"#,
    )
    .unwrap();
    let data = object([
        ("domain", domain),
        ("types", types),
        ("primaryType", "Agent".into()),
        ("message", construct_phantom_agent(&hash, mainnet)),
    ]);
    sign_inner(key, &data)
}

pub fn sign_user_signed_action(
    key: &str,
    action: &mut Value,
    payload_types: Value,
    primary_type: &str,
    mainnet: bool,
) -> Result<Signature, String> {
    let mut assigned = Map::new();
    assigned.insert("signatureChainId".into(), "0x66eee".into());
    assigned.insert(
        "hyperliquidChain".into(),
        if mainnet { "Mainnet" } else { "Testnet" }.into(),
    );
    let message = if let Some(fields) = action.as_object_mut() {
        fields.extend(assigned);
        WalletView::Json(action.clone())
    } else if action.is_array() {
        WalletView::Augmented {
            base: Box::new(WalletView::Json(action.clone())),
            fields: assigned,
        }
    } else {
        return Err("user action must be an object or array".into());
    };
    let domain:Value=serde_json::from_str(r#"{"name":"HyperliquidSignTransaction","version":"1","chainId":421614,"verifyingContract":"0x0000000000000000000000000000000000000000"}"#).unwrap();
    let types = Value::Object(
        [(primary_type.to_owned(), payload_types)]
            .into_iter()
            .collect(),
    );
    sign_inner_with_message(
        key,
        &object([
            ("domain", domain),
            ("types", types),
            ("primaryType", primary_type.into()),
            ("message", action.clone()),
        ]),
        Some(message),
    )
}
fn transfer_types() -> Value {
    serde_json::from_str(r#"[{"name":"hyperliquidChain","type":"string"},{"name":"destination","type":"string"},{"name":"amount","type":"string"},{"name":"time","type":"uint64"}]"#).unwrap()
}
pub fn sign_usd_transfer_action(
    key: &str,
    action: &mut Value,
    mainnet: bool,
) -> Result<Signature, String> {
    sign_user_signed_action(
        key,
        action,
        transfer_types(),
        "HyperliquidTransaction:UsdSend",
        mainnet,
    )
}
pub fn sign_withdraw_from_bridge_action(
    key: &str,
    action: &mut Value,
    mainnet: bool,
) -> Result<Signature, String> {
    sign_user_signed_action(
        key,
        action,
        transfer_types(),
        "HyperliquidTransaction:Withdraw",
        mainnet,
    )
}
pub fn sign_agent(key: &str, action: &mut Value, mainnet: bool) -> Result<Signature, String> {
    let types=serde_json::from_str(r#"[{"name":"hyperliquidChain","type":"string"},{"name":"agentAddress","type":"address"},{"name":"agentName","type":"string"},{"name":"nonce","type":"uint64"}]"#).unwrap();
    sign_user_signed_action(
        key,
        action,
        types,
        "HyperliquidTransaction:ApproveAgent",
        mainnet,
    )
}
// Views retain stock inherited references until the declared EIP-712 field
// consumes them. An inherited function is not the same as a missing JSON field.
#[derive(Clone)]
enum WalletView {
    Augmented {
        base: Box<WalletView>,
        fields: Map<String, Value>,
    },
    Json(Value),
    Units(Vec<u16>),
    Stock(safe_cex_num::prototype_realm::PrototypeValue),
    Function {
        name: String,
        length: u8,
        prototype: Option<WalletPrototype>,
    },
    Prototype(WalletPrototype),
}
#[derive(Clone, Copy)]
enum WalletPrototype {
    Array,
    String,
    Number,
    Boolean,
}
fn stock_function(name: &str, length: u8, prototype: Option<WalletPrototype>) -> WalletView {
    WalletView::Function {
        name: name.into(),
        length,
        prototype,
    }
}
fn string_method_length(name: &str) -> Option<u8> {
    Some(match name {
        "anchor" | "charAt" | "charCodeAt" | "codePointAt" | "concat" | "endsWith"
        | "fontcolor" | "fontsize" | "includes" | "indexOf" | "lastIndexOf" | "link"
        | "localeCompare" | "match" | "matchAll" | "padEnd" | "padStart" | "repeat" | "search"
        | "startsWith" | "at" => 1,
        "replace" | "replaceAll" | "slice" | "split" | "substr" | "substring" => 2,
        "big" | "blink" | "bold" | "fixed" | "isWellFormed" | "italics" | "normalize" | "small"
        | "strike" | "sub" | "sup" | "toWellFormed" | "trim" | "trimStart" | "trimLeft"
        | "trimEnd" | "trimRight" | "toLocaleLowerCase" | "toLocaleUpperCase" | "toLowerCase"
        | "toUpperCase" | "toString" | "valueOf" => 0,
        _ => return None,
    })
}
fn array_method_length(name: &str) -> u8 {
    match name {
        "entries" | "flat" | "keys" | "pop" | "reverse" | "shift" | "toLocaleString"
        | "toReversed" | "toString" | "values" => 0,
        "copyWithin" | "slice" | "splice" | "toSpliced" | "with" => 2,
        _ => 1,
    }
}
fn wallet_prototype_property(
    prototype: WalletPrototype,
    name: &str,
    realm: &std::rc::Rc<safe_cex_num::prototype_realm::PrototypeRealm>,
) -> WalletView {
    use safe_cex_num::get_key::GetKeyUnknownValue;
    if name == "__proto__" {
        return WalletView::Stock(realm.inherited("__proto__"));
    }
    if name == "constructor" {
        let name = match prototype {
            WalletPrototype::Array => "Array",
            WalletPrototype::String => "String",
            WalletPrototype::Number => "Number",
            WalletPrototype::Boolean => "Boolean",
        };
        return stock_function(name, 1, Some(prototype));
    }
    match prototype {
        WalletPrototype::Array => match safe_cex_num::get_key::array_lookup(&[], name) {
            GetKeyUnknownValue::Length(_) => return WalletView::Json(0.into()),
            GetKeyUnknownValue::ArrayInherited(member) => {
                return stock_function(member.name(), array_method_length(member.name()), None);
            }
            GetKeyUnknownValue::ObjectInherited(member) => {
                return WalletView::Stock(
                    safe_cex_num::prototype_realm::PrototypeValue::Reference(
                        realm.reference(member),
                    ),
                );
            }
            _ => {}
        },
        WalletPrototype::String => {
            if name == "length" {
                return WalletView::Json(0.into());
            }
            if let Some(length) = string_method_length(name) {
                return stock_function(name, length, None);
            }
        }
        WalletPrototype::Number => match name {
            "toFixed" | "toExponential" | "toPrecision" | "toString" => {
                return stock_function(name, 1, None);
            }
            "toLocaleString" | "valueOf" => return stock_function(name, 0, None),
            _ => {}
        },
        WalletPrototype::Boolean => {}
    }
    // All these prototypes inherit the same stock Object.prototype.
    WalletView::Stock(realm.inherited(name))
}
fn wallet_get(
    value: &WalletView,
    name: &str,
    realm: &std::rc::Rc<safe_cex_num::prototype_realm::PrototypeRealm>,
) -> Result<WalletView, String> {
    use safe_cex_num::prototype_realm::PrototypeValue;
    let result = match value {
        WalletView::Augmented { base, fields } => {
            if let Some(value) = fields.get(name) {
                WalletView::Json(value.clone())
            } else {
                wallet_get(base, name, realm)?
            }
        }
        WalletView::Json(Value::Object(values)) => values
            .get(name)
            .cloned()
            .map(WalletView::Json)
            .unwrap_or_else(|| WalletView::Stock(realm.inherited(name))),
        WalletView::Json(Value::Array(values)) => {
            if name == "length" {
                WalletView::Json(values.len().into())
            } else if let Some(item) =
                safe_cex_num::js_array_index(name).and_then(|i| values.get(i as usize))
            {
                WalletView::Json(item.clone())
            } else if name == "__proto__" {
                WalletView::Prototype(WalletPrototype::Array)
            } else {
                wallet_prototype_property(WalletPrototype::Array, name, realm)
            }
        }
        WalletView::Json(Value::String(text)) => wallet_get(
            &WalletView::Units(text.encode_utf16().collect()),
            name,
            realm,
        )?,
        WalletView::Units(units) => {
            if name == "length" {
                WalletView::Json(units.len().into())
            } else if let Some(unit) =
                safe_cex_num::js_array_index(name).and_then(|i| units.get(i as usize))
            {
                WalletView::Units(vec![*unit])
            } else if name == "__proto__" {
                WalletView::Prototype(WalletPrototype::String)
            } else {
                wallet_prototype_property(WalletPrototype::String, name, realm)
            }
        }
        WalletView::Json(Value::Number(_)) => {
            if name == "__proto__" {
                WalletView::Prototype(WalletPrototype::Number)
            } else {
                wallet_prototype_property(WalletPrototype::Number, name, realm)
            }
        }
        WalletView::Json(Value::Bool(_)) => {
            if name == "__proto__" {
                WalletView::Prototype(WalletPrototype::Boolean)
            } else {
                wallet_prototype_property(WalletPrototype::Boolean, name, realm)
            }
        }
        WalletView::Json(Value::Null) => {
            return Err(format!("Cannot read properties of null (reading '{name}')"));
        }
        WalletView::Stock(PrototypeValue::Reference(reference)) => {
            if reference.is_callable() && matches!(name, "caller" | "arguments") {
                return Err("restricted function property".into());
            }
            WalletView::Stock(reference.get(name))
        }
        WalletView::Stock(PrototypeValue::String(value)) => {
            wallet_get(&WalletView::Json(value.clone().into()), name, realm)?
        }
        WalletView::Stock(PrototypeValue::Number(value)) => {
            wallet_get(&WalletView::Json(Value::from(*value)), name, realm)?
        }
        WalletView::Stock(PrototypeValue::Boolean(value)) => {
            wallet_get(&WalletView::Json((*value).into()), name, realm)?
        }
        WalletView::Stock(value) => WalletView::Stock(realm.get(value, name)?),
        WalletView::Function {
            name: function_name,
            length,
            prototype,
        } => match name {
            "name" => WalletView::Json(function_name.clone().into()),
            "length" => WalletView::Json((*length).into()),
            "prototype" if prototype.is_some() => WalletView::Prototype(prototype.unwrap()),
            "caller" | "arguments" => return Err("restricted function property".into()),
            _ => {
                let function_prototype = realm
                    .reference(safe_cex_num::ProtoMember::ToString)
                    .get("__proto__");
                if name == "__proto__" {
                    WalletView::Stock(function_prototype)
                } else {
                    WalletView::Stock(realm.get(&function_prototype, name)?)
                }
            }
        },
        WalletView::Prototype(prototype) => wallet_prototype_property(*prototype, name, realm),
    };
    Ok(result)
}
fn wallet_truthy(value: &WalletView) -> bool {
    match value {
        WalletView::Json(value) => truthy(value),
        WalletView::Units(units) => !units.is_empty(),
        WalletView::Stock(value) => value.truthy(),
        WalletView::Augmented { .. } | WalletView::Function { .. } | WalletView::Prototype(_) => {
            true
        }
    }
}
fn wallet_scalar(value: WalletView) -> Result<Value, String> {
    use safe_cex_num::prototype_realm::PrototypeValue;
    match value {
        WalletView::Json(value) => Ok(value),
        WalletView::Units(units) => String::from_utf16(&units)
            .map(Value::String)
            .map_err(|_| "typed-data string member is not a scalar UTF-8 string".into()),
        WalletView::Stock(PrototypeValue::String(value)) => Ok(value.into()),
        WalletView::Stock(PrototypeValue::Number(value)) => {
            serde_json::from_str(&safe_cex_num::js_number_string(value))
                .map_err(|error| error.to_string())
        }
        WalletView::Stock(PrototypeValue::Boolean(value)) => Ok(value.into()),
        WalletView::Stock(PrototypeValue::Null) => Ok(Value::Null),
        _ => Err("invalid typed-data scalar value".into()),
    }
}
fn project_wallet_field(
    types: &Value,
    kind: &str,
    value: WalletView,
    depth: usize,
    realm: &std::rc::Rc<safe_cex_num::prototype_realm::PrototypeRealm>,
) -> Result<Value, String> {
    if depth >= 128 {
        return Err("EIP-712 wallet projection nesting limit exceeded".into());
    }
    if kind == "bool" {
        return Ok(Value::Bool(wallet_truthy(&value)));
    }
    if kind == "bytes"
        || kind
            .strip_prefix("bytes")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    {
        let value = wallet_scalar(value)?;
        let text = value.as_str().ok_or("invalid BytesLike value")?;
        address_to_bytes(text)?;
        return Ok(value);
    }
    if let Some((element, _)) = kind.strip_suffix(']').and_then(|k| k.rsplit_once('[')) {
        let values = match value {
            WalletView::Json(Value::Array(values)) => values,
            WalletView::Prototype(WalletPrototype::Array) => vec![],
            _ => return Err("invalid typed-data array value".into()),
        };
        return values
            .into_iter()
            .map(|value| {
                project_wallet_field(types, element, WalletView::Json(value), depth + 1, realm)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array);
    }
    if let Some(fields) = types.get(kind).and_then(Value::as_array) {
        let mut result = Map::new();
        for field in fields {
            let name = field
                .get("name")
                .and_then(Value::as_str)
                .ok_or("invalid typed-data field name")?;
            let kind = field
                .get("type")
                .and_then(Value::as_str)
                .ok_or("invalid typed-data field type")?;
            let property = wallet_get(&value, name, realm)?;
            result.insert(
                name.to_owned(),
                project_wallet_field(types, kind, property, depth + 1, realm)?,
            );
        }
        return Ok(Value::Object(result));
    }
    let value = wallet_scalar(value)?;
    let integer_kind = kind
        .strip_prefix("uint")
        .or_else(|| kind.strip_prefix("int"));
    if integer_kind
        .is_some_and(|suffix| suffix.is_empty() || suffix.bytes().all(|byte| byte.is_ascii_digit()))
    {
        if let Value::Number(number) = &value {
            let number = number.as_f64().ok_or("invalid typed-data number")?;
            // Ethers getBigInt accepts safe integer Numbers; JSON float storage
            // must not turn integer1.0 into a rejected native integer token.
            if !number.is_finite() || number.fract() != 0.0 || number.abs() > 9007199254740991.0 {
                return Err("invalid typed-data integer number".into());
            }
            return Ok(Value::String(if number == 0.0 {
                "0".into()
            } else {
                format!("{number:.0}")
            }));
        }
    }
    Ok(value)
}
fn normalize_wallet_field(
    types: &Value,
    kind: &str,
    value: &mut Value,
    depth: usize,
) -> Result<(), String> {
    let realm = std::rc::Rc::new(safe_cex_num::prototype_realm::PrototypeRealm::default());
    *value = project_wallet_field(types, kind, WalletView::Json(value.clone()), depth, &realm)?;
    Ok(())
}
fn sign_inner(key: &str, data: &Value) -> Result<Signature, String> {
    sign_inner_with_message(key, data, None)
}
fn sign_inner_with_message(
    key: &str,
    data: &Value,
    message_view: Option<WalletView>,
) -> Result<Signature, String> {
    let mut projected = data.clone();
    let types = data.get("types").cloned().unwrap_or(Value::Null);
    // Wallet.signTypedData receives domain/types/message, never data.primaryType.
    // Infer the unique root of the declared type graph instead of trusting a
    // caller field which the maintained source ignores.
    let records = types.as_object().ok_or("invalid typed-data types")?;
    let mut referenced = std::collections::BTreeSet::new();
    for fields in records.values() {
        for field in fields.as_array().ok_or("invalid typed-data fields")? {
            let kind = field
                .get("type")
                .and_then(Value::as_str)
                .ok_or("invalid typed-data field type")?;
            let kind = kind.split('[').next().unwrap_or(kind);
            if records.contains_key(kind) {
                referenced.insert(kind);
            }
        }
    }
    let roots: Vec<_> = records
        .keys()
        .filter(|name| !referenced.contains(name.as_str()))
        .collect();
    if roots.len() != 1 {
        return Err("ambiguous or missing primary type".into());
    }
    let kind = roots[0];
    projected["primaryType"] = kind.clone().into();
    if let Some(message) = projected.get_mut("message") {
        if let Some(view) = message_view {
            let realm = std::rc::Rc::new(safe_cex_num::prototype_realm::PrototypeRealm::default());
            *message = project_wallet_field(&types, kind, view, 0, &realm)?;
        } else {
            normalize_wallet_field(&types, kind, message, 0)?;
        }
    }
    split_sig(&safe_cex_crypto::eip712::sign_typed_data(
        &projected.to_string(),
        key,
    )?)
}
fn float_to_int(x: f64, power: i32) -> Result<f64, String> {
    let value = x * 10.0f64.powi(power);
    let floor = value.floor();
    let rounded = if !value.is_finite() || value == 0.0 {
        value
    } else {
        let n = if value - floor < 0.5 {
            floor
        } else {
            floor + 1.0
        };
        if n == 0.0 { 0.0f64.copysign(value) } else { n }
    };
    if (rounded - value).abs() >= 1e-3 {
        return Err(format!(
            "floatToInt causes rounding: {}",
            safe_cex_num::js_number_string(x)
        ));
    }
    Ok(rounded)
}
pub fn float_to_int_for_hashing(x: f64) -> Result<f64, String> {
    float_to_int(x, 8)
}
pub fn float_to_usd_int(x: f64) -> Result<f64, String> {
    float_to_int(x, 6)
}

#[derive(Debug, Clone, PartialEq)]
pub struct CancelOrderResponse {
    pub status: String,
    pub response: CancelOrderResponseBody,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "CancelOrderResponse")]
struct CancelOrderResponseWire {
    pub status: String,
    pub response: CancelOrderResponseBody,
}

impl Serialize for CancelOrderResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CancelOrderResponseWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for CancelOrderResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = CancelOrderResponse;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                CancelOrderResponseWire::deserialize(serde::de::value::MapAccessDeserializer::new(
                    map,
                ))
            }
        }
        deserializer.deserialize_map(OnlyMap)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CancelOrderResponseBody {
    pub r#type: String,
    pub data: CancelOrderResponseData,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "CancelOrderResponseBody")]
struct CancelOrderResponseBodyWire {
    pub r#type: String,
    pub data: CancelOrderResponseData,
}

impl Serialize for CancelOrderResponseBody {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CancelOrderResponseBodyWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for CancelOrderResponseBody {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = CancelOrderResponseBody;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                CancelOrderResponseBodyWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        deserializer.deserialize_map(OnlyMap)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CancelOrderResponseData {
    pub statuses: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "CancelOrderResponseData")]
struct CancelOrderResponseDataWire {
    pub statuses: Vec<String>,
}

impl Serialize for CancelOrderResponseData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CancelOrderResponseDataWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for CancelOrderResponseData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OnlyMap;
        impl<'de> serde::de::Visitor<'de> for OnlyMap {
            type Value = CancelOrderResponseData;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object record")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                CancelOrderResponseDataWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
            }
        }
        deserializer.deserialize_map(OnlyMap)
    }
}

pub fn get_timestamp_ms(clock: &dyn safe_cex_transport::rate_limit::RateLimitClock) -> f64 {
    clock.now_ms() as f64
}

#[cfg(test)]
mod findings_regression {
    use super::*;
    fn vectors() -> Value {
        serde_json::from_str(include_str!(
            "../tests/fixtures/finding-signing-vectors.json"
        ))
        .unwrap()
    }
    #[test]
    fn native_signing_wire_retains_literal_marker_object_before_serialization() {
        let mut marker = serde_json::Map::new();
        marker.insert(
            "$serde_json::private::Number".into(),
            Value::String("120".into()),
        );
        let input = serde_json::json!({"is_buy":true,"limit_px":1,"sz":2,"reduce_only":false,"order_type":{"limit":{"tif":"Gtc"}}});
        let mut input = input;
        input["order_type"]["limit"]["extra"] = Value::Object(marker);
        let prepared = super::order_request_to_order_wire(&input, 1.).unwrap();
        let canonical = prepared.canonical_wire().unwrap();
        let native = &canonical["t"]["limit"]["extra"];
        assert!(
            native.is_object(),
            "SIGNING native outgoing marker stays object before wire"
        );
        assert_eq!(
            native["$serde_json::private::Number"],
            Value::String("120".into())
        );
        assert_eq!(
            super::action_hash(&prepared, None, &serde_json::json!(1)).unwrap(),
            "0xdcf216b5c1a08a7581a95b2d7273562517e0eafc802ff7d8263b6d78f1cbf960",
            "SIGNING source marker-object hash"
        );
        let wire = serde_json::to_string(&prepared).unwrap();
        assert!(wire.contains(r#""extra":{"$serde_json::private::Number":"120"}"#));
    }

    #[test]
    fn wallet_projection_and_signature_parser_follow_source_native_values() {
        let vectors = vectors();
        let key = format!("0x{}", "11".repeat(32));
        let compare = |actual: Signature, expected: &Value, label: &str| {
            assert_eq!(
                actual.r,
                expected["r"].as_str().unwrap(),
                "SIGN native signature r {label}"
            );
            assert_eq!(
                actual.s,
                expected["s"].as_str().unwrap(),
                "SIGN native signature s {label}"
            );
            assert_eq!(
                actual.v.to_bits(),
                expected["v"].as_f64().unwrap().to_bits(),
                "SIG native recovery bits {label}"
            );
        };
        let mut signing = 0;
        let mut float_inputs = 0;
        for i in 0..38 {
            let row = &vectors["cases"][i];
            assert!(row.is_object(), "SIGN independent native vector index");
            let mut input = row["data"].clone();
            if i >= 33 {
                let (name, expected) = [
                    ("uint-number-one", 1.0f64),
                    ("uint-number-zero", 0.0),
                    ("uint-number-fraction", 1.5),
                    ("uint-number-safe-edge", 9007199254740991.0),
                    ("uint-number-overflow", 9007199254740992.0),
                ][i - 33];
                assert_eq!(
                    row["name"], name,
                    "SIGN independently expected native f64 case identity"
                );
                float_inputs += 1;
                input["message"]["value"] =
                    Value::Number(serde_json::Number::from_f64(expected).unwrap());
                assert_eq!(
                    input["message"]["value"].as_f64().unwrap().to_bits(),
                    expected.to_bits(),
                    "SIGN native f64 integer input bits"
                );
            }
            let actual = sign_inner(&key, &input);
            if row["error"] == true {
                assert!(
                    actual.is_err(),
                    "SIGN native refused source-invalid wallet payload {}",
                    row["name"]
                );
            } else {
                compare(
                    actual.unwrap_or_else(|error| {
                        panic!(
                            "SIGN native source-accepted wallet payload {}: {error}",
                            row["name"]
                        )
                    }),
                    &row["signature"],
                    row["name"].as_str().unwrap(),
                );
            }
            signing += 1;
        }
        assert_eq!(float_inputs, 5, "SIGN independent native f64 input floor");
        assert_eq!(signing, 38, "SIGN independent completed native floor");
        let mut split = 0;
        for i in 0..20 {
            let row = &vectors["split"][i];
            assert!(row.is_object(), "SIG independent native vector index");
            let actual = split_sig(row["input"].as_str().unwrap());
            if row["error"] == true {
                assert!(
                    actual.is_err(),
                    "SIG native refused malformed recovery/scalar"
                );
            } else {
                compare(
                    actual.expect("SIG native source-accepted compact/full signature"),
                    &row["signature"],
                    row["name"].as_str().unwrap(),
                );
            }
            split += 1;
        }
        assert_eq!(split, 20, "SIG independent completed native floor");
        let mut keys = 0;
        for i in 0..8 {
            let row = &vectors["keys"][i];
            assert!(row.is_object(), "KEY independent native vector index");
            assert_eq!(
                validate_wallet_key(row["input"].as_str().unwrap()).is_ok(),
                row["accepted"].as_bool().unwrap(),
                "KEY native accepted set {}",
                row["name"]
            );
            keys += 1;
        }
        assert_eq!(keys, 8, "KEY independent completed native floor");
        let source = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/finding-signing-source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            source.status.success(),
            "SIGN actual maintained source {}",
            String::from_utf8_lossy(&source.stderr)
        );
        let marker: Value =
            serde_json::from_slice(&source.stdout).expect("SIGN source completion marker");
        assert_eq!(marker["signing"], 38, "SIGN independent source floor");
        assert_eq!(marker["split"], 20, "SIG independent source floor");
        assert_eq!(marker["keys"], 8, "KEY independent source floor");
        assert_eq!(marker["last"], "order-minus-one", "KEY source stop7");
    }
}
