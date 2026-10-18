//! Function-for-function port of blofin.income.ts on the retained JSON domain.
use safe_cex_num::{numeric_ingress::JsNumberOutput, parse_float, parse_int};
use safe_cex_types::{
    ClosedPnlEntry, ClosedPnlEntryContracts, ClosedPnlEntryEntryPrice, ClosedPnlEntryExitPrice,
    ClosedPnlEntryFees, ClosedPnlEntryLeverage, ClosedPnlEntryNotional,
    ClosedPnlEntryUnrealizedPnl, DataProvenance, IncomeCategory, IncomeRecord,
    js_coercion::{JsCoercionError, js_string},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Capture the complete open input before any JSON numeric projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IncomeInput {
    #[serde(deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough")]
    value: Value,
}
impl JsNumberOutput for IncomeInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {} passthrough { value });
    }
}
impl IncomeInput {
    pub(crate) fn from_retained(value: Value) -> Self {
        Self { value }
    }
    fn field(&self, key: &str) -> Result<Option<Self>, JsCoercionError> {
        if self.value.is_null() {
            return Err(JsCoercionError {
                name: "TypeError",
                message: match key {
                    "ts" => "Cannot read properties of null (reading 'ts')",
                    "createTime" => "Cannot read properties of null (reading 'createTime')",
                    _ => unreachable!("mapper reads its timestamp first"),
                },
            });
        }
        Ok(self.value.get(key).cloned().map(|value| Self { value }))
    }
}
fn text(value: Option<&IncomeInput>, fallback: &str) -> Result<String, JsCoercionError> {
    match value
        .map(|value| &value.value)
        .filter(|value| !value.is_null())
    {
        Some(value) => js_string(Some(value)),
        None => Ok(fallback.into()),
    }
}

/// parseFloat(String(value ?? '')), with nonfinite results replaced by +0.
pub fn to_number(value: Option<&IncomeInput>) -> Result<f64, JsCoercionError> {
    let parsed = parse_float(&text(value, "")?);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

pub fn map_blofin_fill_to_income_records(
    row: &IncomeInput,
    mut resolve_symbol: impl FnMut(&str) -> Result<Option<String>, safe_cex_types::StoreReadError>,
) -> Result<Vec<IncomeRecord>, crate::reads::MappingError> {
    let timestamp = parse_int(&text(row.field("ts")?.as_ref(), "0")?, Some(10));
    let inst_id = text(row.field("instId")?.as_ref(), "")?;
    if timestamp == 0.0 || timestamp.is_nan() || inst_id.is_empty() {
        return Ok(vec![]);
    }
    let symbol = resolve_symbol(&inst_id)?
        .filter(|value| !value.is_empty())
        .unwrap_or(inst_id.clone());
    let id = text(
        row.field("tradeId")?.as_ref(),
        &format!("{}:{inst_id}", safe_cex_num::js_number_string(timestamp)),
    )?;
    let fill_pnl = to_number(row.field("fillPnl")?.as_ref())?;
    let fee = to_number(row.field("fee")?.as_ref())?;
    let mut records = Vec::new();
    if fill_pnl != 0.0 {
        records.push(IncomeRecord {
            symbol: symbol.clone(),
            income_type: "REALIZED_PNL".into(),
            income: fill_pnl,
            asset: "USDT".into(),
            timestamp,
            tran_id: format!("{id}:pnl"),
            trade_id: Some(id.clone()),
            category: Some(IncomeCategory::RealizedPnl),
        });
    }
    if fee != 0.0 {
        records.push(IncomeRecord {
            symbol,
            income_type: if fee > 0.0 { "COMMISSION" } else { "REBATE" }.into(),
            income: -fee,
            asset: "USDT".into(),
            timestamp,
            tran_id: format!("{id}:fee"),
            trade_id: Some(id),
            category: Some(if fee > 0.0 {
                IncomeCategory::Commission
            } else {
                IncomeCategory::Rebate
            }),
        });
    }
    Ok(records)
}

pub fn map_blofin_closed_position(
    row: &IncomeInput,
    mut resolve_symbol: impl FnMut(&str) -> Result<Option<String>, safe_cex_types::StoreReadError>,
    account_name: &str,
) -> Result<Option<ClosedPnlEntry>, crate::reads::MappingError> {
    let open_time = parse_int(&text(row.field("createTime")?.as_ref(), "0")?, Some(10));
    let inst_id = text(row.field("instId")?.as_ref(), "")?;
    if open_time == 0.0 || open_time.is_nan() || inst_id.is_empty() {
        return Ok(None);
    }
    let close_price = to_number(row.field("closeAveragePrice")?.as_ref())?;
    let contracts = to_number(row.field("closePositions")?.as_ref())?;
    // Preserve source object-initializer evaluation order, including callback effects.
    let symbol = resolve_symbol(&inst_id)?
        .filter(|value| !value.is_empty())
        .unwrap_or(inst_id);
    let entry_price = to_number(row.field("openAveragePrice")?.as_ref())?;
    let unrealized_pnl = to_number(row.field("realizedPnl")?.as_ref())?;
    let side = if text(row.field("positionSide")?.as_ref(), "")?.to_lowercase() == "short" {
        "short"
    } else {
        "long"
    };
    let fees = to_number(row.field("fee")?.as_ref())?.abs();
    let close_time = parse_int(&text(row.field("updateTime")?.as_ref(), "0")?, Some(10));
    let closed_time = if close_time == 0.0 || close_time.is_nan() {
        open_time
    } else {
        close_time
    };
    let leverage = to_number(row.field("leverage")?.as_ref())?;
    Ok(Some(ClosedPnlEntry {
        provenance: DataProvenance::Venue,
        symbol,
        notional: ClosedPnlEntryNotional::Variant1(contracts * close_price),
        contracts: ClosedPnlEntryContracts::Variant1(contracts),
        entry_price: ClosedPnlEntryEntryPrice::Variant1(entry_price),
        exit_price: ClosedPnlEntryExitPrice::Variant1(close_price),
        unrealized_pnl: ClosedPnlEntryUnrealizedPnl::Variant1(unrealized_pnl),
        side: side.into(),
        fees: ClosedPnlEntryFees::Variant1(fees),
        time: open_time,
        closed_time,
        account: account_name.into(),
        leverage: ClosedPnlEntryLeverage::Variant0(leverage),
    }))
}
