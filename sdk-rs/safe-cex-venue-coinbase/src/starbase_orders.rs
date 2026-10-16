//! Inert Starbase order-write contracts; no transport or signing.
use serde::Serialize;

// ts: CoinbaseStarbaseWriteOrderType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseWriteOrderType {
    #[serde(rename = "limit")]
    Limit,
    #[serde(rename = "stop_limit")]
    StopLimit,
    #[serde(rename = "take_limit")]
    TakeLimit,
    #[serde(rename = "market")]
    Market,
    #[serde(rename = "stop_market")]
    StopMarket,
    #[serde(rename = "take_market")]
    TakeMarket,
    #[serde(rename = "market_limit")]
    MarketLimit,
    #[serde(rename = "trailing_stop")]
    TrailingStop,
}

// ts: CoinbaseStarbaseLinkedOrderType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseLinkedOrderType {
    #[serde(rename = "one_triggers_one_cancels_other")]
    OneTriggersOneCancelsOther,
}

// ts: CoinbaseStarbaseTriggerFillCondition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseTriggerFillCondition {
    #[serde(rename = "first_hit")]
    FirstHit,
    #[serde(rename = "complete_fill")]
    CompleteFill,
    #[serde(rename = "incremental")]
    Incremental,
}

// ts: CoinbaseStarbaseWriteSide
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseWriteSide {
    #[serde(rename = "buy")]
    Buy,
    #[serde(rename = "sell")]
    Sell,
}

// ts: CoinbaseStarbaseWriteParityBlocker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseWriteParityBlocker {
    #[serde(rename = "contract-unit-conversion-unverified")]
    ContractUnitConversionUnverified,
    #[serde(rename = "edit-total-vs-remaining-amount-unresolved")]
    EditTotalVsRemainingAmountUnresolved,
    #[serde(rename = "market-limit-response-schema-missing")]
    MarketLimitResponseSchemaMissing,
    #[serde(rename = "response-filled-amount-unit-conflict")]
    ResponseFilledAmountUnitConflict,
    #[serde(rename = "response-trade-amount-unit-conflict")]
    ResponseTradeAmountUnitConflict,
    #[serde(rename = "valid-until-unit-unpublished")]
    ValidUntilUnitUnpublished,
}

// Anonymous singleton fields in the amount and timestamp intent contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseBaseAmountSource {
    #[serde(rename = "coinbase-technical-migration-guide")]
    TechnicalMigrationGuide,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseBaseAmountUnit {
    #[serde(rename = "base")]
    Base,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseContractAmountSource {
    #[serde(rename = "gateway-contract-count")]
    GatewayContractCount,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseContractAmountUnit {
    #[serde(rename = "contracts")]
    Contracts,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseUnresolvedTimestampUnit {
    #[serde(rename = "upstream-unspecified")]
    UpstreamUnspecified,
}

// ts: CoinbaseStarbaseBaseAmountIntent
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseBaseAmountIntent {
    #[serde(rename = "totalBaseAmount")]
    pub total_base_amount: f64,
    pub source: CoinbaseStarbaseBaseAmountSource,
    pub unit: CoinbaseStarbaseBaseAmountUnit,
}

// ts: CoinbaseStarbaseContractAmountIntent
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseContractAmountIntent {
    #[serde(rename = "totalContracts")]
    pub total_contracts: f64,
    pub source: CoinbaseStarbaseContractAmountSource,
    pub unit: CoinbaseStarbaseContractAmountUnit,
}

// ts: CoinbaseStarbaseWriteAmountIntent
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CoinbaseStarbaseWriteAmountIntent {
    Base(CoinbaseStarbaseBaseAmountIntent),
    Contracts(CoinbaseStarbaseContractAmountIntent),
}

// ts: CoinbaseStarbaseWriteGate
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status")]
pub enum CoinbaseStarbaseWriteGate<T, Intent> {
    #[serde(rename = "ready")]
    Ready { value: T },
    #[serde(rename = "blocked")]
    Blocked {
        blocker: CoinbaseStarbaseWriteParityBlocker,
        intent: Intent,
    },
}

// ts: CoinbaseStarbaseUnresolvedValidUntilIntent
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseUnresolvedValidUntilIntent {
    #[serde(rename = "gatewayIntegerTimestamp")]
    pub gateway_integer_timestamp: f64,
    pub unit: CoinbaseStarbaseUnresolvedTimestampUnit,
}

// ts: CoinbaseStarbaseTifParams
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseTifParams {
    pub post_only: bool,
    pub time_in_force: crate::starbase_private::CoinbaseStarbaseTimeInForce,
}

// ts: CoinbaseStarbaseEditIntent
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseEditIntent {
    #[serde(rename = "orderId")]
    pub order_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(rename = "postOnly", skip_serializing_if = "Option::is_none")]
    pub post_only: Option<bool>,
    #[serde(rename = "reduceOnly", skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(rename = "rejectPostOnly", skip_serializing_if = "Option::is_none")]
    pub reject_post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<CoinbaseStarbaseWriteAmountIntent>,
    #[serde(rename = "triggerOffset", skip_serializing_if = "Option::is_none")]
    pub trigger_offset: Option<f64>,
    #[serde(rename = "triggerPrice", skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
    #[serde(rename = "validUntil", skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<CoinbaseStarbaseUnresolvedValidUntilIntent>,
}

// ts: CoinbaseStarbaseEditParams
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseEditParams {
    pub order_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reject_post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_offset: Option<f64>,
}

// ts: CoinbaseStarbaseClosePositionIntent
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseClosePositionIntent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(rename = "savedSymbol")]
    pub saved_symbol: String,
    #[serde(rename = "type")]
    pub order_type: CoinbaseStarbaseClosePositionType,
}

