//! The legacy unrealized-PnL field carries venue-reported realized PnL, net of fees and funding.
use crate::income::{JsCoercionError, nullish_string};
use safe_cex_types::*;
use serde_json::Value;

pub fn to_number(value: &Value) -> Result<f64, JsCoercionError> {
    crate::income::to_number(value)
}

pub fn map_okx_closed_position<E: From<JsCoercionError>>(
    row: &serde_json::Map<String, Value>,
    market: Option<&Market>,
    contracts_to_base: impl Fn(f64, &Market, f64) -> Result<f64, E>,
    account_name: &str,
) -> Result<Option<ClosedPnlEntry>, E> {
    let open_time = safe_cex_num::parse_int(
        &nullish_string(row.get("cTime").unwrap_or(&Value::Null), "0")?,
        Some(10),
    );
    let instrument = nullish_string(row.get("instId").unwrap_or(&Value::Null), "")?;
    if open_time == 0.0 || open_time.is_nan() || instrument.is_empty() {
        return Ok(None);
    }
    let close_price = to_number(row.get("closeAvgPx").unwrap_or(&Value::Null))?;
    let native_size = to_number(row.get("closeTotalPos").unwrap_or(&Value::Null))?;
    let contracts = match market.filter(|_| close_price > 0.0) {
        Some(market) => contracts_to_base(native_size, market, close_price)?,
        None => native_size,
    };
    let closed_time = safe_cex_num::parse_int(
        &nullish_string(row.get("uTime").unwrap_or(&Value::Null), "0")?,
        Some(10),
    );
    Ok(Some(ClosedPnlEntry {
        provenance: DataProvenance::Venue,
        symbol: market
            .map(|m| m.symbol.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or(instrument),
        notional: ClosedPnlEntryNotional::Variant1(contracts * close_price),
        contracts: ClosedPnlEntryContracts::Variant1(contracts),
        entry_price: ClosedPnlEntryEntryPrice::Variant1(to_number(
            row.get("openAvgPx").unwrap_or(&Value::Null),
        )?),
        exit_price: ClosedPnlEntryExitPrice::Variant1(close_price),
        unrealized_pnl: ClosedPnlEntryUnrealizedPnl::Variant1(to_number(
            row.get("realizedPnl").unwrap_or(&Value::Null),
        )?),
        side: if nullish_string(row.get("direction").unwrap_or(&Value::Null), "")?.to_lowercase()
            == "short"
        {
            "short"
        } else {
            "long"
        }
        .into(),
        fees: ClosedPnlEntryFees::Variant1(
            to_number(row.get("fee").unwrap_or(&Value::Null))?.abs(),
        ),
        time: open_time,
        closed_time: if closed_time == 0.0 || closed_time.is_nan() {
            open_time
        } else {
            closed_time
        },
        account: account_name.into(),
        leverage: ClosedPnlEntryLeverage::Variant0(to_number(
            row.get("lever").unwrap_or(&Value::Null),
        )?),
    }))
}
