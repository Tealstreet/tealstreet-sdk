//! Closed store vocabulary: never accept venue error text as an outcome.
use crate::Store;
use safe_cex_types::RequestOutcome;

/// Host supplies the signing-capture predicate; Axios wiring remains pending.
pub fn record_request_outcome(
    store: Option<&dyn Store>,
    status: Option<f64>,
    had_response: bool,
    signing_capture: bool,
) {
    if signing_capture {
        return;
    }
    let outcome = if !had_response {
        RequestOutcome::Network
    } else if status.is_none() {
        RequestOutcome::Other
    } else if matches!(status, Some(429.0 | 418.0)) {
        RequestOutcome::RateLimit
    } else if matches!(status, Some(401.0 | 403.0)) {
        RequestOutcome::Auth
    } else if status.is_some_and(|s| (200.0..300.0).contains(&s)) {
        RequestOutcome::Ok
    } else {
        RequestOutcome::Other
    };
    if let Some(store) = store {
        // safeCexAxiosCreate.ts219-222: counting must never break a request.
        let _ = store.record_request_outcome(outcome);
    }
}
