use safe_cex_num::{JsCoercionError, js_number};
use safe_cex_transport::{TransportFuture, rate_limit::RateLimitClock};
use serde_json::Value;
use std::borrow::Cow;
use std::{
    collections::VecDeque,
    future::poll_fn,
    sync::{Arc, Mutex},
    task::{Poll, Waker},
};

pub fn is_endpoint(url: Option<&str>, endpoint: &str) -> bool {
    url.is_some_and(|url| url == endpoint || url.ends_with(endpoint))
}
pub fn parse_request_data(data: &Value) -> Cow<'_, Value> {
    if let Value::String(raw) = data {
        if let Ok(parsed) =
            serde_json::from_str::<safe_cex_types::retained_json::RetainedJsonInput>(raw)
                .and_then(safe_cex_types::retained_json::RetainedJsonInput::into_value)
        {
            return Cow::Owned(parsed);
        }
    }
    Cow::Borrowed(data)
}
pub fn get_array_length(value: Option<&Value>) -> f64 {
    value
        .and_then(Value::as_array)
        .map_or(0.0, |values| values.len() as f64)
}
pub fn get_exchange_batch_length(action: Option<&Value>) -> f64 {
    let kind = action
        .and_then(|action| action.get("type"))
        .and_then(Value::as_str);
    let key = match kind {
        Some("order") => "orders",
        Some("cancel" | "cancelByCloid") => "cancels",
        Some("batchModify") => "modifies",
        _ => return 0.0,
    };
    get_array_length(action.and_then(|action| action.get(key)))
}
pub fn get_info_base_weight(kind: Option<&Value>) -> f64 {
    match kind.and_then(Value::as_str) {
        Some("userRole") => 60.0,
        Some(
            "allMids"
            | "l2Book"
            | "orderStatus"
            | "clearinghouseState"
            | "spotClearinghouseState"
            | "exchangeStatus",
        ) => 2.0,
        _ => 20.0,
    }
}
pub fn get_info_extra_weight(
    kind: Option<&Value>,
    response_limit: Option<&Value>,
) -> Result<f64, JsCoercionError> {
    let limit = js_number(response_limit)?;
    if !limit.is_finite() || limit <= 0.0 {
        return Ok(0.0);
    }
    Ok(match kind.and_then(Value::as_str) {
        Some("candleSnapshot") => (limit / 60.0).floor(),
        Some(
            "historicalOrders"
            | "userFills"
            | "userFillsByTime"
            | "fundingHistory"
            | "userFunding"
            | "userNonFundingLedgerUpdates"
            | "recentTrades"
            | "twapHistory"
            | "userTwapSliceFills"
            | "userTwapSliceFillsByTime"
            | "delegatorHistory"
            | "delegatorRewards"
            | "validatorStats",
        ) => (limit / 20.0).floor(),
        _ => 0.0,
    })
}
fn raw_member<'a>(
    object: &'a crate::json_values::JsonObject<Box<serde_json::value::RawValue>>,
    key: &str,
) -> Option<&'a str> {
    object
        .get(&key.encode_utf16().collect::<Vec<_>>())
        .map(|raw| raw.get())
}
fn raw_kind(raw: Option<&str>) -> Option<String> {
    raw.filter(|raw| raw.starts_with('"'))
        .and_then(|raw| serde_json::from_str::<crate::spot_json::JsString>(raw).ok())
        .and_then(|value| String::from_utf16(&value.0).ok())
}
fn raw_number(value: Option<&crate::json_values::JsonValue>) -> Result<f64, JsCoercionError> {
    use crate::json_values::JsonValue as J;
    fn text(value: &J) -> Result<Vec<u16>, JsCoercionError> {
        Ok(match value {
            J::Null => vec![],
            J::Bool(value) => value.to_string().encode_utf16().collect(),
            J::Number(value) => safe_cex_num::js_number_string(*value)
                .encode_utf16()
                .collect(),
            J::String(value) => value.0.clone(),
            J::Object(value) => {
                if value
                    .get(&"toString".encode_utf16().collect::<Vec<_>>())
                    .is_some()
                {
                    return Err(JsCoercionError {
                        name: "TypeError",
                        message: "Cannot convert object to primitive value",
                    });
                }
                "[object Object]".encode_utf16().collect()
            }
            J::Array(values) => {
                let mut result = Vec::new();
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        result.push(b',' as u16);
                    }
                    result.extend(text(value)?);
                }
                result
            }
        })
    }
    Ok(match value {
        None => f64::NAN,
        Some(J::Null) => 0.0,
        Some(J::Bool(value)) => u8::from(*value) as f64,
        Some(J::Number(value)) => *value,
        Some(value) => String::from_utf16(&text(value)?)
            .map_or(f64::NAN, |text| safe_cex_num::number_from_string(&text)),
    })
}
fn raw_request_weight(url: &str, raw: &str) -> Option<Result<f64, JsCoercionError>> {
    use crate::json_values::{JsonObject, JsonValue};
    use serde_json::value::RawValue;
    let data: JsonObject<Box<RawValue>> = serde_json::from_str(raw).ok()?;
    if is_endpoint(Some(url), "/exchange") {
        let length = raw_member(&data, "action")
            .and_then(|raw| serde_json::from_str::<JsonObject<Box<RawValue>>>(raw).ok())
            .and_then(|action| {
                let key = match raw_kind(raw_member(&action, "type")).as_deref() {
                    Some("order") => "orders",
                    Some("cancel" | "cancelByCloid") => "cancels",
                    Some("batchModify") => "modifies",
                    _ => return None,
                };
                serde_json::from_str::<Vec<Box<RawValue>>>(raw_member(&action, key)?)
                    .ok()
                    .map(|values| values.len())
            })
            .unwrap_or(0);
        return Some(Ok(1.0 + (length as f64 / 40.0).floor()));
    }
    if is_endpoint(Some(url), "/info") {
        let kind = raw_kind(raw_member(&data, "type")).map(Value::String);
        let limit = raw_member(&data, "limit")
            .map(serde_json::from_str::<JsonValue>)
            .transpose();
        return Some(match limit {
            Ok(limit) => raw_number(limit.as_ref()).and_then(|limit| {
                let extra = if !limit.is_finite() || limit <= 0.0 {
                    0.0
                } else {
                    get_info_extra_weight(kind.as_ref(), Some(&Value::from(limit)))?
                };
                Ok(get_info_base_weight(kind.as_ref()) + extra)
            }),
            Err(_) => Err(JsCoercionError {
                name: "TypeError",
                message: "Cannot decode Hyperliquid response limit",
            }),
        });
    }
    Some(Ok(1.0))
}
pub fn get_hyperliquid_rest_request_weight(
    url: &str,
    data: &Value,
) -> Result<f64, JsCoercionError> {
    if let Value::String(raw) = data {
        if let Some(weight) = raw_request_weight(url, raw) {
            return weight;
        }
        return Ok(if is_endpoint(Some(url), "/info") {
            20.0
        } else {
            1.0
        });
    }
    let data = parse_request_data(data);
    if is_endpoint(Some(url), "/exchange") {
        return Ok(1.0 + (get_exchange_batch_length(data.get("action")) / 40.0).floor());
    }
    if is_endpoint(Some(url), "/info") {
        return Ok(get_info_base_weight(data.get("type"))
            + get_info_extra_weight(data.get("type"), data.get("limit"))?);
    }
    Ok(1.0)
}
#[derive(Default)]
struct State {
    next: f64,
    ticket: u64,
    queue: VecDeque<(u64, Option<Waker>)>,
}
pub struct RateLimiter {
    clock: Arc<dyn RateLimitClock>,
    state: Mutex<State>,
    ms_per_token: f64,
}
// Drop leaves this shared limiter recoverable: remove only this ticket, retain reserved budget.
struct Guard<'a> {
    state: &'a Mutex<State>,
    ticket: u64,
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        let wake = {
            let mut s = self.state.lock().unwrap();
            s.queue.retain(|(id, _)| *id != self.ticket);
            s.queue.front_mut().and_then(|(_, w)| w.take())
        };
        if let Some(w) = wake {
            w.wake()
        }
    }
}
impl RateLimiter {
    pub(crate) fn clock(&self) -> &dyn RateLimitClock {
        self.clock.as_ref()
    }
    pub fn new(clock: Arc<dyn RateLimitClock>, capacity: f64, window_ms: f64) -> Self {
        Self {
            clock,
            state: Mutex::default(),
            ms_per_token: window_ms / capacity,
        }
    }
    /// Omitted source parameters use their defaults; explicit zero and NaN do not.
    pub fn with_defaults(
        clock: Arc<dyn RateLimitClock>,
        capacity: Option<f64>,
        window_ms: Option<f64>,
    ) -> Self {
        Self::new(
            clock,
            capacity.unwrap_or(1200.0),
            window_ms.unwrap_or(60000.0),
        )
    }
    /// None models an omitted/undefined source weight; f64 callers remain accepted.
    pub fn wait_for_token(&self, weight: impl Into<Option<f64>>) -> TransportFuture<'_, ()> {
        let weight = weight.into().unwrap_or(1.0);
        Box::pin(async move {
            let ticket = {
                let mut s = self.state.lock().unwrap();
                let id = s.ticket;
                s.ticket += 1;
                s.queue.push_back((id, None));
                id
            };
            let guard = Guard {
                state: &self.state,
                ticket,
            };
            let wait = poll_fn(|cx| {
                let mut s = self.state.lock().unwrap();
                if s.queue.front().is_none_or(|(id, _)| *id != ticket) {
                    if let Some((_, w)) = s.queue.iter_mut().find(|(id, _)| *id == ticket) {
                        *w = Some(cx.waker().clone())
                    }
                    return Poll::Pending;
                }
                let now = self.clock.now_ms() as f64;
                let start = now.max(s.next);
                s.next = start
                    + if weight.is_finite() && weight > 0.0 {
                        weight
                    } else {
                        1.0
                    } * self.ms_per_token;
                Poll::Ready(start - now)
            })
            .await;
            if wait > 0.0 {
                self.clock.sleep_ms(wait as u64).await?
            }
            drop(guard);
            Ok(())
        })
    }
}

