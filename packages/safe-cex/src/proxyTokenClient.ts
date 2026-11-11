/**
 * Proxy Token Client
 *
 * Simple client for fetching and caching proxy tokens.
 * Uses direct fetch to /api/proxykey with BroadcastChannel for sharing
 * tokens across tabs and workers.
 */

import jwt from 'jsonwebtoken';

import { isTokenExpired } from './proxyTokenExpiry';
import { diagnosticConsole } from './utils/diagnosticRedaction';

export { isTokenExpired } from './proxyTokenExpiry';

// BroadcastChannel message types
type BroadcastMessage = { type: 'proxyTokenUpdate'; token: string; expiresAt: number } | { type: 'proxyTokenRequest' };

// Local cache
let cachedToken = '';
let cachedExpiresAt = 0;

// BroadcastChannel for cross-context communication
let broadcastChannel: BroadcastChannel | null = null;

// Callbacks for token updates
const tokenUpdateCallbacks: ((token: string) => void)[] = [];

// Fetch state
let isFetching = false;
let fetchPromise: Promise<string> | null = null;
const REFRESH_BUFFER_MS = 60 * 1000;
const MIN_REFRESH_INTERVAL_MS = 30 * 1000;
let lastFetchAttempt = 0;
let refreshTimeout: ReturnType<typeof setTimeout> | null = null;

// Helper to detect context
function isMainWindow(): boolean {
  return typeof window !== 'undefined' && typeof document !== 'undefined';
}

function isWorkerContext(): boolean {
  return !isMainWindow() && typeof self !== 'undefined';
}

/**
 * Get the API URL with absolute path for worker compatibility
 */
function getProxyKeyApiUrl(): string {
  const baseUrl = typeof self !== 'undefined' && self.location?.origin ? self.location.origin : '';
  return `${baseUrl}/api/proxykey`;
}

/**
 * Get token expiration from JWT
 */
function getTokenExpiration(token: string): number {
  try {
    const decoded = jwt.decode(token, { json: true });
    if (decoded?.exp) {
      return decoded.exp * 1000;
    }
  } catch {
    // Ignore decode errors
  }
  return 0;
}

/**
 * Initialize BroadcastChannel for sharing tokens across contexts
 */
function initBroadcastChannel(): void {
  if (broadcastChannel) return;

  try {
    if (typeof BroadcastChannel === 'undefined') {
      return;
    }

    broadcastChannel = new BroadcastChannel('proxy-token-channel');

    broadcastChannel.onmessage = (event: MessageEvent<BroadcastMessage>) => {
      const data = event.data;

      if (data.type === 'proxyTokenUpdate') {
        cachedToken = data.token;
        cachedExpiresAt = data.expiresAt;

        // Notify callbacks
        for (const callback of tokenUpdateCallbacks) {
          try {
            callback(data.token);
          } catch {
            // Ignore callback errors
          }
        }
      } else if (data.type === 'proxyTokenRequest' && isMainWindow()) {
        // Main thread responds to token requests from workers
        if (cachedToken && !isTokenExpired(cachedToken)) {
          broadcastTokenUpdate(cachedToken, cachedExpiresAt);
        } else {
          // Fetch fresh token and broadcast
          fetchProxyToken().catch(() => {});
        }
      }
    };
  } catch (err) {
    diagnosticConsole.error('[ProxyTokenClient] Failed to create BroadcastChannel:', err);
  }
}

/**
 * Broadcast token update to all contexts via BroadcastChannel
 */
function broadcastTokenUpdate(token: string, expiresAt: number): void {
  if (!broadcastChannel) {
    initBroadcastChannel();
  }

  try {
    broadcastChannel?.postMessage({
      type: 'proxyTokenUpdate',
      token,
      expiresAt,
    } as BroadcastMessage);
  } catch {
    // Ignore broadcast errors
  }
}

/**
 * Request token from main thread (called from workers)
 */
function requestTokenFromMainThread(): void {
  if (!broadcastChannel) {
    initBroadcastChannel();
  }

  try {
    broadcastChannel?.postMessage({
      type: 'proxyTokenRequest',
    } as BroadcastMessage);
  } catch {
    // Ignore request errors
  }
}

/**
 * Schedule next token refresh
 */
function scheduleRefresh(): void {
  if (refreshTimeout) {
    clearTimeout(refreshTimeout);
  }

  if (!cachedToken || cachedExpiresAt <= 0) {
    return;
  }

  const refreshAt = cachedExpiresAt - REFRESH_BUFFER_MS;
  const delay = Math.max(refreshAt - Date.now(), MIN_REFRESH_INTERVAL_MS);

  refreshTimeout = setTimeout(() => {
    fetchProxyToken().catch(() => {});
  }, delay);
}

