use safe_cex_num::js_trim;
use serde::Serialize;

// ts: LIGHTER_POLICY_REASON_RE
pub const LIGHTER_POLICY_REASON_RE: fn(&str) -> bool = |reason| {
    // The TS regex lacks /u: non-ASCII characters must not fold into ASCII.
    let lower = reason.to_ascii_lowercase();
    if [
        "geo",
        "region",
        "jurisdiction",
        "restricted",
        "policy",
        "forbidden",
    ]
    .iter()
    .any(|hint| lower.contains(hint))
    {
        return true;
    }
    let bytes = lower.as_bytes();
    lower.match_indices("ip").any(|(index, _)| {
        let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
        (index == 0 || !word(bytes[index - 1]))
            && (index + 2 == bytes.len() || !word(bytes[index + 2]))
    })
};

// ts: LIGHTER_AUTH_REASON_RE
pub const LIGHTER_AUTH_REASON_RE: fn(&str) -> bool = |reason| {
    let lower = reason.to_ascii_lowercase();
    if ["auth", "token", "signature", "permission", "credential"]
        .iter()
        .any(|hint| lower.contains(hint))
    {
        return true;
    }
    lower.match_indices("api").any(|(index, _)| {
        let tail = &lower[index + 3..];
        if tail.starts_with("key") {
            return true;
        }
        tail.chars().next().is_some_and(|separator| {
            (separator == '-' || separator == '_' || js_trim(&separator.to_string()).is_empty())
                && tail[separator.len_utf8()..].starts_with("key")
        })
    })
};

// ts: LighterPublicCloseClassification
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LighterPublicCloseClassification {
    pub is_rate_limited: bool,
    pub is_policy_restricted: bool,
}

// ts: LighterPrivateCloseClassification
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LighterPrivateCloseClassification {
    pub is_handshake_failure: bool,
    pub is_rate_limited: bool,
    pub is_policy_restricted: bool,
    pub is_auth_issue: bool,
    pub is_readonly_fallback_signal: bool,
    pub should_backoff: bool,
}

// ts: classifyLighterPublicClose
pub fn classify_lighter_public_close(
    close_code: Option<f64>,
    close_reason: &str,
) -> LighterPublicCloseClassification {
    let normalized_reason = close_reason.to_lowercase();
    let is_rate_limited = close_code == Some(1013.0)
        || normalized_reason.contains("too many websocket messages")
        || normalized_reason.contains("rate limit");
    let is_policy_restricted = close_code == Some(1008.0) && LIGHTER_POLICY_REASON_RE(close_reason);
    LighterPublicCloseClassification {
        is_rate_limited,
        is_policy_restricted,
    }
}

// ts: classifyLighterPrivateClose
pub fn classify_lighter_private_close(
    close_code: Option<f64>,
    close_reason: &str,
    failed_before_open: bool,
    early_post_open_no_stream: bool,
) -> LighterPrivateCloseClassification {
    let lower_reason = close_reason.to_lowercase();
    let is_auth_issue = close_code == Some(1008.0) && LIGHTER_AUTH_REASON_RE(close_reason);
    let has_policy_hint = LIGHTER_POLICY_REASON_RE(close_reason);
    let is_policy_restricted = close_code == Some(1008.0)
        && !is_auth_issue
        && (has_policy_hint || close_reason.is_empty());
    let is_rate_limited = close_code == Some(1013.0)
        || lower_reason.contains("too many websocket messages")
        || lower_reason.contains("rate limit");
    let is_early_like_failure = failed_before_open || early_post_open_no_stream;
    let is_readonly_fallback_signal = close_code == Some(1006.0) || is_policy_restricted;
    let is_handshake_failure = is_early_like_failure
        && (close_code == Some(1006.0)
            || close_code == Some(1013.0)
            || close_code == Some(1015.0)
            || is_policy_restricted
            || is_rate_limited);
    LighterPrivateCloseClassification {
        is_handshake_failure,
        is_rate_limited,
        is_policy_restricted,
        is_auth_issue,
        is_readonly_fallback_signal,
        should_backoff: is_handshake_failure || is_rate_limited,
    }
}

