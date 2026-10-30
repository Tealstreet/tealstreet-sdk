import type { BaseExchange } from './base';

import { RingBuffer } from '@tealstreet/data-structures/src/RingBuffer';
import { RelayWebSocket } from '@tealstreet/http-over-websocket/src/RelayWebSocket';
import { getTransport } from '../transport/registry';
import { Options, ReconnectingWebSocket } from '@tealstreet/reconnecting-websocket/src/ReconnectingWebsocket';
import { sleep } from '@tealstreet/sleep/src/sleep';
import {
  ENABLE_RECONNECTING_WEBSOCKET,
  ENABLE_WS_LIFECYCLE_DEBUG_LOGS,
  RECONNECTING_WEBSOCKET_TUNING,
  WS_BACKOFF_BASE_DELAY,
  WS_BACKOFF_FACTOR,
  WS_BACKOFF_MAX_DELAY,
  WS_FAILURE_STREAK_PAUSE_MS,
  WS_FAILURE_STREAK_PAUSE_THRESHOLD,
  WS_MIN_RECONNECTION_DELAY,
} from '../config/config';
import { LogSeverity } from '../types';
import {
  isHeartbeatFrame,
  isWsDiagnosticsEnabled,
  redactWsDiagnosticPayload,
  stringifyWsDiagnosticPayload,
  WsDiagnosticSampler,
} from '../utils/wsDiagnostics';

export { isHeartbeatFrame };
import { MessageHandlers, MessageHandlersStats } from './types';

// Per exchange: the last re-open that triggered a private-state resync, and its socket.
const lastReopenResync = new WeakMap<object, { at: number; socket: object }>();
// Per exchange: a resync still running, and whether a re-open arrived during it.
// A socket flapping faster than a resync's retries (a rejected key over a relay)
// must not stack loops of signed requests, yet a re-open mid-run may have missed
// more than that run's snapshot saw, so it earns exactly one follow-up run.
const reopenResyncInFlight = new WeakMap<object, { rerun: boolean }>();
const REOPEN_RESYNC_COALESCE_MS = 2_000;

const runReopenResync = (parent: BaseExchange) => {
  const state = { rerun: false };
  reopenResyncInFlight.set(parent, state);
  let resync: Promise<void>;
  try {
    resync = Promise.resolve(parent.fetchAndSetPrivateSnapshot(10));
  } catch (error) {
    resync = Promise.reject(error);
  }
  void resync
    .catch((error: unknown) => parent.log(`Private resync after re-open failed: ${String(error)}`, LogSeverity.Warning))
    .finally(() => {
      reopenResyncInFlight.delete(parent);
      if (state.rerun && !parent.isDisposed) runReopenResync(parent);
    });
};

type FreshHeadersProvider = () => Record<string, string>;
type WebSocketConnectionOptions = Omit<Options, 'headers'> & {
  headers?: Record<string, string> | FreshHeadersProvider;
  /**
   * `true` only when the socket cannot authenticate without upgrade headers.
   * Static routing and browser-identification headers remain best effort so
   * browser/native WebSocket fallback keeps working.
   */
  headersRequired?: boolean;
};

/**
 * Debug helper for BaseWebSocket lifecycle logs. Suppressed by default to keep
 * DevTools usable during reconnect storms; flip ENABLE_WS_LIFECYCLE_DEBUG_LOGS
 * (or NEXT_PUBLIC_WS_LIFECYCLE_DEBUG=1) when actively debugging.
 */
const lifecycleLog = (...args: unknown[]): void => {
  if (ENABLE_WS_LIFECYCLE_DEBUG_LOGS) {
    console.log(...args);
  }
};

// How often the whole stats map is re-read against `messageHandlers`. Cheap at
// this cadence (~600 keys on bybit linear) and off the per-frame path.
const STATS_RECONCILE_INTERVAL_MS = 5000;
// Floor for the forced walk. `cleanupMessageHandlerStats` has ~69 call sites and
// several fire in a single teardown, so closing many panels at once would
// otherwise be O(panels x topics).
const STATS_RECONCILE_FORCE_MIN_MS = 250;

type ModuleWithCreateRequire = {
  createRequire?: (url: string | URL) => (id: string) => unknown;
};

export interface WsDiagnosticReceiveOptions {
  allowChannel?: (json: unknown) => boolean;
  format?: (data: unknown) => string | null | undefined;
  redact?: (message: string) => string | null | undefined;
}

export interface WsDiagnosticSendOptions {
  format?: (redactedMessage: string) => string | null | undefined;
  redact?: (message: string) => string | null | undefined;
}

const getNodeWebSocket = (): typeof WebSocket | undefined => {
  if (typeof process === 'undefined' || !(process.versions?.node || (process as any).isBun)) return undefined;

  // Node 22 ESM has no lexical `require`. `getBuiltinModule` is synchronous,
  // does not add a browser bundle import, and lets the ESM artifact resolve
  // the installed `ws` package. Keep the CJS/Bun path for those runtimes.
  try {
    const module = (
      process as typeof process & {
        getBuiltinModule?: (name: string) => ModuleWithCreateRequire | undefined;
      }
    ).getBuiltinModule?.('module');
    if (module?.createRequire) {
      return module.createRequire(import.meta.url)('ws') as typeof WebSocket;
    }
  } catch {
    // Try the CommonJS/Bun fallback below.
  }

  try {
    if (typeof require === 'function') {
      return require('ws') as typeof WebSocket;
    }
  } catch {
    // A Node-marked renderer may expose neither resolver nor `ws`.
  }

  return undefined;
};

/**
 * Raised instead of opening a native WebSocket that would silently drop
 * required HTTP upgrade headers.
 */
export class HeaderCapableWebSocketUnavailableError extends Error {
  constructor() {
    super('Required WebSocket upgrade headers need a header-capable ws transport');
    this.name = 'HeaderCapableWebSocketUnavailableError';
  }
}

// WebSocket health check breakpoints
export const WS_PING_THRESHOLD_MS = 6_000; // Time after which to send a ping
export const WS_RECONNECT_THRESHOLD_MS = 20_000; // Time after which to reconnect/close
export const WS_CLOSE_LOG_COOLDOWN_MS = 15_000; // Dedupe repeated close logs in reconnect storms
export const WS_ERROR_LOG_COOLDOWN_MS = 15_000; // Dedupe repeated websocket error logs