// Anonymous type field in CoinbaseStarbaseClosePositionIntent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseClosePositionType {
    #[serde(rename = "limit")]
    Limit,
    #[serde(rename = "market")]
    Market,
}

// Anonymous closed type field in CoinbaseStarbaseOtocoLegIntent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseOtocoLegType {
    #[serde(rename = "take_limit")]
    TakeLimit,
    #[serde(rename = "take_market")]
    TakeMarket,
    #[serde(rename = "stop_limit")]
    StopLimit,
    #[serde(rename = "stop_market")]
    StopMarket,
}

// ts: CoinbaseStarbaseOtocoLegIntent (reachable through CoinbaseStarbaseOtocoIntent)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseOtocoLegIntent {
    pub direction: CoinbaseStarbaseWriteSide,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(rename = "postOnly", skip_serializing_if = "Option::is_none")]
    pub post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(rename = "reduceOnly")]
    pub reduce_only: safe_cex_types::True,
    #[serde(rename = "rejectPostOnly", skip_serializing_if = "Option::is_none")]
    pub reject_post_only: Option<bool>,
    pub size: CoinbaseStarbaseBaseAmountIntent,
    #[serde(rename = "timeInForce", skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<safe_cex_types::OrderTimeInForce>,
    pub trigger: crate::starbase_private::CoinbaseStarbaseTriggerType,
    #[serde(rename = "triggerPrice", skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
    #[serde(rename = "type")]
    pub order_type: CoinbaseStarbaseOtocoLegType,
}

// ts: CoinbaseStarbaseOtocoIntent
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseOtocoIntent {
    pub legs: [CoinbaseStarbaseOtocoLegIntent; 2],
    #[serde(rename = "linkedOrderType")]
    pub linked_order_type: CoinbaseStarbaseLinkedOrderType,
    #[serde(
        rename = "triggerFillCondition",
        skip_serializing_if = "Option::is_none"
    )]
    pub trigger_fill_condition: Option<CoinbaseStarbaseTriggerFillCondition>,
}

// ts: CoinbaseStarbaseOrderIdentity
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseOrderIdentity {
    #[serde(rename = "canonicalOrderId")]
    pub canonical_order_id: String,
    #[serde(rename = "nonCanonicalLabel")]
    pub non_canonical_label: String,
    #[serde(rename = "preTriggerOrderId", skip_serializing_if = "Option::is_none")]
    pub pre_trigger_order_id: Option<String>,
    #[serde(rename = "otocoChildOrderIds")]
    pub otoco_child_order_ids: Vec<String>,
    #[serde(
        rename = "otocoPrimaryOrderId",
        skip_serializing_if = "Option::is_none"
    )]
    pub otoco_primary_order_id: Option<String>,
}

// ts: CoinbaseStarbaseWriteResult
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseWriteResult {
    // Both source factories insert order, trades, identity in this order.
    pub order: crate::starbase_private::CoinbaseStarbasePrivateOrder,
    pub trades: Vec<crate::starbase_private::CoinbaseStarbasePrivateTrade>,
    pub identity: CoinbaseStarbaseOrderIdentity,
}

// ts: CoinbaseStarbasePlaceOrderIntent
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbasePlaceOrderIntent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otoco: Option<CoinbaseStarbaseOtocoIntent>,
    #[serde(rename = "postOnly", skip_serializing_if = "Option::is_none")]
    pub post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(rename = "reduceOnly", skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(rename = "rejectPostOnly", skip_serializing_if = "Option::is_none")]
    pub reject_post_only: Option<bool>,
    #[serde(rename = "savedSymbol")]
    pub saved_symbol: String,
    pub side: CoinbaseStarbaseWriteSide,
    pub size: CoinbaseStarbaseWriteAmountIntent,
    #[serde(rename = "timeInForce", skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<safe_cex_types::OrderTimeInForce>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<crate::starbase_private::CoinbaseStarbaseTriggerType>,
    #[serde(rename = "triggerOffset", skip_serializing_if = "Option::is_none")]
    pub trigger_offset: Option<f64>,
    #[serde(rename = "triggerPrice", skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
    pub r#type: CoinbaseStarbaseWriteOrderType,
    #[serde(rename = "validUntil", skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<CoinbaseStarbaseUnresolvedValidUntilIntent>,
}

// ts: CoinbaseStarbaseSideOrderIntent
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseSideOrderIntent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otoco: Option<CoinbaseStarbaseOtocoIntent>,
    #[serde(rename = "postOnly", skip_serializing_if = "Option::is_none")]
    pub post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(rename = "reduceOnly", skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(rename = "rejectPostOnly", skip_serializing_if = "Option::is_none")]
    pub reject_post_only: Option<bool>,
    #[serde(rename = "savedSymbol")]
    pub saved_symbol: String,
    pub size: CoinbaseStarbaseWriteAmountIntent,
    #[serde(rename = "timeInForce", skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<safe_cex_types::OrderTimeInForce>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<crate::starbase_private::CoinbaseStarbaseTriggerType>,
    #[serde(rename = "triggerOffset", skip_serializing_if = "Option::is_none")]
    pub trigger_offset: Option<f64>,
    #[serde(rename = "triggerPrice", skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
    pub r#type: CoinbaseStarbaseWriteOrderType,
    #[serde(rename = "validUntil", skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<CoinbaseStarbaseUnresolvedValidUntilIntent>,
}

