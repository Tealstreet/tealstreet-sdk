//! Inert position-mode evidence and descriptors; no live account route is enabled.
use crate::v3_account_writes::{
    WooV3AccountModeProvenance, WooV3AccountMutationResult, WooV3AccountMutationTarget,
    WooV3AccountPositionMode, WooV3AccountWriteError, decode_success, require_boolean,
    require_enum, require_record, require_string, validate_account_provenance,
};
use crate::v3_contracts::{WooV3Endpoint, WooV3HttpMethod};
use crate::v3_private::WooV3FuturesPosition;
use crate::v3_private_orders::{WooV3AlgoOrder, WooV3RegularOrder};
use crate::v3_request::WooV3RequestInput;
use safe_cex_num::numeric_ingress::JsNumberInput;
use serde::{Deserialize, Serialize};

// Capture numbers before nested Serde buffering; every record requires map ingress.

fn complete_literal<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let v = bool::deserialize(d)?;
    if v {
        Ok(v)
    } else {
        Err(serde::de::Error::custom("complete must be true"))
    }
}
fn source_positions<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = String::deserialize(d)?;
    if v == "decoded-v3-futures-positions-complete-page" {
        Ok(v)
    } else {
        Err(serde::de::Error::custom("unsupported snapshot source"))
    }
}

#[derive(Debug, Clone)]
pub struct WooV3PositionsEvidence {
    pub source: String,
    pub reconciliation_id: String,
    pub complete: bool,
    pub rows: Vec<WooV3FuturesPosition>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PositionsEvidence")]
struct WooV3PositionsEvidenceFields {
    #[serde(rename = "source", deserialize_with = "source_positions")]
    source: String,
    #[serde(rename = "reconciliationId")]
    reconciliation_id: String,
    #[serde(rename = "complete", deserialize_with = "complete_literal")]
    complete: bool,
    #[serde(rename = "rows")]
    rows: Vec<WooV3FuturesPosition>,
}
impl Serialize for WooV3PositionsEvidence {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3PositionsEvidenceFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3PositionsEvidence {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3PositionsEvidenceFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PositionsEvidence {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{source,reconciliation_id,complete,rows});
    }
}
fn source_regularorders<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = String::deserialize(d)?;
    if v == "decoded-v3-pending-regular-orders-complete-pages" {
        Ok(v)
    } else {
        Err(serde::de::Error::custom("unsupported snapshot source"))
    }
}

#[derive(Debug, Clone)]
pub struct WooV3RegularOrdersEvidence {
    pub source: String,
    pub reconciliation_id: String,
    pub complete: bool,
    pub rows: Vec<WooV3RegularOrder>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3RegularOrdersEvidence")]
struct WooV3RegularOrdersEvidenceFields {
    #[serde(rename = "source", deserialize_with = "source_regularorders")]
    source: String,
    #[serde(rename = "reconciliationId")]
    reconciliation_id: String,
    #[serde(rename = "complete", deserialize_with = "complete_literal")]
    complete: bool,
    #[serde(rename = "rows")]
    rows: Vec<WooV3RegularOrder>,
}
impl Serialize for WooV3RegularOrdersEvidence {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3RegularOrdersEvidenceFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3RegularOrdersEvidence {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3RegularOrdersEvidenceFields::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3RegularOrdersEvidence {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{source,reconciliation_id,complete,rows});
    }
}
fn source_algoorders<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = String::deserialize(d)?;
    if v == "decoded-v3-pending-algo-orders-complete-pages" {
        Ok(v)
    } else {
        Err(serde::de::Error::custom("unsupported snapshot source"))
    }
}

