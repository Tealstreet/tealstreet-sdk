//! Account-bound, testnet-only trading statements. No fund or approval actions.
use safe_cex_engine::signing::SignError;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Order {
    a: u32,
    b: bool,
    p: String,
    s: String,
    r: bool,
    t: OrderType,
    #[serde(skip_serializing_if = "Option::is_none")]
    c: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OrderType {
    limit: Limit,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Limit {
    tif: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cancel {
    a: u32,
    o: u64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Modify {
    o: u64,
    p: String,
    s: String,
}
#[derive(Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
enum Action {
    #[serde(rename = "order")]
    Order {
        grouping: String,
        orders: Vec<Order>,
    },
    #[serde(rename = "cancel")]
    Cancel { cancels: Vec<Cancel> },
    #[serde(rename = "modify2")]
    Modify { order: Modify },
    #[serde(rename = "updateLeverage")]
    Leverage { asset: u32, leverage: i32 },
    #[serde(rename = "updateMarginMode")]
    Margin {
        asset: u32,
        #[serde(rename = "isCross")]
        is_cross: bool,
    },
    #[serde(rename = "updatePositionMode")]
    Mode {
        #[serde(rename = "isOneway")]
        is_oneway: bool,
    },
}
// Named fields are serialized in the published canonical order, without type.
impl Serialize for Action {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(match self {
            Self::Order { .. } | Self::Leverage { .. } | Self::Margin { .. } => 2,
            _ => 1,
        }))?;
        match self {
            Self::Order { grouping, orders } => {
                map.serialize_entry("grouping", grouping)?;
                map.serialize_entry("orders", orders)?;
            }
            Self::Cancel { cancels } => map.serialize_entry("cancels", cancels)?,
            Self::Modify { order } => map.serialize_entry("order", order)?,
            Self::Leverage { asset, leverage } => {
                map.serialize_entry("asset", asset)?;
                map.serialize_entry("leverage", leverage)?;
            }
            Self::Margin { asset, is_cross } => {
                map.serialize_entry("asset", asset)?;
                map.serialize_entry("isCross", is_cross)?;
            }
            Self::Mode { is_oneway } => map.serialize_entry("isOneway", is_oneway)?,
        }
        map.end()
    }
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Statement<'a> {
    kind: &'a str,
    user: &'a str,
    #[serde(borrow)]
    action: &'a RawValue,
    nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vault_address: Option<&'a str>,
    testnet: bool,
}
pub fn encode(
    action: &RawValue,
    nonce: u64,
    vault: Option<&str>,
    user: &str,
) -> Result<String, SignError> {
    serde_json::to_string(&Statement {
        kind: "txflow/1",
        user,
        action,
        nonce,
        vault_address: vault,
        testnet: true,
    })
    .map_err(|_| SignError::InvalidRequest)
}
fn action(json: &str) -> Result<Action, SignError> {
    if json.len() > 256 * 1024 {
        return Err(SignError::InvalidRequest);
    }
    serde_json::from_str(json).map_err(|_| SignError::InvalidRequest)
}
pub fn connection_id(json: &str, vault: Option<&str>, nonce: u64) -> Result<[u8; 32], SignError> {
    super::action_hash(&action(json)?, vault, nonce).map_err(|_| SignError::InvalidRequest)
}
pub fn derive(json: &str, own_user: &str, own_vault: Option<&str>) -> Result<[u8; 32], SignError> {
    if json.len() > 256 * 1024 {
        return Err(SignError::InvalidRequest);
    }
    let statement: Statement<'_> =
        serde_json::from_str(json).map_err(|_| SignError::InvalidRequest)?;
    if statement.kind != "txflow/1" || !statement.testnet || statement.nonce > 9_007_199_254_740_991
    {
        return Err(SignError::InvalidRequest);
    }
    let address =
        |s: &str| safe_cex_crypto::ethereum::get_address(s).map_err(|_| SignError::InvalidRequest);
    if address(statement.user)? != address(own_user)? {
        return Err(SignError::InvalidRequest);
    }
    match (statement.vault_address, own_vault) {
        (None, None) => {}
        (Some(a), Some(b)) if address(a)? == address(b)? => {}
        _ => return Err(SignError::InvalidRequest),
    }
    Ok(super::agent_digest(connection_id(
        statement.action.get(),
        statement.vault_address,
        statement.nonce,
    )?))
}
