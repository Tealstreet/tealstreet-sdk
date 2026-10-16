//! Transport-free route plans; selection never opens a migration gate.
use crate::v3_contracts::{
    WOO_V3_MIGRATION_GATES, WOO_V3_MIGRATION_ROUTES, WooV3HttpMethod, WooV3MigrationGate,
    WooV3MigrationRoute, WooV3MigrationRouteRuntimeLifecycle,
};
use serde_json::{Value, json};

// ts: requireRoute
pub fn require_route(route_id: &str) -> Result<&'static WooV3MigrationRoute, String> {
    WOO_V3_MIGRATION_ROUTES
        .iter()
        .find(|route| route.id == route_id)
        .ok_or_else(|| format!("Unknown WOO V3 migration route {route_id}"))
}

// ts: target
pub fn target(generation: &str, method: WooV3HttpMethod, path: &str) -> Value {
    json!({"generation": generation, "method": method, "path": path})
}

use serde::Serialize;

// ts: WooV3MigrationRouteId
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub enum WooV3MigrationRouteId {
    #[serde(rename = "markets")]
    Markets,
    #[serde(rename = "tickers")]
    Tickers,
    #[serde(rename = "orderbook")]
    Orderbook,
    #[serde(rename = "recent-klines")]
    RecentKlines,
    #[serde(rename = "historical-klines")]
    HistoricalKlines,
    #[serde(rename = "place-order")]
    PlaceOrder,
    #[serde(rename = "cancel-order")]
    CancelOrder,
    #[serde(rename = "fetch-orders")]
    FetchOrders,
    #[serde(rename = "cancel-symbol-orders")]
    CancelSymbolOrders,
    #[serde(rename = "cancel-all-orders")]
    CancelAllOrders,
    #[serde(rename = "amend-order")]
    AmendOrder,
    #[serde(rename = "place-algo-order")]
    PlaceAlgoOrder,
    #[serde(rename = "amend-algo-order")]
    AmendAlgoOrder,
    #[serde(rename = "cancel-algo-order")]
    CancelAlgoOrder,
    #[serde(rename = "fetch-algo-orders")]
    FetchAlgoOrders,
    #[serde(rename = "cancel-all-algo-orders")]
    CancelAllAlgoOrders,
    #[serde(rename = "cancel-all-after")]
    CancelAllAfter,
    #[serde(rename = "recent-trades")]
    RecentTrades,
    #[serde(rename = "historical-trades")]
    HistoricalTrades,
    #[serde(rename = "transaction-history")]
    TransactionHistory,
    #[serde(rename = "account-info")]
    AccountInfo,
    #[serde(rename = "balances")]
    Balances,
    #[serde(rename = "positions")]
    Positions,
    #[serde(rename = "position-mode")]
    PositionMode,
    #[serde(rename = "fetch-futures-leverage")]
    FetchFuturesLeverage,
    #[serde(rename = "set-futures-leverage")]
    SetFuturesLeverage,
}
impl<'de> serde::Deserialize<'de> for WooV3MigrationRouteId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        #[derive(serde::Deserialize)]
        #[serde(remote = "WooV3MigrationRouteId")]
        enum Wire {
            #[serde(rename = "markets")]
            Markets,
            #[serde(rename = "tickers")]
            Tickers,
            #[serde(rename = "orderbook")]
            Orderbook,
            #[serde(rename = "recent-klines")]
            RecentKlines,
            #[serde(rename = "historical-klines")]
            HistoricalKlines,
            #[serde(rename = "place-order")]
            PlaceOrder,
            #[serde(rename = "cancel-order")]
            CancelOrder,
            #[serde(rename = "fetch-orders")]
            FetchOrders,
            #[serde(rename = "cancel-symbol-orders")]
            CancelSymbolOrders,
            #[serde(rename = "cancel-all-orders")]
            CancelAllOrders,
            #[serde(rename = "amend-order")]
            AmendOrder,
            #[serde(rename = "place-algo-order")]
            PlaceAlgoOrder,
            #[serde(rename = "amend-algo-order")]
            AmendAlgoOrder,
            #[serde(rename = "cancel-algo-order")]
            CancelAlgoOrder,
            #[serde(rename = "fetch-algo-orders")]
            FetchAlgoOrders,
            #[serde(rename = "cancel-all-algo-orders")]
            CancelAllAlgoOrders,
            #[serde(rename = "cancel-all-after")]
            CancelAllAfter,
            #[serde(rename = "recent-trades")]
            RecentTrades,
            #[serde(rename = "historical-trades")]
            HistoricalTrades,
            #[serde(rename = "transaction-history")]
            TransactionHistory,
            #[serde(rename = "account-info")]
            AccountInfo,
            #[serde(rename = "balances")]
            Balances,
            #[serde(rename = "positions")]
            Positions,
            #[serde(rename = "position-mode")]
            PositionMode,
            #[serde(rename = "fetch-futures-leverage")]
            FetchFuturesLeverage,
            #[serde(rename = "set-futures-leverage")]
            SetFuturesLeverage,
        }
        Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}

