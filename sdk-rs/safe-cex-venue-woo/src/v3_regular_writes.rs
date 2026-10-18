//! Inert V3 regular-order writes. No transport, demo lifecycle or live exchange route is enabled.
use crate::v3_contracts::{WooV3Endpoint, WooV3HttpMethod};
use crate::v3_private::WooV3AccountPositionMode;
use crate::v3_private_orders::{WooV3OrderSide, WooV3RegularOrder};
use crate::v3_request::WooV3RequestInput;
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};
use safe_cex_types::object_map::JsObjectMap;
use serde::{Deserialize, Serialize};

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
// ts: WooV3RegularTimeInForce
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3RegularTimeInForce {
    #[serde(rename = "GTC")]
    Gtc,
    #[serde(rename = "IOC")]
    Ioc,
    #[serde(rename = "FOK")]
    Fok,
    #[serde(rename = "POST_ONLY")]
    PostOnly,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3RegularTimeInForce")]
enum AcceptedWooV3RegularTimeInForceWire {
    #[serde(rename = "GTC")]
    Gtc,
    #[serde(rename = "IOC")]
    Ioc,
    #[serde(rename = "FOK")]
    Fok,
    #[serde(rename = "POST_ONLY")]
    PostOnly,
}
impl<'de> Deserialize<'de> for WooV3RegularTimeInForce {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3RegularTimeInForceWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}
impl JsNumberOutput for WooV3RegularTimeInForce {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Gtc,Ioc,Fok,PostOnly});
    }
}
// Supporting exact anonymous string union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3RegularPlaceKind {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3RegularPlaceKind")]
enum AcceptedWooV3RegularPlaceKindWire {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
}
impl<'de> Deserialize<'de> for WooV3RegularPlaceKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3RegularPlaceKindWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}
impl JsNumberOutput for WooV3RegularPlaceKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Limit,Market});
    }
}
// Supporting exact anonymous string union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3RegularPlaceType {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "IOC")]
    Ioc,
    #[serde(rename = "FOK")]
    Fok,
    #[serde(rename = "POST_ONLY")]
    PostOnly,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3RegularPlaceType")]
enum AcceptedWooV3RegularPlaceTypeWire {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "IOC")]
    Ioc,
    #[serde(rename = "FOK")]
    Fok,
    #[serde(rename = "POST_ONLY")]
    PostOnly,
}
impl<'de> Deserialize<'de> for WooV3RegularPlaceType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3RegularPlaceTypeWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}
impl JsNumberOutput for WooV3RegularPlaceType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Limit,Market,Ioc,Fok,PostOnly});
    }
}
// Supporting exact anonymous string union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3RegularMutationStatus {
    #[serde(rename = "NEW")]
    New,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "PARTIAL_FILLED")]
    PartialFilled,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "REJECTED")]
    Rejected,
    #[serde(rename = "INCOMPLETE")]
    Incomplete,
    #[serde(rename = "COMPLETED")]
    Completed,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3RegularMutationStatus")]
enum AcceptedWooV3RegularMutationStatusWire {
    #[serde(rename = "NEW")]
    New,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "PARTIAL_FILLED")]
    PartialFilled,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "REJECTED")]
    Rejected,
    #[serde(rename = "INCOMPLETE")]
    Incomplete,
    #[serde(rename = "COMPLETED")]
    Completed,
}
impl<'de> Deserialize<'de> for WooV3RegularMutationStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3RegularMutationStatusWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}
impl JsNumberOutput for WooV3RegularMutationStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{New,Cancelled,PartialFilled,Filled,Rejected,Incomplete,Completed});
    }
}
// Supporting exact anonymous string union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3RegularCancelStatus {
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "REJECTED")]
    Rejected,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3RegularCancelStatus")]
