use safe_cex_types::{Market, OrderSide};
use serde::{Deserialize, Serialize};

pub type ChaserOrigin = safe_cex_types::store_model::TaskOrigin;
pub type TwapOrigin = safe_cex_types::store_model::TaskOrigin;
pub type TwapTimeoutAction = safe_cex_types::store_model::SliceAction;
pub type TwapFailAction = safe_cex_types::store_model::SliceAction;
pub type TwapStatus = safe_cex_types::store_model::TwapStatus;
pub const CHASER_KEPT_ORDER_STOP_REASON: &str = "User cancelled, kept order in book";
pub const CHASER_UNCONFIRMED_ORDER_STOP_REASON: &str = "Could not confirm order placement";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChaserTerminateKind {
    Price,
    Percent,
    Usd,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ChaserTerminateKind")]
enum ChaserTerminateKindWire {
    #[serde(rename = "price")]
    Price,
    #[serde(rename = "percent")]
    Percent,
    #[serde(rename = "usd")]
    Usd,
}
impl Serialize for ChaserTerminateKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ChaserTerminateKindWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for ChaserTerminateKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        ChaserTerminateKindWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            value,
        ))
    }
}
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChaserTerminate {
    #[serde(rename = "type")]
    pub kind: ChaserTerminateKind,
    pub value: f64,
    pub anchor_price: f64,
}

#[derive(Deserialize)]
#[serde(remote = "ChaserTerminate")]
#[serde(rename_all = "camelCase")]
struct ChaserTerminateWire {
    #[serde(rename = "type")]
    kind: ChaserTerminateKind,
    value: f64,
    anchor_price: f64,
}
impl<'de> Deserialize<'de> for ChaserTerminate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                ChaserTerminateWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChaserOrderOptions {
    pub symbol: String,
    pub size: f64,
    pub side: OrderSide,
    pub max_attempts: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    pub delay: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<ChaserOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminate: Option<ChaserTerminate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(remote = "ChaserOrderOptions")]
#[serde(rename_all = "camelCase")]
struct ChaserOrderOptionsWire {
    symbol: String,
    size: f64,
    side: OrderSide,
    max_attempts: f64,
    #[serde(default, deserialize_with = "present")]
    reduce_only: Option<bool>,
    delay: f64,
    #[serde(default, deserialize_with = "present")]
    origin: Option<ChaserOrigin>,
    #[serde(default, deserialize_with = "present")]
    account_name: Option<String>,
    #[serde(default, deserialize_with = "present")]
    terminate: Option<ChaserTerminate>,
    #[serde(default, deserialize_with = "present")]
    client_id: Option<String>,
}
impl<'de> Deserialize<'de> for ChaserOrderOptions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                ChaserOrderOptionsWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChaserCompleteEvent {
    pub id: String,
    pub client_id: Option<String>,
    pub symbol: String,
    pub side: OrderSide,
    pub reduce_only: bool,
    pub size: f64,
    pub filled: f64,
    pub stop_reason: String,
}
impl Serialize for ChaserCompleteEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut record =
            serializer.serialize_struct(safe_cex_types::model_field::MODEL_RECORD, 8)?;
        record.serialize_field("id", &self.id)?;
        safe_cex_types::ModelField::from(self.client_id.as_ref())
            .into_property()
            .serialize_field(&mut record, "clientId")?;
        record.serialize_field("symbol", &self.symbol)?;
        record.serialize_field("side", &self.side)?;
        record.serialize_field("reduceOnly", &self.reduce_only)?;
        record.serialize_field("size", &self.size)?;
        record.serialize_field("filled", &self.filled)?;
        record.serialize_field("stopReason", &self.stop_reason)?;
        record.end()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChaserOrderState {
    pub attempts: f64,
    pub is_stopped: bool,
    pub stop_reason: String,
    pub current_orders_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(remote = "ChaserOrderState")]
#[serde(rename_all = "camelCase")]
struct ChaserOrderStateWire {
    attempts: f64,
    is_stopped: bool,
    stop_reason: String,
    current_orders_ids: Vec<String>,
}
impl<'de> Deserialize<'de> for ChaserOrderState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                ChaserOrderStateWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TwapOptions {
    pub account_name: String,
    pub active_market: Market,
    pub size: f64,
    pub duration: f64,
    pub orders_count: f64,
    pub side: OrderSide,
    pub chaser_delay: f64,
    pub reduce_only: bool,
    pub randomness_percentage: f64,
    pub use_limit_orders: bool,
    pub use_chaser_orders: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_action: Option<TwapTimeoutAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fail_action: Option<TwapFailAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<TwapOrigin>,
}

#[derive(Deserialize)]
#[serde(remote = "TwapOptions")]
#[serde(rename_all = "camelCase")]
struct TwapOptionsWire {
    account_name: String,
    active_market: Market,
    size: f64,
    duration: f64,
    orders_count: f64,
    side: OrderSide,
    chaser_delay: f64,
    reduce_only: bool,
    randomness_percentage: f64,
    use_limit_orders: bool,
    use_chaser_orders: bool,
    #[serde(default, deserialize_with = "present")]
    timeout_pct: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    timeout_action: Option<TwapTimeoutAction>,
    #[serde(default, deserialize_with = "present")]
    fail_action: Option<TwapFailAction>,
    #[serde(default, deserialize_with = "present")]
    origin: Option<TwapOrigin>,
}
impl<'de> Deserialize<'de> for TwapOptions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                TwapOptionsWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TwapState {
    pub account_name: String,
    pub active_market: Market,
    pub size: f64,
    pub orders_sizes: Vec<f64>,
    pub side: OrderSide,
    pub status: TwapStatus,
    pub orders_count: f64,
    pub orders_executed: f64,
    pub next_order_at: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark_for_cleanup: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<TwapOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<f64>,
}

