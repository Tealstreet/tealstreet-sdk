use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreWorkerIncomingOp {
    Init,
    Dispose,
    Reinit,
    Deactivate,
    SayHi,
    Store,
    MergeLoaded,
    MergeMessageHandlerStats,
    SendSnapshot,
    Settings,
    SyncTime,
    SubscribeOHLCV,
    UnsubscribeOHLCV,
    SubscribeOrderBook,
    UnsubscribeOrderBook,
    ChaserStart,
    ChaserTerminate,
    ChaserTerminateAll,
    TwapStart,
    TwapResume,
    TwapCleanup,
    TwapPause,
    TwapStop,
    TwapState,
    UpdatePositions,
    CallMethod,
    Exec,
    DevSimulateWebSocketsReconnect,
    DevSimulateHOWClose,
    DevForceOpen,
    DevForceClose,
    UiState,
    SignRes,
    WorkerPing,
    WorkerConfig,
    PerfFixtureDump,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "StoreWorkerIncomingOp")]
enum StoreWorkerIncomingOpWire {
    #[serde(rename = "init")]
    Init,
    #[serde(rename = "dispose")]
    Dispose,
    #[serde(rename = "reinit")]
    Reinit,
    #[serde(rename = "deactivate")]
    Deactivate,
    #[serde(rename = "sayHi")]
    SayHi,
    #[serde(rename = "store")]
    Store,
    #[serde(rename = "merge-loaded")]
    MergeLoaded,
    #[serde(rename = "merge-message-handler-stats")]
    MergeMessageHandlerStats,
    #[serde(rename = "sendSnapshot")]
    SendSnapshot,
    #[serde(rename = "settings")]
    Settings,
    #[serde(rename = "sync-time")]
    SyncTime,
    #[serde(rename = "subscribeOHLCV")]
    SubscribeOHLCV,
    #[serde(rename = "unsubscribeOHLCV")]
    UnsubscribeOHLCV,
    #[serde(rename = "subscribeOrderBook")]
    SubscribeOrderBook,
    #[serde(rename = "unsubscribeOrderBook")]
    UnsubscribeOrderBook,
    #[serde(rename = "chaser:start")]
    ChaserStart,
    #[serde(rename = "chaser:terminate")]
    ChaserTerminate,
    #[serde(rename = "chaser:terminateAll")]
    ChaserTerminateAll,
    #[serde(rename = "twap:start")]
    TwapStart,
    #[serde(rename = "twap:resume")]
    TwapResume,
    #[serde(rename = "twap:cleanup")]
    TwapCleanup,
    #[serde(rename = "twap:pause")]
    TwapPause,
    #[serde(rename = "twap:stop")]
    TwapStop,
    #[serde(rename = "twap:state")]
    TwapState,
    #[serde(rename = "updatePositions")]
    UpdatePositions,
    #[serde(rename = "callMethod")]
    CallMethod,
    #[serde(rename = "exec")]
    Exec,
    #[serde(rename = "dev:simulateWebSocketsReconnect")]
    DevSimulateWebSocketsReconnect,
    #[serde(rename = "dev:simulateHOWClose")]
    DevSimulateHOWClose,
    #[serde(rename = "dev:forceOpen")]
    DevForceOpen,
    #[serde(rename = "dev:forceClose")]
    DevForceClose,
    #[serde(rename = "ui:state")]
    UiState,
    #[serde(rename = "sign-res")]
    SignRes,
    #[serde(rename = "__worker_ping__")]
    WorkerPing,
    #[serde(rename = "__workerConfig")]
    WorkerConfig,
    #[serde(rename = "__perfFixtureDump")]
    PerfFixtureDump,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreWorkerOutgoingOp {
    Store,
    StoreDelta,
    ListenOrderBook,
    ListenOrderBookDelta,
    ListenOHLCV,
    Fill,
    UpdatePositionsDone,
    CallMethodDone,
    TwapStateUpdate,
    ChaserComplete,
    DeactivateStatus,
    StartStatus,
    WsReconnected,
    Log,
    Error,
    Warning,
    ExecLog,
    ExecResult,
    GrafanaInfo,
    GrafanaError,
    GrafanaPws,
    GrafanaPwsSub,
    GrafanaHttpRequest,
    GrafanaHttpResponse,
    ConnectionState,
    SessionLost,
    DiagnosticServerHealth,
    SigningRequest,
    WorkerPong,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "StoreWorkerOutgoingOp")]
