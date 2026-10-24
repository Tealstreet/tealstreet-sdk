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
        Row {
            key: Key::Int(-2017),
            verdict: Verdict::Refused(ErrorKind::InvalidInput),
            only: None,
        },
    ],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[],
};

pub fn resend(response: &Response<'_>) -> bool {
    let envelope = serde_json::from_str::<crate::wire::Envelope<'_>>(response.body).ok();
    let code = envelope
        .as_ref()
        .and_then(|e| classify::decode_int(&e.code));
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
    }
}
