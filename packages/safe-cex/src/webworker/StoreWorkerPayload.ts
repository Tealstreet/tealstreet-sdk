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
  | 'crosstrade:start'
  | 'crosstrade:stop'
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
  | '__worker_ping__';

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
