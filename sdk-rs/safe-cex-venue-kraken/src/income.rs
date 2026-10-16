//! Function-by-function port of kraken.income.ts on the JSON wire domain.
use crate::wire::{decode, text};
use safe_cex_num::{
    JsCoercionError, js_date_parse, js_number, js_truthy, numeric_ingress::JsNumberInput,
};
use safe_cex_types::{IncomeCategory, IncomeRecord};
use serde_json::Value;

pub fn is_position_size_leg(row: &Value) -> Result<bool, JsCoercionError> {
    let asset = text(row.get("asset"))?.to_lowercase();
    let contract = text(row.get("contract"))?.to_lowercase();
    Ok(!asset.is_empty() && asset == contract)
}
pub fn num(value: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    if value.is_none_or(|v| v.is_null() || v.as_str() == Some("")) {
        return Ok(None);
    }
    let parsed = js_number(value)?;
    Ok(parsed.is_finite().then_some(parsed))
}
pub fn to_ms(
    value: Option<&Value>,
    local_to_utc: impl FnOnce(f64) -> f64,
) -> Result<f64, JsCoercionError> {
    let parsed = js_date_parse(&text(value)?, local_to_utc);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}
pub fn map_kraken_account_log_row(
    input: JsNumberInput,
    resolve_symbol: &mut impl FnMut(&str) -> Result<Option<String>, Box<dyn std::error::Error>>,
    local_to_utc: &mut impl FnMut(f64) -> f64,
) -> Result<Vec<IncomeRecord>, Box<dyn std::error::Error>> {
    let row = decode(input)?;
    if !js_truthy(Some(&row)) || is_position_size_leg(&row)? {
        return Ok(Vec::new());
    }
    let timestamp = to_ms(row.get("date"), local_to_utc)?;
    let booking_uid = text(
        row.get("booking_uid")
            .filter(|v| !v.is_null())
            .or_else(|| row.get("id")),
    )?;
    if timestamp == 0.0 || booking_uid.is_empty() {
        return Ok(Vec::new());
    }
    let contract = text(row.get("contract"))?;
    let info = match row.get("info") {
        None | Some(Value::Null) => "unknown".to_owned(),
        value => text(value)?,
    };
    let base = IncomeRecord {
        symbol: if contract.is_empty() {
            String::new()
        } else {
            resolve_symbol(&contract)?.unwrap_or(contract)
        },
        income_type: info.clone(),
        asset: text(row.get("asset"))?.to_uppercase(),
        timestamp,
        trade_id: if js_truthy(row.get("execution")) {
            Some(text(row.get("execution"))?)
        } else {
            None
        },
        tran_id: String::new(),
        income: 0.0,
        category: None,
    };
    let realized_pnl = num(row.get("realized_pnl"))?;
    let realized_funding = num(row.get("realized_funding"))?;
    let fee = num(row.get("fee"))?;
    let liquidation_fee = num(row.get("liquidation_fee"))?;
    let mut records = Vec::new();
    if let Some(value) = realized_pnl.filter(|v| *v != 0.0) {
        records.push(IncomeRecord {
            tran_id: format!("{booking_uid}:pnl"),
            income: value,
            category: Some(IncomeCategory::RealizedPnl),
            ..base.clone()
        });
    }
    if let Some(value) = realized_funding.filter(|v| *v != 0.0) {
        records.push(IncomeRecord {
            tran_id: format!("{booking_uid}:funding"),
            income_type: format!("{info}_funding"),
            income: value,
            category: Some(IncomeCategory::Funding),
            ..base.clone()
        });
    }
    if let Some(value) = fee.filter(|v| *v != 0.0) {
        records.push(IncomeRecord {
            tran_id: format!("{booking_uid}:fee"),
            income_type: format!("{info}_fee"),
            income: -value,
            category: Some(IncomeCategory::Commission),
            ..base.clone()
        });
    }
    if let Some(value) = liquidation_fee.filter(|v| *v != 0.0) {
        records.push(IncomeRecord {
            tran_id: format!("{booking_uid}:liquidation"),
            income_type: format!("{info}_liquidation_fee"),
            income: -value,
            category: Some(IncomeCategory::Liquidation),
            ..base.clone()
        });
    }
    if records.is_empty() {
        let delta = num(row.get("new_balance"))?.unwrap_or(0.0)
            - num(row.get("old_balance"))?.unwrap_or(0.0);
        if delta != 0.0 {
            let transfer = ["transfer", "conversion", "deposit", "withdraw"]
                .iter()
                .any(|word| info.to_ascii_lowercase().contains(word));
            let (label, category) = if transfer {
                ("transfer", IncomeCategory::Transfer)
            } else {
                ("other", IncomeCategory::Other)
            };
            records.push(IncomeRecord {
                tran_id: format!("{booking_uid}:{label}"),
                income: delta,
                category: Some(category),
                ..base
            });
        }
    }
    Ok(records)
}
pub fn map_kraken_account_log(
    input: JsNumberInput,
    resolve_symbol: &mut impl FnMut(&str) -> Result<Option<String>, Box<dyn std::error::Error>>,
    local_to_utc: &mut impl FnMut(f64) -> f64,
) -> Result<Vec<IncomeRecord>, Box<dyn std::error::Error>> {
    let rows = input.decode::<Vec<JsNumberInput>>()?;
    let mut records = Vec::new();
    for row in rows {
        records.extend(map_kraken_account_log_row(
            row,
            resolve_symbol,
            local_to_utc,
        )?);
    }
    Ok(records)
}
