//! Version-one DELEGATED order wire contract, shared with router consumers.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const SUBMIT_MAX: usize = 64 * 1024;
pub const ACK_MAX: usize = 4 * 1024;
pub const OUTCOME_MAX: usize = 8 * 1024;
pub const KEY_MAX: usize = 128;

pub fn enabled() -> bool {
    cfg!(feature = "delegated-order-frames") && crate::delegated::enabled("bybit")
}

/// The compiled protocol and venue opt-in are separate from rollout activation.
pub fn runtime_enabled() -> bool {
    enabled() && crate::config::delegated_order_frames_from_env().unwrap_or(false)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrderSubmit {
    pub account_name: String,
    pub instance_id: Option<String>,
    pub intent_id: String,
    pub params: Value,
    pub seq: u64,
    #[serde(serialize_with = "serialize_age_ms")]
    pub age_ms: f64,
    pub fingerprint: String,
}

// Match the client's JSON number representation without discarding fractional
// monotonic ages. In particular, zero is `0`, including negative zero.
fn serialize_age_ms<S: serde::Serializer>(age: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if (0.0..=9_007_199_254_740_991.0).contains(age) && age.fract() == 0.0 {
        serializer.serialize_u64(*age as u64)
    } else {
        serializer.serialize_f64(*age)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelQueued {
    pub account_name: String,
    pub instance_id: Option<String>,
    pub intent_id: String,
    pub seq: u64,
    pub fingerprint: String,
}

pub fn key_time(id: &str) -> Option<u64> {
    if id.len() != 36
        || !id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
        || id.as_bytes()[14] != b'7'
        || !b"89ab".contains(&id.as_bytes()[19])
    {
        return None;
    }
    u64::from_str_radix(&id[..13].replace('-', ""), 16).ok()
}

pub fn check_key(id: &str, now: u64) -> Result<(), &'static str> {
    let created = key_time(id).ok_or("not sent: invalid intent key")?;
    let age = now
        .checked_sub(created)
        .ok_or("not sent: invalid key time")?;
    if age >= 10_000 {
        Err("not sent: expired")
    } else {
        Ok(())
    }
}

/// SHA256 of domain + canonical JSON. Keys are ASCII, numbers use ECMAScript's
/// finite binary64 serialization, and negative zero is normalized to zero.
pub fn fingerprint(
    account: &str,
    id: &str,
    params: &Value,
    seq: u64,
) -> Result<String, &'static str> {
    if key_time(id).is_none() || seq == 0 || seq > 9_007_199_254_740_991 {
        return Err("not sent: invalid intent");
    }
    let value = json!({"accountName":account,"intentId":id,"params":params,"seq":seq});
    let mut canonical = String::from("tealstreet/delegated-order/v1\0");
    canonical_value(&value, &mut canonical, 0)?;
    if canonical.len() > SUBMIT_MAX {
        return Err("not sent: parameters too large");
    }
    Ok(safe_cex_crypto::sha256(canonical.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn canonical_value(value: &Value, out: &mut String, depth: usize) -> Result<(), &'static str> {
    if depth > 32 || out.len() > SUBMIT_MAX {
        return Err("not sent: parameters too large");
    }
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        Value::String(v) => {
            out.push_str(&serde_json::to_string(v).map_err(|_| "not sent: invalid string")?)
        }
        Value::Number(v) => {
            let n = v
                .as_f64()
                .filter(|n| n.is_finite())
                .ok_or("not sent: invalid number")?;
            if n.fract() == 0.0 && n.abs() > 9_007_199_254_740_991.0 {
                return Err("not sent: unsafe number");
            }
            out.push_str(ryu_js::Buffer::new().format_finite(if n == 0.0 { 0.0 } else { n }));
        }
        Value::Array(values) => {
            out.push('[');
            for (i, v) in values.iter().enumerate() {
                if i != 0 {
                    out.push(',');
                }
                canonical_value(v, out, depth + 1)?;
            }
            out.push(']');
        }
        Value::Object(values) => {
            out.push('{');
            let mut keys: Vec<_> = values.keys().collect();
            keys.sort();
            for (i, k) in keys.into_iter().enumerate() {
                if !k.is_ascii() {
                    return Err("not sent: non-ASCII parameter key");
                }
                if i != 0 {
                    out.push(',');
                }
                canonical_value(&Value::String(k.clone()), out, depth + 1)?;
                out.push(':');
                canonical_value(&values[k], out, depth + 1)?;
            }
            out.push('}');
        }
    }
    Ok(())
}
