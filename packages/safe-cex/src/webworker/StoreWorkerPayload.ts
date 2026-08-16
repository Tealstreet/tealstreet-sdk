import type { PackedTickerDelta } from '../store/store.interface';
import type { StoreData, Writable } from '../types';

/** Ops the worker receives from the main thread */
export type StoreWorkerIncomingOp =
  // Lifecycle
  | 'init'
  | 'dispose'
  | 'reinit'
  | 'deactivate'
  | 'sayHi'
  // Store
  | 'store'
  | 'merge-loaded'
  | 'merge-message-handler-stats'
  | 'sendSnapshot'
  | 'settings'
  | 'sync-time'
  // Subscriptions
  | 'subscribeOHLCV'
  | 'unsubscribeOHLCV'
  | 'subscribeOrderBook'
  | 'unsubscribeOrderBook'
  // Trading
  | 'chaser:start'
  | 'chaser:terminate'
  | 'twap:start'
  | 'twap:resume'
  | 'twap:cleanup'
  | 'twap:pause'
  | 'twap:stop'
  | 'twap:state'
  | 'updatePositions'
  | 'callMethod'
  | 'exec'
  // Dev
  | 'dev:simulateWebSocketsReconnect'
  | 'dev:simulateHOWClose'
  | 'dev:forceOpen'
  | 'dev:forceClose'
  // UI
  | 'ui:state'
  // Signing
  | 'sign-res'
  // Liveness
  | '__worker_ping__'
  // Global worker config: host app pushes flag-derived knobs once, before any init.
  // Follows the __verbose/__perf convention (global op, handled before exchange routing).
  | '__workerConfig';

/** Ops the worker sends to the main thread */
export type StoreWorkerOutgoingOp =
  // Store
  | 'store'
  | 'store:delta'
  // Orderbook
  | 'listenOrderBook'
  | 'listenOrderBook:delta'
  // OHLCV
  | 'listenOHLCV'
  // Trading events
  | 'fill'
  | 'updatePositions:done'
  | 'callMethod:done'
  | 'twap:stateUpdate'
  // Lifecycle
  | 'deactivate-status'
  | 'start-status'
  | 'ws-reconnected'
  // Logging
  | 'log'
  | 'error'
  | 'warning'
  | 'exec-log'
  // Reply carrying the cli-core ExecuteResult for the most recent
  // `exec`. Echoes the request's reqId so the browser CLI can
  // match an awaited capture-syntax dispatch. Data shape:
  //   { reqId: string; handled: boolean; success?: boolean; error?: string; data?: { orderIds?: string[] } }
  | 'exec-result'
  | 'grafana-info'
  | 'grafana-error'
  | 'grafana-pws'
  | 'grafana-pws-sub'
  | 'grafana-http-request'
  | 'grafana-http-response'
  // Infrastructure (ExchangeServerWorker only)
  | 'connection-state'
  // Hosted exchange-server server-load telemetry (diagnostic builds only).
  | 'diagnostic:server-health'
  | 'signing_request'
  // Liveness
  | '__worker_pong__';

/** All valid worker op codes */
export type StoreWorkerOp = StoreWorkerIncomingOp | StoreWorkerOutgoingOp;

export type StoreWorkerStorePayload = Partial<Writable<StoreData>> & {
  tickerDeltas?: PackedTickerDelta[];
};

export interface StoreWorkerPayload {
  op: StoreWorkerOp;
  instanceId?: string;
  data?: any;
}

/**
 * Typed global worker config carried by the '__workerConfig' op. The host app derives
 * these knobs from feature flags and pushes them once before any init; safe-cex applies
 * them through typed setters and never sees flag names (keeps the mirrored package
 * flag-agnostic). To add a worker-gated knob: add a field here, map it from a flag on the
 * web side, and apply it via a setter in the '__workerConfig' handler in store.webworker.
 */
export interface WorkerConfig {
  // ENABLE_AWS_PROXY_MIGRATION — false (default) => legacy AWS CORS host; true => read fleet.
  awsProxyMigration?: boolean;
  /**
   * Per-exchange polling slowdown, keyed by exchange name. The break-glass lever
   * for a venue that has started rate-limiting us. Values below 1 are ignored —
   * this may only ever reduce request pressure. See `utils/tickIntervalOverride`.
   */
  tickIntervalMultipliers?: Record<string, number>;
}
