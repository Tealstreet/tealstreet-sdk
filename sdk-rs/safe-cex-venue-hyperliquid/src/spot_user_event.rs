//! Native union/intersection: array branches retain their attached user property.
use crate::{
    spot_json::JsString,
    spot_models::{WsFill, WsLiquidation, WsNonUserCancel, WsUserFunding},
};
#[derive(Debug, Clone, PartialEq)]
pub enum WsUserEvent {
    Fills {
        entries: Vec<WsFill>,
        user: JsString,
    },
    Funding {
        entry: WsUserFunding,
        user: JsString,
    },
    Liquidation {
        entry: WsLiquidation,
        user: JsString,
    },
    NonUserCancels {
        entries: Vec<WsNonUserCancel>,
        user: JsString,
    },
}
impl WsUserEvent {
    pub fn user(&self) -> &JsString {
        match self {
            Self::Fills { user, .. }
            | Self::Funding { user, .. }
            | Self::Liquidation { user, .. }
            | Self::NonUserCancels { user, .. } => user,
        }
    }
}
