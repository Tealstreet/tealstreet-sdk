export const WS_MAX_RECONNECTION_DELAY = 5000;
export const WS_MIN_RECONNECTION_DELAY = 1000 + Math.random() * 2000;
export const RECONNECTING_WEBSOCKET_TUNING = {
  maxReconnectionDelay: WS_MAX_RECONNECTION_DELAY,
  minReconnectionDelay: WS_MIN_RECONNECTION_DELAY,
};

// Exponential backoff config for WebSocket reconnection
export const WS_BACKOFF_BASE_DELAY = 2000; // 2 seconds initial delay
export const WS_BACKOFF_MAX_DELAY = 30000; // 30 seconds max delay
export const WS_BACKOFF_FACTOR = 2; // Double delay each attempt
export const WS_FAILURE_STREAK_PAUSE_THRESHOLD = 8; // Pause reconnects after N consecutive close failures
export const WS_FAILURE_STREAK_PAUSE_MS = 120000; // 2 minute pause window for persistent failures
export const ENABLE_RECONNECTING_WEBSOCKET = false;
export const ENABLE_PWS_LOG = true;
export const SIMULATE_HTTP_OVER_WEBSOCKET = Boolean(+(process.env.NEXT_PUBLIC_SIMULATE_HTTP_OVER_WEBSOCKET || '0'));

/**
 * Verbose lifecycle logging for BaseWebSocket (close/reconnect/ping plumbing).
 *
 * When false (default), the emoji-decorated `console.log` lines in
 * `base.ws.ts` (`processClose`, `reconnect`, `setupHouseKeepEventListener`,
 * etc.) are suppressed so that a busy reconnect cycle doesn't flood DevTools
 * with hundreds of messages. The structured `this.parent.log(...)` calls and
 * the `console.error` for non-trivial close codes still fire so that real
 * issues remain visible.
 *
 * Flip to true (or set NEXT_PUBLIC_WS_LIFECYCLE_DEBUG=1) when actively
 * debugging connection lifecycle issues.
 */
export const ENABLE_WS_LIFECYCLE_DEBUG_LOGS = Boolean(+(process.env.NEXT_PUBLIC_WS_LIFECYCLE_DEBUG || '0'));
