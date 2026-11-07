import type { CommandProgress } from '@tealstreet/safe-cex-wasm/host';
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
  | 'forwardFrames'
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
  | 'chaser:terminateAll'
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
  | '__workerConfig'
  // Dev-only: ask the worker to hand back its recorded perf fixture.
  | '__perfFixtureDump';

/** Ops the worker sends to the main thread */
export type StoreWorkerOutgoingOp =
  // Store
  | 'store'
  | 'store:delta'
  // One Rust engine flush frame (an ArrayBuffer), see store/frameApplier
  | 'frame'
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
  | 'chaser:complete'
  // Native order attribution, independent of RPC settlement and store frames.
  | 'command-progress'
  // Lifecycle
  | 'deactivate-status'
  | 'start-status'
  | 'ws-reconnected'
  // Structured Rust event; data is [EngineEvent].
  | 'engine-event'
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
  | 'session-lost'
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
  /**
   * Per-exchange polling slowdown, keyed by exchange name. The break-glass lever
   * for a venue that has started rate-limiting us. Values below 1 are ignored —
   * this may only ever reduce request pressure. See `utils/tickIntervalOverride`.
   */
  tickIntervalMultipliers?: Record<string, number>;
  /**
   * Dev-only benchmarking harness. Absent in production. See `perfFixture.ts`.
   */
  perfFixture?: PerfFixtureConfig;
  /**
   * Runtime gate for raw WebSocket diagnostics. Kept flat because this type is
   * mirrored through the public SDK surface and should stay narrow.
   */
  wsDiagnosticsEnabled?: boolean;
  /**
   * The main thread applies Rust engine frames (`op: 'frame'`) itself, so
   * accounts on the engine send frames instead of venue data as objects.
   */
  acceptsFrames?: boolean;
}

/** One captured outbound worker message, stamped relative to recording start. */
export interface PerfFixtureFrame {
  atMs: number;
  msg: any;
}

export interface PerfFixture {
  version: 1;
  recordedAtMs: number;
  durationMs: number;
  frames: PerfFixtureFrame[];
}

export interface PerfFixtureConfig {
  mode: 'off' | 'record' | 'replay';
  /** Required for `replay`. */
  fixture?: PerfFixture;
  /** Fixed gap between replayed frames. Defaults to 50ms (20 frames/sec). */
  intervalMs?: number;
  /** Restart from frame 0 at the end. Defaults to true. */
  loop?: boolean;
}

/** Receipt ownership is the original native host session and worker generation. */
export type StoreWorkerCommandProgress = CommandProgress & {
  account: string;
  generation: number;
};
