//! Bybit V5 native codes for the engine classifier
//! (bybit-exchange.github.io/docs/v5/error). Status precedence, transport loss
//! and malformed acknowledgements are engine rules; this table is data only.
use safe_cex_engine::ErrorKind;
use safe_cex_engine::classify::{CodeTable, Intent, Key, Row, SatisfiedIntent, Verdict};

const fn row(key: Key, verdict: Verdict) -> Row {
    Row {
        key,
        verdict,
        only: None,
    }
}

const fn only(key: Key, verdict: Verdict, intent: Intent) -> Row {
    Row {
        key,
        verdict,
        only: Some(intent),
    }
}

pub(crate) const BYBIT: CodeTable = CodeTable {
    rows: &[
        // Server timeout, internal error and spot's unknown order states prove nothing.
        row(Key::Int(10000), Verdict::Unknown(Some(ErrorKind::Timeout))),
        row(
            Key::Int(10016),
            Verdict::Unknown(Some(ErrorKind::VenueUnavailable)),
        ),
        row(Key::Int(170001), Verdict::Unknown(None)),
        row(Key::Int(170007), Verdict::Unknown(None)),
        // A recv-window or signature refusal permits one re-signed resend.
        row(
            Key::Int(10002),
            Verdict::RefusedResend(ErrorKind::AuthInvalid),
        ),
        row(
            Key::Int(10004),
            Verdict::RefusedResend(ErrorKind::AuthInvalid),
        ),
        row(Key::Int(10003), Verdict::Refused(ErrorKind::AuthInvalid)),
        row(Key::Int(10005), Verdict::Refused(ErrorKind::AuthPermission)),
        row(Key::Int(10006), Verdict::Refused(ErrorKind::RateLimited)),
        row(Key::Int(10010), Verdict::Refused(ErrorKind::AuthPermission)),
        row(Key::Int(33004), Verdict::Refused(ErrorKind::AuthExpired)),
        row(Key::Int(10001), Verdict::Refused(ErrorKind::InvalidInput)),
        // Not-modified answers: the requested setting already holds.
        only(
            Key::Int(110043),
            Verdict::Satisfied(SatisfiedIntent::Setting),
            Intent::Setting,
        ),
        only(
            Key::Int(110025),
            Verdict::Satisfied(SatisfiedIntent::Setting),
            Intent::Setting,
        ),
        // TP/SL not modified: only on an attached TP/SL leg, which the codec
        // narrows to Setting; a plain amend answered 34040 stays refused.
        only(
            Key::Int(34040),
            Verdict::Satisfied(SatisfiedIntent::Setting),
            Intent::Setting,
        ),
        // Duplicate orderLinkId on create: the order may already exist (R2).
        only(Key::Int(10014), Verdict::Unknown(None), Intent::Place),
        only(Key::Int(110072), Verdict::Unknown(None), Intent::Place),
        // "Order does not exist" (derivatives, spot): the order is not open.
        // Not a Cancelled transition; the engine fences it for reconciliation.
        only(
            Key::Int(110001),
            Verdict::Satisfied(SatisfiedIntent::Cancel),
            Intent::Cancel,
        ),
        only(
            Key::Int(170213),
            Verdict::Satisfied(SatisfiedIntent::Cancel),
            Intent::Cancel,
        ),
    ],
    success: &[Key::Int(0)],
    // Any other explicit retCode is a pre-execution refusal, as today.
    unlisted: Verdict::Refused(ErrorKind::Rejected),
    status: &[],
};

#[cfg(test)]
#[test]
fn the_bybit_table_satisfies_the_engine_invariants() {
    safe_cex_engine::classify::assert_table(&BYBIT);
}
