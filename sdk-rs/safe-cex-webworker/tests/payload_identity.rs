use safe_cex_webworker::payload::*;
use serde::Deserialize;
use serde_json::json;

macro_rules! identities {
    ($ty:ident; $($variant:ident => $wire:literal),+ $(,)?) => {{
        $(assert_eq!($ty::deserialize(json!($wire)).unwrap(),$ty::$variant,
            "PAYLOAD-IDENTITY native {} for {}",stringify!($variant),$wire);
        assert_eq!(StoreWorkerOp::deserialize(json!($wire)).unwrap(),StoreWorkerOp::$variant,
            "PAYLOAD-IDENTITY combined native {} for {}",stringify!($variant),$wire);)+
        let names=std::collections::BTreeSet::from([$($wire),+]);
        let source=include_str!("../../../packages/safe-cex/src/webworker/StoreWorkerPayload.ts");
        let source=source.lines().map(|l|l.split("//").next().unwrap()).collect::<Vec<_>>().join("\n");
        let declaration=format!("export type {} =",stringify!($ty));
        let block=source.split(&declaration).nth(1).unwrap().split(';').next().unwrap();
        let actual=block.split('|').map(str::trim).filter(|s|!s.is_empty()).map(|s|{
            s.strip_prefix('\'').and_then(|s|s.strip_suffix('\'')).expect("PAYLOAD-IDENTITY unsupported source union shape")
        }).collect::<std::collections::BTreeSet<_>>();
        assert_eq!(names,actual,"PAYLOAD-IDENTITY full source variant coverage");
    }};
}
#[test]
fn operation_names_select_native_variants_without_reserializing() {
    identities!(StoreWorkerIncomingOp;
        Init=>"init", Dispose=>"dispose", Reinit=>"reinit", Deactivate=>"deactivate",
        SayHi=>"sayHi", Store=>"store", MergeLoaded=>"merge-loaded",
        MergeMessageHandlerStats=>"merge-message-handler-stats", SendSnapshot=>"sendSnapshot",
        Settings=>"settings", SyncTime=>"sync-time", SubscribeOHLCV=>"subscribeOHLCV",
        UnsubscribeOHLCV=>"unsubscribeOHLCV", SubscribeOrderBook=>"subscribeOrderBook",
        UnsubscribeOrderBook=>"unsubscribeOrderBook", ChaserStart=>"chaser:start",
        ChaserTerminate=>"chaser:terminate", ChaserTerminateAll=>"chaser:terminateAll",
        TwapStart=>"twap:start", TwapResume=>"twap:resume", TwapCleanup=>"twap:cleanup",
        TwapPause=>"twap:pause", TwapStop=>"twap:stop", TwapState=>"twap:state",
        UpdatePositions=>"updatePositions", CallMethod=>"callMethod", Exec=>"exec",
        DevSimulateWebSocketsReconnect=>"dev:simulateWebSocketsReconnect",
        DevSimulateHOWClose=>"dev:simulateHOWClose", DevForceOpen=>"dev:forceOpen",
        DevForceClose=>"dev:forceClose", UiState=>"ui:state", SignRes=>"sign-res",
        WorkerPing=>"__worker_ping__", WorkerConfig=>"__workerConfig", PerfFixtureDump=>"__perfFixtureDump"
    );
    identities!(StoreWorkerOutgoingOp;
        Store=>"store", StoreDelta=>"store:delta", ListenOrderBook=>"listenOrderBook",
        ListenOrderBookDelta=>"listenOrderBook:delta", ListenOHLCV=>"listenOHLCV", Fill=>"fill",
        UpdatePositionsDone=>"updatePositions:done", CallMethodDone=>"callMethod:done",
        TwapStateUpdate=>"twap:stateUpdate", ChaserComplete=>"chaser:complete",
        DeactivateStatus=>"deactivate-status", StartStatus=>"start-status", WsReconnected=>"ws-reconnected",
        Log=>"log", Error=>"error", Warning=>"warning", ExecLog=>"exec-log", ExecResult=>"exec-result",
        GrafanaInfo=>"grafana-info", GrafanaError=>"grafana-error", GrafanaPws=>"grafana-pws",
        GrafanaPwsSub=>"grafana-pws-sub", GrafanaHttpRequest=>"grafana-http-request",
        GrafanaHttpResponse=>"grafana-http-response", ConnectionState=>"connection-state",
        SessionLost=>"session-lost", DiagnosticServerHealth=>"diagnostic:server-health",
        SigningRequest=>"signing_request", WorkerPong=>"__worker_pong__"
    );
}
