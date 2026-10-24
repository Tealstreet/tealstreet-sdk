//! Native Bitunix codes; the engine owns error precedence and retry evidence.
use safe_cex_engine::{
    ErrorKind, Response,
    classify::{self, CodeTable, Key, Native, NativeCode, ReadFailure, Released, Row, Verdict},
};
use serde::Deserialize;
use std::borrow::Cow;

const fn row(code: Key, verdict: Verdict) -> Row {
    Row {
        key: code,
        verdict,
        only: None,
    }
}
pub(crate) const BITUNIX: CodeTable = CodeTable {
    rows: &[
        row(Key::Int(403), Verdict::Refused(ErrorKind::AuthPermission)),
        row(Key::Int(10004), Verdict::Refused(ErrorKind::AuthPermission)),
        row(Key::Int(10003), Verdict::Refused(ErrorKind::AuthInvalid)),
        row(Key::Int(10007), Verdict::Refused(ErrorKind::AuthInvalid)),
        row(Key::Int(10005), Verdict::Refused(ErrorKind::RateLimited)),
        row(Key::Int(10006), Verdict::Refused(ErrorKind::RateLimited)),
        row(Key::Int(10001), Verdict::Unknown(Some(ErrorKind::Network))),
        row(Key::Int(10002), Verdict::Refused(ErrorKind::InvalidInput)),
        row(Key::Int(10008), Verdict::Refused(ErrorKind::InvalidInput)),
        row(Key::Int(10010), Verdict::Refused(ErrorKind::InvalidInput)),
        row(Key::Int(10011), Verdict::Refused(ErrorKind::InvalidInput)),
        row(Key::Int(10009), Verdict::Refused(ErrorKind::Rejected)),
        row(
            Key::Range(20001, 20016),
            Verdict::Refused(ErrorKind::Rejected),
        ),
        row(
            Key::Range(30001, 30042),
            Verdict::Refused(ErrorKind::Rejected),
        ),
        row(
            Key::Range(40001, 40008),
            Verdict::Refused(ErrorKind::Rejected),
        ),
    ],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(Some(ErrorKind::VenueUnavailable)),
    status: &[],
};
#[derive(Deserialize)]
struct Evidence<'a> {
    #[serde(default, borrow)]
    code: Option<&'a serde_json::value::RawValue>,
    #[serde(default, borrow, deserialize_with = "crate::wire::text")]
    msg: Cow<'a, str>,
    #[serde(default, borrow)]
    data: Option<&'a serde_json::value::RawValue>,
}
pub(crate) fn seed_success(response: &Response<'_>) -> bool {
    (200..300).contains(&response.status)
        && serde_json::from_str::<Evidence<'_>>(response.body).is_ok_and(|e| {
            e.code
                .and_then(|c| serde_json::from_str::<crate::error::Code>(c.get()).ok())
                .is_some_and(|c| c.0 == 0)
                && e.data.is_some()
        })
}
pub(crate) fn read_failure(response: &Response<'_>, fallback: &str) -> ReadFailure {
    let parsed = serde_json::from_str::<Evidence<'_>>(response.body).ok();
    let native = parsed.as_ref().and_then(|e| {
        e.code
            .and_then(|c| classify::decode_int(c.get()))
            .filter(|code| *code != 0)
            .map(|c| Native {
                code: NativeCode::Int(c),
                message: &e.msg,
            })
    });
    let message = native
        .as_ref()
        .filter(|native| !native.message.is_empty())
        .map(|native| native.message.to_owned())
        .unwrap_or_else(|| {
            if (200..300).contains(&response.status) {
                fallback.into()
            } else {
                format!("Bitunix HTTP {}", response.status)
            }
        });
    classify::read(
        &BITUNIX,
        Released::Response(response),
        native.as_ref(),
        message,
    )
}
#[cfg(test)]
mod tests {
    #[test]
    fn shared_native_table_is_valid() {
        safe_cex_engine::classify::assert_table(&super::BITUNIX);
    }
}