// ts: CoinbaseStarbaseOtocoParams
// Field order follows the source factory's runtime insertion order, which is
// separate from the TypeScript interface's declaration order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseOtocoParams {
    pub amount: f64,
    pub direction: CoinbaseStarbaseWriteSide,
    pub reduce_only: safe_cex_types::True,
    pub trigger: crate::starbase_private::CoinbaseStarbaseTriggerType,
    pub r#type: CoinbaseStarbaseOtocoLegType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<crate::starbase_private::CoinbaseStarbaseTimeInForce>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reject_post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
}

// ts: CoinbaseStarbasePlaceOrderParams
// Field order follows the source factory's runtime insertion order, which is
// separate from the TypeScript interface's declaration order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbasePlaceOrderParams {
    pub amount: f64,
    pub instrument_name: String,
    pub r#type: CoinbaseStarbaseWriteOrderType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<crate::starbase_private::CoinbaseStarbaseTimeInForce>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reject_post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<crate::starbase_private::CoinbaseStarbaseTriggerType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_offset: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_order_type: Option<CoinbaseStarbaseLinkedOrderType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otoco_config: Option<[CoinbaseStarbaseOtocoParams; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_fill_condition: Option<CoinbaseStarbaseTriggerFillCondition>,
}

// Anonymous method literals in the instantiated write request aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbasePlaceOrderMethod {
    #[serde(rename = "private/buy")]
    Buy,
    #[serde(rename = "private/sell")]
    Sell,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseEditMethod {
    #[serde(rename = "private/edit")]
    Edit,
}

// ts: CoinbaseStarbasePlaceOrderRequest
// Concrete expansion of the TS request alias preserves typed params without
// requiring a venue-local object conversion solely for the generic bound.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbasePlaceOrderRequest {
    pub method: CoinbaseStarbasePlaceOrderMethod,
    pub params: CoinbaseStarbasePlaceOrderParams,
}

// ts: CoinbaseStarbaseEditRequest
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseEditRequest {
    pub method: CoinbaseStarbaseEditMethod,
    pub params: CoinbaseStarbaseEditParams,
}

// ts: protocolError
pub fn protocol_error<T>(
    message: impl Into<String>,
) -> Result<T, crate::starbase::CoinbaseStarbaseProtocolError> {
    Err(crate::starbase::CoinbaseStarbaseProtocolError::new(message))
}

// ts: requirePositive
// None represents every non-number input: this guard neither coerces nor reads it.
pub fn require_positive(
    value: Option<f64>,
    context: &str,
) -> Result<f64, crate::starbase::CoinbaseStarbaseProtocolError> {
    match value {
        Some(value) if value.is_finite() && value > 0.0 => Ok(value),
        _ => protocol_error(format!(
            "Invalid {context}: expected a positive finite number"
        )),
    }
}

// ts: coinbaseStarbaseTotalBaseAmount
pub fn coinbase_starbase_total_base_amount(
    total_base_amount: f64,
) -> Result<CoinbaseStarbaseBaseAmountIntent, crate::starbase::CoinbaseStarbaseProtocolError> {
    require_positive(Some(total_base_amount), "Starbase total base amount")?;
    Ok(CoinbaseStarbaseBaseAmountIntent {
        total_base_amount,
        source: CoinbaseStarbaseBaseAmountSource::TechnicalMigrationGuide,
        unit: CoinbaseStarbaseBaseAmountUnit::Base,
    })
}

// ts: coinbaseStarbaseTotalContracts
pub fn coinbase_starbase_total_contracts(
    total_contracts: f64,
) -> Result<CoinbaseStarbaseContractAmountIntent, crate::starbase::CoinbaseStarbaseProtocolError> {
    require_positive(Some(total_contracts), "Starbase total contract count")?;
    Ok(CoinbaseStarbaseContractAmountIntent {
        total_contracts,
        source: CoinbaseStarbaseContractAmountSource::GatewayContractCount,
        unit: CoinbaseStarbaseContractAmountUnit::Contracts,
    })
}

// ts: coinbaseStarbaseUnresolvedValidUntil
pub fn coinbase_starbase_unresolved_valid_until(
    gateway_integer_timestamp: f64,
) -> Result<
    CoinbaseStarbaseUnresolvedValidUntilIntent,
    crate::starbase::CoinbaseStarbaseProtocolError,
> {
    if !gateway_integer_timestamp.is_finite()
        || gateway_integer_timestamp.fract() != 0.0
        || gateway_integer_timestamp.abs() > 9_007_199_254_740_991.0
    {
        return protocol_error("Invalid Starbase valid_until: expected a safe integer");
    }
    Ok(CoinbaseStarbaseUnresolvedValidUntilIntent {
        gateway_integer_timestamp,
        unit: CoinbaseStarbaseUnresolvedTimestampUnit::UpstreamUnspecified,
    })
}

// ts: requireRecord
pub fn require_record<'a>(
    value: Option<&'a serde_json::Value>,
    context: &str,
) -> Result<
    &'a serde_json::Map<String, serde_json::Value>,
    crate::starbase::CoinbaseStarbaseProtocolError,
> {
    crate::starbase_private::require_record(value, context)
}

// ts: requireArray
pub fn require_array<'a>(
    value: Option<&'a serde_json::Value>,
    context: &str,
) -> Result<&'a Vec<serde_json::Value>, crate::starbase::CoinbaseStarbaseProtocolError> {
    crate::starbase_private::require_array(value, context)
}