impl WooV3MigrationRouteId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Markets => "markets",
            Self::Tickers => "tickers",
            Self::Orderbook => "orderbook",
            Self::RecentKlines => "recent-klines",
            Self::HistoricalKlines => "historical-klines",
            Self::PlaceOrder => "place-order",
            Self::CancelOrder => "cancel-order",
            Self::FetchOrders => "fetch-orders",
            Self::CancelSymbolOrders => "cancel-symbol-orders",
            Self::CancelAllOrders => "cancel-all-orders",
            Self::AmendOrder => "amend-order",
            Self::PlaceAlgoOrder => "place-algo-order",
            Self::AmendAlgoOrder => "amend-algo-order",
            Self::CancelAlgoOrder => "cancel-algo-order",
            Self::FetchAlgoOrders => "fetch-algo-orders",
            Self::CancelAllAlgoOrders => "cancel-all-algo-orders",
            Self::CancelAllAfter => "cancel-all-after",
            Self::RecentTrades => "recent-trades",
            Self::HistoricalTrades => "historical-trades",
            Self::TransactionHistory => "transaction-history",
            Self::AccountInfo => "account-info",
            Self::Balances => "balances",
            Self::Positions => "positions",
            Self::PositionMode => "position-mode",
            Self::FetchFuturesLeverage => "fetch-futures-leverage",
            Self::SetFuturesLeverage => "set-futures-leverage",
        }
    }
}

// ts: WooV3RuntimeMode
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub enum WooV3RuntimeMode {
    #[serde(rename = "legacy")]
    Legacy,
    #[serde(rename = "shadow-v3")]
    ShadowV3,
    #[serde(rename = "v3")]
    V3,
}
impl<'de> serde::Deserialize<'de> for WooV3RuntimeMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        #[derive(serde::Deserialize)]
        #[serde(remote = "WooV3RuntimeMode")]
        enum Wire {
            #[serde(rename = "legacy")]
            Legacy,
            #[serde(rename = "shadow-v3")]
            ShadowV3,
            #[serde(rename = "v3")]
            V3,
        }
        Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}

impl WooV3RuntimeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::ShadowV3 => "shadow-v3",
            Self::V3 => "v3",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WooV3RuntimeGeneration {
    Legacy,
    V3,
}

// ts: WooV3RuntimeRouteTarget
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WooV3RuntimeRouteTarget {
    generation: WooV3RuntimeGeneration,
    method: WooV3HttpMethod,
    path: String,
}
impl WooV3RuntimeRouteTarget {
    pub fn generation(&self) -> WooV3RuntimeGeneration {
        self.generation
    }
    pub fn method(&self) -> WooV3HttpMethod {
        self.method
    }
    pub fn path(&self) -> &str {
        &self.path
    }
}
fn native_target(
    generation: WooV3RuntimeGeneration,
    method: WooV3HttpMethod,
    path: &str,
) -> WooV3RuntimeRouteTarget {
    WooV3RuntimeRouteTarget {
        generation,
        method,
        path: path.into(),
    }
}

// ts: WooV3LegacyRouteSelection
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WooV3LegacyRouteSelection {
    status: &'static str,
    route_id: WooV3MigrationRouteId,
    requested_mode: WooV3RuntimeMode,
    gate: WooV3MigrationGate,
    authoritative: WooV3RuntimeRouteTarget,
    store_authority: &'static str,
}
impl WooV3LegacyRouteSelection {
    pub fn status(&self) -> &str {
        self.status
    }
    pub fn route_id(&self) -> WooV3MigrationRouteId {
        self.route_id
    }
    pub fn gate(&self) -> WooV3MigrationGate {
        self.gate
    }
    pub fn store_authority(&self) -> &str {
        self.store_authority
    }
    pub fn requested_mode(&self) -> WooV3RuntimeMode {
        self.requested_mode
    }
    pub fn authoritative(&self) -> &WooV3RuntimeRouteTarget {
        &self.authoritative
    }
}

