import type { BaseExchange } from './base';

import { RingBuffer } from '@tealstreet/data-structures/src/RingBuffer';
import { Options, ReconnectingWebSocket } from '@tealstreet/reconnecting-websocket/src/ReconnectingWebsocket';
import { sleep } from '@tealstreet/sleep/src/sleep';
import {
  ENABLE_PWS_LOG,
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
import { MessageHandlers, MessageHandlersStats } from './types';

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

const TEALSTREET_WS_HEADERS = {
  Origin: 'https://preview.tealstreet.io',
  'User-Agent':
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36',
};

const isTealstreetWebSocketUrl = (url: string): boolean => {
  try {
    const hostname = new URL(url).hostname;
    return hostname === 'tealstreet.io' || hostname.endsWith('.tealstreet.io');
  } catch {
    return false;
  }
};

type ModuleWithCreateRequire = {
  createRequire?: (url: string | URL) => (id: string) => unknown;
};

const getNodeWebSocket = (): typeof WebSocket | undefined => {
  if (typeof process === 'undefined' || !(process.versions?.node || (process as any).isBun)) return undefined;

  // Node 22 ESM has no lexical `require`. `getBuiltinModule` is synchronous,
  // does not add a browser bundle import, and lets the ESM artifact resolve
  // the installed `ws` package. Keep the CJS/Bun path for those runtimes.
  try {
    const module = (process as typeof process & {
      getBuiltinModule?: (name: string) => ModuleWithCreateRequire | undefined;
    }).getBuiltinModule?.('module');
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
  lastReceived = 0;
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
  reconnecting = false;
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
    // than allocating a fresh object per frame. The previous full-map walk
    // refreshing `lastCallbacksCount` for every other topic ran on every WS
    // frame (N×N with subscription count); `cleanupMessageHandlerStats`
    // already deletes entries when handlers go to zero, and downstream
    // consumers only treat `lastCallbacksCount` as truthy/falsy.
    const handlers = this.messageHandlers[topic];
    const callbacksCount = handlers ? handlers.length : 1;
    const existing = this.messageHandlersStats[topic];
    if (existing) {
      existing.lastMessageTimestamp = Date.now();
      existing.lastCallbacksCount = callbacksCount;
    } else {
      this.messageHandlersStats[topic] = {
        lastMessageTimestamp: Date.now(),
        lastCallbacksCount: callbacksCount,
      };
    }
  }

  protected cleanupMessageHandlerStats(topic: string) {
    delete this.messageHandlersStats[topic];
  }

  sendWithLog = (data: string) => {
    this.send(data);
    if (ENABLE_PWS_LOG) this.parent.emitter.emit('grafana-pws-sub', `${data}`);
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
    if (isTealstreetWebSocketUrl(url)) {
      const headers = reconnectingOptions.headers;
      ws = new ReconnectingWebSocket(url, protocols, {
        ...reconnectingOptions,
        ...(NodeWebSocket ? { WebSocket: NodeWebSocket } : {}),
        // These proxy-routing headers are best effort: browser-native sockets
        // cannot attach them, and must retain the existing fallback.
        headers:
          typeof headers === 'function'
            ? () => ({ ...headers(), ...TEALSTREET_WS_HEADERS })
            : { ...headers, ...TEALSTREET_WS_HEADERS },
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

    // Also override onerror to catch any errors that might bypass addEventListener
    ws.onerror = this.handleWebSocketError as any;

    return ws;
  }

  protected handleWebSocketError = (event: Event) => {
    // Prevent the error from propagating
    event.preventDefault?.();
    event.stopPropagation?.();
    event.stopImmediatePropagation?.();

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

      if (message.match(/(ping|pong)/i)) {
        // Silently ignore ping/pong when WebSocket is not OPEN
        this.parent.log(`Skipping ping/pong - WebSocket state: ${stateName} (${wsState})`, LogSeverity.Info);
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
  private processClose = (closeCode: number | undefined, closeReason: string, closeCodeDescription: string): void => {
    this.lastReceived = 0;
    if (this.lastReceivedTimer) {
      clearTimeout(this.lastReceivedTimer);
      this.lastReceivedTimer = undefined;
    }

    // Notify parent that socket health changed
    this.parent.syncSocketHealthToStore?.();
    this.registerCloseResult(closeCode);

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
          LogSeverity.Error,
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
          LogSeverity.Error,
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
      if (this.housekeepEventListener) {
        this.ws?.removeEventListener?.('open', this.housekeepEventListener);
      }
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

        setTimeout(() => {
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
            this.setupHouseKeepEventListener();
          }
          lifecycleLog(`[BaseWebSocket] ✅ ${this.parent.name} reconnection complete, setting reconnecting=false`);
          this.reconnecting = false;
          this.lastCloseTime = Date.now();
        }, reconnectDelay);
      }
    }
  };

  private housekeepEventListener: (() => void) | undefined;
  setupHouseKeepEventListener = () => {
    this.housekeepEventListener = () => {
      if (this.parent.isDisposed) return;
      this.parent.fetchAndSetPrivateSnapshot(10);
      (this.ws as any as WebSocket)?.removeEventListener('open', this.housekeepEventListener!);
    };

    (this.ws as any as WebSocket)?.addEventListener('open', this.housekeepEventListener);
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

  private forceClosed = false;

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

    // Check reconnect threshold FIRST (larger value)
    if (diff > WS_RECONNECT_THRESHOLD_MS) {
      this.parent.log(
        `[trackLastReceived] ${this.parent.name} last received message ${diff}ms ago (at ${lastReceivedTime}), reconnecting`,
        LogSeverity.Error,
      );
      clearTimeout(this.lastReceivedTimer);
      this.lastReceivedTimer = undefined;
      this.lastReceived = 0;
      this.pingSent = false;
      this.reconnect();
    } else if (diff > WS_PING_THRESHOLD_MS && !this.pingSent) {
      this.parent.log(
        `[trackLastReceived] ${this.parent.name} last received message ${diff}ms ago (at ${lastReceivedTime}), sending ping`,
        LogSeverity.Info,
      );
      try {
        this.ping();
        this.pingSent = true;
      } catch (_e) {}
    } else if (diff <= WS_PING_THRESHOLD_MS) {
      // Reset pingSent when connection is healthy
      this.pingSent = false;
    }
  };
}