// WebSocket close code descriptions
// See: https://developer.mozilla.org/en-US/docs/Web/API/CloseEvent/code
const WS_CLOSE_CODE_DESCRIPTIONS: Record<number, string> = {
  1000: 'Normal closure',
  1001: 'Going away (server shutdown or browser navigating away)',
  1002: 'Protocol error',
  1003: 'Unsupported data type',
  1005: 'No status code received',
  1006: 'Abnormal closure (connection lost)',
  1007: 'Invalid frame payload data',
  1008: 'Policy violation',
  1009: 'Message too big',
  1010: 'Missing extension',
  1011: 'Internal server error',
  1012: 'Service restart',
  1013: 'Try again later',
  1014: 'Bad gateway',
  1015: 'TLS handshake failure',
};

/**
 * Get a human-readable description for a WebSocket close code
 */
export function getCloseCodeDescription(code: number): string {
  return WS_CLOSE_CODE_DESCRIPTIONS[code] || `Unknown close code (${code})`;
}


/*
 * ws onOpen must contain
 *
 * public sockets -> this.emitWsReconnected();
 * private sockets -> this.parent.fetchAndSetPrivateSnapshot(10);
 */

export class BaseWebSocket<T extends BaseExchange> {
  ws?: ReconnectingWebSocket | WebSocket;
  parent: T;
  lastCloseTime = 0;
  onCloseCount = 0;
  reconnectCount = 0;
  /**
   * Monotonic close counter — deliberately NOT reset by the healthy-traffic
   * path that clears `onCloseCount`/`reconnectCount`. Those two are
   * streak-scoped so their `> 100` caps bound one unhealthy streak rather than
   * the socket's lifetime; that makes them read 0 for any observer sampling
   * after recovery. Diagnostics need the durable count, so it lives separately.
   */
  lifetimeCloseCount = 0;
  /** Last close code seen, and when. Survives reconnect for post-hoc reads. */
  lastCloseCode: number | undefined;
  lastCloseAt = 0;
  lastReceived = 0;
  /** Timestamp of the most recent successful open event for this socket. */
  lastConnectedAt = 0;
  lastReceivedTimer: NodeJS.Timeout | undefined;
  pingSent = false;
  /** Track if this WebSocket has ever successfully connected and received messages */
  isInitialized = false;

  constructor(parent: T) {
    this.parent = parent;
  }

  get isDisposed() {
    return this.parent.isDisposed;
  }

  get isRecoveryComplete() {
    return true;
  }

  get store() {
    return this.parent.store;
  }

  get emitter() {
    return this.parent.emitter;
  }

  orderBookInnerState: {
    [key: string]: {
      updates: any[];
      isSnapshotLoaded: boolean;
    };
  } = {};

  resetOrderbook = () => {
    Object.keys(this.orderBookInnerState).forEach((hash) => {
      this.orderBookInnerState[hash].updates = [];
      this.orderBookInnerState[hash].isSnapshotLoaded = false;
    });
  };

  pingAt = 0;
  pingTimeoutId?: NodeJS.Timeout;
  pongMissTimeoutId?: NodeJS.Timeout;

  messageHandlers: MessageHandlers = {};
  messageHandlersStats: MessageHandlersStats = {};
  private lastStatsReconcileAt = 0;
  reconnecting = false;
  /** The native path's pending reconnect, so a recovery can replace it. */
  private reconnectTimer: ReturnType<typeof setTimeout> | undefined;
  private reconnectBackoffAttempts = 0;
  private reconnectBackoffCloseCode: number | undefined;
  private reconnectBackoffLastFailureAt = 0;
  private consecutiveCloseFailures = 0;
  private reconnectPauseUntil = 0;
  private reconnectPauseAnnounced = false;
  private closeLogSignature: string | null = null;
  private closeLogLastAt = 0;
  private closeLogSuppressedCount = 0;
  private wsErrorLogSignature: string | null = null;
  private wsErrorLogLastAt = 0;
  private wsErrorLogSuppressedCount = 0;
  private wsDiagnosticSampler = new WsDiagnosticSampler();

  /**
   * When true, the pong-miss reconnect path is suppressed: any inbound message
   * clears an already-armed miss timer, and `handlePongEvent` skips arming a new
   * one. Pings continue on their normal cadence — the flag ONLY affects the
   * "no pong after N ms → reconnect" watchdog. Use for exchanges whose upstream
   * doesn't reliably echo our pongs back but does send steady data/ping traffic.
   * The 20s `checkLastReceived` watchdog remains the backstop for truly dead sockets.
   */
  protected treatAnyMessageAsPong = false;

  ping = (): void => {
    throw new Error('Not implemented');
  };

  protected shouldEnforcePongMissReconnect = (): boolean => {
    // Opt-out entirely for exchanges whose upstream doesn't reliably pong.
    if (this.treatAnyMessageAsPong) {
      return false;
    }
    // If we recently received any message, the connection is alive —
    // no need to reconnect just because pong was delayed.
    // trackLastReceived (20s threshold) already handles truly dead connections.
    if (this.lastReceived > 0 && Date.now() - this.lastReceived < WS_RECONNECT_THRESHOLD_MS) {
      return false;
    }
    return true;
  };

  /**
   * Allows exchange-specific sockets to opt into a custom watchdog stale threshold.
   * Return undefined to use BaseExchange default threshold.
   */
  getWatchdogStaleThresholdMs = (): number | undefined => undefined;

  /**
   * Allows exchange-specific sockets to opt into a custom idle ping threshold.
   * Return undefined to use the BaseWebSocket default threshold.
   */
  getWatchdogPingThresholdMs = (): number | undefined => undefined;

  handlePongEvent = (nextPingTime = 5_000) => {
    this.recordMessageHandlerStats('pong');
    if (this.pingAt) {
      const diff = performance.now() - this.pingAt;
      this.store.update({ latency: Math.round(diff / 2) });
    }

    if (this.pingTimeoutId) {
      clearTimeout(this.pingTimeoutId);
      this.pingTimeoutId = undefined;
    }

    this.pingTimeoutId = setTimeout(() => this.ping(), nextPingTime);

    if (this.pongMissTimeoutId) {
      clearTimeout(this.pongMissTimeoutId);
      this.pongMissTimeoutId = undefined;
    }

    if (!this.shouldEnforcePongMissReconnect()) {
      return;
    }

    this.pongMissTimeoutId = setTimeout(() => {
      const msg = `WebSocket pong miss after ${1.5 * nextPingTime}ms, reconnecting...`;
      // Already routed to grafana-error for durable observability — only emit to console
      // when the lifecycle debug flag is on so reconnect storms don't flood DevTools.
      lifecycleLog(`${this.parent.name} ${msg}`);
      this.parent.emitter.emit('grafana-error', msg);
      this.reconnect();
    }, 1.5 * nextPingTime);
  };

