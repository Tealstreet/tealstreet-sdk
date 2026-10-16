//! Classic v2 bills keep gross PnL and signed commissions in separate records.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsCoercionError, TableHit, js_string, js_table_lookup, js_truthy, parse_int};
use safe_cex_types::{IncomeCategory, IncomeRecord};
use serde_json::Value;

// ts: num
pub fn num(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    // Both source helpers are identical; shared coercion stays in one implementation.
    crate::fills::num(value)
}

// ts: mapBitgetBill
pub fn map_bitget_bill(row: Option<&Value>) -> Result<Vec<IncomeRecord>, ExchangeError> {
    let row = row.filter(|v| !v.is_null()).ok_or_else(|| ExchangeError {
        name: "TypeError".into(),
        message: if row.is_none() {
            "Cannot read properties of undefined (reading 'cTime')"
        } else {
            "Cannot read properties of null (reading 'cTime')"
        }
        .into(),
        metadata: Box::default(),
    })?;
    let coercion = |e: JsCoercionError| ExchangeError {
        name: e.name.into(),
        message: e.message.into(),
        metadata: Box::default(),
    };
    let zero = Value::String("0".into());
    let empty = Value::String(String::new());
    let timestamp = parse_int(
        &js_string(Some(
            row.get("cTime").filter(|v| !v.is_null()).unwrap_or(&zero),
        ))
        .map_err(coercion)?,
        Some(10),
    );
    if !timestamp.is_finite() || timestamp <= 0.0 {
        return Ok(Vec::new());
    }
    let kind = js_string(Some(
        row.get("businessType")
            .filter(|v| !v.is_null())
            .unwrap_or(&empty),
    ))
    .map_err(coercion)?;
    if kind.is_empty() {
        return Ok(Vec::new());
    }
    let bill_id = js_string(Some(
        row.get("billId").filter(|v| !v.is_null()).unwrap_or(&empty),
    ))
    .map_err(coercion)?;
    let usdt = Value::String("USDT".into());
    let coin = row.get("coin");
    let asset = js_string(if js_truthy(coin) { coin } else { Some(&usdt) }).map_err(coercion)?;
    let symbol = js_string(Some(
        row.get("symbol").filter(|v| !v.is_null()).unwrap_or(&empty),
    ))
    .map_err(coercion)?;
    let amount = num(row.get("amount")).map_err(coercion)?;
    let fee = num(row.get("fee")).map_err(coercion)?;
    let record = |income_type: String, suffix: &str, income: f64, category| IncomeRecord {
        symbol: symbol.clone(),
        income_type,
        asset: asset.clone(),
        timestamp,
        tran_id: format!("{bill_id}:{suffix}"),
        income,
        category,
        trade_id: None,
    };
    let mut records = Vec::new();
    if matches!(
        kind.as_str(),
        "buy" | "sell" | "open_long" | "open_short" | "close_long" | "close_short"
    ) {
        if amount != 0.0 {
            records.push(record(
                kind.clone(),
                "pnl",
                amount,
                Some(IncomeCategory::RealizedPnl),
            ));
        }
        if fee != 0.0 {
            records.push(record(
                format!("{kind}_fee"),
                "fee",
                fee,
                Some(IncomeCategory::Commission),
            ));
        }
        return Ok(records);
    }
    // ts-bug: mapBitgetBill indexes a prototype-bearing CATEGORY_BY_TYPE.
    // Inherited hits are non-nullish: preserve their member-specific interpolation.
    // CHOSEN closed-typed-output-boundary: optional category refuses the member
    // as None while retaining the record. Source callables disappear only during
    // JSON serialization; the separately witnessed __proto__ object remains {}.
    // Both output prices are declared in MANIFEST-SPEC.md#closed-typed-output-boundary.
    let categories = [
        ("contract_settle_fee", (IncomeCategory::Funding, "funding")),
        (
            "trans_from_exchange",
            (IncomeCategory::Transfer, "transfer"),
        ),
        ("trans_to_exchange", (IncomeCategory::Transfer, "transfer")),
        ("transfer_in", (IncomeCategory::Transfer, "transfer")),
        ("transfer_out", (IncomeCategory::Transfer, "transfer")),
    ];
    let (category, suffix) = match js_table_lookup(&categories, &kind) {
        TableHit::Mapped((category, suffix)) => (Some(category.clone()), *suffix),
        TableHit::Inherited(member) => (None, member.js_string()),
        TableHit::Missing => (Some(IncomeCategory::Other), "other"),
    };
    if amount != 0.0 || fee != 0.0 {
        records.push(record(kind, suffix, amount + fee, category));
    }
    Ok(records)
}

// ts: mapBitgetBills
pub fn map_bitget_bills(rows: &[Value]) -> Result<Vec<IncomeRecord>, ExchangeError> {
    let mut records = Vec::new();
    for row in rows {
        records.extend(map_bitget_bill(Some(row))?);
    }
    Ok(records)
}