// A TS object record has named fields; serde's positional sequence form is wider.
#[derive(serde::Deserialize)]
#[serde(remote = "LighterPublicCloseClassification", rename_all = "camelCase")]
struct LighterPublicCloseClassificationWire {
    is_rate_limited: bool,
    is_policy_restricted: bool,
}
impl<'de> serde::Deserialize<'de> for LighterPublicCloseClassification {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields =
            <serde_json::Map<String, serde_json::Value> as serde::Deserialize>::deserialize(
                deserializer,
            )?;
        LighterPublicCloseClassificationWire::deserialize(serde_json::Value::Object(fields))
            .map_err(serde::de::Error::custom)
    }
}

// A TS object record has named fields; serde's positional sequence form is wider.
#[derive(serde::Deserialize)]
#[serde(remote = "LighterPrivateCloseClassification", rename_all = "camelCase")]
struct LighterPrivateCloseClassificationWire {
    is_handshake_failure: bool,
    is_rate_limited: bool,
    is_policy_restricted: bool,
    is_auth_issue: bool,
    is_readonly_fallback_signal: bool,
    should_backoff: bool,
}
impl<'de> serde::Deserialize<'de> for LighterPrivateCloseClassification {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields =
            <serde_json::Map<String, serde_json::Value> as serde::Deserialize>::deserialize(
                deserializer,
            )?;
        LighterPrivateCloseClassificationWire::deserialize(serde_json::Value::Object(fields))
            .map_err(serde::de::Error::custom)
    }
}

// ts: calculateExponentialBackoffDelay.opts
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LighterBackoffOptions {
    pub multiplier: Option<f64>,
    pub max_exponent: Option<f64>,
    pub max_delay_ms: Option<f64>,
}

// ts: calculateExponentialBackoffDelay
pub fn calculate_exponential_backoff_delay(
    base_delay_ms: f64,
    failure_count: f64,
    opts: Option<LighterBackoffOptions>,
) -> f64 {
    let opts = opts.unwrap_or_default();
    let multiplier = opts.multiplier.unwrap_or(1.5);
    let max_exponent = opts.max_exponent.unwrap_or(8.0);
    let max_delay_ms = opts.max_delay_ms.unwrap_or(30_000.0);
    safe_cex_num::math_min(
        base_delay_ms
            * safe_cex_num::math_pow(
                multiplier,
                safe_cex_num::math_min(failure_count, max_exponent),
            ),
        max_delay_ms,
    )
}

// ts: calculateLighterPublicReconnectDelay
pub fn calculate_lighter_public_reconnect_delay(
    base_delay_ms: f64,
    failure_count: f64,
    is_rate_limited: bool,
    rate_limit_reconnect_delay_ms: f64,
) -> f64 {
    let backoff_delay_ms = calculate_exponential_backoff_delay(base_delay_ms, failure_count, None);
    if is_rate_limited {
        safe_cex_num::math_max(backoff_delay_ms, rate_limit_reconnect_delay_ms)
    } else {
        backoff_delay_ms
    }
}

// ts: calculateLighterPrivateReconnectDelay
pub fn calculate_lighter_private_reconnect_delay(
    base_delay_ms: f64,
    failed_handshakes: f64,
    should_backoff: bool,
    is_rate_limited: bool,
    is_ws_rate_limited: bool,
    rate_limit_reconnect_delay_ms: f64,
) -> f64 {
    let backoff_delay_ms =
        calculate_exponential_backoff_delay(base_delay_ms, failed_handshakes, None);
    let delay_before_rate_limit_ms = if should_backoff {
        backoff_delay_ms
    } else {
        base_delay_ms
    };
    if is_rate_limited || is_ws_rate_limited {
        safe_cex_num::math_max(delay_before_rate_limit_ms, rate_limit_reconnect_delay_ms)
    } else {
        delay_before_rate_limit_ms
    }
}