#[cfg(test)]
mod defaults_regression {
    use super::*;
    struct Clock;
    impl RateLimitClock for Clock {
        fn now_ms(&self) -> u64 {
            0
        }
        fn sleep_ms(&self, _: u64) -> TransportFuture<'_, ()> {
            Box::pin(async { Ok(()) })
        }
    }
    #[test]
    fn source_constructor_defaults_preserve_omission_and_explicit_values() {
        let cases = [
            (None, None, 50.0),
            (Some(600.0), None, 100.0),
            (None, Some(1200.0), 1.0),
            (Some(2.0), Some(10.0), 5.0),
            (Some(0.0), None, f64::INFINITY),
            (Some(f64::NAN), None, f64::NAN),
        ];
        let mut completed = 0;
        for (capacity, window, expected) in cases {
            let limiter = RateLimiter::with_defaults(Arc::new(Clock), capacity, window);
            if expected.is_nan() {
                assert!(
                    limiter.ms_per_token.is_nan(),
                    "DEFAULTS native explicit NaN"
                );
            } else {
                assert_eq!(
                    limiter.ms_per_token.to_bits(),
                    expected.to_bits(),
                    "DEFAULTS native msPerToken"
                );
            }
            let state = limiter.state.lock().unwrap();
            assert_eq!(
                state.next.to_bits(),
                0.0f64.to_bits(),
                "DEFAULTS native initial nextAvailableAt"
            );
            assert_eq!(state.queue.len(), 0, "DEFAULTS native empty queue");
            completed += 1;
        }
        assert_eq!(completed, 6, "DEFAULTS independent constructor floor");
    }
}