#[derive(Debug, Clone)]
pub struct WooV3AlgoOrdersEvidence {
    pub source: String,
    pub reconciliation_id: String,
    pub complete: bool,
    pub rows: Vec<WooV3AlgoOrder>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AlgoOrdersEvidence")]
struct WooV3AlgoOrdersEvidenceFields {
    #[serde(rename = "source", deserialize_with = "source_algoorders")]
    source: String,
    #[serde(rename = "reconciliationId")]
    reconciliation_id: String,
    #[serde(rename = "complete", deserialize_with = "complete_literal")]
    complete: bool,
    #[serde(rename = "rows")]
    rows: Vec<WooV3AlgoOrder>,
}
impl Serialize for WooV3AlgoOrdersEvidence {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3AlgoOrdersEvidenceFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3AlgoOrdersEvidence {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3AlgoOrdersEvidenceFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AlgoOrdersEvidence {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{source,reconciliation_id,complete,rows});
    }
}
// ts: WooV3CompletePositionModeEvidence

#[derive(Debug, Clone)]
pub struct WooV3CompletePositionModeEvidence {
    pub account: WooV3AccountModeProvenance,
    pub positions: WooV3PositionsEvidence,
    pub regular_orders: WooV3RegularOrdersEvidence,
    pub algo_orders: WooV3AlgoOrdersEvidence,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3CompletePositionModeEvidence")]
struct WooV3CompletePositionModeEvidenceFields {
    #[serde(rename = "account")]
    account: WooV3AccountModeProvenance,
    #[serde(rename = "positions")]
    positions: WooV3PositionsEvidence,
    #[serde(rename = "regularOrders")]
    regular_orders: WooV3RegularOrdersEvidence,
    #[serde(rename = "algoOrders")]
    algo_orders: WooV3AlgoOrdersEvidence,
}
impl Serialize for WooV3CompletePositionModeEvidence {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3CompletePositionModeEvidenceFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3CompletePositionModeEvidence {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3CompletePositionModeEvidenceFields::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3CompletePositionModeEvidence {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{account,positions,regular_orders,algo_orders});
    }
}
// ts: WooV3SetPositionModeInput

#[derive(Debug, Clone)]
pub struct WooV3SetPositionModeInput {
    pub target_position_mode: WooV3AccountPositionMode,
    pub evidence: WooV3CompletePositionModeEvidence,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3SetPositionModeInput")]
struct WooV3SetPositionModeInputFields {
    #[serde(rename = "targetPositionMode")]
    target_position_mode: WooV3AccountPositionMode,
    #[serde(rename = "evidence")]
    evidence: WooV3CompletePositionModeEvidence,
}
impl Serialize for WooV3SetPositionModeInput {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3SetPositionModeInputFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3SetPositionModeInput {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3SetPositionModeInputFields::deserialize(safe_cex_types::strict_decode::object(
                    d,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3SetPositionModeInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{target_position_mode,evidence});
    }
}

// ts: WooV3SetPositionModeBuildResult
// Operation outputs, not arbitrary declaration ingress. Literals enforced by the builder.
#[derive(Debug, Clone)]
pub enum WooV3SetPositionModeBuildResult {
    Ready {
        request: WooV3RequestInput,
        current_position_mode: WooV3AccountPositionMode,
        target_position_mode: WooV3AccountPositionMode,
        precondition_evidence: String,
    },
    Blocked {
        blocker: String,
        current_position_mode: Option<WooV3AccountPositionMode>,
        target_position_mode: WooV3AccountPositionMode,
    },
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3SetPositionModeBuildResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Ready{request,current_position_mode,target_position_mode,precondition_evidence},Blocked{blocker,current_position_mode,target_position_mode}});
    }
}
// Supporting owned result for validateSnapshotEvidence: rows are NOT typed during validation.
#[derive(Debug, Clone)]
pub struct WooV3ValidatedSnapshotEvidence {
    pub reconciliation_id: String,
    pub rows: Vec<JsNumberInput>,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3ValidatedSnapshotEvidence {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{reconciliation_id,rows});
    }
}
// ts: validateSnapshotEvidence
pub fn validate_snapshot_evidence(
    input: Option<&JsNumberInput>,
    expected_source: &str,
    path: &str,
) -> Option<WooV3ValidatedSnapshotEvidence> {
    let row = require_record(input, path).ok()?;
    require_enum(
        row.get("source"),
        &[expected_source],
        &format!("{path}.source"),
    )
    .ok()?;
    if require_boolean(row.get("complete"), &format!("{path}.complete")).ok() != Some(true) {
        return None;
    }
    let rows = row
        .get("rows")?
        .clone()
        .decode::<Vec<JsNumberInput>>()
        .ok()?;
    Some(WooV3ValidatedSnapshotEvidence {
        reconciliation_id: require_string(
            row.get("reconciliationId"),
            &format!("{path}.reconciliationId"),
        )
        .ok()?,
        rows,
    })
}
// ts: isExactDecimalZero
pub fn is_exact_decimal_zero(value: Option<&JsNumberInput>) -> bool {
    let Some(value) = value.and_then(|v| v.clone().decode::<String>().ok()) else {
        return false;
    };
    let text = value.strip_prefix('-').unwrap_or(&value);
    (match text.split_once('.') {
        Some((whole, fraction)) => !whole.is_empty() || !fraction.is_empty(),
        None => !text.is_empty(),
    }) && text.bytes().all(|b| b == b'0' || b == b'.')
        && text.bytes().filter(|b| *b == b'.').count() <= 1
}
// ts: positionProvesNoExposure
pub fn position_proves_no_exposure(input: Option<&JsNumberInput>) -> bool {
    let Ok(row) = require_record(input, "WOO V3 position-mode position row") else {
        return false;
    };
    is_exact_decimal_zero(row.get("holding"))
        && is_exact_decimal_zero(row.get("pendingLongQty"))
        && is_exact_decimal_zero(row.get("pendingShortQty"))
}
fn blocked(
    blocker: &str,
    current_position_mode: Option<WooV3AccountPositionMode>,
    target_position_mode: WooV3AccountPositionMode,
) -> WooV3SetPositionModeBuildResult {
    WooV3SetPositionModeBuildResult::Blocked {
        blocker: blocker.into(),
        current_position_mode,
        target_position_mode,
    }
}
// ts: buildWooV3SetPositionModeRequest
pub fn build_woo_v3_set_position_mode_request(
    input: Option<&JsNumberInput>,
) -> Result<WooV3SetPositionModeBuildResult, String> {
    let input = require_record(input, "WOO V3 position-mode input")?;
    let target = match require_enum(
        input.get("targetPositionMode"),
        &["ONE_WAY", "HEDGE_MODE"],
        "WOO V3 target positionMode",
    )?
    .as_str()
    {
        "ONE_WAY" => WooV3AccountPositionMode::OneWay,
        _ => WooV3AccountPositionMode::HedgeMode,
    };
    let evidence = input
        .get("evidence")
        .and_then(|v| require_record(Some(v), "evidence").ok());
    let account =
        match validate_account_provenance(evidence.as_ref().and_then(|v| v.get("account"))) {
            Ok(a) => a,
            Err(_) => {
                return Ok(blocked(
                    "account-state-evidence-incomplete-or-mixed",
                    None,
                    target,
                ));
            }
        };
    let positions = validate_snapshot_evidence(
        evidence.as_ref().and_then(|v| v.get("positions")),
        "decoded-v3-futures-positions-complete-page",
        "WOO V3 position-mode evidence.positions",
    );
    let regular = validate_snapshot_evidence(
        evidence.as_ref().and_then(|v| v.get("regularOrders")),
        "decoded-v3-pending-regular-orders-complete-pages",
        "WOO V3 position-mode evidence.regularOrders",
    );
    let algo = validate_snapshot_evidence(
        evidence.as_ref().and_then(|v| v.get("algoOrders")),
        "decoded-v3-pending-algo-orders-complete-pages",
        "WOO V3 position-mode evidence.algoOrders",
    );
    let (Some(positions), Some(regular), Some(algo)) = (positions, regular, algo) else {
        return Ok(blocked(
            "account-state-evidence-incomplete-or-mixed",
            Some(account.position_mode),
            target,
        ));
    };
    if [
        &positions.reconciliation_id,
        &regular.reconciliation_id,
        &algo.reconciliation_id,
    ]
    .iter()
    .any(|id| **id != account.reconciliation_id)
    {
        return Ok(blocked(
            "account-state-evidence-incomplete-or-mixed",
            Some(account.position_mode),
            target,
        ));
    }
    // Source ordering: margin account, already active, exposure, regular orders, algo orders.
    if account.account_mode != "MARGIN" {
        return Ok(blocked(
            "futures-margin-account-mode-not-proven",
            Some(account.position_mode),
            target,
        ));
    }
    if account.position_mode == target {
        return Ok(blocked(
            "position-mode-already-active",
            Some(account.position_mode),
            target,
        ));
    }
    if !positions
        .rows
        .iter()
        .all(|v| position_proves_no_exposure(Some(v)))
    {
        return Ok(blocked(
            "open-positions-must-be-closed",
            Some(account.position_mode),
            target,
        ));
    }
    if !regular.rows.is_empty() {
        return Ok(blocked(
            "pending-regular-orders-must-be-cancelled",
            Some(account.position_mode),
            target,
        ));
    }
    if !algo.rows.is_empty() {
        return Ok(blocked(
            "pending-algo-orders-must-be-cancelled",
            Some(account.position_mode),
            target,
        ));
    }
    let wire = if target == WooV3AccountPositionMode::OneWay {
        "ONE_WAY"
    } else {
        "HEDGE_MODE"
    };
    let value = serde_json::from_str::<JsNumberInput>(
        &serde_json::to_string(wire).expect("literal string"),
    )
    .expect("literal capture");
    Ok(WooV3SetPositionModeBuildResult::Ready {
        request: WooV3RequestInput {
            method: WooV3HttpMethod::Put,
            path: WooV3Endpoint::FuturesPositionMode,
            query: None,
            body: Some([("positionMode".into(), value)].into_iter().collect()),
        },
        current_position_mode: account.position_mode,
        target_position_mode: target,
        precondition_evidence: "complete-reconciled-v3-positions-zero-and-pending-orders-empty"
            .into(),
    })
}
// ts: decodeWooV3SetPositionModeResponse
pub fn decode_woo_v3_set_position_mode_response(
    input: Option<&JsNumberInput>,
    request: &WooV3RequestInput,
    target_position_mode: WooV3AccountPositionMode,
) -> Result<WooV3AccountMutationResult, WooV3AccountWriteError> {
    if request.method != WooV3HttpMethod::Put || request.path != WooV3Endpoint::FuturesPositionMode
    {
        return Err(
            "WOO V3 position-mode response requires its matching request"
                .to_string()
                .into(),
        );
    }
    Ok(WooV3AccountMutationResult {
        accepted: true,
        timestamp: decode_success(input, "set position mode")?,
        target: WooV3AccountMutationTarget::PositionMode {
            position_mode: target_position_mode,
        },
        state_evidence: "request-target-only-response-does-not-echo-resulting-state".into(),
    })
}
