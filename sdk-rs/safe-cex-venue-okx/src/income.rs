//! Bill decomposition preserves the fee/PnL/residual reconciliation identity.
use safe_cex_types::{IncomeCategory, IncomeRecord};
use serde_json::Value;

pub const OKX_BILL_TYPE_BY_INCOME_TYPE: [(&str, &str); 4] = [
    ("REALIZED_PNL", "2"),
    ("COMMISSION", "2"),
    ("REBATE", "2"),
    ("FUNDING_FEE", "8"),
];

pub fn to_number(value: &Value) -> Result<f64, JsCoercionError> {
    let parsed = parse_float(&nullish_string(value, "")?);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

fn bill_category_hit(
    value: &Value,
) -> Result<safe_cex_num::TableHit<IncomeCategory>, JsCoercionError> {
    use safe_cex_num::{TableHit, js_table_lookup};
    let table = [
        ("1", IncomeCategory::Transfer),
        ("6", IncomeCategory::Transfer),
        ("12", IncomeCategory::Transfer),
        ("2", IncomeCategory::RealizedPnl),
        ("5", IncomeCategory::Liquidation),
        ("9", IncomeCategory::Liquidation),
        ("8", IncomeCategory::Funding),
    ];
    Ok(match js_table_lookup(&table, &nullish_string(value, "")?) {
        TableHit::Mapped(category) => TableHit::Mapped(category.clone()),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Mapped(IncomeCategory::Other),
    })
}
fn category_refusal() -> JsCoercionError {
    JsCoercionError {
        name: "RepresentationError",
        message: "OKX inherited income category cannot cross the closed typed output boundary",
    }
}
pub fn okx_bill_category(value: &Value) -> Result<IncomeCategory, JsCoercionError> {
    match bill_category_hit(value)? {
        safe_cex_num::TableHit::Mapped(category) => Ok(category),
        safe_cex_num::TableHit::Inherited(_) => Err(category_refusal()),
        safe_cex_num::TableHit::Missing => {
            unreachable!("unknown codes already take the source fallback")
        }
    }
}

pub fn okx_income_type_for_category(category: &IncomeCategory) -> &'static str {
    match category {
        IncomeCategory::RealizedPnl => "REALIZED_PNL",
        IncomeCategory::Funding => "FUNDING_FEE",
        IncomeCategory::Commission => "COMMISSION",
        IncomeCategory::Rebate => "REBATE",
        IncomeCategory::Transfer => "TRANSFER",
        IncomeCategory::Liquidation => "LIQUIDATION",
        IncomeCategory::Other => "OTHER",
    }
}

pub fn map_okx_bill_to_income_records<E: From<JsCoercionError>>(
    row: &serde_json::Map<String, Value>,
    resolve_symbol: impl Fn(&str) -> Result<Option<String>, E>,
) -> Result<Vec<IncomeRecord>, E> {
    let timestamp = safe_cex_num::parse_int(
        &nullish_string(row.get("ts").unwrap_or(&Value::Null), "0")?,
        Some(10),
    );
    if !timestamp.is_finite() || timestamp <= 0.0 {
        return Ok(vec![]);
    }
    let balance_change = to_number(row.get("balChg").unwrap_or(&Value::Null))?;
    let fee = to_number(row.get("fee").unwrap_or(&Value::Null))?;
    let pnl = to_number(row.get("pnl").unwrap_or(&Value::Null))?;
    let instrument = truthy_string(row.get("instId").unwrap_or(&Value::Null))?;
    let symbol = if instrument.is_empty() {
        String::new()
    } else {
        resolve_symbol(&instrument)?
            .filter(|s| !s.is_empty())
            .unwrap_or(instrument)
    };
    let asset = truthy_string(row.get("ccy").unwrap_or(&Value::Null))?;
    let bill_id = if truthy(row.get("billId").unwrap_or(&Value::Null)) {
        js_string(Some(row.get("billId").unwrap_or(&Value::Null)))?
    } else {
        truthy_string(row.get("ts").unwrap_or(&Value::Null))?
    };
    let trade_id = truthy(row.get("tradeId").unwrap_or(&Value::Null))
        .then(|| js_string(Some(row.get("tradeId").unwrap_or(&Value::Null))))
        .transpose()?
        .filter(|s| !s.is_empty());
    let category_hit = bill_category_hit(row.get("type").unwrap_or(&Value::Null))?;
    // Keep the inherited hit through source arithmetic and fee-only/no-output branches.
    // An ordinary fallback must never escape for an inherited category.
    let category = match &category_hit {
        safe_cex_num::TableHit::Mapped(category) => category.clone(),
        _ => IncomeCategory::Other,
    };
    let mut records = Vec::new();
    let mut push = |income, category: IncomeCategory, suffix| {
        records.push(IncomeRecord {
            symbol: symbol.clone(),
            income_type: okx_income_type_for_category(&category).into(),
            income,
            asset: asset.clone(),
            timestamp,
            tran_id: format!("{bill_id}:{suffix}"),
            trade_id: trade_id.clone(),
            category: Some(category),
        })
    };
    if pnl != 0.0 {
        push(pnl, category.clone(), "pnl");
    }
    if fee != 0.0 {
        push(
            fee,
            if fee < 0.0 {
                IncomeCategory::Commission
            } else {
                IncomeCategory::Rebate
            },
            "fee",
        );
    }
    let residual = balance_change - fee - pnl;
    let epsilon = 1e-12_f64.max(1e-9 * balance_change.abs());
    if residual.abs() > epsilon {
        push(residual, category, "adj");
    }
    if matches!(category_hit, safe_cex_num::TableHit::Inherited(_))
        && (pnl != 0.0 || residual.abs() > epsilon)
    {
        return Err(category_refusal().into());
    }
    Ok(records)
}

pub(crate) use safe_cex_num::{JsCoercionError, js_string, parse_float};
pub(crate) fn nullish_string(value: &Value, fallback: &str) -> Result<String, JsCoercionError> {
    if value.is_null() {
        Ok(fallback.into())
    } else {
        js_string(Some(value))
    }
}
pub(crate) fn truthy(value: &Value) -> bool {
    safe_cex_num::js_truthy(Some(value))
}
pub(crate) fn truthy_string(value: &Value) -> Result<String, JsCoercionError> {
    if truthy(value) {
        js_string(Some(value))
    } else {
        Ok(String::new())
    }
}
