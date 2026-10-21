//! Inherited legacy capabilities: no DXtrade history axis is implemented by TS.
use crate::Pending;
use safe_cex_engine::{CommandId, Cx, Query};
pub(crate) fn query(cx: &mut Cx<'_, Pending>, command: CommandId, query: Query) {
    match query {
        Query::Fills { .. } | Query::OrderHistory { .. } | Query::Income { .. } => {
            cx.done(command, "[]".into())
        }
        Query::PublicTrades { .. } => {
            cx.fail(command, "BreakoutProp public trades are not implemented")
        }
        Query::OrderByClientId { .. } => {
            cx.done(command, r#"{"found":false,"definitive":false}"#.into())
        }
    }
}
