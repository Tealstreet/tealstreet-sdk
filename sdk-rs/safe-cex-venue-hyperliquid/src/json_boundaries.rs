//! Host-bound JavaScript payload/diagnostic leaves. Host primitives retain opaque
//! values, property effects, errors and JSON/String semantics; algorithms live here.
use crate::spot_json::JsString;

pub trait JsPayloadHost {
    /// Cloning a handle must preserve object identity.
    type Value: Clone;
    type Error;
    /// Primitive typeof-string values only; boxed strings remain objects.
    fn string_units<'a>(&self, value: &'a Self::Value) -> Option<&'a [u16]>;
    /// Non-null typeof-object values, including arrays and boxed primitives.
    fn is_object(&self, value: &Self::Value) -> bool;
    /// Native Array.isArray, including its revoked-proxy exception.
    fn is_array(&mut self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn is_nullish(&self, value: &Self::Value) -> bool;
    fn undefined(&self) -> Self::Value;
    fn parse_json(&mut self, text: &[u16]) -> Result<Self::Value, Self::Error>;
    fn get(&mut self, value: &Self::Value, property: &str) -> Result<Self::Value, Self::Error>;
    fn json_stringify(&mut self, value: &Self::Value) -> Result<Option<JsString>, Self::Error>;
    fn to_string(&mut self, value: &Self::Value) -> Result<JsString, Self::Error>;
}
fn equal<H: JsPayloadHost>(host: &H, value: &H::Value, text: &str) -> bool {
    host.string_units(value)
        .is_some_and(|units| units.iter().copied().eq(text.encode_utf16()))
}
pub fn parse_json_object<H: JsPayloadHost>(
    host: &mut H,
    data: &H::Value,
) -> Result<Option<H::Value>, H::Error> {
    // The direct-object classifier is outside the source try.
    if host.is_object(data) && !host.is_array(data)? {
        return Ok(Some(data.clone()));
    }
    let Some(text) = host.string_units(data).map(<[u16]>::to_vec) else {
        return Ok(None);
    };
    let Ok(parsed) = host.parse_json(&text) else {
        return Ok(None);
    };
    if !host.is_object(&parsed) {
        return Ok(None);
    }
    // Array.isArray(parsed) is INSIDE the parse try and may throw for a revoked proxy.
    match host.is_array(&parsed) {
        Ok(false) => Ok(Some(parsed)),
        Ok(true) | Err(_) => Ok(None),
    }
}
pub fn stringify_diagnostic_payload<H: JsPayloadHost>(
    host: &mut H,
    data: &H::Value,
) -> Result<JsString, H::Error> {
    if let Some(text) = host.string_units(data) {
        return Ok(JsString(text.to_vec()));
    }
    match host.json_stringify(data) {
        Ok(Some(text)) => Ok(text),
        // String(data) is INSIDE the try. If it throws there, the catch performs
        // a fresh second String(data), whose result/error is authoritative.
        Ok(None) => match host.to_string(data) {
            Ok(text) => Ok(text),
            Err(_) => host.to_string(data),
        },
        Err(_) => host.to_string(data),
    }
}
fn word(unit: u16) -> bool {
    matches!(unit,0x30..=0x39|0x41..=0x5a|0x61..=0x7a|0x5f)
}
fn hex(unit: u16) -> bool {
    matches!(unit,0x30..=0x39|0x41..=0x46|0x61..=0x66)
}
fn redact(text: &[u16]) -> Vec<u16> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < text.len() {
        let end = index + 42;
        let matched = end <= text.len()
            && text[index] == b'0' as u16
            && text[index + 1] == b'x' as u16
            && (index == 0 || !word(text[index - 1]))
            && (end == text.len() || !word(text[end]))
            && text[index + 2..end].iter().copied().all(hex);
        if matched {
            output.extend("[redacted]".encode_utf16());
            index = end;
        } else {
            output.push(text[index]);
            index += 1;
        }
    }
    output
}
pub fn format_hyperliquid_ws_diagnostic<H: JsPayloadHost>(
    host: &mut H,
    scope: &JsString,
    data: &H::Value,
) -> Result<JsString, H::Error> {
    let payload = stringify_diagnostic_payload(host, data)?;
    let mut text: Vec<u16> = "[hyperliquid ".encode_utf16().collect();
    text.extend(&scope.0);
    text.extend("] ".encode_utf16());
    text.extend(redact(&payload.0));
    Ok(JsString(text))
}
pub fn should_emit_hyperliquid_ws_send_diagnostic<H: JsPayloadHost>(
    host: &mut H,
    data: &H::Value,
) -> Result<bool, H::Error> {
    let Some(parsed) = parse_json_object(host, data)? else {
        return Ok(true);
    };
    let method = host.get(&parsed, "method")?;
    if equal(host, &method, "ping") {
        return Ok(false);
    }
    let subscription = host.get(&parsed, "subscription")?;
    let kind = if host.is_nullish(&subscription) {
        host.undefined()
    } else {
        host.get(&subscription, "type")?
    };
    Ok(!equal(host, &kind, "activeAssetData"))
}
pub fn should_emit_hyperliquid_ws_receive_diagnostic<H: JsPayloadHost>(
    host: &mut H,
    scope: &JsString,
    data: &H::Value,
) -> Result<bool, H::Error> {
    let Some(parsed) = parse_json_object(host, data)? else {
        return Ok(false);
    };
    let channel = host.get(&parsed, "channel")?;
    const PUBLIC: &[&str] = &[
        "error",
        "subscribed",
        "subscriptionResponse",
        "SubscriptionResponse",
    ];
    const PRIVATE: &[&str] = &[
        "error",
        "notification",
        "orderUpdates",
        "subscribed",
        "subscriptionResponse",
        "SubscriptionResponse",
        "userEvents",
        "userFills",
        "userFundings",
        "userNonFundingLedgerUpdates",
    ];
    let channels = if scope.0.iter().copied().eq("private".encode_utf16()) {
        PRIVATE
    } else {
        PUBLIC
    };
    Ok(channels.iter().any(|name| equal(host, &channel, name)))
}
pub fn mask_hyperliquid_address<H: JsPayloadHost>(
    host: &H,
    address: &H::Value,
) -> Option<JsString> {
    let units = host.string_units(address)?;
    if units.is_empty() {
        return None;
    }
    if units.len() > 14 {
        Some(JsString(
            units[..8]
                .iter()
                .copied()
                .chain("...".encode_utf16())
                .chain(units[units.len() - 6..].iter().copied())
                .collect(),
        ))
    } else {
        Some(JsString(units.to_vec()))
    }
}
pub fn parse_hyperliquid_exchange_payload<H: JsPayloadHost>(
    host: &mut H,
    data: &H::Value,
) -> Result<Option<H::Value>, H::Error> {
    if let Some(units) = host.string_units(data) {
        let text = units.to_vec();
        return Ok(host.parse_json(&text).ok());
    }
    Ok((host.is_object(data) && !host.is_array(data)?).then(|| data.clone()))
}
