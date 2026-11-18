use safe_cex_engine::ErrorKind;
use safe_cex_engine::history::IncompleteReason;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub(super) struct HistoryCompleteness {
    incomplete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<HistoryFailure>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryFailure {
    operation: &'static str,
    kind: ErrorKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    venue_code: Option<&'static str>,
}

impl HistoryCompleteness {
    pub(super) fn from_fields(fields: &Value) -> Self {
        let reason = fields
            .get("reason")
            .and_then(Value::as_str)
            .and_then(|raw| {
                [
                    IncompleteReason::PageFull,
                    IncompleteReason::CursorStalled,
                    IncompleteReason::PageBudget,
                    IncompleteReason::RequestBudget,
                    IncompleteReason::LaterPageFailed,
                    IncompleteReason::CoverageGap,
                    IncompleteReason::RetentionLimit,
                    IncompleteReason::NoNativeRead,
                ]
                .into_iter()
                .map(IncompleteReason::code)
                .find(|known| *known == raw)
            });
        let failure = (reason == Some(IncompleteReason::LaterPageFailed.code()))
            .then(|| HistoryFailure::from_fields(fields))
            .flatten();
        Self {
            incomplete: true,
            reason,
            failure,
        }
    }
}

impl HistoryFailure {
    fn from_fields(fields: &Value) -> Option<Self> {
        let operation = fields.get("historyOperation")?.as_str()?;
        let operation = [
            "bybitClosedPnl",
            "bybitOrders",
            "bybitFills",
            "bybitIncome",
            "bybitTrades",
        ]
        .into_iter()
        .find(|known| *known == operation)?;
        let raw_kind = fields.get("historyFailureKind")?.as_str()?;
        let kind = [
            ErrorKind::Rejected,
            ErrorKind::InvalidInput,
            ErrorKind::InsufficientBalance,
            ErrorKind::AuthInvalid,
            ErrorKind::AuthExpired,
            ErrorKind::AuthPermission,
            ErrorKind::RateLimited,
            ErrorKind::Network,
            ErrorKind::Timeout,
            ErrorKind::VenueUnavailable,
            ErrorKind::Unsupported,
            ErrorKind::Internal,
        ]
        .into_iter()
        .find(|known| known.as_str() == raw_kind)?;
        let venue_code = match fields.get("historyVenueCode") {
            None => None,
            Some(value) => {
                let raw = value.as_str()?;
                Some(
                    [
                        "10000", "10001", "10002", "10003", "10004", "10005", "10006", "10010",
                        "10016", "10018", "10024", "10027", "10028", "10029", "100028", "33004",
                        "429", "other",
                    ]
                    .into_iter()
                    .find(|known| *known == raw)?,
                )
            }
        };
        Some(Self {
            operation,
            kind,
            venue_code,
        })
    }
}

// Native NoticeFields serializes bounded integral command IDs as floating JSON numbers.
pub(super) fn command_owner(value: &Value) -> Option<u32> {
    let command = value.as_f64()?;
    if !command.is_finite()
        || command.fract() != 0.0
        || !(0.0..=f64::from(u32::MAX)).contains(&command)
    {
        return None;
    }
    command.to_string().parse().ok()
}