  // reconnecting banner stats
  protected recordMessageHandlerStats(topic: string) {
    // bybit only
    if (topic.includes('publicTrade') && !topic.match(/BTCUSDT|ETHUSDT/)) {
      return;
    }

    // Hot path: only touch the current topic. Mutate an existing entry rather
    // than allocating a fresh object per frame. The full-map walk this replaced
    // ran on every WS frame (N×N with subscription count); the map-wide
    // refresh it provided now happens in `reconcileMessageHandlerStats` at
    // `STATS_RECONCILE_INTERVAL_MS`, which is what keeps torn-down topics from
    // being reported as live subscriptions.
    const now = Date.now();
    const handlers = this.messageHandlers[topic];
    const hasHandlers = Boolean(handlers && handlers.length);
    const callbacksCount = handlers ? handlers.length : 1;
    const existing = this.messageHandlersStats[topic];
    if (existing) {
      existing.lastMessageTimestamp = now;
      existing.lastCallbacksCount = callbacksCount;
      if (hasHandlers) existing.hadHandlers = true;
    } else {
      this.messageHandlersStats[topic] = {
        lastMessageTimestamp: now,
        lastCallbacksCount: callbacksCount,
        hadHandlers: hasHandlers,
      };
    }

    this.reconcileMessageHandlerStats(now);
  }

  /**
   * Re-reads handler counts for the whole stats map.
   *
   * `recordMessageHandlerStats` only ever refreshes the topic that just
   * received a frame, so a topic whose subscription is torn down keeps its last
   * non-zero `lastCallbacksCount` forever and goes on being reported as a live
   * feed by `getGroupedMessageTopicStats` until it ages past its staleness
   * threshold and raises a warning nobody can act on. Per-exchange
   * `cleanupMessageHandlerStats` calls were meant to cover that and are
   * inconsistent across venues, so this walk is the general guard.
   *
   * Only `hadHandlers` entries are touched. Topics that never had a
   * `messageHandlers` key (`pong`, bare `ticker`) legitimately have no handler
   * count and must keep the `: 1` presumption, or the socket's only liveness
   * signals would be silently dropped.
   */
  private reconcileMessageHandlerStats(now: number, force = false) {
    const minInterval = force ? STATS_RECONCILE_FORCE_MIN_MS : STATS_RECONCILE_INTERVAL_MS;
    if (now - this.lastStatsReconcileAt < minInterval) return;
    this.lastStatsReconcileAt = now;

    for (const key in this.messageHandlersStats) {
      const entry = this.messageHandlersStats[key];
      if (!entry.hadHandlers) continue;
      const handlers = this.messageHandlers[key];
      entry.lastCallbacksCount = handlers ? handlers.length : 0;
    }
  }

  protected cleanupMessageHandlerStats(topic: string) {
    delete this.messageHandlersStats[topic];
    // An exchange that unsubscribes one topic has usually just dropped handlers
    // for others in the same teardown, and its socket may now be quiet enough
    // that no frame arrives to trigger the throttled walk.
    this.reconcileMessageHandlerStats(Date.now(), true);
  }

  private parseWsDiagnosticJson(data: unknown): unknown {
    if (typeof data !== 'string') return data;
    try {
      return JSON.parse(data);
    } catch {
      return undefined;
    }
  }

  private shouldDropWsDiagnostic(): boolean {
    return this.wsDiagnosticSampler.drop();
  }

  private emitWsDiagnosticEvent(eventName: 'grafana-pws' | 'grafana-pws-sub', message: string): void {
    this.wsDiagnosticSampler.emit((text) => this.parent.emitter.emit(eventName, text), this.parent.name, message);
  }

  protected emitWsDiagnosticReceive(data: unknown, options: WsDiagnosticReceiveOptions = {}): void {
    if (!isWsDiagnosticsEnabled()) return;

    const rawMessage = stringifyWsDiagnosticPayload(data);
    if (isHeartbeatFrame(rawMessage)) return;

    if (options.allowChannel) {
      let allowed = false;
      try {
        allowed = options.allowChannel(this.parseWsDiagnosticJson(data));
      } catch {
        allowed = false;
      }
      if (!allowed) return;
    }

    if (this.shouldDropWsDiagnostic()) return;

    const formatted = options.format ? options.format(data) : rawMessage;
    if (!formatted) return;

    const redacted = redactWsDiagnosticPayload(formatted);
    const exchangeRedacted = options.redact?.(redacted) ?? redacted;
    if (!exchangeRedacted) return;

    this.emitWsDiagnosticEvent('grafana-pws', exchangeRedacted);
  }

  protected emitWsDiagnosticSend(data: string, options: WsDiagnosticSendOptions = {}): void {
    if (!isWsDiagnosticsEnabled()) return;
    if (isHeartbeatFrame(data)) return;
    if (this.shouldDropWsDiagnostic()) return;

    const redacted = redactWsDiagnosticPayload(data);
    const formatted = options.format ? options.format(redacted) : redacted;
    if (!formatted) return;

    const exchangeRedacted = options.redact?.(formatted) ?? formatted;
    if (!exchangeRedacted) return;

    this.emitWsDiagnosticEvent('grafana-pws-sub', exchangeRedacted);
  }

  sendWithLog = (data: string) => {
    this.send(data);
    this.emitWsDiagnosticSend(data);
  };

  emitAndLogError(err: string | Error, msg?: string, extraData?: Record<string, any>) {
    return this.parent.emitAndLogError(err, msg, extraData);
  }

  connectAndSubscribe = (_isReconnect = false): void => {
    throw new Error('Not implemented');
  };

  onOpen = (): void => {
    throw new Error('Not implemented');
  };

  onMessage = (_event: MessageEvent): void => {
    throw new Error('Not implemented');
  };

  emitWsReconnected = () => {
    this.parent.emitWsReconnected();
  };

