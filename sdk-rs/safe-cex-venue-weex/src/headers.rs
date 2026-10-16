//! WEEX header decisions over explicit source runtime capabilities.
use crate::retry::{
    WeexRetryAfterValue, clamp_weex_rate_limit_delay, parse_rate_limit_interval_ms,
    parse_retry_after_ms,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeexHeaderValueKind {
    Undefined,
    Null,
    Boolean,
    Number,
    String,
    Object,
    Function,
    Symbol,
    BigInt,
}

/// Values and runtime operations are supplied by the host without a JSON cast.
/// Method calls retain the original receiver; property/coercion failures propagate.
pub struct WeexHeaderRuntime<H, V, E> {
    pub kind: fn(&V) -> WeexHeaderValueKind,
    pub undefined: fn() -> V,
    pub has_property: fn(&mut H, &V, &[u16]) -> Result<bool, E>,
    pub get_property: fn(&mut H, &V, &[u16]) -> Result<V, E>,
    pub invoke: fn(&mut H, &V, &V, Option<&[u16]>) -> Result<V, E>,
    pub entries: fn(&mut H, &V) -> Result<Vec<(Vec<u16>, V)>, E>,
    pub lowercase: fn(&mut H, &[u16]) -> Vec<u16>,
    pub uppercase: fn(&mut H, &[u16]) -> Vec<u16>,
    pub number_value: fn(&V) -> f64,
    pub string_value: fn(&V) -> Vec<u16>,
    pub number: fn(&mut H, &V) -> Result<f64, E>,
    pub maximum: fn(&mut H, &[f64]) -> Result<f64, E>,
    pub now_ms: fn(&mut H) -> f64,
    pub parse_date: fn(&mut H, &[u16]) -> f64,
}

pub fn get_header<H, V, E>(
    headers: &V,
    name: &[u16],
    host: &mut H,
    runtime: &WeexHeaderRuntime<H, V, E>,
) -> Result<V, E> {
    use WeexHeaderValueKind::{Function, Null, Object, Undefined};
    if (runtime.kind)(headers) != Object {
        return Ok((runtime.undefined)());
    }
    let get: Vec<u16> = "get".encode_utf16().collect();
    if (runtime.has_property)(host, headers, &get)?
        && (runtime.kind)(&(runtime.get_property)(host, headers, &get)?) == Function
    {
        let method = (runtime.get_property)(host, headers, &get)?;
        return (runtime.invoke)(host, &method, headers, Some(name));
    }
    let exact = (runtime.get_property)(host, headers, name)?;
    if !matches!((runtime.kind)(&exact), Undefined | Null) {
        return Ok(exact);
    }
    let lower = (runtime.lowercase)(host, name);
    let lower_value = (runtime.get_property)(host, headers, &lower)?;
    if !matches!((runtime.kind)(&lower_value), Undefined | Null) {
        return Ok(lower_value);
    }
    let upper = (runtime.uppercase)(host, name);
    (runtime.get_property)(host, headers, &upper)
}

pub fn get_header_entries<H, V, E>(
    headers: &V,
    host: &mut H,
    runtime: &WeexHeaderRuntime<H, V, E>,
) -> Result<Vec<(Vec<u16>, V)>, E> {
    use WeexHeaderValueKind::{Function, Object};
    if (runtime.kind)(headers) != Object {
        return Ok(Vec::new());
    }
    let to_json: Vec<u16> = "toJSON".encode_utf16().collect();
    let owned;
    let serialized = if (runtime.has_property)(host, headers, &to_json)?
        && (runtime.kind)(&(runtime.get_property)(host, headers, &to_json)?) == Function
    {
        let method = (runtime.get_property)(host, headers, &to_json)?;
        owned = (runtime.invoke)(host, &method, headers, None)?;
        &owned
    } else {
        headers
    };
    if (runtime.kind)(serialized) == Object {
        (runtime.entries)(host, serialized)
    } else {
        Ok(Vec::new())
    }
}

/// The caller projects error.response?.headers without changing its value kind.
pub fn get_weex_rate_limit_retry_delay_ms<H, V, E>(
    headers: &V,
    host: &mut H,
    runtime: &WeexHeaderRuntime<H, V, E>,
) -> Result<f64, E> {
    let retry_after = get_header(
        headers,
        &"retry-after".encode_utf16().collect::<Vec<_>>(),
        host,
        runtime,
    )?;
    let text = match (runtime.kind)(&retry_after) {
        WeexHeaderValueKind::String => Some((runtime.string_value)(&retry_after)),
        _ => None,
    };
    let input = match (runtime.kind)(&retry_after) {
        WeexHeaderValueKind::Number => {
            WeexRetryAfterValue::Number((runtime.number_value)(&retry_after))
        }
        WeexHeaderValueKind::String => WeexRetryAfterValue::String(text.as_ref().unwrap()),
        _ => WeexRetryAfterValue::Other,
    };
    let now = (runtime.now_ms)(host);
    let retry_after_ms =
        parse_retry_after_ms(input, now, |units| (runtime.parse_date)(host, units));
    if retry_after_ms > 0.0 {
        return Ok(clamp_weex_rate_limit_delay(retry_after_ms));
    }

    let (mut exhausted, mut observed) = (Vec::new(), Vec::new());
    for (raw_name, raw_value) in get_header_entries(headers, host, runtime)? {
        let name = (runtime.uppercase)(host, &raw_name);
        let interval_ms = parse_rate_limit_interval_ms(&name);
        if interval_ms == 0.0 {
            continue;
        }
        let prefix = |text: &str| name.starts_with(&text.encode_utf16().collect::<Vec<_>>());
        if (prefix("X-REMAINING-WEIGHT-") || prefix("X-ORDER-REMAINING-"))
            && (runtime.number)(host, &raw_value)? <= 0.0
        {
            exhausted.push(interval_ms);
        } else if prefix("X-USED-WEIGHT-") || prefix("X-ORDER-COUNT-") {
            observed.push(interval_ms);
        }
    }
    let intervals = if exhausted.is_empty() {
        &observed
    } else {
        &exhausted
    };
    if !intervals.is_empty() {
        return Ok(clamp_weex_rate_limit_delay((runtime.maximum)(
            host, intervals,
        )?));
    }
    Ok(10_000.0)
}
