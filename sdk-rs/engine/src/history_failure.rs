//! Closed metadata for a failed later native history page, never response values.
use crate::{ErrorKind, Value, VenueError};

#[derive(Clone, Copy)]
pub enum HistoryOperation {
    BybitClosedPnl,
    BybitOrders,
    BybitFills,
    BybitIncome,
    BybitTrades,
}

impl HistoryOperation {
    fn code(self) -> &'static str {
        match self {
            Self::BybitClosedPnl => "bybitClosedPnl",
            Self::BybitOrders => "bybitOrders",
            Self::BybitFills => "bybitFills",
            Self::BybitIncome => "bybitIncome",
            Self::BybitTrades => "bybitTrades",
        }
    }
}

#[derive(Clone)]
pub(super) struct HistoryFailure {
    operation: HistoryOperation,
    kind: ErrorKind,
    venue_code: Option<&'static str>,
}

impl HistoryFailure {
    pub(super) fn new(operation: HistoryOperation, error: &VenueError) -> Self {
        // The code is a closed classification; unknown native strings never escape.
        let venue_code = error.venue_code.as_deref().map(|code| match code {
            "10000" => "10000",
            "10001" => "10001",
            "10002" => "10002",
            "10003" => "10003",
            "10004" => "10004",
            "10005" => "10005",
            "10006" => "10006",
            "10010" => "10010",
            "10016" => "10016",
            "10018" => "10018",
            "10024" => "10024",
            "10027" => "10027",
            "10028" => "10028",
            "10029" => "10029",
            "100028" => "100028",
            "33004" => "33004",
            "429" => "429",
            _ => "other",
        });
        Self {
            operation,
            kind: error.kind.unwrap_or(ErrorKind::Internal),
            venue_code,
        }
    }

    pub(super) fn append_fields(self, fields: &mut Vec<(&'static str, Value)>) {
        fields.push(("historyOperation", Value::Str(self.operation.code().into())));
        fields.push(("historyFailureKind", Value::Str(self.kind.as_str().into())));
        if let Some(code) = self.venue_code {
            fields.push(("historyVenueCode", Value::Str(code.into())));
        }
    }
}