  lastLog = new RingBuffer<string>(20);
  logLastLog = (message: string) => {
    try {
      this.lastLog.add(message);
    } catch (e) {
      console.error('Error adding log to ring buffer from websocket', e);
    }
  };

  createWebsocket(
    url: string,
    protocols?: string | string[],
    options: WebSocketConnectionOptions = RECONNECTING_WEBSOCKET_TUNING,
  ) {
    let ws: ReconnectingWebSocket | WebSocket;
    const NodeWebSocket = getNodeWebSocket();
    const { headersRequired = false, ...reconnectingOptions } = options;
    if (headersRequired && !NodeWebSocket) {
      // Native/browser constructors do not support HTTP upgrade headers. Do
      // not silently open a socket that cannot carry required authentication.
      throw new HeaderCapableWebSocketUnavailableError();
    }
    const connection = getTransport(this.parent.options).wsConnection(url);
    const withConnectionHeaders = (extra: Record<string, string>) => {
      const headers = reconnectingOptions.headers;
      return typeof headers === 'function' ? () => ({ ...headers(), ...extra }) : { ...headers, ...extra };
    };
    if (connection?.kind === 'relay') {
      // Tunnel rpc.relay: RelayWebSocket owns the rpc.auth/rpc.relay preface and
      // the inner transport (`ws` in Node with the transport's headers, native
      // WebSocket in browsers).
      ws = new ReconnectingWebSocket(url, protocols, {
        ...reconnectingOptions,
        WebSocket: RelayWebSocket,
        headers: withConnectionHeaders(connection.headers),
      } as Options);
    } else if (connection?.kind === 'headers') {
      ws = new ReconnectingWebSocket(url, protocols, {
        ...reconnectingOptions,
        ...(NodeWebSocket ? { WebSocket: NodeWebSocket } : {}),
        // These routing headers are best effort: browser-native sockets cannot
        // attach them, and must retain the existing fallback.
        headers: withConnectionHeaders(connection.headers),
      } as Options);
    } else if (NodeWebSocket && reconnectingOptions.headers) {
      // Native/browser WebSocket constructors cannot attach upgrade headers.
      // Header-authenticated exchange sockets therefore use the vendored RWS
      // transport in Node/Bun, which also evaluates header providers per retry.
      ws = new ReconnectingWebSocket(url, protocols, {
        ...reconnectingOptions,
        WebSocket: NodeWebSocket,
      } as Options);
    } else if (ENABLE_RECONNECTING_WEBSOCKET) {
      ws = new ReconnectingWebSocket(url, protocols, reconnectingOptions as Options);
    } else {
      ws = new WebSocket(url, protocols);
    }

    // Add error handler immediately to prevent uncaught errors
    ws.addEventListener('error', this.handleWebSocketError as any);
    ws.addEventListener('open', this.trackConnected);

    // Also override onerror to catch any errors that might bypass addEventListener
    ws.onerror = this.handleWebSocketError as any;

    return ws;
  }

  private trackConnected = () => {
    if (this.parent.isDisposed) return;
    this.lastConnectedAt = Date.now();
    const reopened = this.hasOpenedBefore;
    this.hasOpenedBefore = true;
    if (reopened) this.resyncPrivateStateAfterReopen();
  };

  private hasOpenedBefore = false;

  /**
   * Whether a RE-open of this socket means private state may have moved while it
   * was down. The exchange's private socket by default; a socket carrying private
   * data under another name opts in, and one that resyncs only after its own
   * acknowledged auth opts out.
   */
  protected get resyncsPrivateStateOnReopen(): boolean {
    return this.parent.getPrivateSockets().includes(this);
  }

  // Runs from the open listener every socket gets in createWebsocket, so it covers
  // the native, library, relay and header paths and sockets created after an await.
  private resyncPrivateStateAfterReopen() {
    const { options } = this.parent;
    if (this.forceClosed || options.publicDataOnly || !options.key || !this.resyncsPrivateStateOnReopen) return;
    const now = Date.now();
    // Sibling sockets that drop together (OKX private + business) resync once;
    // the same socket dropping again still resyncs, since it may have missed more.
    const last = lastReopenResync.get(this.parent);
    if (last && last.socket !== this && now - last.at < REOPEN_RESYNC_COALESCE_MS) return;
    lastReopenResync.set(this.parent, { at: now, socket: this });
    const running = reopenResyncInFlight.get(this.parent);
    if (running) {
      running.rerun = true;
      return;
    }
    runReopenResync(this.parent);
  }

  protected handleWebSocketError = (event: Event) => {
    // Prevent the error from propagating
    event.preventDefault?.();
    event.stopPropagation?.();
    event.stopImmediatePropagation?.();

    // A socket WE closed is not a failure. Tearing the exchange down while a
    // handshake is still in flight makes the runtime fire `error` with
    // "closed before the connection is established", which a user reads as a
    // broken connection right after a command printed its result.
    if (this.parent.isDisposed) return;

    // Extract error message if available
    const errorMessage = (event as any)?.message || (event as any)?.error?.message || (event as any)?.type;

    // Log WebSocket errors (state errors are now prevented in send() method).
    // Dedupe repeated identical error messages during reconnect storms.
    const dedupedErrorMessage = this.getWebSocketErrorLogMessage(errorMessage || 'Unknown error');
    if (dedupedErrorMessage) {
      this.parent.log(`WebSocket error: ${dedupedErrorMessage}`, LogSeverity.Error);
    }
    return false;
  };

