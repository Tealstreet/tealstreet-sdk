//! Inherited legacy capabilities: no DXtrade history axis is implemented by TS.
use crate::Pending;
use safe_cex_engine::{CommandId, Cx, Query};
pub(crate) fn query(cx: &mut Cx<'_, Pending>, command: CommandId, query: Query) {
    match query {
        Query::Fills { .. } | Query::OrderHistory { .. } | Query::Income { .. } => {
            cx.done(command, Ok("[]".into()))
        }
        Query::PublicTrades { .. } => cx.done(
            command,
            Err("BreakoutProp public trades are not implemented".into()),
        ),
        Query::OrderByClientId { .. } => {
            cx.done(command, Ok(r#"{"found":false,"definitive":false}"#.into()))
        }
    }
}
