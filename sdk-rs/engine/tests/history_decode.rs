use safe_cex_engine::{ErrorCause, ErrorKind, VenueError};

#[test]
fn ordinary_history_error_wire_has_no_row_detail() {
    let error = VenueError::new("history rejected")
        .kind(ErrorKind::Internal)
        .cause_code(ErrorCause::HistoryDecode);
    assert_eq!(
        serde_json::to_string(&error).unwrap(),
        r#"{"message":"history rejected","kind":"internal","causeCode":"HISTORY_DECODE"}"#
    );
}

#[cfg(feature = "history-row-diagnostics")]
#[test]
fn enabled_row_detail_is_closed_and_keeps_the_existing_error_wire() {
    use safe_cex_engine::{HistoryDecodeReason, HistorySchema};
    let error = VenueError::new("history rejected").history_decode(
        HistorySchema::KrakenOrders,
        HistoryDecodeReason::FilledQuantity,
    );
    assert_eq!(
        serde_json::to_string(&error).unwrap(),
        r#"{"message":"history rejected","kind":"internal","causeCode":"HISTORY_DECODE","historyDecode":{"schema":"krakenOrders","reason":"filledQuantity"}}"#
    );
}
