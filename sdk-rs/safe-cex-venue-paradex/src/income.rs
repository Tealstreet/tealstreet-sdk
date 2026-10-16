//! Source correspondence: paradex.income.ts; JSON-shaped inputs, millisecond ledger time.
use safe_cex_num::{
    JsCoercionError, js_number, js_number_string, js_string, js_truthy, parse_float,
};
use safe_cex_types::{IncomeCategory, IncomeRecord};
use serde_json::{Map, Value};

// ts: toNumber
pub fn to_number(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    let text = js_string(
        value
            .filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )?;
    let parsed = parse_float(&text);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

// ts: mapParadexFillToIncomeRecords
pub fn map_paradex_fill_to_income_records<E: From<JsCoercionError>>(
    row: &Map<String, Value>,
    mut resolve_symbol: impl FnMut(&str) -> Result<Option<String>, E>,
) -> Result<Vec<IncomeRecord>, E> {
    let timestamp = js_number(
        row.get("created_at")
            .filter(|v| !v.is_null())
            .or(Some(&Value::Null)),
    )?;
    let market = js_string(
        row.get("market")
            .filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )?;
    if timestamp == 0.0 || timestamp.is_nan() || market.is_empty() {
        return Ok(Vec::new());
    }

    let symbol = resolve_symbol(&market)?
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| market.clone());
    let asset = js_string(
        row.get("fee_currency")
            .filter(|v| js_truthy(Some(v)))
            .or(Some(&Value::String("USDC".into()))),
    )?;
    // Preserve ?? laziness: an explicit id must not coerce order_id.
    let id = if let Some(id) = row.get("id").filter(|v| !v.is_null()) {
        js_string(Some(id))?
    } else {
        let order_id = js_string(
            row.get("order_id")
                .filter(|v| !v.is_null())
                .or(Some(&Value::String(String::new()))),
        )?;
        format!("{}:{market}:{order_id}", js_number_string(timestamp))
    };

    let realized_pnl = to_number(row.get("realized_pnl"))?;
    let fee = to_number(row.get("fee"))?;
    let funding = to_number(row.get("realized_funding"))?;

    let mut records = Vec::new();
    let mut push = |income, category, income_type: &str, suffix| {
        records.push(IncomeRecord {
            symbol: symbol.clone(),
            income_type: income_type.into(),
            income,
            asset: asset.clone(),
            timestamp,
            tran_id: format!("{id}:{suffix}"),
            trade_id: Some(id.clone()),
            category: Some(category),
        });
    };
    if realized_pnl != 0.0 {
        push(
            realized_pnl,
            IncomeCategory::RealizedPnl,
            "REALIZED_PNL",
            "pnl",
        );
    }
    if fee != 0.0 {
        push(
            -fee,
            if fee > 0.0 {
                IncomeCategory::Commission
            } else {
                IncomeCategory::Rebate
            },
            if fee > 0.0 { "COMMISSION" } else { "REBATE" },
            "fee",
        );
    }
    if funding != 0.0 {
        push(funding, IncomeCategory::Funding, "FUNDING_FEE", "funding");
    }
    Ok(records)
}