  send = (message: string) => {
    // Disposal guard: a disposed exchange must never send frames or trigger a
    // reconnect. Without this, a queued auth/subscribe/ping after dispose hits
    // the "not OPEN -> reconnect()" branch below, which re-arms the
    // ReconnectingWebSocket (_shouldReconnect=true) and resurrects the socket —
    // its onOpen re-authenticates and the private snapshot re-signs forever
    // against a dead remote signer. See base.ts fetchAndSetPrivateSnapshot.
    if (this.parent.isDisposed) return;

    // Pre-check WebSocket state to avoid the opaque DOMException
    if (this.ws?.readyState !== WebSocket.OPEN) {
      const wsState = this.ws?.readyState;
      const stateNames = ['CONNECTING', 'OPEN', 'CLOSING', 'CLOSED'];
      const stateName = wsState !== undefined ? stateNames[wsState] : 'undefined';

      if (isHeartbeatFrame(message)) {
        // Silently ignore ping/pong when WebSocket is not OPEN
        this.parent.log(`Skipping ping/pong - WebSocket state: ${stateName} (${wsState})`, LogSeverity.Debug);
        return;
      }

      // Trigger reconnection when WebSocket is not ready
      this.parent.log(
        `WebSocket not ready for send (state: ${stateName}, stateCode: ${wsState}, messageType: ${typeof message}, messageLength: ${message.length}). Triggering reconnection.`,
        LogSeverity.Warning,
      );

      // Don't reconnect if already reconnecting
      if (!this.reconnecting) {
        this.reconnect();
      }

      // Don't throw — reconnection is already triggered above.
      // Throwing here crashes Node.js server processes with unhandled exceptions.
      return;
    }

    // Safe to call send - state is OPEN
    this.ws.send(message);
  };

