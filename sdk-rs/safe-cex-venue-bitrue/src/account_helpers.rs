//! Account-schema guards used before replacing a private position snapshot.
use safe_cex_num::{JsCoercionError, js_number, js_truthy};
use safe_cex_types::PositionSide;
use serde_json::Value;

pub fn get_position_accounts(data: Option<&Value>) -> Result<Option<Vec<&Value>>, JsCoercionError> {
    let Some(accounts) = data
        .and_then(|data| data.get("data"))
        .and_then(|data| data.get("account"))
        .and_then(Value::as_array)
    else {
        return Ok(None);
    };
    let futures_accounts: Vec<_> = accounts
        .iter()
        .filter(|account| {
            js_truthy(Some(account))
                && matches!(
                    account.get("marginCoin").and_then(Value::as_str),
                    Some("USDT" | "USDC")
                )
        })
        .collect();
    if futures_accounts.is_empty() {
        return Ok(None);
    }
    for account in &futures_accounts {
        let Some(position_vos) = account.get("positionVos").and_then(Value::as_array) else {
            return Ok(None);
        };
        for position_vo in position_vos {
            if !js_truthy(Some(position_vo))
                || !position_vo
                    .get("contractName")
                    .and_then(Value::as_str)
                    .is_some_and(|name| !name.is_empty())
            {
                return Ok(None);
            }
            let Some(positions) = position_vo.get("positions").and_then(Value::as_array) else {
                return Ok(None);
            };
            for position in positions {
                if !js_truthy(Some(position))
                    || !matches!(
                        position.get("side").and_then(Value::as_str),
                        Some("BUY" | "SELL")
                    )
                {
                    return Ok(None);
                }
                for field in ["volume", "openPrice", "leverageLevel"] {
                    if !js_number(position.get(field))?.is_finite() {
                        return Ok(None);
                    }
                }
                if js_number(position.get("leverageLevel"))? <= 0.0 {
                    return Ok(None);
                }
                if !js_number(position.get("positionType"))?.is_finite() {
                    return Ok(None);
                }
            }
        }
    }
    Ok(Some(futures_accounts))
}

/// Anonymous source snapshot fields retain raw leverage/type values after the
/// Number-based validation: source strings, arrays and booleans are not coerced.
#[derive(Debug, Clone)]
pub struct AccountPositionSnapshot<'a> {
    pub symbol: &'a str,
    pub side: PositionSide,
    pub leverage_level: &'a Value,
    pub position_type: &'a Value,
}

pub fn get_account_position_snapshots(
    data: Option<&Value>,
) -> Result<Option<Vec<AccountPositionSnapshot<'_>>>, JsCoercionError> {
    let Some(accounts) = get_position_accounts(data)? else {
        return Ok(None);
    };
    let mut snapshots = Vec::new();
    for account in accounts {
        for position_vo in account["positionVos"].as_array().expect("validated array") {
            for position in position_vo["positions"]
                .as_array()
                .expect("validated array")
            {
                snapshots.push(AccountPositionSnapshot {
                    symbol: position_vo["contractName"]
                        .as_str()
                        .expect("validated string"),
                    side: if position["side"] == "BUY" {
                        PositionSide::Long
                    } else {
                        PositionSide::Short
                    },
                    leverage_level: &position["leverageLevel"],
                    position_type: &position["positionType"],
                });
            }
        }
    }
    Ok(Some(snapshots))
}
