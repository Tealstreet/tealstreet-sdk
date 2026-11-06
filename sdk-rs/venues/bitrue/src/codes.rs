use safe_cex_engine::classify::{
    self, CodeTable, Intent, Key, Native, NativeCode, Released, Row, Verdict,
};
use safe_cex_engine::{ErrorKind, Response};

pub const BITRUE: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::Int(-1022),
            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(-1102),
            verdict: Verdict::Refused(ErrorKind::InvalidInput),
            only: None,
        },
        Row {
            key: Key::Int(-1121),
            verdict: Verdict::Refused(ErrorKind::InvalidInput),
            only: None,
        },
        // Insufficient balance: a business refusal, as native classified it.
        Row {
            key: Key::Int(-2017),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
    ],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    // Bitrue futures API "HTTP Return Codes": 403 is the WAF limit and 429 a
    // request-rate limit, both before execution; 401 keeps native's
    // unauthenticated refusal.
    status: &[
        (401, Verdict::Refused(ErrorKind::AuthInvalid)),
        (403, Verdict::Refused(ErrorKind::AuthPermission)),
        (429, Verdict::Refused(ErrorKind::RateLimited)),
    ],
};

// Only guarded replacement / put-back calls use this extension. Ordinary
// orders retain BITRUE byte-for-byte. Explicit placement validation/business
// refusals from the official Futures error list; no blanket numeric ranges:
// https://www.bitrue.com/api_docs_includes_file/futures/index.html#error-code-list
pub const GUARDED_REPLACEMENT: CodeTable = CodeTable {
    rows: &[
        BITRUE.rows[0],
        BITRUE.rows[1],
        BITRUE.rows[2],
        BITRUE.rows[3],
        protective_refusal(-1014),
        protective_refusal(-1020),
        protective_refusal(-1100),
        protective_refusal(-1101),
        protective_refusal(-1103),
        protective_refusal(-1104),
        protective_refusal(-1105),
        protective_refusal(-1106),
        protective_refusal(-1111),
        protective_refusal(-1116),
        protective_refusal(-1117),
        protective_refusal(-1118),
        protective_refusal(-1136),
        protective_refusal(-1137),
        protective_refusal(-1138),
        protective_refusal(-1139),
        protective_refusal(-1140),
        protective_refusal(-1146),
        protective_refusal(-1147),
        protective_refusal(-1148),
        protective_refusal(-1149),
        protective_refusal(-1150),
        protective_refusal(-1154),
        protective_refusal(-1155),
        protective_refusal(-1156),
        protective_refusal(-1157),
        protective_refusal(-1158),
        protective_refusal(-1159),
        protective_refusal(-1160),
        protective_refusal(-1161),
        protective_refusal(-1162),
        protective_refusal(-1164),
        protective_refusal(-1165),
        protective_refusal(-1166),
        protective_refusal(-2016),
        protective_refusal(-2100),
        Row {
            key: Key::Int(-1003),
            verdict: Verdict::Unknown(Some(ErrorKind::RateLimited)),
            only: None,
        },
    ],
    status: &[
        (401, Verdict::Refused(ErrorKind::AuthInvalid)),
        (403, Verdict::Refused(ErrorKind::AuthPermission)),
        (418, Verdict::Unknown(Some(ErrorKind::RateLimited))),
        (429, Verdict::Unknown(Some(ErrorKind::RateLimited))),
    ],
    ..BITRUE
};

const fn protective_refusal(code: i64) -> Row {
    Row {
        key: Key::Int(code),
        verdict: Verdict::Refused(ErrorKind::Rejected),
        only: Some(Intent::Place),
    }
}

#[derive(serde::Deserialize)]
struct Envelope<'a> {
    #[serde(borrow)]
    code: &'a serde_json::value::RawValue,
    #[serde(borrow, default)]
    msg: std::borrow::Cow<'a, str>,
}

pub fn resend(response: &Response<'_>) -> bool {
    let envelope = serde_json::from_str::<Envelope<'_>>(response.body).ok();
    let code = envelope
        .as_ref()
        .and_then(|e| classify::decode_int(e.code.get()));
    let native = code.map(|code| Native {
        code: NativeCode::Int(code),
        message: envelope.as_ref().map_or("", |e| e.msg.as_ref()),
    });
    matches!(
        classify::verdict(
            &BITRUE,
            Intent::Read,
            Released::Response(response),
            native.as_ref()
        ),
        Verdict::RefusedResend(_)
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn table_is_valid() {
        safe_cex_engine::classify::assert_table(&super::BITRUE);
        safe_cex_engine::classify::assert_table(&super::GUARDED_REPLACEMENT);
    }
}
