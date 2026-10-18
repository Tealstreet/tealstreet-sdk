use safe_cex_num::{JsCoercionError, js_string, parse_float};
use serde_json::Value;

// ts: toNumber (binance.closed-pnl.ts)
pub fn to_number(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    let text = js_string(
        value
            .filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )?;
    let parsed = parse_float(&text);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

// ts: deriveBinanceClosedPositions
pub fn derive_binance_closed_positions(
    rows: &[serde_json::Map<String, Value>],
    account_name: &str,
) -> Result<Vec<safe_cex_types::ClosedPnlEntry>, JsCoercionError> {
    use safe_cex_num::js_number;
    use safe_cex_types::{
        ClosedPnlEntry, ClosedPnlEntryContracts, ClosedPnlEntryEntryPrice, ClosedPnlEntryExitPrice,
        ClosedPnlEntryFees, ClosedPnlEntryLeverage, ClosedPnlEntryNotional,
        ClosedPnlEntryUnrealizedPnl, DataProvenance,
    };
    let mut entries = Vec::new();
    for row in rows {
        let realized_pnl = to_number(row.get("realizedPnl"))?;
        if realized_pnl == 0.0 {
            continue;
        }
        let time = js_number(
            row.get("time")
                .filter(|v| !v.is_null())
                .or(Some(&Value::Null)),
        )?;
        let empty = Value::String(String::new());
        let symbol = js_string(row.get("symbol").filter(|v| !v.is_null()).or(Some(&empty)))?;
        if time == 0.0 || time.is_nan() || symbol.is_empty() {
            continue;
        }
        let price = to_number(row.get("price"))?;
        let qty = to_number(row.get("qty"))?;
        let notional = to_number(row.get("quoteQty"))?;
        let side = js_string(row.get("side").filter(|v| !v.is_null()).or(Some(&empty)))?;
        let fees = to_number(row.get("commission"))?;
        entries.push(ClosedPnlEntry {
            provenance: DataProvenance::Derived,
            symbol,
            notional: ClosedPnlEntryNotional::Variant1(if notional != 0.0 {
                notional
            } else {
                price * qty
            }),
            contracts: ClosedPnlEntryContracts::Variant1(qty),
            entry_price: ClosedPnlEntryEntryPrice::Variant1(0.0),
            exit_price: ClosedPnlEntryExitPrice::Variant1(price),
            unrealized_pnl: ClosedPnlEntryUnrealizedPnl::Variant1(realized_pnl),
            side: if side.to_uppercase() == "SELL" {
                "long"
            } else {
                "short"
            }
            .into(),
            fees: ClosedPnlEntryFees::Variant1(fees),
            time,
            closed_time: time,
            account: account_name.into(),
            leverage: ClosedPnlEntryLeverage::Variant0(0.0),
        });
    }
    Ok(entries)
}