  get isConnected() {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  /**
   * Returns true if the WebSocket is healthy:
   * - Connected (readyState === OPEN)
   * - Has received messages recently (within threshold)
   *
   * Uses getWatchdogStaleThresholdMs() when available so that
   * event-driven sockets (e.g. GRVT private WS) can declare a
   * longer acceptable silence window.
   */
  get isHealthy(): boolean {
    if (!this.isConnected) return false;
    if (this.lastReceived === 0) return false;
    const timeSinceLastMessage = Date.now() - this.lastReceived;
    const threshold = this.getWatchdogStaleThresholdMs() ?? WS_RECONNECT_THRESHOLD_MS;
    return timeSinceLastMessage < threshold;
  }

  /**
   * Returns the timestamp of the last received message, or undefined if none
   */
  get lastMessageTimestamp(): number | undefined {
    return this.lastReceived > 0 ? this.lastReceived : undefined;
  }

  /**
   * Handler for close events - accepts CloseEvent when called as event listener
   * or can be called without arguments for manual reconnection
   * Uses 'any' type to maintain compatibility with existing addEventListener calls
   */

  protected onClose = (event?: any): void => {
    // Extract close code and reason if event is a CloseEvent
    let closeCode: number | undefined;
    let closeReason = '';

    if (event && typeof event === 'object' && 'code' in event) {
      closeCode = event.code;
      closeReason = event.reason || '';
    }

    const closeCodeDescription = closeCode !== undefined ? getCloseCodeDescription(closeCode) : 'Unknown';

    lifecycleLog(`[BaseWebSocket] 🔌 ${this.parent.name} onClose() called`, {
      wsType: this.ws instanceof ReconnectingWebSocket ? 'ReconnectingWebSocket' : 'Native WebSocket',
      isDisposed: this.parent.isDisposed,
      reconnecting: this.reconnecting,
      onCloseCount: this.onCloseCount,
      closeCode,
      closeReason,
      closeCodeDescription,
    });

    this.processClose(closeCode, closeReason, closeCodeDescription);
  };

  private resetReconnectBackoff = () => {
    this.reconnectBackoffAttempts = 0;
    this.reconnectBackoffCloseCode = undefined;
    this.reconnectBackoffLastFailureAt = 0;
  };

  private resetFailureStreak = () => {
    this.connectFailureAnnounced = false;
    this.consecutiveCloseFailures = 0;
    this.reconnectPauseUntil = 0;
    this.reconnectPauseAnnounced = false;
  };

  private registerCloseResult = (closeCode?: number) => {
    if (closeCode === 1000) {
      this.resetFailureStreak();
      return;
    }
    if (closeCode === undefined) {
      return;
    }

    this.consecutiveCloseFailures += 1;

    if (this.consecutiveCloseFailures >= WS_FAILURE_STREAK_PAUSE_THRESHOLD) {
      // Extend pause on every failure once threshold is crossed.
      this.reconnectPauseUntil = Date.now() + WS_FAILURE_STREAK_PAUSE_MS;
    }
  };

  private getReconnectPauseRemainingMs = () => Math.max(0, this.reconnectPauseUntil - Date.now());

  get reconnectPauseRemainingMs(): number {
    return this.getReconnectPauseRemainingMs();
  }

  get isReconnectPauseActive(): boolean {
    return this.getReconnectPauseRemainingMs() > 0;
  }

  private getReconnectDelay = (closeCode?: number) => {
    const now = Date.now();
    const hasCode = typeof closeCode === 'number';
    const isRetriableFailure = hasCode && closeCode !== 1000;
    const staleFailureWindow = now - this.reconnectBackoffLastFailureAt > WS_BACKOFF_MAX_DELAY * 2;
    const sameCloseCode = this.reconnectBackoffCloseCode === closeCode;

    if (!isRetriableFailure) {
      this.resetReconnectBackoff();
      return WS_MIN_RECONNECTION_DELAY;
    }

    if (this.reconnectBackoffAttempts === 0 || staleFailureWindow || !sameCloseCode) {
      this.reconnectBackoffAttempts = 1;
    } else {
      this.reconnectBackoffAttempts += 1;
    }

    this.reconnectBackoffCloseCode = closeCode;
    this.reconnectBackoffLastFailureAt = now;

    const exponentialDelay = Math.min(
      WS_BACKOFF_MAX_DELAY,
      WS_BACKOFF_BASE_DELAY * WS_BACKOFF_FACTOR ** Math.max(0, this.reconnectBackoffAttempts - 1),
    );

    return Math.max(WS_MIN_RECONNECTION_DELAY, exponentialDelay);
  };

  private getCloseLogMessage = (
    closeCode: number | undefined,
    closeReason: string,
    closeCodeDescription: string,
  ): string | null => {
    const hasCloseCode = closeCode !== undefined && closeCode !== 1000;
    if (!hasCloseCode || this.parent.isDisposed) {
      return null;
    }

    const now = Date.now();
    const signature = `${closeCode}:${closeCodeDescription}`;
    const withinCooldown = this.closeLogSignature === signature && now - this.closeLogLastAt < WS_CLOSE_LOG_COOLDOWN_MS;

    if (withinCooldown) {
      this.closeLogSuppressedCount += 1;
      return null;
    }

    const suppressedCount = this.closeLogSignature === signature ? this.closeLogSuppressedCount : 0;
    this.closeLogSignature = signature;
    this.closeLogLastAt = now;
    this.closeLogSuppressedCount = 0;

    const baseMessage = closeReason
      ? `WebSocket closed: ${closeCodeDescription} (code ${closeCode}) - ${closeReason}`
      : `WebSocket closed: ${closeCodeDescription} (code ${closeCode})`;

    if (suppressedCount > 0) {
      return `${baseMessage} (suppressed ${suppressedCount} duplicate close events)`;
    }

    return baseMessage;
  };

  private getWebSocketErrorLogMessage = (rawMessage: string): string | null => {
    const normalizedMessage = rawMessage || 'Unknown error';
    const now = Date.now();
    const signature = normalizedMessage.toLowerCase();
    const withinCooldown =
      this.wsErrorLogSignature === signature && now - this.wsErrorLogLastAt < WS_ERROR_LOG_COOLDOWN_MS;

    if (withinCooldown) {
      this.wsErrorLogSuppressedCount += 1;
      return null;
    }

    const suppressedCount = this.wsErrorLogSignature === signature ? this.wsErrorLogSuppressedCount : 0;
    this.wsErrorLogSignature = signature;
    this.wsErrorLogLastAt = now;
    this.wsErrorLogSuppressedCount = 0;

    if (suppressedCount > 0) {
      return `${normalizedMessage} (suppressed ${suppressedCount} duplicate ws error events)`;
    }

    return normalizedMessage;
  };

  /**
   * Process the close event logic
   */
  /**
   * Records the durable close diagnostics that `store.socketHealth` publishes.
   *
   * Separate from `processClose` because several exchanges (lighter,
   * polymarket) replace `onClose` wholesale instead of delegating to the base
   * implementation, and run their own reconnect loops. Without an explicit call
   * from those overrides they would report `closeCount: 0` — indistinguishable
   * from a healthy socket, which is worse than reporting nothing at all.
   *
   * Pure instrumentation: safe to call from any close path, changes no
   * reconnect behavior.
   */
  protected recordCloseDiagnostics = (closeCode?: number, options?: { syntheticIfNoSocket?: boolean }): void => {
    // `reconnect()` calls `onClose()` directly when there is no socket, to drive
    // the reconnect path. Nothing actually closed, so counting it would inflate
    // the total that diagnostics present as real churn.
    if (options?.syntheticIfNoSocket && !this.ws) return;

    this.lifetimeCloseCount += 1;
    // Some paths invoke onClose() with no event (asterdex, polymarket), so the
    // code is unknown — don't clobber a known previous code with undefined.
    if (closeCode !== undefined) {
      this.lastCloseCode = closeCode;
      this.lastCloseAt = Date.now();
    }
  };

  private processClose = (closeCode: number | undefined, closeReason: string, closeCodeDescription: string): void => {
    this.lastReceived = 0;
    if (this.lastReceivedTimer) {
      clearTimeout(this.lastReceivedTimer);
      this.lastReceivedTimer = undefined;
    }

    this.recordCloseDiagnostics(closeCode, { syntheticIfNoSocket: true });

    // registerCloseResult BEFORE the sync, otherwise the pause/failure state
    // published to the store lags one close behind until the next watchdog tick.
    this.registerCloseResult(closeCode);
    // Notify parent that socket health changed
    this.parent.syncSocketHealthToStore?.();

    // Log close event for non-trivial closes (don't emit error toast - too noisy).
    // In reconnect storms, dedupe repeated signatures within cooldown windows.
    const errorMessage = this.getCloseLogMessage(closeCode, closeReason, closeCodeDescription);
    if (errorMessage) {
      console.error(`[${this.parent.name}] ${errorMessage}`);
      this.parent.emitter.emit('grafana-error', errorMessage);
    }

    if (this.ws instanceof ReconnectingWebSocket) {
      if (!this.parent.isDisposed) {
        this.parent.log(
          `WebSocket connection disconnected (code: ${closeCode ?? 'unknown'}, reason: ${closeReason || 'none'}), reconnecting...`,
          LogSeverity.Warning,
        );
      }

      if (this.pingTimeoutId) {
        clearTimeout(this.pingTimeoutId);
        this.pingTimeoutId = undefined;
      }

      if (this.pongMissTimeoutId) {
        clearTimeout(this.pongMissTimeoutId);
        this.pongMissTimeoutId = undefined;
      }
    } else {
      lifecycleLog(`[BaseWebSocket] 🔔 ${this.parent.name} onClose handler called for native WebSocket`);
      if (!this.parent.isDisposed) {
        this.parent.log(
          `WebSocket connection disconnected (code: ${closeCode ?? 'unknown'}, reason: ${closeReason || 'none'}), reconnecting...`,
          LogSeverity.Warning,
        );
      }

      if (this.reconnecting) {
        lifecycleLog(`[BaseWebSocket] ⚠️ ${this.parent.name} already reconnecting, skipping...`);
        this.parent.log('WebSocket connection already reconnecting, skipping...', LogSeverity.Info);
        return;
      }

      lifecycleLog(`[BaseWebSocket] 🔄 ${this.parent.name} setting reconnecting = true`);
      this.reconnecting = true;

      if (this.pingTimeoutId) {
        clearTimeout(this.pingTimeoutId);
        this.pingTimeoutId = undefined;
      }

      if (this.pongMissTimeoutId) {
        clearTimeout(this.pongMissTimeoutId);
        this.pongMissTimeoutId = undefined;
      }

      this.ws?.removeEventListener?.('open', this.onOpen);
      this.ws?.removeEventListener?.('message', this.onMessage);
      this.ws?.removeEventListener?.('close', this.onClose);
      this.ws?.removeEventListener('open', this.emitWsReconnected);
      this.ws = undefined;

      this.onCloseCount++;
      if (this.onCloseCount > 100) {
        this.parent.log('onClose count > 100...', LogSeverity.Error);
        return;
      }

      if (!this.parent.isDisposed) {
        const computedBackoffDelay = this.getReconnectDelay(closeCode);
        const reconnectPauseRemaining = this.getReconnectPauseRemainingMs();
        const timeSinceLastReconnect = Date.now() - this.lastCloseTime;
        const reconnectDelay = Math.max(
          computedBackoffDelay,
          reconnectPauseRemaining,
          WS_MIN_RECONNECTION_DELAY - timeSinceLastReconnect,
          0,
        );
        if (reconnectPauseRemaining > 0 && !this.reconnectPauseAnnounced) {
          this.reconnectPauseAnnounced = true;
          this.parent.log(
            `Reconnection pause active after ${this.consecutiveCloseFailures} consecutive failures; retrying in ${Math.ceil(
              reconnectPauseRemaining / 1000,
            )}s`,
            LogSeverity.Warning,
          );
        }
        this.parent.log(`Reconnecting in ${reconnectDelay}ms`, LogSeverity.Info);
        lifecycleLog(`[BaseWebSocket] ⏰ ${this.parent.name} will reconnect in ${reconnectDelay}ms`);

        this.reconnectTimer = setTimeout(this.reconnectNow, reconnectDelay);
      }
    }
  };

  dispose() {
    // Mark force-closed so the native-WS reconnect path (processClose) also
    // refuses to reconnect, matching the ReconnectingWebSocket close() which
    // sets _shouldReconnect=false. Belt-and-suspenders with the send() guard.
    this.forceClosed = true;
    // Clear any pending timeouts to prevent reconnection attempts after disposal
    if (this.pingTimeoutId) {
      clearTimeout(this.pingTimeoutId);
      this.pingTimeoutId = undefined;
    }
    if (this.pongMissTimeoutId) {
      clearTimeout(this.pongMissTimeoutId);
      this.pongMissTimeoutId = undefined;
    }
    if (this.lastReceivedTimer) {
      clearTimeout(this.lastReceivedTimer);
      this.lastReceivedTimer = undefined;
    }
    // Close the WebSocket connection. Node's `ws` can keep a connecting
    // socket alive after close(); terminate when the wrapper exposes it.
    (this.ws as any)?.terminate?.();
    this.ws?.close?.();
    this.ws = undefined; // Clear the reference to prevent any accidental reuse
  }

  reconnect = async () => {
    lifecycleLog(`[BaseWebSocket] 🔄 ${this.parent.name} reconnect() called`);
    this.parent.log('Explicit reconnect call from ping pong miss or online status change', LogSeverity.Info);
    if (this.ws) {
      this.reconnectCount++;
      lifecycleLog(`[BaseWebSocket] 🔄 ${this.parent.name} reconnect count: ${this.reconnectCount}`);
      if (this.reconnectCount > 100) {
        this.parent.log('Reconnect count > 100...', LogSeverity.Error);
        return;
      }
      if (this.ws instanceof ReconnectingWebSocket) {
        lifecycleLog(`[BaseWebSocket] 🔄 ${this.parent.name} using ReconnectingWebSocket.reconnect()`);
        await sleep(WS_MIN_RECONNECTION_DELAY);
        if (this.parent.isDisposed) {
          lifecycleLog(`[BaseWebSocket] ⚠️ ${this.parent.name} disposed, aborting reconnect`);
          return;
        }
        this.ws.reconnect();
        lifecycleLog(`[BaseWebSocket] ✅ ${this.parent.name} ReconnectingWebSocket.reconnect() called`);
      } else {
        lifecycleLog(
          `[BaseWebSocket] 🔄 ${this.parent.name} using native WebSocket - closing to trigger onClose reconnection`,
        );
        // For native WebSocket, just close the connection - onClose will handle cleanup and reconnection
        this.ws.close();
        lifecycleLog(
          `[BaseWebSocket] 🔌 ${this.parent.name} WebSocket.close() called - onClose will handle reconnection`,
        );
      }
    } else {
      lifecycleLog(`[BaseWebSocket] ⚠️ ${this.parent.name} no WebSocket to reconnect, calling onClose()`);
      this.onClose();
    }
  };

  /** The native close path's reconnect: run by its backoff timer, or at once by a recovery. */
  private reconnectNow = () => {
    this.reconnectTimer = undefined;
    if (this.parent.isDisposed) {
      lifecycleLog(`[BaseWebSocket] ⚠️ ${this.parent.name} disposed during reconnect delay, aborting`);
      return;
    }

    // Log the options to understand why we're taking a specific path
    lifecycleLog(`[BaseWebSocket] 🔍 ${this.parent.name} options check:`, {
      publicDataOnly: this.parent.options.publicDataOnly,
      hasKey: this.parent.options.key !== '',
      key: this.parent.options.key ? '[REDACTED]' : 'empty',
      forceClosed: this.forceClosed,
    });

    // don't fetch snapshot for public exchanges
    if (this.parent.options.publicDataOnly || this.parent.options.key === '') {
      lifecycleLog(
        `[BaseWebSocket] 🌍 ${this.parent.name} taking PUBLIC path (publicDataOnly=${this.parent.options.publicDataOnly}, key empty=${this.parent.options.key === ''})`,
      );
      if (!this.forceClosed) {
        lifecycleLog(`[BaseWebSocket] 📡 ${this.parent.name} calling connectAndSubscribe() for public WebSocket`);
        this.connectAndSubscribe();
      } else {
        lifecycleLog(`[BaseWebSocket] ⚠️ ${this.parent.name} forceClosed=true, skipping connectAndSubscribe()`);
      }
      (this.ws as any as WebSocket)?.addEventListener('open', this.emitWsReconnected);
    } else {
      lifecycleLog(`[BaseWebSocket] 🔐 ${this.parent.name} taking PRIVATE path`);
      if (!this.forceClosed) {
        lifecycleLog(
          `[BaseWebSocket] 📡 ${this.parent.name} calling connectAndSubscribe() for private WebSocket`,
        );
        this.connectAndSubscribe();
      } else {
        lifecycleLog(`[BaseWebSocket] ⚠️ ${this.parent.name} forceClosed=true, skipping connectAndSubscribe()`);
      }
      (this.ws as any as WebSocket)?.addEventListener('open', this.emitWsReconnected);
    }
    lifecycleLog(`[BaseWebSocket] ✅ ${this.parent.name} reconnection complete, setting reconnecting=false`);
    this.reconnecting = false;
    this.lastCloseTime = Date.now();
  };

  /**
   * A connect that failed before any socket existed (a listen-key or token fetch).
   * Without this it was never retried. Toasts the first failure of a streak only, then
   * retries through the close path's backoff and failure-streak pause.
   */
  protected retryFailedConnect = (message: string) => {
    if (this.parent.isDisposed || this.forceClosed) return;
    if (this.connectFailureAnnounced) this.parent.log(message, LogSeverity.Warning);
    else this.parent.emitAndLogError(message);
    this.connectFailureAnnounced = true;
    this.onClose({ code: 1006 });
  };

  /** Cleared with the failure streak (healthy traffic, a normal close, a recovery). */
  private connectFailureAnnounced = false;

  /** False where a subclass runs its own close loop (Lighter, Polymarket, HyroTrader's private socket). */
  protected readonly recoversAfterOffline: boolean = true;

  /**
   * The browser came back online. Time offline left this socket in the worst of
   * its retry states — a long backoff, the 120s failure-streak pause, or past
   * the 100-close cap with `reconnecting` stuck — none of which a plain
   * `reconnect()` clears. Forget them and connect now; a link that is not up
   * yet just starts a fresh, short backoff.
   */
  recoverAfterOffline = (): boolean => {
    if (this.parent.isDisposed || this.forceClosed) return true;
    if (!this.recoversAfterOffline) return false;
    // The library runs its own backoff; a reconnect() during its wait starts a second chain,
    // so only a socket it thinks is open (likely half-open) is reconnected.
    if (this.ws instanceof ReconnectingWebSocket) {
      if (this.ws.readyState === ReconnectingWebSocket.OPEN) this.ws.reconnect();
      return true;
    }
    this.resetFailureStreak();
    this.resetReconnectBackoff();
    this.onCloseCount = 0;
    this.reconnectCount = 0;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectNow();
    } else if (this.ws && this.ws.readyState !== WebSocket.CONNECTING) {
      // Usually half-open after an offline spell. Its own close event drives the close path:
      // detaching instead left venue close handlers (Kraken, Bitunix, MEXC, AsterDEX) on the
      // stale socket, and its late close tore down the replacement.
      this.ws.close();
    } else if (this.reconnecting) {
      // Stuck past the 100-close cap with no timer.
      this.reconnectNow();
    }
    // Otherwise a connect is in flight (still CONNECTING, or awaiting a listen-key or token): it
    // lands or retries itself, and closing or doubling it would only cost or leak a socket.
    return true;
  };