enum StoreWorkerOutgoingOpWire {
    #[serde(rename = "store")]
    Store,
    #[serde(rename = "store:delta")]
    StoreDelta,
    #[serde(rename = "listenOrderBook")]
    ListenOrderBook,
    #[serde(rename = "listenOrderBook:delta")]
    ListenOrderBookDelta,
    #[serde(rename = "listenOHLCV")]
    ListenOHLCV,
    #[serde(rename = "fill")]
    Fill,
    #[serde(rename = "updatePositions:done")]
    UpdatePositionsDone,
    #[serde(rename = "callMethod:done")]
    CallMethodDone,
    #[serde(rename = "twap:stateUpdate")]
    TwapStateUpdate,
    #[serde(rename = "chaser:complete")]
    ChaserComplete,
    #[serde(rename = "deactivate-status")]
    DeactivateStatus,
    #[serde(rename = "start-status")]
    StartStatus,
    #[serde(rename = "ws-reconnected")]
    WsReconnected,
    #[serde(rename = "log")]
    Log,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "exec-log")]
    ExecLog,
    #[serde(rename = "exec-result")]
    ExecResult,
    #[serde(rename = "grafana-info")]
    GrafanaInfo,
    #[serde(rename = "grafana-error")]
    GrafanaError,
    #[serde(rename = "grafana-pws")]
    GrafanaPws,
    #[serde(rename = "grafana-pws-sub")]
    GrafanaPwsSub,
    #[serde(rename = "grafana-http-request")]
    GrafanaHttpRequest,
    #[serde(rename = "grafana-http-response")]
    GrafanaHttpResponse,
    #[serde(rename = "connection-state")]
    ConnectionState,
    #[serde(rename = "session-lost")]
    SessionLost,
    #[serde(rename = "diagnostic:server-health")]
    DiagnosticServerHealth,
    #[serde(rename = "signing_request")]
    SigningRequest,
    #[serde(rename = "__worker_pong__")]
    WorkerPong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreWorkerOp {
    Init,
    Dispose,
    Reinit,
    Deactivate,
    SayHi,
    Store,
    MergeLoaded,
    MergeMessageHandlerStats,
    SendSnapshot,
    Settings,
    SyncTime,
    SubscribeOHLCV,
    UnsubscribeOHLCV,
    SubscribeOrderBook,
    UnsubscribeOrderBook,
    ChaserStart,
    ChaserTerminate,
    ChaserTerminateAll,
    TwapStart,
    TwapResume,
    TwapCleanup,
    TwapPause,
    TwapStop,
    TwapState,
    UpdatePositions,
    CallMethod,
    Exec,
    DevSimulateWebSocketsReconnect,
    DevSimulateHOWClose,
    DevForceOpen,
    DevForceClose,
    UiState,
    SignRes,
    WorkerPing,
    WorkerConfig,
    PerfFixtureDump,
    StoreDelta,
    ListenOrderBook,
    ListenOrderBookDelta,
    ListenOHLCV,
    Fill,
    UpdatePositionsDone,
    CallMethodDone,
    TwapStateUpdate,
    ChaserComplete,
    DeactivateStatus,
    StartStatus,
    WsReconnected,
    Log,
    Error,
    Warning,
    ExecLog,
    ExecResult,
    GrafanaInfo,
    GrafanaError,
    GrafanaPws,
    GrafanaPwsSub,
    GrafanaHttpRequest,
    GrafanaHttpResponse,
    ConnectionState,
    SessionLost,
    DiagnosticServerHealth,
    SigningRequest,
    WorkerPong,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "StoreWorkerOp")]