enum AcceptedWooV3RegularCancelStatusWire {
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "REJECTED")]
    Rejected,
}
impl<'de> Deserialize<'de> for WooV3RegularCancelStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3RegularCancelStatusWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}
impl JsNumberOutput for WooV3RegularCancelStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Cancelled,Rejected});
    }
}
// ts: WooV3RegularPlaceInput
#[derive(Debug, Clone)]
pub struct WooV3RegularPlaceInput {
    pub symbol: String,
    pub side: WooV3OrderSide,
    pub position_mode: WooV3AccountPositionMode,
    pub r#type: WooV3RegularPlaceKind,
    pub time_in_force: Option<WooV3RegularTimeInForce>,
    pub quantity: String,
    pub price: Option<String>,
    pub reduce_only: bool,
    pub client_order_id: Option<f64>,
    pub order_tag: Option<String>,
    pub visible_quantity: Option<String>,
    pub post_only_adjusted: Option<bool>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3RegularPlaceInput")]
struct WooV3RegularPlaceInputFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "side")]
    side: WooV3OrderSide,
    #[serde(rename = "positionMode")]
    position_mode: WooV3AccountPositionMode,
    #[serde(rename = "type")]
    r#type: WooV3RegularPlaceKind,
    #[serde(
        rename = "timeInForce",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    time_in_force: Option<WooV3RegularTimeInForce>,
    #[serde(rename = "quantity")]
    quantity: String,
    #[serde(
        rename = "price",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    price: Option<String>,
    #[serde(rename = "reduceOnly")]
    reduce_only: bool,
    #[serde(
        rename = "clientOrderId",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    client_order_id: Option<f64>,
    #[serde(
        rename = "orderTag",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    order_tag: Option<String>,
    #[serde(
        rename = "visibleQuantity",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    visible_quantity: Option<String>,
    #[serde(
        rename = "postOnlyAdjusted",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    post_only_adjusted: Option<bool>,
}
impl Serialize for WooV3RegularPlaceInput {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3RegularPlaceInputFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3RegularPlaceInput {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3RegularPlaceInputFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3RegularPlaceInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{symbol,side,position_mode,r#type,time_in_force,quantity,price,reduce_only,client_order_id,order_tag,visible_quantity,post_only_adjusted});
    }
}
// ts: WooV3RegularPlaceResult
#[derive(Debug, Clone)]
pub struct WooV3RegularPlaceResult {
    pub order_id: f64,
    pub r#type: WooV3RegularPlaceType,
    pub price: String,
    pub quantity: String,
    pub amount: String,
    pub client_order_id: f64,
    pub bid_ask_level: f64,
    pub timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3RegularPlaceResult")]
struct WooV3RegularPlaceResultFields {
    #[serde(rename = "orderId")]
    order_id: f64,
    #[serde(rename = "type")]
    r#type: WooV3RegularPlaceType,
    #[serde(rename = "price")]
    price: String,
    #[serde(rename = "quantity")]
    quantity: String,
    #[serde(rename = "amount")]
    amount: String,
    #[serde(rename = "clientOrderId")]
    client_order_id: f64,
    #[serde(rename = "bidAskLevel")]
    bid_ask_level: f64,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3RegularPlaceResult {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3RegularPlaceResultFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3RegularPlaceResult {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3RegularPlaceResultFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3RegularPlaceResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{order_id,r#type,price,quantity,amount,client_order_id,bid_ask_level,timestamp});
    }
}
// Supporting anonymous record.
#[derive(Debug, Clone)]
pub struct WooV3RegularAmendUpdate {
    pub price: Option<String>,
    pub total_quantity: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3RegularAmendUpdate")]
struct WooV3RegularAmendUpdateFields {
    #[serde(
        rename = "price",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    price: Option<String>,
    #[serde(
        rename = "totalQuantity",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    total_quantity: Option<String>,
}
impl Serialize for WooV3RegularAmendUpdate {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3RegularAmendUpdateFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3RegularAmendUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3RegularAmendUpdateFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3RegularAmendUpdate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{price,total_quantity});
    }
}
// ts: WooV3RegularAmendInput
#[derive(Debug, Clone)]
pub struct WooV3RegularAmendInput {
    pub current_order: WooV3RegularOrder,
    pub target: WooV3RegularWriteTarget,
    pub update: WooV3RegularAmendUpdate,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3RegularAmendInput")]
struct WooV3RegularAmendInputFields {
    #[serde(rename = "currentOrder")]
    current_order: WooV3RegularOrder,
    #[serde(rename = "target")]
    target: WooV3RegularWriteTarget,
    #[serde(rename = "update")]
    update: WooV3RegularAmendUpdate,
}
impl Serialize for WooV3RegularAmendInput {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3RegularAmendInputFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3RegularAmendInput {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3RegularAmendInputFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3RegularAmendInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{current_order,target,update});
    }
}
fn identity_evidence<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if s == "request-target-only-response-does-not-echo-id" {
        Ok(s)
    } else {
        Err(serde::de::Error::custom("unsupported identityEvidence"))
    }
}
// ts: WooV3RegularMutationResult
#[derive(Debug, Clone)]
pub struct WooV3RegularMutationResult {
    pub status: WooV3RegularMutationStatus,
    pub target: WooV3RegularWriteTarget,
    pub timestamp: f64,
    pub identity_evidence: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3RegularMutationResult")]
struct WooV3RegularMutationResultFields {
    #[serde(rename = "status")]
    status: WooV3RegularMutationStatus,
    #[serde(rename = "target")]
    target: WooV3RegularWriteTarget,
    #[serde(rename = "timestamp")]
    timestamp: f64,
    #[serde(rename = "identityEvidence")]
    #[serde(deserialize_with = "identity_evidence")]
    identity_evidence: String,
}
impl Serialize for WooV3RegularMutationResult {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3RegularMutationResultFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3RegularMutationResult {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3RegularMutationResultFields::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3RegularMutationResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{status,target,timestamp,identity_evidence});
    }
}
// Supporting anonymous record.
#[derive(Debug, Clone)]
pub struct WooV3RegularCancelScope {
    pub symbol: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3RegularCancelScope")]
struct WooV3RegularCancelScopeFields {
    #[serde(
        rename = "symbol",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    symbol: Option<String>,
}
impl Serialize for WooV3RegularCancelScope {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3RegularCancelScopeFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3RegularCancelScope {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3RegularCancelScopeFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3RegularCancelScope {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{symbol});
    }
}
// ts: WooV3RegularCancelPendingResult
#[derive(Debug, Clone)]
pub struct WooV3RegularCancelPendingResult {
    pub status: WooV3RegularCancelStatus,
    pub scope: WooV3RegularCancelScope,
    pub timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3RegularCancelPendingResult")]
struct WooV3RegularCancelPendingResultFields {
    #[serde(rename = "status")]
    status: WooV3RegularCancelStatus,
    #[serde(rename = "scope")]
    scope: WooV3RegularCancelScope,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3RegularCancelPendingResult {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3RegularCancelPendingResultFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3RegularCancelPendingResult {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3RegularCancelPendingResultFields::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WooV3RegularCancelPendingResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{status,scope,timestamp});
    }
}

// ts: WooV3RegularWriteTarget
// An exclusive owned ID sum; the dynamic validator below still rejects both/missing IDs in source order.
#[derive(Debug, Clone, PartialEq)]
pub enum WooV3RegularWriteTarget {
    OrderId(f64),
    ClientOrderId(f64),
}
impl<'de> Deserialize<'de> for WooV3RegularWriteTarget {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = JsNumberInput::deserialize(d)?;
        let row = input
            .decode::<JsObjectMap<JsNumberInput>>()
            .map_err(serde::de::Error::custom)?;
        let parse = |key: &str| {
            row.get(key)
                .unwrap()
                .clone()
                .decode::<f64>()
                .map_err(serde::de::Error::custom)
        };
        match (
            row.contains_key("orderId"),
            row.contains_key("clientOrderId"),
        ) {
            (true, false) => Ok(Self::OrderId(parse("orderId")?)),
            (false, true) => Ok(Self::ClientOrderId(parse("clientOrderId")?)),
            _ => Err(serde::de::Error::custom("target requires exactly one ID")),
        }
    }
}
impl Serialize for WooV3RegularWriteTarget {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = s.serialize_map(Some(1))?;
        match self {
            Self::OrderId(v) => map.serialize_entry("orderId", v)?,
            Self::ClientOrderId(v) => map.serialize_entry("clientOrderId", v)?,
        };
        map.end()
    }
}
impl JsNumberOutput for WooV3RegularWriteTarget {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{OrderId(id),ClientOrderId(id)});
    }
}
// ts: WooV3RegularAmendBuildResult
// Native operation output. Literal ready/blocker/quantitySemantics combinations are constructed here.
#[derive(Debug, Clone)]
pub enum WooV3RegularAmendBuildResult {
    Ready {
        request: WooV3RequestInput,
        target: WooV3RegularWriteTarget,
        quantity_semantics: &'static str,
    },
    Blocked {
        blocker: &'static str,
        target: WooV3RegularWriteTarget,
    },
}
impl JsNumberOutput for WooV3RegularAmendBuildResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Ready{request,target,quantity_semantics},Blocked{blocker,target}});
    }
}
// ts: WooV3WriteResponseError
#[derive(Debug, Clone)]
pub struct WooV3WriteResponseError {
    pub message: String,
    pub name: String,
    pub code: Option<f64>,
    pub gateway_response: JsObjectMap<JsNumberInput>,
    pub contract_status: &'static str,
}
impl WooV3WriteResponseError {
    // ts: constructor
    pub fn new(
        message: String,
        gateway_response: &JsObjectMap<JsNumberInput>,
        code: Option<f64>,
    ) -> Self {
        Self {
            message,
            name: "WooV3WriteResponseError".into(),
            code,
            gateway_response: gateway_response.clone(),
            contract_status: "vendor-error-envelope-undocumented",
        }
    }
}
// Concrete class transport, distinct from ordinary validation Errors.
#[derive(Debug, Clone)]
pub enum WooV3RegularWriteError {
    Validation(String),
    Gateway(WooV3WriteResponseError),
}
impl From<String> for WooV3RegularWriteError {
    fn from(s: String) -> Self {
        Self::Validation(s)
    }
}
impl From<&str> for WooV3RegularWriteError {
    fn from(s: &str) -> Self {
        Self::Validation(s.into())
    }
}
// Supporting fixed metadata record.
#[derive(Debug, Clone, Copy)]
pub struct WooV3RegularWriteParity {
    pub request_and_response_fixtures: bool,
    pub runtime_imported: bool,
    pub credentialed_demo_lifecycle: bool,
    pub partial_fill_amend_quantity_semantics: &'static str,
    pub amend_response_identity: &'static str,
    pub client_order_id_range: &'static str,
    pub cross_family_all_orders: &'static str,
}
// ts: WOO_V3_REGULAR_WRITE_PARITY
pub const WOO_V3_REGULAR_WRITE_PARITY: WooV3RegularWriteParity = WooV3RegularWriteParity {
    request_and_response_fixtures: true,
    runtime_imported: false,
    credentialed_demo_lifecycle: false,
    partial_fill_amend_quantity_semantics: "blocked-pending-demo-evidence",
    amend_response_identity: "request-target-only",
    client_order_id_range: "javascript-safe-integers-only",
    cross_family_all_orders: "deferred-because-it-also-cancels-algo-orders",
};
// The five source validators intentionally retain their own symbols for future source-by-source maintenance.
// ts: requireRecord
pub fn require_record(
    v: Option<&JsNumberInput>,
    path: &str,
) -> Result<JsObjectMap<JsNumberInput>, String> {
    crate::v3_account_writes::require_record(v, path)
}
// ts: requireBoolean
pub fn require_boolean(v: Option<&JsNumberInput>, path: &str) -> Result<bool, String> {
    crate::v3_account_writes::require_boolean(v, path)
}
// ts: requireInteger
pub fn require_integer(v: Option<&JsNumberInput>, path: &str, minimum: f64) -> Result<f64, String> {
    crate::v3_account_writes::require_integer(v, path, minimum)
}
// ts: requireString
pub fn require_string(v: Option<&JsNumberInput>, path: &str) -> Result<String, String> {
    crate::v3_account_writes::require_string(v, path)
}
// ts: requireEnum
pub fn require_enum(
    v: Option<&JsNumberInput>,
    values: &[&str],
    path: &str,
) -> Result<String, String> {
    crate::v3_account_writes::require_enum(v, values, path)
}
// ts: requireDecimal
pub fn require_decimal(
    v: Option<&JsNumberInput>,
    path: &str,
    allow_zero: bool,
) -> Result<String, String> {
    let s = v
        .and_then(|v| v.clone().decode::<String>().ok())
        .filter(|s| {
            let mut split = s.split('.');
            let whole = split.next().unwrap_or("");
            let fraction = split.next();
            split.next().is_none()
                && whole.bytes().all(|b| b.is_ascii_digit())
                && match fraction {
                    None => !whole.is_empty(),
                    Some(f) => {
                        f.bytes().all(|b| b.is_ascii_digit())
                            && (!whole.is_empty() || !f.is_empty())
                    }
                }
        })
        .ok_or_else(|| format!("{path} must be a finite non-negative decimal string"))?;
    if !allow_zero && !s.bytes().any(|b| matches!(b, b'1'..=b'9')) {
        return Err(format!("{path} must be greater than zero"));
    }
    Ok(s)
}
// ts: decimalParts
pub fn decimal_parts(value: &str) -> (String, String) {
    let mut parts = value.split('.');
    let raw_whole = parts.next().unwrap_or("0");
    let bytes = raw_whole.as_bytes();
    let mut offset = 0;
    while bytes.get(offset) == Some(&b'0') && bytes.get(offset + 1).is_some_and(u8::is_ascii_digit)
    {
        offset += 1;
    }
    let whole = &raw_whole[offset..];
    (
        if whole.is_empty() {
            "0".into()
        } else {
            whole.into()
        },
        parts.next().unwrap_or("").trim_end_matches('0').into(),
    )
}
fn cmp_utf16(a: &str, b: &str) -> i32 {
    match a.encode_utf16().cmp(b.encode_utf16()) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}
