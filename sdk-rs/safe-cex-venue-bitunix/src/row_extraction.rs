//! Bitunix response row selection; inputs must use retained numeric capture.
use crate::response_helpers::{get_response_candidates, is_record};
use safe_cex_num::{JsCoercionError, TableHit, js_string, js_table_lookup, js_truthy};
use serde_json::{Map, Value};

/// First value excluding undefined, null and empty string, retaining inherited identity.
pub fn get_first_value<'a>(record: &'a Map<String, Value>, fields: &[&str]) -> TableHit<&'a Value> {
    for field in fields {
        if let Some(value) = record.get(*field) {
            if !value.is_null() && !matches!(value, Value::String(s) if s.is_empty()) {
                return TableHit::Mapped(value);
            }
        } else if let TableHit::Inherited(member) = js_table_lookup::<&str, Value>(&[], field) {
            return TableHit::Inherited(member);
        }
    }
    TableHit::Missing
}

/// None represents the source's newly allocated empty fallback object.
pub fn extract_account_row<'a>(
    payload: Option<&'a Value>,
    margin_coin: &str,
) -> Result<Option<&'a Value>, JsCoercionError> {
    fn is_account_row(value: &Value) -> bool {
        is_record(Some(value))
            && [
                "marginCoin",
                "available",
                "availableBalance",
                "frozen",
                "margin",
                "positionMode",
                "crossUnrealizedPNL",
                "isolationUnrealizedPNL",
            ]
            .iter()
            .any(|field| value.get(*field).is_some())
    }
    for candidate in get_response_candidates(payload) {
        if let Value::Array(values) = candidate {
            let rows: Vec<_> = values.iter().filter(|v| is_account_row(v)).collect();
            let mut preferred = None;
            for row in &rows {
                let coin = row.get("marginCoin");
                let coin = if js_truthy(coin) {
                    js_string(coin)?
                } else {
                    String::new()
                };
                if coin.to_uppercase() == margin_coin.to_uppercase() {
                    preferred = Some(*row);
                    break;
                }
            }
            if let Some(row) = preferred.or_else(|| rows.first().copied()) {
                return Ok(Some(row));
            }
        }
        if is_account_row(candidate) {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

/// First candidate array containing qualifying positions; scalar rows are not returned.
pub fn extract_position_rows(payload: Option<&Value>) -> Vec<&Value> {
    for candidate in get_response_candidates(payload) {
        let Value::Array(values) = candidate else {
            continue;
        };
        let rows: Vec<_> = values
            .iter()
            .filter(|value| {
                value.as_object().is_some_and(|record| {
                    get_first_value(record, &["symbol", "instId", "contract", "instrument"])
                        .is_truthy()
                })
            })
            .collect();
        if !rows.is_empty() {
            return rows;
        }
    }
    Vec::new()
}