enum StoreWorkerOpWire {
    #[serde(rename = "init")]
    Init,
    #[serde(rename = "dispose")]
    Dispose,
    #[serde(rename = "reinit")]
    Reinit,
    #[serde(rename = "deactivate")]
    Deactivate,
    #[serde(rename = "sayHi")]
    SayHi,
    #[serde(rename = "store")]
    Store,
    #[serde(rename = "merge-loaded")]
    MergeLoaded,
    #[serde(rename = "merge-message-handler-stats")]
    MergeMessageHandlerStats,
    #[serde(rename = "sendSnapshot")]
    SendSnapshot,
    #[serde(rename = "settings")]
    Settings,
    #[serde(rename = "sync-time")]
    SyncTime,
    #[serde(rename = "subscribeOHLCV")]
    SubscribeOHLCV,
    #[serde(rename = "unsubscribeOHLCV")]
    UnsubscribeOHLCV,
    #[serde(rename = "subscribeOrderBook")]
    SubscribeOrderBook,
    #[serde(rename = "unsubscribeOrderBook")]
    UnsubscribeOrderBook,
    #[serde(rename = "chaser:start")]
    ChaserStart,
    #[serde(rename = "chaser:terminate")]
    ChaserTerminate,
    #[serde(rename = "chaser:terminateAll")]
    ChaserTerminateAll,
    #[serde(rename = "twap:start")]
    TwapStart,
    #[serde(rename = "twap:resume")]
    TwapResume,
    #[serde(rename = "twap:cleanup")]
    TwapCleanup,
    #[serde(rename = "twap:pause")]
    TwapPause,
    #[serde(rename = "twap:stop")]
    TwapStop,
    #[serde(rename = "twap:state")]
    TwapState,
    #[serde(rename = "updatePositions")]
    UpdatePositions,
    #[serde(rename = "callMethod")]
    CallMethod,
    #[serde(rename = "exec")]
    Exec,
    #[serde(rename = "dev:simulateWebSocketsReconnect")]
    DevSimulateWebSocketsReconnect,
    #[serde(rename = "dev:simulateHOWClose")]
    DevSimulateHOWClose,
    #[serde(rename = "dev:forceOpen")]
    DevForceOpen,
    #[serde(rename = "dev:forceClose")]
    DevForceClose,
    #[serde(rename = "ui:state")]
    UiState,
    #[serde(rename = "sign-res")]
    SignRes,
    #[serde(rename = "__worker_ping__")]
    WorkerPing,
    #[serde(rename = "__workerConfig")]
    WorkerConfig,
    #[serde(rename = "__perfFixtureDump")]
    PerfFixtureDump,
    #[serde(rename = "store:delta")]
    StoreDelta,
    #[serde(rename = "listenOrderBook")]
    ListenOrderBook,
    #[serde(rename = "listenOrderBook:delta")]
    ListenOrderBookDelta,
    #[serde(rename = "listenOHLCV")]
    ListenOHLCV,
    #[serde(rename = "fill")]
    Fill,
    #[serde(rename = "updatePositions:done")]
    UpdatePositionsDone,
    #[serde(rename = "callMethod:done")]
    CallMethodDone,
    #[serde(rename = "twap:stateUpdate")]
    TwapStateUpdate,
    #[serde(rename = "chaser:complete")]
    ChaserComplete,
    #[serde(rename = "deactivate-status")]
    DeactivateStatus,
    #[serde(rename = "start-status")]
    StartStatus,
    #[serde(rename = "ws-reconnected")]
    WsReconnected,
    #[serde(rename = "log")]
    Log,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "exec-log")]
    ExecLog,
    #[serde(rename = "exec-result")]
    ExecResult,
    #[serde(rename = "grafana-info")]
    GrafanaInfo,
    #[serde(rename = "grafana-error")]
    GrafanaError,
    #[serde(rename = "grafana-pws")]
    GrafanaPws,
    #[serde(rename = "grafana-pws-sub")]
    GrafanaPwsSub,
    #[serde(rename = "grafana-http-request")]
    GrafanaHttpRequest,
    #[serde(rename = "grafana-http-response")]
    GrafanaHttpResponse,
    #[serde(rename = "connection-state")]
    ConnectionState,
    #[serde(rename = "session-lost")]
    SessionLost,
    #[serde(rename = "diagnostic:server-health")]
    DiagnosticServerHealth,
    #[serde(rename = "signing_request")]
    SigningRequest,
    #[serde(rename = "__worker_pong__")]
    WorkerPong,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreWorkerPayload {
    pub op: StoreWorkerOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_value",
        skip_serializing_if = "Option::is_none"
    )]
    pub data: Option<Value>,
}

