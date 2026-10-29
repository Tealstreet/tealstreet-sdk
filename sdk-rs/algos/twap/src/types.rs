//! Wire shapes of `twap:*` ops and `twap:stateUpdate`, unchanged from the TS worker.

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TwapStatus {
    Idle,
    Running,
    Paused,
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TwapOrigin {
    Web,
    Cli,
    Webhook,
    Automation,
}

/// `TwapTimeoutAction` and `TwapFailAction` share the same two values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SliceAction {
    Market,
    Next,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TwapOptions {
    pub account_name: String,
    /// The client's `Market` object, kept verbatim; TWAP reads only `symbol`.
    #[serde(default)]
    pub active_market: Value,
    #[serde(serialize_with = "js_number")]
    pub size: f64,
    /// Minutes.
    #[serde(serialize_with = "js_number")]
    pub duration: f64,
    pub orders_count: u32,
    pub side: OrderSide,
    #[serde(default, serialize_with = "js_number")]
    pub chaser_delay: f64,
    #[serde(default)]
    pub reduce_only: bool,
    #[serde(default, serialize_with = "js_number")]
    pub randomness_percentage: f64,
    #[serde(default)]
    pub use_limit_orders: bool,
    #[serde(default)]
    pub use_chaser_orders: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_option_number"
    )]
    pub timeout_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_action: Option<SliceAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fail_action: Option<SliceAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<TwapOrigin>,
    /// Fields this port does not read round-trip untouched.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl TwapOptions {
    pub fn symbol(&self) -> &str {
        self.active_market
            .get("symbol")
            .and_then(Value::as_str)
            .unwrap_or("")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TwapState {
    pub account_name: String,
    #[serde(default)]
    pub active_market: Value,
    #[serde(serialize_with = "js_number")]
    pub size: f64,
    #[serde(serialize_with = "js_numbers")]
    pub orders_sizes: Vec<f64>,
    pub side: OrderSide,
    pub status: TwapStatus,
    pub orders_count: u32,
    pub orders_executed: u32,
    #[serde(serialize_with = "js_number")]
    pub next_order_at: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark_for_cleanup: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<TwapOrigin>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_option_number"
    )]
    pub created_at: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_option_number"
    )]
    pub started_at: Option<f64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One `twap:state` restore entry, as the client persisted it.
#[derive(Clone, Debug, Deserialize)]
pub struct PersistedTwap {
    pub id: String,
    pub options: TwapOptions,
    pub state: TwapState,
}

/// JS numbers have no integer/float split: integral values serialize without `.0`.
fn js_number<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        serializer.serialize_i64(*value as i64)
    } else {
        serializer.serialize_f64(*value)
    }
}

fn js_option_number<S: Serializer>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => js_number(value, serializer),
        None => serializer.serialize_none(),
    }
}

fn js_numbers<S: Serializer>(values: &[f64], serializer: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    struct Number(f64);
    impl Serialize for Number {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            js_number(&self.0, serializer)
        }
    }
    let mut seq = serializer.serialize_seq(Some(values.len()))?;
    for value in values {
        seq.serialize_element(&Number(*value))?;
    }
    seq.end()
}