#[derive(Deserialize)]
#[serde(remote = "TwapState")]
#[serde(rename_all = "camelCase")]
struct TwapStateWire {
    account_name: String,
    active_market: Market,
    size: f64,
    orders_sizes: Vec<f64>,
    side: OrderSide,
    status: TwapStatus,
    orders_count: f64,
    orders_executed: f64,
    next_order_at: f64,
    #[serde(default, deserialize_with = "present")]
    mark_for_cleanup: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    origin: Option<TwapOrigin>,
    #[serde(default, deserialize_with = "present")]
    created_at: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    started_at: Option<f64>,
}
impl<'de> Deserialize<'de> for TwapState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                TwapStateWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for ChaserTerminate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { kind, value, anchor_price });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ChaserTerminateKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Price, Percent, Usd });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ChaserCompleteEvent {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, client_id, symbol, side, reduce_only, size, filled, stop_reason });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ChaserOrderState {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { attempts, is_stopped, stop_reason, current_orders_ids });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for ChaserOrderOptions {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, size, side, max_attempts, reduce_only, delay, origin, account_name, terminate, client_id });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for TwapOptions {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account_name, active_market, size, duration, orders_count, side, chaser_delay, reduce_only, randomness_percentage, use_limit_orders, use_chaser_orders, timeout_pct, timeout_action, fail_action, origin });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for TwapState {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account_name, active_market, size, orders_sizes, side, status, orders_count, orders_executed, next_order_at, mark_for_cleanup, origin, created_at, started_at });
    }
}

#[cfg(test)]
mod inventory {
    use super::*;
    #[derive(Default)]
    struct Inventory(Option<&'static [&'static str]>);
    impl<'de> serde::Deserializer<'de> for &mut Inventory {
        type Error = serde::de::value::Error;
        fn deserialize_enum<V: serde::de::Visitor<'de>>(
            self,
            _: &'static str,
            variants: &'static [&'static str],
            _: V,
        ) -> Result<V::Value, Self::Error> {
            self.0 = Some(variants);
            Err(serde::de::Error::custom("inventory"))
        }
        fn deserialize_struct<V: serde::de::Visitor<'de>>(
            self,
            _: &'static str,
            fields: &'static [&'static str],
            _: V,
        ) -> Result<V::Value, Self::Error> {
            self.0 = Some(fields);
            Err(serde::de::Error::custom("inventory"))
        }
        fn deserialize_any<V: serde::de::Visitor<'de>>(
            self,
            _: V,
        ) -> Result<V::Value, Self::Error> {
            Err(serde::de::Error::custom("expected named metadata"))
        }
        serde::forward_to_deserialize_any! {bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map identifier ignored_any}
    }
    #[derive(Debug)]
    struct AcceptedNames(Vec<String>);
    impl std::fmt::Display for AcceptedNames {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl std::error::Error for AcceptedNames {}
    impl serde::de::Error for AcceptedNames {
        fn custom<T: std::fmt::Display>(message: T) -> Self {
            Self(vec![message.to_string()])
        }
        fn unknown_variant(_: &str, expected: &'static [&'static str]) -> Self {
            Self(expected.iter().map(|s| (*s).to_owned()).collect())
        }
    }
    fn union<T: serde::de::DeserializeOwned>(schema: &serde_json::Value, name: &str) {
        // Strict shared decoders preserve serde's unknown_variant metadata through their string gate.
        let actual = T::deserialize(serde::de::value::StringDeserializer::<AcceptedNames>::new(
            "__inventory_unknown__".into(),
        ))
        .err()
        .expect("unknown must reject")
        .0;
        let expected = schema["unions"][name]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            actual
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>(),
            expected,
            "{name} accepted spellings"
        );
    }
    #[test]
    fn serde_generated_accepted_names_equal_live_typescript() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/task_types_oracle.mjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let schema: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        macro_rules! check {
            ($wire:ident,$group:literal,$name:literal,$field:literal) => {{
                let mut inventory = Inventory::default();
                let _ = $wire::deserialize(&mut inventory);
                let actual: std::collections::BTreeSet<_> = inventory
                    .0
                    .expect("serde supplied inventory")
                    .iter()
                    .copied()
                    .collect();
                let expected = &schema[$group][$name];
                let expected = if $field.is_empty() {
                    expected
                } else {
                    &expected[$field]
                };
                assert_eq!(
                    actual,
                    expected
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap())
                        .collect(),
                    "{}",
                    $name
                );
            }};
        }
        check!(ChaserTerminateKindWire, "unions", "ChaserTerminateKind", "");
        union::<ChaserOrigin>(&schema, "ChaserOrigin");
        union::<TwapOrigin>(&schema, "TwapOrigin");
        union::<TwapTimeoutAction>(&schema, "TwapTimeoutAction");
        union::<TwapFailAction>(&schema, "TwapFailAction");
        union::<TwapStatus>(&schema, "TwapStatus");
        check!(ChaserTerminateWire, "records", "ChaserTerminate", "fields");
        check!(
            ChaserOrderOptionsWire,
            "records",
            "ChaserOrderOptions",
            "fields"
        );
        check!(
            ChaserOrderStateWire,
            "records",
            "ChaserOrderState",
            "fields"
        );
        check!(TwapOptionsWire, "records", "TwapOptions", "fields");
        check!(TwapStateWire, "records", "TwapState", "fields");
    }
}