#[derive(Deserialize)]
#[serde(remote = "StoreWorkerPayload")]
#[serde(rename_all = "camelCase")]
struct StoreWorkerPayloadWire {
    op: StoreWorkerOp,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    instance_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_value",
        skip_serializing_if = "Option::is_none"
    )]
    data: Option<Value>,
}

impl<'de> Deserialize<'de> for StoreWorkerPayload {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                StoreWorkerPayloadWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

fn present_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    safe_cex_num::numeric_ingress::js_deserialize_passthrough(deserializer).map(Some)
}

pub type StoreWorkerStorePayload = safe_cex_store::StoreUpdate;

impl<'de> Deserialize<'de> for StoreWorkerIncomingOp {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = String::deserialize(deserializer)?;
        StoreWorkerIncomingOpWire::deserialize(serde::de::IntoDeserializer::into_deserializer(wire))
    }
}
impl Serialize for StoreWorkerIncomingOp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        StoreWorkerIncomingOpWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for StoreWorkerOutgoingOp {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = String::deserialize(deserializer)?;
        StoreWorkerOutgoingOpWire::deserialize(serde::de::IntoDeserializer::into_deserializer(wire))
    }
}
impl Serialize for StoreWorkerOutgoingOp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        StoreWorkerOutgoingOpWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for StoreWorkerOp {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = String::deserialize(deserializer)?;
        StoreWorkerOpWire::deserialize(serde::de::IntoDeserializer::into_deserializer(wire))
    }
}
impl Serialize for StoreWorkerOp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        StoreWorkerOpWire::serialize(self, serializer)
    }
}

#[cfg(test)]
#[path = "../tests/support/literal_inventory.rs"]
mod literal_inventory;

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for StoreWorkerPayload {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { op, instance_id } passthrough { data });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for StoreWorkerOp {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Init, Dispose, Reinit, Deactivate, SayHi, Store, MergeLoaded, MergeMessageHandlerStats, SendSnapshot, Settings, SyncTime, SubscribeOHLCV, UnsubscribeOHLCV, SubscribeOrderBook, UnsubscribeOrderBook, ChaserStart, ChaserTerminate, ChaserTerminateAll, TwapStart, TwapResume, TwapCleanup, TwapPause, TwapStop, TwapState, UpdatePositions, CallMethod, Exec, DevSimulateWebSocketsReconnect, DevSimulateHOWClose, DevForceOpen, DevForceClose, UiState, SignRes, WorkerPing, WorkerConfig, PerfFixtureDump, StoreDelta, ListenOrderBook, ListenOrderBookDelta, ListenOHLCV, Fill, UpdatePositionsDone, CallMethodDone, TwapStateUpdate, ChaserComplete, DeactivateStatus, StartStatus, WsReconnected, Log, Error, Warning, ExecLog, ExecResult, GrafanaInfo, GrafanaError, GrafanaPws, GrafanaPwsSub, GrafanaHttpRequest, GrafanaHttpResponse, ConnectionState, SessionLost, DiagnosticServerHealth, SigningRequest, WorkerPong });
    }
}