/** One /api/proxykey request, body included, may take this long before it is aborted. */
const PROXY_KEY_FETCH_TIMEOUT_MS = 10_000;

// Why the latest /api/proxykey fetch produced no token (a lapsed session answers 401);
// cleared by the next token. Status and failure class only, never a body.
let lastFailure: string | undefined;

/** The latest /api/proxykey failure, if no token has arrived since. */
export const lastProxyKeyFailure = (): string | undefined => lastFailure;

/**
 * Fetch proxy token from /api/proxykey
 */
async function fetchProxyToken(): Promise<string> {
  // If already fetching, wait for ongoing request
  if (isFetching && fetchPromise) {
    return fetchPromise;
  }

  // Rate limit
  const now = Date.now();
  if (now - lastFetchAttempt < MIN_REFRESH_INTERVAL_MS && cachedToken) {
    return cachedToken;
  }

  isFetching = true;
  lastFetchAttempt = now;

  fetchPromise = (async () => {
    // Every token caller shares this one request: a hang must not block them all.
    const abort = new AbortController();
    const timeout = setTimeout(() => abort.abort(), PROXY_KEY_FETCH_TIMEOUT_MS);
    try {
      const res = await fetch(getProxyKeyApiUrl(), {
        credentials: 'include',
        signal: abort.signal,
      });

      if (!res.ok) {
        lastFailure = `/api/proxykey answered HTTP ${res.status}`;
        return cachedToken;
      }

      const data = (await res.json()) as { token?: string };
      if (!data.token) {
        lastFailure = '/api/proxykey returned no token';
        return cachedToken;
      }

      lastFailure = undefined;
      cachedToken = data.token;
      cachedExpiresAt = getTokenExpiration(cachedToken);

      // Broadcast to other contexts
      broadcastTokenUpdate(cachedToken, cachedExpiresAt);

      // Notify callbacks
      for (const callback of tokenUpdateCallbacks) {
        try {
          callback(cachedToken);
        } catch {
          // Ignore callback errors
        }
      }

      // Schedule next refresh
      scheduleRefresh();

      return cachedToken;
    } catch {
      lastFailure = abort.signal.aborted
        ? `/api/proxykey timed out after ${PROXY_KEY_FETCH_TIMEOUT_MS / 1000} s`
        : '/api/proxykey failed: network error';
      return cachedToken;
    } finally {
      clearTimeout(timeout);
      isFetching = false;
      fetchPromise = null;
    }
  })();

  return fetchPromise;
}

/**
 * Get the current proxy token
 */
export async function getProxyToken(): Promise<string> {
  // Always init broadcast channel to receive updates
  initBroadcastChannel();

  // If we have a valid cached token, return it
  if (cachedToken && !isTokenExpired(cachedToken)) {
    return cachedToken;
  }

  // Main window: fetch directly
  if (isMainWindow()) {
    return fetchProxyToken();
  }

  // Worker context: try to get from main thread, fall back to direct fetch
  if (isWorkerContext()) {
    // Request token from main thread
    requestTokenFromMainThread();

    // If we already received a valid broadcast while requesting, use it.
    if (cachedToken && !isTokenExpired(cachedToken)) {
      return cachedToken;
    }

    // Try direct fetch from worker
    return fetchProxyToken();
  }

  return cachedToken;
}

/**
 * Force refresh the proxy token
 */
export async function refreshProxyToken(): Promise<string> {
  // Reset rate limit to force refresh
  lastFetchAttempt = 0;
  return fetchProxyToken();
}

/**
 * Get the current cached token (synchronous)
 */
export function getCurrentToken(): string {
  return cachedToken;
}

/**
 * Set the token directly (used for initialization or fallback)
 */
export function setToken(token: string): void {
  cachedToken = token;
  if (token) {
    try {
      const decoded = jwt.decode(token, { json: true });
      cachedExpiresAt = decoded?.exp ? decoded.exp * 1000 : 0;
    } catch {
      cachedExpiresAt = 0;
    }
  } else {
    cachedExpiresAt = 0;
  }
}

/**
 * Register a callback for token updates
 */
export function onTokenUpdate(callback: (token: string) => void): () => void {
  tokenUpdateCallbacks.push(callback);

  // Return unsubscribe function
  return () => {
    const index = tokenUpdateCallbacks.indexOf(callback);
    if (index > -1) {
      tokenUpdateCallbacks.splice(index, 1);
    }
  };
}

/**
 * Initialize the client and fetch initial token
 */
export async function initializeProxyTokenClient(): Promise<void> {
  initBroadcastChannel();

  if (isMainWindow()) {
    try {
      await fetchProxyToken();
    } catch (err) {
      diagnosticConsole.warn('[ProxyTokenClient] Initialization failed, will retry on demand:', err);
    }
  }
}