// ts: requireString
pub fn require_string<'a>(
    value: Option<&'a serde_json::Value>,
    context: &str,
) -> Result<&'a str, crate::starbase::CoinbaseStarbaseProtocolError> {
    crate::starbase_private::require_string(value, context, false)
}

// ts: parseEnum
pub fn parse_enum<'a>(
    value: Option<&'a serde_json::Value>,
    values: &[&str],
    context: &str,
) -> Result<&'a str, crate::starbase::CoinbaseStarbaseProtocolError> {
    value
        .and_then(serde_json::Value::as_str)
        .filter(|value| values.contains(value))
        .ok_or_else(|| {
            crate::starbase::CoinbaseStarbaseProtocolError::new(format!("Invalid {context}"))
        })
}

// ts: assertUnique
pub fn assert_unique(
    values: &[String],
    context: &str,
) -> Result<(), crate::starbase::CoinbaseStarbaseProtocolError> {
    let unique: std::collections::HashSet<_> = values.iter().collect();
    if unique.len() != values.len() {
        return protocol_error(format!("Duplicate {context}"));
    }
    Ok(())
}

// ts: decodeStringArray
pub fn decode_string_array(
    value: Option<&serde_json::Value>,
    context: &str,
) -> Result<Vec<String>, crate::starbase::CoinbaseStarbaseProtocolError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let result = require_array(Some(value), context)?
        .iter()
        .map(|entry| require_string(Some(entry), context).map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    assert_unique(&result, context)?;
    Ok(result)
}

// ts: validateLabel
// String representation follows shared-repr:utf8-string; length counts UTF-16
// code units, including two for supplementary Unicode scalar values.
pub fn validate_label(
    value: Option<&serde_json::Value>,
) -> Result<(), crate::starbase::CoinbaseStarbaseProtocolError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value
        .as_str()
        .is_none_or(|label| label.encode_utf16().count() > 64)
    {
        return protocol_error("Starbase label must be at most 64 characters");
    }
    Ok(())
}

// ts: decodeOrderIdentity
pub fn decode_order_identity(
    raw: &serde_json::Map<String, serde_json::Value>,
    order: &crate::starbase_private::CoinbaseStarbasePrivateOrder,
) -> Result<CoinbaseStarbaseOrderIdentity, crate::starbase::CoinbaseStarbaseProtocolError> {
    Ok(CoinbaseStarbaseOrderIdentity {
        canonical_order_id: order.id.clone(),
        non_canonical_label: order.label.clone(),
        pre_trigger_order_id: raw
            .get("trigger_order_id")
            .map(|v| require_string(Some(v), "Starbase order trigger_order_id").map(str::to_owned))
            .transpose()?,
        otoco_child_order_ids: decode_string_array(
            raw.get("oto_order_ids"),
            "Starbase order oto_order_ids",
        )?,
        otoco_primary_order_id: raw
            .get("primary_order_id")
            .map(|v| require_string(Some(v), "Starbase order primary_order_id").map(str::to_owned))
            .transpose()?,
    })
}

// ts: decodeCoinbaseStarbaseWriteResult
pub fn decode_coinbase_starbase_write_result(
    value: Option<&serde_json::Value>,
) -> Result<CoinbaseStarbaseWriteResult, crate::starbase::CoinbaseStarbaseProtocolError> {
    let raw = require_record(value, "Starbase write result")?;
    let order_raw = require_record(raw.get("order"), "Starbase write result order")?;
    let trades_raw = require_array(raw.get("trades"), "Starbase write result trades")?;
    let order = crate::starbase_private::decode_coinbase_starbase_order(raw.get("order"))?;
    let trades = trades_raw
        .iter()
        .map(|trade| crate::starbase_private::decode_coinbase_starbase_trade(Some(trade)))
        .collect::<Result<Vec<_>, _>>()?;
    assert_unique(
        &trades
            .iter()
            .map(|trade| trade.id.clone())
            .collect::<Vec<_>>(),
        "Starbase write trade_id",
    )?;
    let identity = decode_order_identity(order_raw, &order)?;
    Ok(CoinbaseStarbaseWriteResult {
        order,
        trades,
        identity,
    })
}

// ts: decodeCoinbaseStarbaseCancelResult
pub fn decode_coinbase_starbase_cancel_result(
    value: Option<&serde_json::Value>,
) -> Result<CoinbaseStarbaseWriteResult, crate::starbase::CoinbaseStarbaseProtocolError> {
    let order_raw = require_record(value, "Starbase cancel result")?;
    let order = crate::starbase_private::decode_coinbase_starbase_order(value)?;
    let identity = decode_order_identity(order_raw, &order)?;
    Ok(CoinbaseStarbaseWriteResult {
        order,
        trades: Vec::new(),
        identity,
    })
}

// ts: assessCoinbaseStarbaseWriteResultParity
pub fn assess_coinbase_starbase_write_result_parity(
    result: &CoinbaseStarbaseWriteResult,
) -> CoinbaseStarbaseWriteGate<&CoinbaseStarbaseWriteResult, &CoinbaseStarbaseWriteResult> {
    if !result.trades.is_empty() {
        return CoinbaseStarbaseWriteGate::Blocked {
            blocker: CoinbaseStarbaseWriteParityBlocker::ResponseTradeAmountUnitConflict,
            intent: result,
        };
    }
    if result.order.gateway_filled_amount.unwrap_or(0.0) > 0.0 {
        return CoinbaseStarbaseWriteGate::Blocked {
            blocker: CoinbaseStarbaseWriteParityBlocker::ResponseFilledAmountUnitConflict,
            intent: result,
        };
    }
    CoinbaseStarbaseWriteGate::Ready { value: result }
}

