//! Inherited legacy capabilities: no DXtrade history axis is implemented by TS.
use crate::Pending;
use safe_cex_engine::history::{Completion, HistoryRows};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Outcome, Query, VenueError};
pub(crate) fn query(cx: &mut Cx<'_, Pending>, command: CommandId, query: Query) {
    let mut completion = Completion::new(command);
    match query {
        Query::Fills { .. } | Query::OrderHistory { .. } | Query::Income { .. } => {
            completion.incomplete(
                "history is INCOMPLETE: BreakoutProp adapter has no native history read",
            );
            HistoryRows::<()>::new(completion, |rows| {
                serde_json::to_string(rows).map_err(|error| VenueError::new(error.to_string()))
            })
            .finish(cx, Ok(()));
        }
        Query::PublicTrades { .. } => {
            completion.finish(
                cx,
                Err(
                    VenueError::new("BreakoutProp public trades are not implemented")
                        .kind(ErrorKind::Unsupported)
                        .outcome(Outcome::NotSent),
                ),
            );
        }
        Query::OrderByClientId { .. } => {
            completion.finish(cx, Ok(r#"{"found":false,"definitive":false}"#.into()));
        }
    }
}
