//! Non-owner Store fixtures and packed ticker encoding; native owner lives in exchange.
use crate::{
    PackedTickerDelta,
    model_patch::{PatchSource, StorePatchError},
};
use indexmap::IndexMap;
use safe_cex_types::StoreData;
use serde_json::{Value, json};
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PackedTickerDeltas {
    pub packed: Vec<PackedTickerDelta>,
}

pub const PACKED_TICKER_FIELDS: [&str; 11] = [
    "bid",
    "ask",
    "last",
    "mark",
    "index",
    "percentage",
    "openInterest",
    "fundingRate",
    "fundingTime",
    "volume",
    "quoteVolume",
];
pub static DEFAULT_STORE: LazyLock<StoreData> = LazyLock::new(|| {
    serde_json::from_value(json!({
    "latency":0.0,"balance":{"used":0.0,"free":0.0,"total":0.0,"upnl":0.0},
    "markets":[],"tickers":[],"positions":[],"displayedPositions":[],
    "displayedMargin":{"longExposure":0.0,"shortExposure":0.0,"totalNotional":0.0},
    "ordersRisk":{},"activeChasers":[],"activeTWAPs":[],"orders":[],"messageHandlerStats":{},
    "requestStats":{"ok":0.0,"rateLimit":0.0,"auth":0.0,"network":0.0,"other":0.0},
    "fetchedLeverage":{},"loaded":{"balance":false,"orders":false,"markets":false,"tickers":false,"positions":false},
    "options":{"isHedged":false},"updateIds":{"positions":0.0,"orders":0.0},
    "socketHealth":{"publicHealthy":false,"privateHealthy":false,"publicInitialized":false,"privateInitialized":false}
})).expect("defaultStore shape")
});

pub fn pack_ticker_deltas<P: PatchSource>(
    deltas: &[(f64, P)],
) -> Result<PackedTickerDeltas, StorePatchError> {
    let mut packed = Vec::new();
    for (index, changes) in deltas {
        let mut numbers = IndexMap::new();
        changes.visit(&mut |field, value| {
            let value = match value {
                crate::model_patch::PatchValueRef::Number(value) => Some(value),
                crate::model_patch::PatchValueRef::Native(value) => {
                    if let Some(value) = value.downcast_ref::<Option<f64>>() {
                        *value
                    } else {
                        value.downcast_ref::<f64>().copied()
                    }
                }
                crate::model_patch::PatchValueRef::Json(Value::Number(value)) => {
                    Some(safe_cex_num::number_from_string(&value.to_string()))
                }
                _ => None,
            };
            if let Some(value) = value {
                numbers.insert(field.to_owned(), value);
            }
            Ok(())
        })?;
        let mut mask = 0_u32;
        let mut values = Vec::new();
        for (bit, field) in PACKED_TICKER_FIELDS.iter().enumerate() {
            if let Some(value) = numbers.get(*field) {
                mask |= 1 << bit;
                values.push(*value);
            }
        }
        if mask != 0 {
            packed.push(PackedTickerDelta {
                index: *index,
                mask: f64::from(mask),
                values,
            });
        }
    }
    Ok(PackedTickerDeltas { packed })
}