// ts: mapCoinbaseStarbaseTimeInForce
pub fn map_coinbase_starbase_time_in_force(
    value: Option<&serde_json::Value>,
) -> Result<CoinbaseStarbaseTifParams, crate::starbase::CoinbaseStarbaseProtocolError> {
    use crate::starbase_private::CoinbaseStarbaseTimeInForce as Gateway;
    use safe_cex_types::OrderTimeInForce as Source;
    // Deserialize through the shared string-only boundary, preserving its exact
    // spelling rather than duplicating the core enum's protocol table.
    let source = value.and_then(|value| serde_json::from_value::<Source>(value.clone()).ok());
    let (post_only, time_in_force) = match source {
        Some(Source::GoodTillCancel) => (false, Gateway::GoodTilCancelled),
        Some(Source::ImmediateOrCancel) => (false, Gateway::ImmediateOrCancel),
        Some(Source::FillOrKill) => (false, Gateway::FillOrKill),
        Some(Source::PostOnly) => (true, Gateway::GoodTilCancelled),
        None => return protocol_error("Unsupported safe-cex time in force for Starbase"),
    };
    Ok(CoinbaseStarbaseTifParams {
        post_only,
        time_in_force,
    })
}

// ts: buildCoinbaseStarbaseCancel
pub fn build_coinbase_starbase_cancel(
    order_id: Option<&serde_json::Value>,
) -> Result<
    crate::starbase_private::CoinbaseStarbasePrivateRequest<
        &'static str,
        serde_json::Map<String, serde_json::Value>,
    >,
    crate::starbase::CoinbaseStarbaseProtocolError,
> {
    let order_id = require_string(order_id, "Starbase cancel order_id")?;
    Ok(crate::starbase_private::CoinbaseStarbasePrivateRequest {
        method: "private/cancel",
        params: serde_json::Map::from_iter([("order_id".into(), order_id.into())]),
    })
}

// ts: buildCoinbaseStarbaseEdit
pub fn build_coinbase_starbase_edit(
    intent: &CoinbaseStarbaseEditIntent,
) -> Result<
    CoinbaseStarbaseWriteGate<CoinbaseStarbaseEditRequest, &CoinbaseStarbaseEditIntent>,
    crate::starbase::CoinbaseStarbaseProtocolError,
> {
    let order_id_value = serde_json::Value::from(intent.order_id.clone());
    let order_id = require_string(Some(&order_id_value), "Starbase edit order_id")?;
    if matches!(
        intent.size,
        Some(CoinbaseStarbaseWriteAmountIntent::Contracts(_))
    ) {
        return Ok(CoinbaseStarbaseWriteGate::Blocked {
            blocker: CoinbaseStarbaseWriteParityBlocker::ContractUnitConversionUnverified,
            intent,
        });
    }
    if intent.size.is_some() {
        return Ok(CoinbaseStarbaseWriteGate::Blocked {
            blocker: CoinbaseStarbaseWriteParityBlocker::EditTotalVsRemainingAmountUnresolved,
            intent,
        });
    }
    if intent.valid_until.is_some() {
        return Ok(CoinbaseStarbaseWriteGate::Blocked {
            blocker: CoinbaseStarbaseWriteParityBlocker::ValidUntilUnitUnpublished,
            intent,
        });
    }
    if intent.reject_post_only == Some(true) && intent.post_only != Some(true) {
        return protocol_error("Starbase edit reject_post_only requires post_only");
    }
    let params = CoinbaseStarbaseEditParams {
        order_id: order_id.to_owned(),
        price: intent
            .price
            .map(|v| require_positive(Some(v), "Starbase edit price"))
            .transpose()?,
        post_only: intent.post_only,
        reduce_only: intent.reduce_only,
        reject_post_only: intent.reject_post_only,
        trigger_price: intent
            .trigger_price
            .map(|v| require_positive(Some(v), "Starbase edit trigger_price"))
            .transpose()?,
        trigger_offset: intent
            .trigger_offset
            .map(|v| require_positive(Some(v), "Starbase edit trigger_offset"))
            .transpose()?,
    };
    if params.price.is_none()
        && params.post_only.is_none()
        && params.reduce_only.is_none()
        && params.reject_post_only.is_none()
        && params.trigger_price.is_none()
        && params.trigger_offset.is_none()
    {
        return protocol_error("Starbase edit requires at least one change");
    }
    Ok(CoinbaseStarbaseWriteGate::Ready {
        value: CoinbaseStarbaseEditRequest {
            method: CoinbaseStarbaseEditMethod::Edit,
            params,
        },
    })
}

// ts: buildCoinbaseStarbaseClosePosition
pub fn build_coinbase_starbase_close_position(
    intent: &CoinbaseStarbaseClosePositionIntent,
) -> Result<
    crate::starbase_private::CoinbaseStarbasePrivateRequest<
        &'static str,
        serde_json::Map<String, serde_json::Value>,
    >,
    crate::starbase::CoinbaseStarbaseProtocolError,