// ts: WooV3ShadowReadSelection
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WooV3ShadowReadSelection {
    status: &'static str,
    route_id: WooV3MigrationRouteId,
    requested_mode: WooV3RuntimeMode,
    gate: WooV3MigrationGate,
    authoritative: WooV3RuntimeRouteTarget,
    observer: WooV3RuntimeRouteTarget,
    store_authority: &'static str,
    mutation_dispatch: &'static str,
}
impl WooV3ShadowReadSelection {
    pub fn status(&self) -> &str {
        self.status
    }
    pub fn route_id(&self) -> WooV3MigrationRouteId {
        self.route_id
    }
    pub fn gate(&self) -> WooV3MigrationGate {
        self.gate
    }
    pub fn store_authority(&self) -> &str {
        self.store_authority
    }
    pub fn requested_mode(&self) -> WooV3RuntimeMode {
        self.requested_mode
    }
    pub fn authoritative(&self) -> &WooV3RuntimeRouteTarget {
        &self.authoritative
    }
    pub fn observer(&self) -> &WooV3RuntimeRouteTarget {
        &self.observer
    }
    pub fn mutation_dispatch(&self) -> &str {
        self.mutation_dispatch
    }
}

// ts: WooV3V3RouteSelection
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WooV3V3RouteSelection {
    status: &'static str,
    route_id: WooV3MigrationRouteId,
    requested_mode: WooV3RuntimeMode,
    gate: WooV3MigrationGate,
    authoritative: WooV3RuntimeRouteTarget,
    store_authority: &'static str,
}
impl WooV3V3RouteSelection {
    pub fn status(&self) -> &str {
        self.status
    }
    pub fn route_id(&self) -> WooV3MigrationRouteId {
        self.route_id
    }
    pub fn gate(&self) -> WooV3MigrationGate {
        self.gate
    }
    pub fn store_authority(&self) -> &str {
        self.store_authority
    }
    pub fn requested_mode(&self) -> WooV3RuntimeMode {
        self.requested_mode
    }
    pub fn authoritative(&self) -> &WooV3RuntimeRouteTarget {
        &self.authoritative
    }
}

// ts: WooV3AlreadyV3RouteSelection
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WooV3AlreadyV3RouteSelection {
    status: &'static str,
    route_id: WooV3MigrationRouteId,
    requested_mode: WooV3RuntimeMode,
    gate: WooV3MigrationGate,
    authoritative: WooV3RuntimeRouteTarget,
    store_authority: &'static str,
}
impl WooV3AlreadyV3RouteSelection {
    pub fn status(&self) -> &str {
        self.status
    }
    pub fn route_id(&self) -> WooV3MigrationRouteId {
        self.route_id
    }
    pub fn gate(&self) -> WooV3MigrationGate {
        self.gate
    }
    pub fn store_authority(&self) -> &str {
        self.store_authority
    }
    pub fn requested_mode(&self) -> WooV3RuntimeMode {
        self.requested_mode
    }
    pub fn authoritative(&self) -> &WooV3RuntimeRouteTarget {
        &self.authoritative
    }
}

// ts: WooV3BlockedRouteSelection
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WooV3BlockedRouteSelection {
    status: &'static str,
    route_id: WooV3MigrationRouteId,
    requested_mode: WooV3RuntimeMode,
    gate: WooV3MigrationGate,
    blocker: &'static str,
    store_authority: &'static str,
}
impl WooV3BlockedRouteSelection {
    pub fn status(&self) -> &str {
        self.status
    }
    pub fn route_id(&self) -> WooV3MigrationRouteId {
        self.route_id
    }
    pub fn gate(&self) -> WooV3MigrationGate {
        self.gate
    }
    pub fn store_authority(&self) -> &str {
        self.store_authority
    }
    pub fn requested_mode(&self) -> WooV3RuntimeMode {
        self.requested_mode
    }
    pub fn blocker(&self) -> &str {
        self.blocker
    }
}

