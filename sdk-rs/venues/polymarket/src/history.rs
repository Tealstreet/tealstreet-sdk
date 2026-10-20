//! Rare queries stay explicit: an upstream endpoint is not a safe-cex contract.
use crate::{Pending, Polymarket};
use safe_cex_engine::{Command, Cx, Query};

impl Polymarket {
    pub(crate) fn read_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::FetchLeverage { command, .. } => {
                cx.done(command, Ok(r#"{"buyLeverage":1,"sellLeverage":1}"#.into()))
            }
            Command::SetLeverage { command, .. }
            | Command::SetMarginType { command, .. }
            | Command::SetPositionMode { command, .. } => cx.done(command, Ok("[]".into())),
            Command::Query { command, query } => {
                let result = match query {
                    Query::OrderByClientId { .. } => {
                        Ok(r#"{"found":false,"definitive":false}"#.into())
                    }
                    Query::Fills { .. } => {
                        Err("Polymarket historical fills are unsupported".into())
                    }
                    Query::OrderHistory { .. } => {
                        Err("Polymarket historical orders are unsupported".into())
                    }
                    Query::Income { .. } => Err("Polymarket income history is unsupported".into()),
                    Query::PublicTrades { .. } => {
                        Err("Polymarket public trades are unsupported".into())
                    }
                };
                cx.done(command, result);
            }
            _ => unreachable!("only rare reads and immutable account settings"),
        }
    }
}