> {
    if intent.order_type == CoinbaseStarbaseClosePositionType::Limit && intent.price.is_none() {
        return protocol_error("Starbase limit close requires price");
    }
    if intent.order_type == CoinbaseStarbaseClosePositionType::Market && intent.price.is_some() {
        return protocol_error("Starbase market close must not include price");
    }
    let instrument_name =
        crate::starbase_public::to_coinbase_starbase_instrument_name(&intent.saved_symbol)?;
    let mut params = serde_json::Map::from_iter([
        ("instrument_name".into(), instrument_name.into()),
        (
            "type".into(),
            match intent.order_type {
                CoinbaseStarbaseClosePositionType::Limit => "limit",
                CoinbaseStarbaseClosePositionType::Market => "market",
            }
            .into(),
        ),
    ]);
    if let Some(price) = intent.price {
        params.insert(
            "price".into(),
            require_positive(Some(price), "Starbase close price")?.into(),
        );
    }
    Ok(crate::starbase_private::CoinbaseStarbasePrivateRequest {
        method: "private/close_position",
        params,
    })
}

// ts: buildCoinbaseStarbaseBuy
pub fn build_coinbase_starbase_buy(
    intent: &CoinbaseStarbaseSideOrderIntent,
) -> Result<
    CoinbaseStarbaseWriteGate<CoinbaseStarbasePlaceOrderRequest, CoinbaseStarbasePlaceOrderIntent>,
    crate::starbase::CoinbaseStarbaseProtocolError,
> {
    let expanded = CoinbaseStarbasePlaceOrderIntent {
        label: intent.label.clone(),
        otoco: intent.otoco.clone(),
        post_only: intent.post_only,
        price: intent.price,
        reduce_only: intent.reduce_only,
        reject_post_only: intent.reject_post_only,
        saved_symbol: intent.saved_symbol.clone(),
        side: CoinbaseStarbaseWriteSide::Buy,
        size: intent.size.clone(),
        time_in_force: intent.time_in_force,
        trigger: intent.trigger,
        trigger_offset: intent.trigger_offset,
        trigger_price: intent.trigger_price,
        r#type: intent.r#type,
        valid_until: intent.valid_until.clone(),
    };
    match build_coinbase_starbase_place_order(&expanded)? {
        CoinbaseStarbaseWriteGate::Ready { value } => {
            Ok(CoinbaseStarbaseWriteGate::Ready { value })
        }
        CoinbaseStarbaseWriteGate::Blocked { blocker, .. } => {
            Ok(CoinbaseStarbaseWriteGate::Blocked {
                blocker,
                intent: expanded,
            })
        }
    }
}

// ts: buildCoinbaseStarbaseSell
pub fn build_coinbase_starbase_sell(
    intent: &CoinbaseStarbaseSideOrderIntent,
) -> Result<
    CoinbaseStarbaseWriteGate<CoinbaseStarbasePlaceOrderRequest, CoinbaseStarbasePlaceOrderIntent>,
    crate::starbase::CoinbaseStarbaseProtocolError,
> {
    let expanded = CoinbaseStarbasePlaceOrderIntent {
        label: intent.label.clone(),
        otoco: intent.otoco.clone(),
        post_only: intent.post_only,
        price: intent.price,
        reduce_only: intent.reduce_only,
        reject_post_only: intent.reject_post_only,
        saved_symbol: intent.saved_symbol.clone(),
        side: CoinbaseStarbaseWriteSide::Sell,
        size: intent.size.clone(),
        time_in_force: intent.time_in_force,
        trigger: intent.trigger,
        trigger_offset: intent.trigger_offset,
        trigger_price: intent.trigger_price,
        r#type: intent.r#type,
        valid_until: intent.valid_until.clone(),
    };
    match build_coinbase_starbase_place_order(&expanded)? {
        CoinbaseStarbaseWriteGate::Ready { value } => {
            Ok(CoinbaseStarbaseWriteGate::Ready { value })
        }
        CoinbaseStarbaseWriteGate::Blocked { blocker, .. } => {
            Ok(CoinbaseStarbaseWriteGate::Blocked {
                blocker,
                intent: expanded,
            })
        }
    }
}

// ts: buildCoinbaseStarbasePlaceOrder
pub fn build_coinbase_starbase_place_order(
    intent: &CoinbaseStarbasePlaceOrderIntent,
) -> Result<
    CoinbaseStarbaseWriteGate<CoinbaseStarbasePlaceOrderRequest, &CoinbaseStarbasePlaceOrderIntent>,
    crate::starbase::CoinbaseStarbaseProtocolError,