// ts: WooV3InvalidModeSelection
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WooV3InvalidModeSelection {
    status: &'static str,
    route_id: WooV3MigrationRouteId,
    requested_mode: Value,
    gate: WooV3MigrationGate,
    blocker: &'static str,
    store_authority: &'static str,
}
impl WooV3InvalidModeSelection {
    pub fn status(&self) -> &str {
        self.status
    }
    pub fn route_id(&self) -> WooV3MigrationRouteId {
        self.route_id
    }
    pub fn gate(&self) -> WooV3MigrationGate {
        self.gate
    }
    pub fn store_authority(&self) -> &str {
        self.store_authority
    }
    pub fn requested_mode(&self) -> &Value {
        &self.requested_mode
    }
    pub fn blocker(&self) -> &str {
        self.blocker
    }
}

// ts: WooV3RuntimeRouteSelection
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WooV3RuntimeRouteSelection {
    Legacy(WooV3LegacyRouteSelection),
    ShadowRead(WooV3ShadowReadSelection),
    V3(WooV3V3RouteSelection),
    AlreadyV3(WooV3AlreadyV3RouteSelection),
    Blocked(WooV3BlockedRouteSelection),
    InvalidMode(WooV3InvalidModeSelection),
}

/// Native readonly route plans. The JSON compatibility API delegates here.
pub fn select_woo_v3_runtime_route_native(
    route_id: &str,
    requested_mode: &Value,
) -> Result<WooV3RuntimeRouteSelection, String> {
    // Lookup still precedes mode validation, including unknown-route errors.
    let route = require_route(route_id)?;
    let route_id: WooV3MigrationRouteId =
        serde_json::from_value(json!(route_id)).map_err(|error| error.to_string())?;
    let mode = match requested_mode.as_str() {
        Some("legacy") => WooV3RuntimeMode::Legacy,
        Some("shadow-v3") => WooV3RuntimeMode::ShadowV3,
        Some("v3") => WooV3RuntimeMode::V3,
        _ => {
            return Ok(WooV3RuntimeRouteSelection::InvalidMode(
                WooV3InvalidModeSelection {
                    status: "blocked",
                    route_id,
                    requested_mode: requested_mode.clone(),
                    gate: route.gate,
                    blocker: "invalid-runtime-mode",
                    store_authority: "legacy-only",
                },
            ));
        }
    };
    let blocked = |blocker| {
        WooV3RuntimeRouteSelection::Blocked(WooV3BlockedRouteSelection {
            status: "blocked",
            route_id,
            requested_mode: mode,
            gate: route.gate,
            blocker,
            store_authority: "legacy-only",
        })
    };
    if route.runtime_lifecycle == Some(WooV3MigrationRouteRuntimeLifecycle::Ownerless) {
        return Ok(blocked(if route.id == "cancel-all-after" {
            "cancel-all-after-has-no-runtime-owner"
        } else {
            "route-has-no-runtime-owner"
        }));
    }
    let path = serde_json::to_value(route.v3_path).map_err(|error| error.to_string())?;
    let v3_path = path
        .as_str()
        .ok_or_else(|| "WOO V3 endpoint serialization must be a string".to_owned())?;
    if route.runtime_lifecycle == Some(WooV3MigrationRouteRuntimeLifecycle::AlreadyV3) {
        if mode == WooV3RuntimeMode::ShadowV3 {
            return Ok(blocked("route-is-already-v3"));
        }
        return Ok(WooV3RuntimeRouteSelection::AlreadyV3(
            WooV3AlreadyV3RouteSelection {
                status: "ready",
                route_id,
                requested_mode: mode,
                gate: route.gate,
                authoritative: native_target(
                    WooV3RuntimeGeneration::V3,
                    WooV3HttpMethod::Get,
                    v3_path,
                ),
                store_authority: "existing-v3",
            },
        ));
    }
    if mode == WooV3RuntimeMode::Legacy {
        return Ok(WooV3RuntimeRouteSelection::Legacy(
            WooV3LegacyRouteSelection {
                status: "ready",
                route_id,
                requested_mode: mode,
                gate: route.gate,
                authoritative: native_target(
                    WooV3RuntimeGeneration::Legacy,
                    route.legacy_method,
                    &route.legacy_path,
                ),
                store_authority: "legacy-only",
            },
        ));
    }
    if mode == WooV3RuntimeMode::ShadowV3 {
        if route.legacy_method != WooV3HttpMethod::Get || route.v3_method != WooV3HttpMethod::Get {
            return Ok(blocked("shadow-mode-is-read-only"));
        }
        if !matches!(
            route.gate,
            WooV3MigrationGate::PublicReadParity | WooV3MigrationGate::PrivateReadParity
        ) {
            return Err(format!(
                "WOO V3 read route {} must use a read-parity gate",
                route.id
            ));
        }
        return Ok(WooV3RuntimeRouteSelection::ShadowRead(
            WooV3ShadowReadSelection {
                status: "ready",
                route_id,
                requested_mode: mode,
                gate: route.gate,
                authoritative: native_target(
                    WooV3RuntimeGeneration::Legacy,
                    WooV3HttpMethod::Get,
                    &route.legacy_path,
                ),
                observer: native_target(WooV3RuntimeGeneration::V3, WooV3HttpMethod::Get, v3_path),
                store_authority: "legacy-only",
                mutation_dispatch: "none",
            },
        ));
    }
    let gate_open = WOO_V3_MIGRATION_GATES
        .iter()
        .any(|(gate, open)| *gate == route.gate && *open);
    if !gate_open {
        return Ok(blocked("migration-gate-closed"));
    }
    Ok(WooV3RuntimeRouteSelection::V3(native_v3_selection(
        route_id, route, v3_path,
    )))
}

