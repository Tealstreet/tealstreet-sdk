//! Classification for Bitunix command writes. Native codes must not default to success.
use safe_cex_engine::{ErrorKind, Outcome, Response, VenueError};
use serde::{Deserialize, Deserializer, de};
use std::borrow::Cow;

#[derive(Clone, Copy)]
pub(crate) struct Code(pub i64);
impl<'de> Deserialize<'de> for Code {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Value<'a> {
            Number(i64),
            Text(#[serde(borrow)] Cow<'a, str>),
        }
        match Value::deserialize(d)? {
            Value::Number(value) => Ok(Self(value)),
            Value::Text(value) => value.parse::<i64>().map(Self).map_err(de::Error::custom),
        }
    }
}
/// Default only when absent; a present null/malformed native code is not zero.
pub(crate) fn present_code<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Code>, D::Error> {
    Code::deserialize(d).map(Some)
}
#[derive(Deserialize)]
struct Envelope<'a> {
    code: Code,
    #[serde(default, borrow, deserialize_with = "crate::wire::text")]
    msg: Cow<'a, str>,
    #[serde(default, borrow)]
    data: Option<&'a serde_json::value::RawValue>,
}
pub(crate) fn not_sent(message: impl Into<String>, kind: ErrorKind) -> VenueError {
    VenueError::new(message)
        .kind(kind)
        .outcome(Outcome::NotSent)
}
pub(crate) fn transport(message: &str, write: bool) -> VenueError {
    let error = VenueError::new(message).kind(ErrorKind::Network);
    if write {
        error.outcome(Outcome::Unknown)
    } else {
        error
    }
}
pub(crate) fn malformed(message: impl Into<String>, write: bool) -> VenueError {
    let error = VenueError::new(message).kind(ErrorKind::Internal);
    if write {
        error.outcome(Outcome::Unknown)
    } else {
        error
    }
}
pub(crate) fn native(code: Option<Code>, message: String, write: bool) -> VenueError {
    let kind = match code.map(|code| code.0) {
        Some(403 | 10004) => ErrorKind::AuthPermission,
        Some(10003 | 10007) => ErrorKind::AuthInvalid,
        Some(10005 | 10006) => ErrorKind::RateLimited,
        Some(10001) => ErrorKind::Network,
        Some(0) => ErrorKind::Internal,
        Some(10002 | 10008 | 10010 | 10011) => ErrorKind::InvalidInput,
        Some(10009 | 20001..=20016 | 30001..=30042 | 40001..=40008) | None => ErrorKind::Rejected,
        _ => ErrorKind::VenueUnavailable,
    };
    let mut error = VenueError::new(message.clone()).kind(kind);
    if let Some(code) = code {
        error = error.venue_code(code.0.to_string()).venue_message(message);
    }
    if write {
        error = error.outcome(
            if matches!(
                kind,
                ErrorKind::Network | ErrorKind::Internal | ErrorKind::VenueUnavailable
            ) {
                Outcome::Unknown
            } else {
                Outcome::Rejected
            },
        );
    }
    error
}
pub(crate) fn response_data<'a>(
    response: &Response<'a>,
    write: bool,
) -> Result<&'a str, VenueError> {
    let parsed = serde_json::from_str::<Envelope<'a>>(response.body);
    if !(200..300).contains(&response.status) {
        // Preserve the old native-message/HTTP-message selection, but HTTP owns classification.
        let message = crate::wire::data(response.body)
            .err()
            .unwrap_or_else(|| format!("Bitunix HTTP {}", response.status));
        let kind = match response.status {
            401 => ErrorKind::AuthInvalid,
            403 => ErrorKind::AuthPermission,
            429 => ErrorKind::RateLimited,
            408 => ErrorKind::Timeout,
            500..=599 => ErrorKind::VenueUnavailable,
            400..=499 => ErrorKind::Rejected,
            _ => ErrorKind::Internal,
        };
        let mut error = VenueError::new(message).kind(kind);
        if let Ok(envelope) = parsed
            && envelope.code.0 != 0
        {
            error = error
                .venue_code(envelope.code.0.to_string())
                .venue_message(envelope.msg.into_owned());
        }
        if write {
            error = error.outcome(
                if response.status == 408 || response.status >= 500 || response.status < 400 {
                    Outcome::Unknown
                } else {
                    Outcome::Rejected
                },
            );
        }
        if response.status == 429
            && let Some(ms) = response
                .header("retry-after")
                .and_then(|text| text.parse::<f64>().ok())
                .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
                .map(|seconds| seconds * 1000.0)
        {
            error = error.retry_after_ms(ms);
        }
        return Err(error);
    }
    let envelope = parsed.map_err(|_| {
        malformed(
            crate::wire::data(response.body)
                .err()
                .unwrap_or_else(|| "Malformed Bitunix response code".into()),
            write,
        )
    })?;
    if envelope.code.0 != 0 {
        return Err(native(
            Some(envelope.code),
            envelope.msg.into_owned(),
            write,
        ));
    }
    envelope
        .data
        .map(|data| data.get())
        .ok_or_else(|| malformed("Bitunix response has no data", write))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_codes_and_http_precedence_never_claim_an_uncertain_write_was_rejected() {
        for code in ["null", "true", "1.5", "\"bad\"", "\"\""] {
            let body = format!(r#"{{"code":{code},"data":{{}}}}"#);
            let error = response_data(
                &Response {
                    status: 200,
                    headers: "",
                    body: &body,
                },
                true,
            )
            .unwrap_err();
            assert_eq!(error.outcome, Some(Outcome::Unknown));
        }
        for status in [408, 500, 503] {
            let error = response_data(
                &Response {
                    status,
                    headers: "",
                    body: r#"{"code":"10007","msg":"Sign signature error"}"#,
                },
                true,
            )
            .unwrap_err();
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            assert_ne!(error.kind, Some(ErrorKind::AuthInvalid));
            assert_eq!(error.venue_code.as_deref(), Some("10007"));
            assert_eq!(error.message, "Sign signature error");
        }
    }
    #[test]
    fn native_codes_preserve_diagnostics_and_read_failures_never_claim_a_write() {
        for (code, kind) in [
            (10001, ErrorKind::Network),
            (10003, ErrorKind::AuthInvalid),
            (10004, ErrorKind::AuthPermission),
            (10005, ErrorKind::RateLimited),
            (10006, ErrorKind::RateLimited),
            (10007, ErrorKind::AuthInvalid),
            (20003, ErrorKind::Rejected),
        ] {
            for quoted in [false, true] {
                let value = if quoted {
                    format!("\"{code}\"")
                } else {
                    code.to_string()
                };
                let body = format!(r#"{{"code":{value},"msg":"verbatim"}}"#);
                for write in [false, true] {
                    let error = response_data(
                        &Response {
                            status: 200,
                            headers: "",
                            body: &body,
                        },
                        write,
                    )
                    .unwrap_err();
                    assert_eq!(error.kind, Some(kind));
                    assert_eq!(error.message, "verbatim");
                    assert_eq!(error.venue_code, Some(code.to_string()));
                    assert_eq!(
                        error.outcome,
                        if !write {
                            None
                        } else if code == 10001 {
                            Some(Outcome::Unknown)
                        } else {
                            Some(Outcome::Rejected)
                        }
                    );
                }
            }
        }
    }
}
