//! BlofinExchange.formatTickers: projection, catch/log, then stored fallback.
use safe_cex_num::{TableHit, js_table_lookup, numeric_ingress::JsNumberOutput, round_usd};
use safe_cex_store::Store;
use safe_cex_types::{
    Ticker, js_coercion::JsCoercionError, retained_json::RetainedJsonInput as Input,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TickerInput {
    value: Input,
}
impl TickerInput {
    pub(crate) fn from_retained(value: Input) -> Self {
        Self { value }
    }
    /// Canonical native data, including UTF-16, own undefined and numeric events.
    pub fn retained(&self) -> &Input {
        &self.value
    }
}
impl JsNumberOutput for TickerInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {} passthrough { value });
    }
}
fn number(value: Option<&Input>) -> Result<f64, JsCoercionError> {
    safe_cex_num::js_retained_parse_float(value)
}
fn project(data: &TickerInput, store: &dyn Store) -> Result<Vec<Ticker>, JsCoercionError> {
    let rows = data.value.as_array().ok_or(JsCoercionError {
        name: "TypeError",
        message: if data.value.is_null() {
            "Cannot read properties of null (reading 'reduce')"
        } else {
            "data.reduce is not a function"
        },
    })?;
    let mut tickers = Vec::new();
    for row in rows {
        if row.is_null() {
            return Err(JsCoercionError {
                name: "TypeError",
                message: "Cannot read properties of null (reading 'instId')",
            });
        }
        let key = safe_cex_num::js_retained_string(row.get("instId"))?;
        let markets = store.markets_by_id();
        let Some(market) = markets.get(&key) else {
            if matches!(
                js_table_lookup::<&str, ()>(&[], &key),
                TableHit::Inherited(_)
            ) {
                return Err(JsCoercionError {
                    name: "RepresentationError",
                    message: "Inherited market cannot produce a typed ticker",
                });
            }
            continue;
        };
        let open = number(row.get("open24h"))?;
        let last = number(row.get("last"))?;
        let percentage = round_usd(((last - open) / open) * 100.0);
        tickers.push(Ticker {
            id: market.id.clone(),
            symbol: market.symbol.clone(),
            bid: number(row.get("bidPrice"))?,
            ask: number(row.get("askPrice"))?,
            last,
            mark: last,
            index: Some(last),
            mid_price: None,
            timestamp: None,
            percentage,
            funding_rate: 0.0,
            funding_time: 0.0,
            volume: number(row.get("volCurrency24h"))?,
            quote_volume: number(row.get("vol24h"))?,
            open_interest: 0.0,
            info: Some(row.clone()),
        });
    }
    Ok(tickers)
}
/// The emitter's error is returned unchanged, including any host-owned identity.
pub fn format_tickers<E>(
    data: &TickerInput,
    store: &dyn Store,
    mut emit_and_log_error: impl FnMut(&str) -> Result<(), E>,
) -> Result<Vec<Ticker>, E> {
    match project(data, store) {
        Ok(tickers) => Ok(tickers),
        Err(error) => {
            emit_and_log_error(error.message)?;
            Ok(store.tickers().to_vec())
        }
    }
}