// ts: compareDecimals
pub fn compare_decimals(left: &str, right: &str) -> i32 {
    let (lw, lf) = decimal_parts(left);
    let (rw, rf) = decimal_parts(right);
    let ll = lw.encode_utf16().count();
    let rl = rw.encode_utf16().count();
    if ll != rl {
        return if ll > rl { 1 } else { -1 };
    }
    if lw != rw {
        return cmp_utf16(&lw, &rw);
    }
    let width = lf.encode_utf16().count().max(rf.encode_utf16().count());
    let l = lf.clone() + &"0".repeat(width - lf.encode_utf16().count());
    let r = rf.clone() + &"0".repeat(width - rf.encode_utf16().count());
    cmp_utf16(&l, &r)
}
// ts: requirePerpetualSymbol
pub fn require_perpetual_symbol(symbol: &str) -> Result<String, String> {
    crate::v3_account_writes::require_perpetual_symbol(symbol)
}
fn stored_string(value: Option<&JsNumberInput>, path: &str) -> Result<String, String> {
    value
        .and_then(|v| v.clone().decode::<String>().ok())
        .ok_or_else(|| format!("{path} must be a string"))
}
fn symbol_input(v: Option<&JsNumberInput>) -> Result<String, String> {
    let s = v
        .and_then(|v| v.clone().decode::<String>().ok())
        .ok_or("WOO V3 symbol must use PERP_BASE_QUOTE form")?;
    require_perpetual_symbol(&s)
}
// ts: validateTarget
pub fn validate_target(
    target: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3RegularWriteTarget, String> {
    let order = target.contains_key("orderId");
    let client = target.contains_key("clientOrderId");
    if order == client {
        return Err("WOO V3 write target requires exactly one ID".into());
    }
    if order {
        Ok(WooV3RegularWriteTarget::OrderId(require_integer(
            target.get("orderId"),
            "WOO V3 orderId",
            1.0,
        )?))
    } else {
        Ok(WooV3RegularWriteTarget::ClientOrderId(require_integer(
            target.get("clientOrderId"),
            "WOO V3 clientOrderId",
            1.0,
        )?))
    }
}
fn place_type(s: &str) -> WooV3RegularPlaceType {
    match s {
        "LIMIT" => WooV3RegularPlaceType::Limit,
        "MARKET" => WooV3RegularPlaceType::Market,
        "IOC" => WooV3RegularPlaceType::Ioc,
        "FOK" => WooV3RegularPlaceType::Fok,
        "POST_ONLY" => WooV3RegularPlaceType::PostOnly,
        _ => unreachable!("validated place type"),
    }
}
fn type_text(t: WooV3RegularPlaceType) -> &'static str {
    match t {
        WooV3RegularPlaceType::Limit => "LIMIT",
        WooV3RegularPlaceType::Market => "MARKET",
        WooV3RegularPlaceType::Ioc => "IOC",
        WooV3RegularPlaceType::Fok => "FOK",
        WooV3RegularPlaceType::PostOnly => "POST_ONLY",
    }
}
// ts: mapPlaceType
pub fn map_place_type(input: &JsObjectMap<JsNumberInput>) -> Result<WooV3RegularPlaceType, String> {
    let kind = require_enum(input.get("type"), &["LIMIT", "MARKET"], "WOO V3 place type")?;
    if kind == "MARKET" {
        if input.contains_key("timeInForce") {
            return Err("WOO V3 market orders do not accept a time-in-force".into());
        }
        return Ok(WooV3RegularPlaceType::Market);
    }
    let tif = match input.get("timeInForce") {
        None => "GTC".into(),
        Some(v) if v.clone().decode::<Option<String>>().ok() == Some(None) => "GTC".into(),
        value => require_enum(
            value,
            &["GTC", "IOC", "FOK", "POST_ONLY"],
            "WOO V3 place timeInForce",
        )?,
    };
    Ok(match tif.as_str() {
        "GTC" => WooV3RegularPlaceType::Limit,
        "IOC" => WooV3RegularPlaceType::Ioc,
        "FOK" => WooV3RegularPlaceType::Fok,
        _ => WooV3RegularPlaceType::PostOnly,
    })
}
fn capture<T: Serialize + JsNumberOutput>(v: &T) -> JsNumberInput {
    v.check_number_output();
    safe_cex_num::numeric_ingress::js_from_str(
        &serde_json::to_string(v).expect("eligible captured field"),
    )
    .expect("captured field")
}
fn target_map(target: &WooV3RegularWriteTarget) -> JsObjectMap<JsNumberInput> {
    match target {
        WooV3RegularWriteTarget::OrderId(id) => {
            [("orderId".into(), capture(id))].into_iter().collect()
        }
        WooV3RegularWriteTarget::ClientOrderId(id) => [("clientOrderId".into(), capture(id))]
            .into_iter()
            .collect(),
    }
}
fn descriptor(
    method: WooV3HttpMethod,
    path: WooV3Endpoint,
    query: Option<JsObjectMap<JsNumberInput>>,
    body: Option<JsObjectMap<JsNumberInput>>,
) -> WooV3RequestInput {
    WooV3RequestInput {
        method,
        path,
        query,
        body,
    }
}
// ts: buildWooV3RegularPlaceRequest
pub fn build_woo_v3_regular_place_request(
    input: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3RequestInput, String> {
    let symbol = symbol_input(input.get("symbol"))?;
    let side = require_enum(input.get("side"), &["BUY", "SELL"], "WOO V3 place side")?;
    let quantity = require_decimal(input.get("quantity"), "WOO V3 place quantity", false)?;
    let kind = map_place_type(input)?;
    let kind_text = type_text(kind);
    let price = input
        .get("price")
        .map(|v| require_decimal(Some(v), "WOO V3 place price", false))
        .transpose()?;
    if kind == WooV3RegularPlaceType::Market && price.is_some() {
        return Err("WOO V3 market orders must not send price".into());
    }
    if kind != WooV3RegularPlaceType::Market && price.is_none() {
        return Err(format!("WOO V3 {kind_text} orders require price"));
    }
    let mode = input
        .get("positionMode")
        .and_then(|v| v.clone().decode::<String>().ok());
    if !matches!(mode.as_deref(), Some("ONE_WAY" | "HEDGE_MODE")) {
        return Err("WOO V3 positionMode must be ONE_WAY or HEDGE_MODE".into());
    }
    if let Some(v) = input.get("orderTag") {
        let tag = v
            .clone()
            .decode::<String>()
            .map_err(|_| "WOO V3 orderTag must contain 1 to 64 characters")?;
        if !(1..=64).contains(&tag.encode_utf16().count()) {
            return Err("WOO V3 orderTag must contain 1 to 64 characters".into());
        }
    }
    if let Some(v) = input.get("clientOrderId") {
        require_integer(Some(v), "WOO V3 clientOrderId", 0.0)?;
    }
    let visible = input
        .get("visibleQuantity")
        .map(|v| require_decimal(Some(v), "WOO V3 visibleQuantity", true))
        .transpose()?;
    if visible
        .as_ref()
        .is_some_and(|v| compare_decimals(v, &quantity) > 0)
    {
        return Err("WOO V3 visibleQuantity must not exceed quantity".into());
    }
    if visible.is_some() && kind == WooV3RegularPlaceType::Market {
        return Err("WOO V3 market orders must not send visibleQuantity".into());
    }
    if input.contains_key("postOnlyAdjusted") && kind != WooV3RegularPlaceType::PostOnly {
        return Err("WOO V3 postOnlyAdjusted is only valid for POST_ONLY".into());
    }
    // The source signature promises booleans/strings; this native adapter checks those at their use boundary.
    let reduce = require_boolean(input.get("reduceOnly"), "WOO V3 place reduceOnly")?;
    if let Some(v) = input.get("postOnlyAdjusted") {
        require_boolean(Some(v), "WOO V3 postOnlyAdjusted")?;
    }
    let mut body = JsObjectMap::new();
    body.insert("symbol".into(), capture(&symbol));
    body.insert("side".into(), capture(&side));
    if mode.as_deref() == Some("HEDGE_MODE") {
        let pos = if reduce {
            if side == "BUY" { "SHORT" } else { "LONG" }
        } else if side == "BUY" {
            "LONG"
        } else {
            "SHORT"
        };
        body.insert("positionSide".into(), capture(&pos.to_string()));
    }
    body.insert("type".into(), capture(&kind_text.to_string()));
    for key in ["clientOrderId", "orderTag"] {
        if let Some(v) = input.get(key) {
            body.insert(key.into(), v.clone());
        }
    }
    if let Some(p) = price {
        body.insert("price".into(), capture(&p));
    }
    body.insert("quantity".into(), capture(&quantity));
    body.insert("reduceOnly".into(), capture(&reduce));
    if let Some(v) = visible {
        body.insert("visibleQuantity".into(), capture(&v));
    }
    body.insert("marginMode".into(), capture(&"CROSS".to_string()));
    if let Some(v) = input.get("postOnlyAdjusted") {
        body.insert("postOnlyAdjusted".into(), v.clone());
    }
    Ok(descriptor(
        WooV3HttpMethod::Post,
        WooV3Endpoint::TradeOrder,
        None,
        Some(body),
    ))
}
// ts: buildWooV3RegularAmendRequest
pub fn build_woo_v3_regular_amend_request(
    input: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3RegularAmendBuildResult, String> {
    let target = validate_target(&require_record(input.get("target"), "WOO V3 amend target")?)?;
    let order = require_record(input.get("currentOrder"), "WOO V3 amend currentOrder")?;
    let matches = match target {
        WooV3RegularWriteTarget::OrderId(id) => {
            order
                .get("orderId")
                .and_then(|v| v.clone().decode::<f64>().ok())
                == Some(id)
        }
        WooV3RegularWriteTarget::ClientOrderId(id) => {
            order
                .get("clientOrderId")
                .and_then(|v| v.clone().decode::<f64>().ok())
                == Some(id)
        }
    };
    if !matches {
        return Ok(WooV3RegularAmendBuildResult::Blocked {
            blocker: "target-id-mismatch",
            target,
        });
    }
    let status = order
        .get("status")
        .and_then(|v| v.clone().decode::<String>().ok());
    if !matches!(status.as_deref(), Some("NEW" | "PARTIAL_FILLED")) {
        return Ok(WooV3RegularAmendBuildResult::Blocked {
            blocker: "terminal-order-cannot-be-amended",
            target,
        });
    }
    let executed = require_decimal(
        order.get("executed"),
        "WOO V3 current executed quantity",
        true,
    )?;
    if status.as_deref() != Some("NEW") || compare_decimals(&executed, "0") != 0 {
        return Ok(WooV3RegularAmendBuildResult::Blocked {
            blocker: "partial-fill-amend-semantics-require-demo-evidence",
            target,
        });
    }
    let update = require_record(input.get("update"), "WOO V3 amend update")?;
    let price = update
        .get("price")
        .map(|v| require_decimal(Some(v), "WOO V3 amend price", false))
        .transpose()?;
    let total = update
        .get("totalQuantity")
        .map(|v| require_decimal(Some(v), "WOO V3 amend totalQuantity", false))
        .transpose()?;
    if price.is_none() && total.is_none() {
        return Err("WOO V3 amend requires price or totalQuantity".into());
    }
    let kind = order
        .get("type")
        .and_then(|v| v.clone().decode::<String>().ok())
        .ok_or("WOO V3 amend currentOrder.type must be a string")?;
    if price.is_some() && !matches!(kind.as_str(), "LIMIT" | "IOC" | "FOK" | "POST_ONLY") {
        return Err(format!("WOO V3 {kind} orders cannot amend price"));
    }
    let same_price = match &price {
        None => true,
        Some(p) => {
            compare_decimals(
                p,
                &stored_string(order.get("price"), "WOO V3 amend currentOrder.price")?,
            ) == 0
        }
    };
    // Preserve JS && short-circuit: an already changed price never inspects the old quantity.
    let unchanged = if same_price {
        match &total {
            None => true,
            Some(q) => {
                compare_decimals(
                    q,
                    &stored_string(order.get("quantity"), "WOO V3 amend currentOrder.quantity")?,
                ) == 0
            }
        }
    } else {
        false
    };
    if unchanged {
        return Err("WOO V3 amend must change at least one field".into());
    }
    let mut body = target_map(&target);
    if let Some(p) = price {
        body.insert("price".into(), capture(&p));
    }
    if let Some(q) = total {
        body.insert("quantity".into(), capture(&q));
    }
    Ok(WooV3RegularAmendBuildResult::Ready {
        request: descriptor(
            WooV3HttpMethod::Put,
            WooV3Endpoint::TradeOrder,
            None,
            Some(body),
        ),
        target,
        quantity_semantics: "total-equals-remaining-because-zero-filled-provenance",
    })
}
// ts: buildWooV3RegularCancelRequest
pub fn build_woo_v3_regular_cancel_request(
    symbol: &str,
    target: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3RequestInput, String> {
    let target = validate_target(target)?;
    let mut query = JsObjectMap::new();
    query.insert("symbol".into(), capture(&require_perpetual_symbol(symbol)?));
    for (k, v) in target_map(&target) {
        query.insert(k, v);
    }
    Ok(descriptor(
        WooV3HttpMethod::Delete,
        WooV3Endpoint::TradeOrder,
        Some(query),
        None,
    ))
}
// ts: buildWooV3RegularCancelPendingRequest
pub fn build_woo_v3_regular_cancel_pending_request(
    symbol: Option<&str>,
) -> Result<WooV3RequestInput, String> {
    let query = symbol
        .map(|s| {
            require_perpetual_symbol(s)
                .map(|s| [("symbol".into(), capture(&s))].into_iter().collect())
        })
        .transpose()?;
    Ok(descriptor(
        WooV3HttpMethod::Delete,
        WooV3Endpoint::TradeOrders,
        query,
        None,
    ))
}
// Supporting anonymous decodeSuccessData result.
#[derive(Debug, Clone)]
pub struct WooV3RegularSuccessData {
    pub data: JsObjectMap<JsNumberInput>,
    pub timestamp: f64,
}
// ts: decodeSuccessData
pub fn decode_success_data(
    input: Option<&JsNumberInput>,
    name: &str,
) -> Result<WooV3RegularSuccessData, WooV3RegularWriteError> {
    let response = require_record(input, &format!("WOO V3 {name} response"))?;
    if !require_boolean(response.get("success"), &format!("WOO V3 {name}.success"))? {
        let code = response
            .get("code")
            .map(|v| require_integer(Some(v), &format!("WOO V3 {name}.code"), 0.0))
            .transpose()?;
        let message = match response.get("message") {
            None => format!("WOO V3 {name} failed with an undocumented error envelope"),
            v => require_string(v, &format!("WOO V3 {name}.message"))?,
        };
        return Err(WooV3RegularWriteError::Gateway(
            WooV3WriteResponseError::new(message, &response, code),
        ));
    }
    let data = require_record(response.get("data"), &format!("WOO V3 {name}.data"))?;
    let timestamp = require_integer(
        response.get("timestamp"),
        &format!("WOO V3 {name}.timestamp"),
        0.0,
    )?;
    Ok(WooV3RegularSuccessData { data, timestamp })
}
// ts: decodeWooV3RegularPlaceResponse
#[derive(Debug, Clone)]
pub enum WooV3PlaceResponseError<E> {
    Response(WooV3RegularWriteError),
    Membership(E),
}
impl<E> From<String> for WooV3PlaceResponseError<E> {
    fn from(error: String) -> Self {
        Self::Response(error.into())
    }
}
impl<E> From<&str> for WooV3PlaceResponseError<E> {
    fn from(error: &str) -> Self {
        Self::Response(error.into())
    }
}
impl<E> From<WooV3RegularWriteError> for WooV3PlaceResponseError<E> {
    fn from(error: WooV3RegularWriteError) -> Self {
        Self::Response(error)
    }
}
pub fn decode_woo_v3_regular_place_response(
    input: Option<&JsNumberInput>,
    request: &WooV3RequestInput,
    known_order_ids: &[f64],
) -> Result<WooV3RegularPlaceResult, WooV3RegularWriteError> {
    decode_woo_v3_regular_place_response_with(input, request, |id| {
        Ok::<_, std::convert::Infallible>(known_order_ids.contains(&id))
    })
    .map_err(|error| match error {
        WooV3PlaceResponseError::Response(error) => error,
        WooV3PlaceResponseError::Membership(error) => match error {},
    })
}
pub fn decode_woo_v3_regular_place_response_with<E>(
    input: Option<&JsNumberInput>,
    request: &WooV3RequestInput,
    mut known_order_ids: impl FnMut(f64) -> Result<bool, E>,
) -> Result<WooV3RegularPlaceResult, WooV3PlaceResponseError<E>> {
    if request.method != WooV3HttpMethod::Post
        || request.path != WooV3Endpoint::TradeOrder
        || request.body.is_none()
    {
        return Err("WOO V3 place response requires its matching place request".into());
    }
    let expected = request.body.as_ref().unwrap();
    let WooV3RegularSuccessData { data, timestamp } = decode_success_data(input, "place order")?;
    let order_id = require_integer(data.get("orderId"), "WOO V3 place order.data.orderId", 1.0)?;
    if known_order_ids(order_id).map_err(WooV3PlaceResponseError::Membership)? {
        return Err("WOO V3 place response reused a known orderId".into());
    }
    let kind = require_enum(
        data.get("type"),
        &["LIMIT", "MARKET", "IOC", "FOK", "POST_ONLY"],
        "WOO V3 place order.data.type",
    )?;
    if expected
        .get("type")
        .and_then(|v| v.clone().decode::<String>().ok())
        .as_deref()
        != Some(&kind)
    {
        return Err("WOO V3 place response type does not match request".into());
    }
    let quantity = require_decimal(
        data.get("quantity"),
        "WOO V3 place order.data.quantity",
        false,
    )?;
    if compare_decimals(
        &quantity,
        &stored_string(expected.get("quantity"), "WOO V3 expected quantity")?,
    ) != 0
    {
        return Err("WOO V3 place response quantity does not match request".into());
    }
    let client_order_id = require_integer(
        data.get("clientOrderId"),
        "WOO V3 place order.data.clientOrderId",
        0.0,
    )?;
    if let Some(v) = expected.get("clientOrderId") {
        if v.clone().decode::<f64>().ok() != Some(client_order_id) {
            return Err("WOO V3 place response clientOrderId does not match request".into());
        }
    }
    let price = require_decimal(data.get("price"), "WOO V3 place order.data.price", true)?;
    if let Some(v) = expected.get("price") {
        let p = v
            .clone()
            .decode::<String>()
            .map_err(|_| "WOO V3 expected price must be a string")?;
        if compare_decimals(&price, &p) != 0 {
            return Err("WOO V3 place response price does not match request".into());
        }
    }
    Ok(WooV3RegularPlaceResult {
        order_id,
        r#type: place_type(&kind),
        price,
        quantity,
        amount: require_decimal(data.get("amount"), "WOO V3 place order.data.amount", true)?,
        client_order_id,
        bid_ask_level: require_integer(
            data.get("bidAskLevel"),
            "WOO V3 place order.data.bidAskLevel",
            0.0,
        )?,
        timestamp,
    })
}
fn mutation_status(s: &str) -> Result<WooV3RegularMutationStatus, WooV3RegularWriteError> {
    Ok(match s {
        "NEW" => WooV3RegularMutationStatus::New,
        "CANCELLED" => WooV3RegularMutationStatus::Cancelled,
        "PARTIAL_FILLED" => WooV3RegularMutationStatus::PartialFilled,
        "FILLED" => WooV3RegularMutationStatus::Filled,
        "REJECTED" => WooV3RegularMutationStatus::Rejected,
        "INCOMPLETE" => WooV3RegularMutationStatus::Incomplete,
        "COMPLETED" => WooV3RegularMutationStatus::Completed,
        _ => return Err("WOO V3 mutation status is outside the declared status domain".into()),
    })
}
// ts: decodeMutationResponse
pub fn decode_mutation_response(
    input: Option<&JsNumberInput>,
    name: &str,
    target: &JsObjectMap<JsNumberInput>,
    statuses: &[&str],
) -> Result<WooV3RegularMutationResult, WooV3RegularWriteError> {
    let WooV3RegularSuccessData { data, timestamp } = decode_success_data(input, name)?;
    let status = require_enum(
        data.get("status"),
        statuses,
        &format!("WOO V3 {name}.data.status"),
    )?;
    Ok(WooV3RegularMutationResult {
        status: mutation_status(&status)?,
        target: validate_target(target)?,
        timestamp,
        identity_evidence: "request-target-only-response-does-not-echo-id".into(),
    })
}
// ts: decodeWooV3RegularAmendResponse
pub fn decode_woo_v3_regular_amend_response(
    input: Option<&JsNumberInput>,
    target: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3RegularMutationResult, WooV3RegularWriteError> {
    decode_mutation_response(
        input,
        "amend order",
        target,
        &[
            "NEW",
            "CANCELLED",
            "PARTIAL_FILLED",
            "FILLED",
            "REJECTED",
            "INCOMPLETE",
            "COMPLETED",
        ],
    )
}
// ts: decodeWooV3RegularCancelResponse
pub fn decode_woo_v3_regular_cancel_response(
    input: Option<&JsNumberInput>,
    target: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3RegularMutationResult, WooV3RegularWriteError> {
    decode_mutation_response(input, "cancel order", target, &["CANCELLED", "REJECTED"])
}
// ts: decodeWooV3RegularCancelPendingResponse
pub fn decode_woo_v3_regular_cancel_pending_response(
    input: Option<&JsNumberInput>,
    symbol: Option<&str>,
) -> Result<WooV3RegularCancelPendingResult, WooV3RegularWriteError> {
    let WooV3RegularSuccessData { data, timestamp } =
        decode_success_data(input, "cancel pending orders")?;
    let status = require_enum(
        data.get("status"),
        &["CANCELLED", "REJECTED"],
        "WOO V3 cancel pending orders.data.status",
    )?;
    let status = if status == "CANCELLED" {
        WooV3RegularCancelStatus::Cancelled
    } else {
        WooV3RegularCancelStatus::Rejected
    };
    Ok(WooV3RegularCancelPendingResult {
        status,
        scope: WooV3RegularCancelScope {
            symbol: symbol.map(require_perpetual_symbol).transpose()?,
        },
        timestamp,
    })
}