fn native_v3_selection(
    route_id: WooV3MigrationRouteId,
    route: &WooV3MigrationRoute,
    v3_path: &str,
) -> WooV3V3RouteSelection {
    WooV3V3RouteSelection {
        status: "ready",
        route_id,
        requested_mode: WooV3RuntimeMode::V3,
        gate: route.gate,
        authoritative: native_target(WooV3RuntimeGeneration::V3, route.v3_method, v3_path),
        store_authority: "v3-after-gate",
    }
}

// ts: selectWooV3RuntimeRoute
pub fn select_woo_v3_runtime_route(
    route_id: &str,
    requested_mode: &Value,
) -> Result<Value, String> {
    serde_json::to_value(select_woo_v3_runtime_route_native(
        route_id,
        requested_mode,
    )?)
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod typed_plan_tests {
    use super::*;

    #[test]
    fn native_v3_declaration_without_opening_runtime_gates() {
        // This record is dormant in the selector: all eight real gates stay false.
        // Exercise its real final-branch constructor without exposing any gate override.
        assert_eq!(
            WOO_V3_MIGRATION_GATES.len(),
            8,
            "native fixed gate inventory"
        );
        assert!(WOO_V3_MIGRATION_GATES.iter().all(|(_, open)| !open));
        let route = require_route("place-order").unwrap();
        let p = native_v3_selection(WooV3MigrationRouteId::PlaceOrder, route, "/v3/trade/order");
        assert_eq!(p.status(), "ready", "native dormant v3 status");
        assert_eq!(
            p.route_id(),
            WooV3MigrationRouteId::PlaceOrder,
            "native dormant v3 ID"
        );
        assert_eq!(
            p.requested_mode(),
            WooV3RuntimeMode::V3,
            "native dormant v3 mode"
        );
        assert_eq!(
            p.gate(),
            WooV3MigrationGate::RegularWriteDemoLifecycle,
            "native dormant v3 gate"
        );
        assert_eq!(
            p.store_authority(),
            "v3-after-gate",
            "native dormant v3 authority"
        );
        assert_eq!(
            p.authoritative().generation(),
            WooV3RuntimeGeneration::V3,
            "native dormant v3 generation"
        );
        assert_eq!(
            p.authoritative().method(),
            WooV3HttpMethod::Post,
            "native dormant v3 method"
        );
        assert_eq!(
            p.authoritative().path(),
            "/v3/trade/order",
            "native dormant v3 path"
        );
        let WooV3RuntimeRouteSelection::Blocked(blocked) =
            select_woo_v3_runtime_route_native("place-order", &json!("v3")).unwrap()
        else {
            panic!("runtime gate must remain closed")
        };
        assert_eq!(blocked.blocker(), "migration-gate-closed");
    }
}