  private forceClosed = false;
  /** Read-only, so a subclass's own retry timer can respect forceClose(). */
  protected get isForceClosed(): boolean {
    return this.forceClosed;
  }

  forceClose = () => {
    this.forceClosed = true;
    this.ws?.close();
  };

  forceOpen = () => {
    this.forceClosed = false;
    this.resetReconnectBackoff();
    this.resetFailureStreak();
    if (this.ws instanceof ReconnectingWebSocket) {
      this.ws?.reconnect();
    } else {
      this.connectAndSubscribe();
    }
  };

  trackLastReceived = () => {
    const wasZero = this.lastReceived === 0;
    this.lastReceived = Date.now();
    // Receiving traffic means connection recovered; clear failure streak so
    // backoff can restart from base. Hot path: this fires on every WS frame
    // (~600/sec on Bybit linear), but in steady state the backoff and
    // failure-streak fields are already at default. Skip the 6 field writes
    // when nothing's pending — only pay the reset cost on actual recovery.
    if (this.reconnectBackoffAttempts !== 0 || this.reconnectBackoffLastFailureAt !== 0) {
      this.resetReconnectBackoff();
    }
    if (this.consecutiveCloseFailures !== 0 || this.reconnectPauseUntil !== 0) {
      this.resetFailureStreak();
    }

    // Opt-in: any inbound message counts as pong satisfaction for the purposes
    // of clearing the pong-miss reconnect timer. Pings themselves are NOT
    // rescheduled here — they continue on their own cadence set by the
    // subclass `ping()` self-heal and by `handlePongEvent` on real pongs.
    if (this.treatAnyMessageAsPong && this.pongMissTimeoutId) {
      clearTimeout(this.pongMissTimeoutId);
      this.pongMissTimeoutId = undefined;
    }

    if (wasZero) {
      // Mark as initialized once we receive our first message
      this.isInitialized = true;

      // Connection confirmed healthy again — reset the close/reconnect
      // counters so their `> 100` caps bound a single unhealthy streak,
      // not the socket's lifetime. Without this, long-lived sessions
      // (flaky networks, exchanges with 24h WS session limits) accumulate
      // 100 closes and the socket silently dies forever.
      this.onCloseCount = 0;
      this.reconnectCount = 0;

      // Notify parent that socket health changed
      this.parent.syncSocketHealthToStore?.();

      // Clear any existing timer just in case
      if (this.lastReceivedTimer) {
        clearTimeout(this.lastReceivedTimer);
      }
      // Start checking with setTimeout
      const checkAndSchedule = () => {
        if (this.lastReceived !== 0) {
          this.checkLastReceived();
          // Periodically notify parent to sync health to store
          this.parent.syncSocketHealthToStore?.();
          this.lastReceivedTimer = setTimeout(checkAndSchedule, 1000);
        }
      };
      this.lastReceivedTimer = setTimeout(checkAndSchedule, 1000);
    }
  };

