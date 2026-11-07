//! BingX refuses a signed request whose `timestamp` left its 5000 ms window with
//! 109400, before any processing, so one freshly stamped resend is safe.
use safe_cex_engine::log::code;
use safe_cex_engine::{Cx, Level};

#[derive(serde::Deserialize)]
struct Refusal<'a> {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default, borrow)]
    msg: std::borrow::Cow<'a, str>,
}

/// 109400 is BingX's generic invalid-parameter code; only its timestamp text
/// proves the refusal was the signing clock.
pub(crate) fn refused(body: &str) -> bool {
    serde_json::from_str::<Refusal<'_>>(body)
        .is_ok_and(|r| r.code == Some(109_400) && r.msg.to_ascii_lowercase().contains("timestamp"))
}

/// The one line for a timestamp refusal's resend, with the offset it signs at.
pub(crate) fn note_resend<P>(cx: &mut Cx<'_, P>) {
    let offset = cx.protocol_now() - cx.now;
    cx.notice(
        Level::Warn,
        code::REQUEST_FAILED,
        format!(
            "BingX refused a request timestamp (109400); re-signing and resending it once at clock offset {offset:+.0} ms"
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::refused;

    #[test]
    fn only_a_109400_naming_the_timestamp_is_clock_evidence() {
        assert!(refused(r#"{"code":109400,"msg":"timestamp is invalid"}"#));
        assert!(refused(
            r#"{"code":109400,"msg":"Invalid Timestamp, please check","data":{}}"#
        ));
        assert!(!refused(r#"{"code":109400,"msg":"invalid symbol"}"#));
        assert!(!refused(r#"{"code":100001,"msg":"timestamp"}"#));
        assert!(!refused("timestamp is invalid"));
    }
}
