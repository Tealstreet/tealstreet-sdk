//! Bitrue account balance reduction over retained JSON input.
use safe_cex_num::{JsCoercionError, js_number, js_truthy, math_max};
use safe_cex_types::{Balance, object_map::JsObjectMap};
use serde_json::Value;
use std::borrow::Cow;

fn type_error(message: &'static str) -> JsCoercionError {
    JsCoercionError {
        name: "TypeError",
        message,
    }
}

pub fn parse_balance<'a>(
    data: Option<&Value>,
    stored_balance: &'a Balance,
    mut warn: impl FnMut(&str),
) -> Result<Cow<'a, Balance>, JsCoercionError> {
    if !js_truthy(data) {
        return Ok(Cow::Borrowed(stored_balance));
    }
    let data = data.expect("truthy data is present");
    let inner = match data.get("data") {
        None => {
            return Err(type_error(
                "Cannot read properties of undefined (reading 'account')",
            ));
        }
        Some(Value::Null) => {
            return Err(type_error(
                "Cannot read properties of null (reading 'account')",
            ));
        }
        Some(inner) => inner,
    };
    let accounts = match inner.get("account") {
        None => {
            return Err(type_error(
                "Cannot read properties of undefined (reading 'reduce')",
            ));
        }
        Some(Value::Null) => {
            return Err(type_error(
                "Cannot read properties of null (reading 'reduce')",
            ));
        }
        Some(Value::Array(accounts)) => accounts,
        Some(_) => return Err(type_error("data.data.account.reduce is not a function")),
    };
    let zero = Value::from(0);
    let mut reduced = JsObjectMap::new();
    for account in accounts {
        if account.is_null() {
            return Err(type_error(
                "Cannot read properties of null (reading 'marginCoin')",
            ));
        }
        let Some(coin @ ("USDT" | "USDC")) = account.get("marginCoin").and_then(Value::as_str)
        else {
            continue;
        };
        let number = |key| {
            let value = account.get(key);
            js_number(if js_truthy(value) { value } else { Some(&zero) })
        };
        let total = number("totalEquity")?;
        let free = number("accountNormal")?;
        let account_lock = number("accountLock")?;
        let maintenance = number("sumMaintenanceMargin")?;
        let position_margin = number("totalPositionNormal")?;
        let total_cost = number("totalCost")?;
        let used = [
            total - free,
            account_lock,
            maintenance,
            position_margin,
            total_cost,
            0.0,
        ]
        .into_iter()
        .reduce(math_max)
        .expect("nonempty candidates");
        let raw_upnl = account.get("sumOpenRealizedAmount");
        let upnl = match raw_upnl {
            Some(value @ Value::Number(_)) => js_number(Some(value))?,
            value => {
                let value = value.map_or_else(|| "undefined".to_owned(), Value::to_string);
                warn(&format!(
                    "Bitrue refused account row {coin}: Balance.upnl requires a number, got {value}"
                ));
                continue;
            }
        };
        reduced.insert(coin.to_owned(), (total, free, used, upnl));
    }
    let mut currencies = JsObjectMap::new();
    for (coin, (total, free, used, upnl)) in reduced {
        currencies.insert(
            coin,
            Balance {
                total,
                free,
                used,
                upnl,
                info: None,
                currencies: None,
                coins: None,
            },
        );
    }
    let mut balance = currencies
        .get("USDT")
        .or_else(|| currencies.get("USDC"))
        .cloned()
        .unwrap_or(Balance {
            total: 0.0,
            free: 0.0,
            used: 0.0,
            upnl: 0.0,
            info: None,
            currencies: None,
            coins: None,
        });
    balance.currencies = Some(
        currencies
            .into_iter()
            .map(|(key, balance)| (key, balance.into()))
            .collect(),
    );
    balance.info = Some(data.clone().into());
    Ok(Cow::Owned(balance))
}