> {
    if let CoinbaseStarbaseWriteAmountIntent::Contracts(_) = &intent.size {
        return Ok(CoinbaseStarbaseWriteGate::Blocked {
            blocker: CoinbaseStarbaseWriteParityBlocker::ContractUnitConversionUnverified,
            intent,
        });
    }
    if intent.r#type == CoinbaseStarbaseWriteOrderType::MarketLimit {
        return Ok(CoinbaseStarbaseWriteGate::Blocked {
            blocker: CoinbaseStarbaseWriteParityBlocker::MarketLimitResponseSchemaMissing,
            intent,
        });
    }
    if intent.valid_until.is_some() {
        return Ok(CoinbaseStarbaseWriteGate::Blocked {
            blocker: CoinbaseStarbaseWriteParityBlocker::ValidUntilUnitUnpublished,
            intent,
        });
    }
    let amount = match &intent.size {
        CoinbaseStarbaseWriteAmountIntent::Base(value) => {
            if value.source != CoinbaseStarbaseBaseAmountSource::TechnicalMigrationGuide {
                return protocol_error("Invalid Starbase base amount source");
            }
            require_positive(Some(value.total_base_amount), "Starbase total base amount")?
        }
        CoinbaseStarbaseWriteAmountIntent::Contracts(_) => {
            return Ok(CoinbaseStarbaseWriteGate::Blocked {
                blocker: CoinbaseStarbaseWriteParityBlocker::ContractUnitConversionUnverified,
                intent,
            });
        }
    };
    validate_typed_label(intent.label.as_deref())?;
    validate_order_shape(
        intent.r#type,
        intent.price,
        intent.trigger,
        intent.trigger_offset,
        intent.trigger_price,
    )?;

    let tif = intent
        .time_in_force
        .map(map_typed_coinbase_starbase_time_in_force)
        .transpose()?;
    let post_only = intent
        .post_only
        .or(tif.as_ref().map(|value| value.post_only));
    if tif.as_ref().is_some_and(|value| value.post_only) && intent.post_only == Some(false) {
        return protocol_error("Starbase PostOnly time in force conflicts with post_only false");
    }
    if post_only == Some(true)
        && tif.as_ref().is_some_and(|value| {
            value.time_in_force
                != crate::starbase_private::CoinbaseStarbaseTimeInForce::GoodTilCancelled
        })
    {
        return protocol_error("Starbase post-only requires good_til_cancelled");
    }
    if intent.reject_post_only == Some(true) && post_only != Some(true) {
        return protocol_error("Starbase reject_post_only requires post_only");
    }

    let otoco = intent.otoco.as_ref().map(build_otoco).transpose()?;
    let params = CoinbaseStarbasePlaceOrderParams {
        amount,
        instrument_name: crate::starbase_public::to_coinbase_starbase_instrument_name(
            &intent.saved_symbol,
        )?,
        label: intent.label.clone(),
        linked_order_type: intent.otoco.as_ref().map(|value| value.linked_order_type),
        otoco_config: otoco,
        post_only,
        price: intent
            .price
            .map(|value| require_positive(Some(value), "Starbase order price"))
            .transpose()?,
        reduce_only: intent.reduce_only,
        reject_post_only: intent.reject_post_only,
        time_in_force: tif.map(|value| value.time_in_force),
        trigger: intent.trigger,
        trigger_fill_condition: intent
            .otoco
            .as_ref()
            .and_then(|value| value.trigger_fill_condition),
        trigger_offset: intent
            .trigger_offset
            .map(|value| require_positive(Some(value), "Starbase order trigger_offset"))
            .transpose()?,
        trigger_price: intent
            .trigger_price
            .map(|value| require_positive(Some(value), "Starbase order trigger_price"))
            .transpose()?,
        r#type: intent.r#type,
    };
    let method = match intent.side {
        CoinbaseStarbaseWriteSide::Buy => CoinbaseStarbasePlaceOrderMethod::Buy,
        CoinbaseStarbaseWriteSide::Sell => CoinbaseStarbasePlaceOrderMethod::Sell,
    };
    Ok(CoinbaseStarbaseWriteGate::Ready {
        value: CoinbaseStarbasePlaceOrderRequest { method, params },
    })
}

fn validate_typed_label(
    value: Option<&str>,
) -> Result<(), crate::starbase::CoinbaseStarbaseProtocolError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.encode_utf16().count() > 64 {
        return protocol_error("Starbase label must be at most 64 characters");
    }
    Ok(())
}

fn map_typed_coinbase_starbase_time_in_force(
    value: safe_cex_types::OrderTimeInForce,
) -> Result<CoinbaseStarbaseTifParams, crate::starbase::CoinbaseStarbaseProtocolError> {
    use crate::starbase_private::CoinbaseStarbaseTimeInForce as Gateway;
    use safe_cex_types::OrderTimeInForce as Source;
    let (post_only, time_in_force) = match value {
        Source::GoodTillCancel => (false, Gateway::GoodTilCancelled),
        Source::ImmediateOrCancel => (false, Gateway::ImmediateOrCancel),
        Source::FillOrKill => (false, Gateway::FillOrKill),
        Source::PostOnly => (true, Gateway::GoodTilCancelled),
    };
    Ok(CoinbaseStarbaseTifParams {
        post_only,
        time_in_force,
    })
}

pub fn validate_order_shape(
    order_type: CoinbaseStarbaseWriteOrderType,
    price: Option<f64>,
    trigger: Option<crate::starbase_private::CoinbaseStarbaseTriggerType>,
    trigger_offset: Option<f64>,
    trigger_price: Option<f64>,
) -> Result<(), crate::starbase::CoinbaseStarbaseProtocolError> {
    use CoinbaseStarbaseWriteOrderType as Type;
    let requires_price = matches!(order_type, Type::Limit | Type::StopLimit | Type::TakeLimit);
    let forbids_price = matches!(
        order_type,
        Type::Market | Type::StopMarket | Type::TakeMarket | Type::TrailingStop
    );
    if requires_price && price.is_none() {
        return protocol_error(format!(
            "Starbase {} requires price",
            write_order_type_str(order_type)
        ));
    }
    if forbids_price && price.is_some() {
        return protocol_error(format!(
            "Starbase {} must not include price",
            write_order_type_str(order_type)
        ));
    }
    let trigger_order = matches!(
        order_type,
        Type::StopLimit
            | Type::TakeLimit
            | Type::StopMarket
            | Type::TakeMarket
            | Type::TrailingStop
    );
    if trigger_order && trigger.is_none() {
        return protocol_error(format!(
            "Starbase {} requires trigger",
            write_order_type_str(order_type)
        ));
    }
    if !trigger_order && trigger.is_some() {
        return protocol_error(format!(
            "Starbase {} must not include trigger",
            write_order_type_str(order_type)
        ));
    }
    if order_type == Type::TrailingStop {
        if trigger_offset.is_none() {
            return protocol_error("Starbase trailing_stop requires trigger_offset");
        }
        if trigger_price.is_some() {
            return protocol_error("Starbase trailing_stop must not include trigger_price");
        }
    } else if trigger_order && trigger_price.is_none() {
        return protocol_error(format!(
            "Starbase {} requires trigger_price",
            write_order_type_str(order_type)
        ));
    }
    Ok(())
}

