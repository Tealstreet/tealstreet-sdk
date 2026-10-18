//! Pure public and secondary-public ticker projections; socket ownership is separate.
use crate::numeric::{FiniteNumberInput, to_finite_number as finite_or};
use safe_cex_num::JsCoercionError;
use serde_json::Value;

// ts: BitgetPublicWebsocket.toFiniteNumber, BitgetSecondaryPublicWebsocket.toFiniteNumber
pub fn to_finite_number(value: FiniteNumberInput<'_>) -> Option<f64> {
    let value = finite_or(value, f64::NAN);
    value.is_finite().then_some(value)
}

// ts: BitgetPublicWebsocket.buildTickerUpdate, BitgetSecondaryPublicWebsocket.buildTickerUpdate
// Native numbers retain absence, signed zero and derived infinities until ModelPatch application.
pub fn build_ticker_update(
    data: Option<&Value>,
) -> Result<Vec<(&'static str, f64)>, JsCoercionError> {
    let data = data
        .filter(|value| !value.is_null())
        .ok_or(JsCoercionError {
            name: "TypeError",
            message: if data.is_none() {
                "Cannot read properties of undefined (reading 'bidPr')"
            } else {
                "Cannot read properties of null (reading 'bidPr')"
            },
        })?;
    let mut update = Vec::new();
    for (field, keys) in [
        ("bid", &["bidPr", "bidPrice", "bid"][..]),
        ("ask", &["askPr", "askPrice", "ask"][..]),
        ("last", &["lastPr", "lastPrice", "last"][..]),
        ("mark", &["markPrice", "markPr", "mark"][..]),
        ("index", &["indexPrice", "indexPr", "index"][..]),
        ("percentage", &["change24h", "changeUtc24h", "change"][..]),
        ("fundingRate", &["fundingRate"][..]),
        ("fundingTime", &["nextFundingTime", "fundingTime"][..]),
        ("volume", &["baseVolume", "volume"][..]),
        ("quoteVolume", &["quoteVolume"][..]),
    ] {
        let raw = keys
            .iter()
            .find_map(|key| data.get(*key).filter(|v| !v.is_null()));
        let input = match raw {
            Some(Value::String(value)) => FiniteNumberInput::String(value),
            Some(Value::Number(value)) => {
                FiniteNumberInput::Number(value.as_f64().unwrap_or(f64::NAN))
            }
            _ => FiniteNumberInput::Other,
        };
        if let Some(value) = to_finite_number(input) {
            // The source guards its input, not this product: finite * 100 can overflow.
            update.push((
                field,
                if field == "percentage" {
                    value * 100.0
                } else {
                    value
                },
            ));
        }
    }
    Ok(update)
}

// ts: BitgetPublicWebsocket.handleTickerTopic, BitgetSecondaryPublicWebsocket.handleTickerTopic
#[derive(Debug)]
pub enum TickerHandlerError {
    Coercion(JsCoercionError),
    Store(safe_cex_store::StoreMutationError),
}
impl From<JsCoercionError> for TickerHandlerError {
    fn from(error: JsCoercionError) -> Self {
        Self::Coercion(error)
    }
}
impl From<safe_cex_store::StoreMutationError> for TickerHandlerError {
    fn from(error: safe_cex_store::StoreMutationError) -> Self {
        Self::Store(error)
    }
}

// ts: BitgetPublicWebsocket.handleTickerTopic, BitgetSecondaryPublicWebsocket.handleTickerTopic
pub fn handle_ticker_topic(
    store: &dyn safe_cex_store::Store,
    json: Option<&Value>,
) -> Result<(), TickerHandlerError> {
    use safe_cex_num::{js_string, js_truthy};
    let Some(rows) = json
        .and_then(|value| value.get("data"))
        .and_then(Value::as_array)
    else {
        return Ok(());
    };
    for data in rows {
        let symbol = data
            .get("instId")
            .filter(|value| js_truthy(Some(value)))
            .or_else(|| data.get("symbol").filter(|value| js_truthy(Some(value))));
        let symbol = match symbol {
            Some(value) => js_string(Some(value))?,
            None => String::new(),
        };
        if symbol.is_empty() {
            continue;
        }
        let ticker = store.lookup_ticker_by_symbol(&symbol)?;
        if !ticker.is_truthy() {
            continue;
        }
        let fields = build_ticker_update(Some(data))?;
        if fields.is_empty() {
            continue;
        }
        let Some(id) = ticker.update_id()? else {
            continue;
        };
        let mut patch = safe_cex_store::model_patch::ModelPatch::default();
        for (field, value) in fields {
            patch.set_number(field, value);
        }
        // Pin the current ABI at the callee, even if error conversions change later.
        let result: Result<(), safe_cex_store::StoreMutationError> =
            store.update_ticker(&id, &patch);
        result?;
    }
    Ok(())
}

impl From<safe_cex_types::StoreReadError> for TickerHandlerError {
    fn from(error: safe_cex_types::StoreReadError) -> Self {
        Self::Store(error.into())
    }
}