  checkLastReceived = () => {
    const now = Date.now();
    const diff = now - this.lastReceived;
    const lastReceivedTime = new Date(this.lastReceived).toISOString();
    const reconnectThresholdMs = this.getWatchdogStaleThresholdMs() ?? WS_RECONNECT_THRESHOLD_MS;
    const pingThresholdMs = this.getWatchdogPingThresholdMs() ?? WS_PING_THRESHOLD_MS;

    // Check reconnect threshold FIRST (larger value)
    if (diff > reconnectThresholdMs) {
      this.parent.log(
        `[trackLastReceived] ${this.parent.name} last received message ${diff}ms ago (at ${lastReceivedTime}), reconnecting`,
        LogSeverity.Error,
      );
      clearTimeout(this.lastReceivedTimer);
      this.lastReceivedTimer = undefined;
      this.lastReceived = 0;
      this.pingSent = false;
      this.reconnect();
    } else if (diff > pingThresholdMs && !this.pingSent) {
      this.parent.log(
        `[trackLastReceived] ${this.parent.name} last received message ${diff}ms ago (at ${lastReceivedTime}), sending ping`,
        LogSeverity.Info,
      );
      try {
        this.ping();
        this.pingSent = true;
      } catch (_e) {}
    } else if (diff <= pingThresholdMs) {
      // Reset pingSent when connection is healthy
      this.pingSent = false;
    }
  };
}
