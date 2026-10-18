//! Remaining module-level utilities; adapters are declared in MANIFEST-SPEC.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsCoercionError, JsRootCollator, js_string, js_truthy};
use safe_cex_types::OrderStopTriggerType;
use serde_json::Value;
use std::cmp::Ordering;

pub fn parse_trigger_signal(
    value: Option<&Value>,
) -> Result<Option<OrderStopTriggerType>, JsCoercionError> {
    let value = if js_truthy(value) {
        js_string(value)?
    } else {
        String::new()
    };
    Ok(match value.to_lowercase().as_str() {
        "mark" => Some(OrderStopTriggerType::MarkPrice),
        "spot" => Some(OrderStopTriggerType::IndexPrice),
        "last" => Some(OrderStopTriggerType::LastPrice),
        _ => None,
    })
}

pub fn sort(collator: &JsRootCollator, left: &str, right: &str) -> Ordering {
    collator.compare(left, right)
}

pub fn create_kraken_api_error(
    message: &str,
    response: Option<&Value>,
    cause: Option<&Value>,
) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    if let Some(response) = response {
        error = error.with_retained_response(response.clone().into());
    }
    error.metadata.original_error = cause.cloned();
    error
}

/// Create once per SDK runtime/module and share among all Kraken Spot venues.
/// Do not recreate it per order or per exchange instance.
/// The host fills every requested byte with fresh CSPRNG entropy, or returns an error.
/// This venue uses the source's default 32-character size only.
pub fn create_kraken_spot_client_order_id(
    mut fill: impl FnMut(&mut [u8]) -> Result<(), JsCoercionError>,
) -> impl FnMut() -> Result<String, JsCoercionError> {
    let mut pool = Vec::new();
    let mut pool_offset = 0;
    let mut pool_next = 0;
    move || {
        let size = 32;
        if pool_offset + size > pool.len() {
            let target = pool_next.max(size);
            pool_next = (target * 16).min(32768);
            let mut buffer = vec![0; target];
            fill(&mut buffer)?;
            for byte in &mut buffer {
                *byte = b"0123456789abcdef"[usize::from(*byte & 15)];
            }
            pool = buffer;
            pool_offset = 0;
        }
        pool_offset += size;
        Ok(
            String::from_utf8(pool[pool_offset - size..pool_offset].to_vec())
                .expect("ASCII alphabet"),
        )
    }
}