fn write_order_type_str(value: CoinbaseStarbaseWriteOrderType) -> &'static str {
    match value {
        CoinbaseStarbaseWriteOrderType::Limit => "limit",
        CoinbaseStarbaseWriteOrderType::StopLimit => "stop_limit",
        CoinbaseStarbaseWriteOrderType::TakeLimit => "take_limit",
        CoinbaseStarbaseWriteOrderType::Market => "market",
        CoinbaseStarbaseWriteOrderType::StopMarket => "stop_market",
        CoinbaseStarbaseWriteOrderType::TakeMarket => "take_market",
        CoinbaseStarbaseWriteOrderType::MarketLimit => "market_limit",
        CoinbaseStarbaseWriteOrderType::TrailingStop => "trailing_stop",
    }
}

pub fn build_otoco(
    intent: &CoinbaseStarbaseOtocoIntent,
) -> Result<[CoinbaseStarbaseOtocoParams; 2], crate::starbase::CoinbaseStarbaseProtocolError> {
    if intent.linked_order_type != CoinbaseStarbaseLinkedOrderType::OneTriggersOneCancelsOther {
        return protocol_error("Unsupported Starbase linked order type");
    }
    if intent.trigger_fill_condition.is_some_and(|value| {
        !matches!(
            value,
            CoinbaseStarbaseTriggerFillCondition::FirstHit
                | CoinbaseStarbaseTriggerFillCondition::CompleteFill
                | CoinbaseStarbaseTriggerFillCondition::Incremental
        )
    }) {
        return protocol_error("Unsupported Starbase trigger fill condition");
    }
    let kinds = intent.legs.each_ref().map(|leg| match leg.order_type {
        CoinbaseStarbaseOtocoLegType::TakeLimit | CoinbaseStarbaseOtocoLegType::TakeMarket => 1,
        CoinbaseStarbaseOtocoLegType::StopLimit | CoinbaseStarbaseOtocoLegType::StopMarket => 2,
    });
    if kinds[0] == kinds[1] {
        return protocol_error("Starbase OTOCO requires one take-profit and one stop-loss leg");
    }
    Ok([
        build_otoco_leg(&intent.legs[0])?,
        build_otoco_leg(&intent.legs[1])?,
    ])
}

pub fn build_otoco_leg(
    leg: &CoinbaseStarbaseOtocoLegIntent,
) -> Result<CoinbaseStarbaseOtocoParams, crate::starbase::CoinbaseStarbaseProtocolError> {
    validate_typed_label(leg.label.as_deref())?;
    if leg.size.source != CoinbaseStarbaseBaseAmountSource::TechnicalMigrationGuide {
        return protocol_error("Invalid Starbase OTOCO base amount source");
    }
    require_positive(
        Some(leg.size.total_base_amount),
        "Starbase OTOCO total base amount",
    )?;
    validate_otoco_leg_shape(leg)?;
    let tif = leg
        .time_in_force
        .map(map_typed_coinbase_starbase_time_in_force)
        .transpose()?;
    let post_only = leg.post_only.or(tif.as_ref().map(|value| value.post_only));
    if tif.as_ref().is_some_and(|value| value.post_only) && leg.post_only == Some(false) {
        return protocol_error(
            "Starbase OTOCO PostOnly time in force conflicts with post_only false",
        );
    }
    if leg.reject_post_only == Some(true) && post_only != Some(true) {
        return protocol_error("Starbase OTOCO reject_post_only requires post_only");
    }
    Ok(CoinbaseStarbaseOtocoParams {
        amount: leg.size.total_base_amount,
        direction: leg.direction,
        label: leg.label.clone(),
        post_only,
        price: leg
            .price
            .map(|value| require_positive(Some(value), "Starbase OTOCO price"))
            .transpose()?,
        reduce_only: safe_cex_types::True,
        reject_post_only: leg.reject_post_only,
        time_in_force: tif.map(|value| value.time_in_force),
        trigger: leg.trigger,
        trigger_price: leg
            .trigger_price
            .map(|value| require_positive(Some(value), "Starbase OTOCO trigger_price"))
            .transpose()?,
        r#type: leg.order_type,
    })
}

pub fn validate_otoco_leg_shape(
    leg: &CoinbaseStarbaseOtocoLegIntent,
) -> Result<(), crate::starbase::CoinbaseStarbaseProtocolError> {
    use CoinbaseStarbaseOtocoLegType as Type;
    let kind = match leg.order_type {
        Type::TakeLimit => "take_limit",
        Type::TakeMarket => "take_market",
        Type::StopLimit => "stop_limit",
        Type::StopMarket => "stop_market",
    };
    if matches!(leg.order_type, Type::TakeLimit | Type::StopLimit) && leg.price.is_none() {
        return protocol_error(format!("Starbase OTOCO {kind} requires price"));
    }
    if matches!(leg.order_type, Type::TakeMarket | Type::StopMarket) && leg.price.is_some() {
        return protocol_error(format!("Starbase OTOCO {kind} must not include price"));
    }
    if leg.order_type != Type::TakeLimit && leg.trigger_price.is_none() {
        return protocol_error(format!("Starbase OTOCO {kind} requires trigger_price"));
    }
    Ok(())
}
