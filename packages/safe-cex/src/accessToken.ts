/**
 * Proxy Token Management
 *
 * This module provides access to the proxy token used for HTTP-over-WebSocket
 * (trade tunnel) authentication.
 *
 * In browser contexts (main thread and workers), tokens are managed by a SharedWorker
 * that handles all fetching centrally. This eliminates duplicate token requests.
 *
 * In CLI/Node.js contexts, a custom refresh function can be provided.
 */

import jwt from 'jsonwebtoken';

import {
  getCurrentToken,
  getProxyToken as getProxyTokenFromClient,
  initializeProxyTokenClient,
  refreshProxyToken as refreshProxyTokenFromClient,
  setToken,
} from './proxyTokenClient';
import { virtualClock } from './utils/virtualClock';

// Optional custom refresh function (for CLI environments)
let customRefreshFn: (() => Promise<string>) | null = null;
let contextualTokenProviderFn: (() => (() => Promise<string> | string) | null) | null = null;

// Detect if we're in a browser context (has SharedWorker support)
const isBrowserContext = typeof window !== 'undefined' || typeof self !== 'undefined';

// Legacy: direct access to current token (for synchronous access)
export let currentProxyToken = '';

// Sync the local variable with the client's cached token
function syncToken() {
  currentProxyToken = getCurrentToken();
}

export const isTokenExpired = (token: string) => {
  if (!token || token === '') {
    return true;
  }
  const decoded = jwt.decode(token, { json: true });
  if (!decoded) {
    return true;
  }
  const expiration = new Date((decoded.exp || 0) * 1000);
  return expiration < (virtualClock.getCurrentTimeMs() || new Date());
};

async function getContextualProxyToken(): Promise<string | null> {
  const contextualTokenProvider = contextualTokenProviderFn?.();
  if (!contextualTokenProvider) return null;

  try {
    const token = await contextualTokenProvider();
    if (token && !isTokenExpired(token)) {
      return token;
    }
  } catch (err) {
    console.error('[accessToken] Contextual token provider failed:', err);
  }

  return null;
}

export async function refreshProxyToken(): Promise<string> {
  const contextualToken = await getContextualProxyToken();
  if (contextualToken) {
    return contextualToken;
  }

  return refreshModuleProxyToken();
}

async function refreshModuleProxyToken(): Promise<string> {
  // If a custom refresh function is set (CLI), use it
  if (customRefreshFn) {
    try {
      const newToken = await customRefreshFn();
      if (newToken) {
        currentProxyToken = newToken;
        setToken(newToken);
      }
      return currentProxyToken;
    } catch {
      return currentProxyToken;
    }
  }

  // In browser context, use SharedWorker via client
  if (isBrowserContext) {
    try {
      const token = await refreshProxyTokenFromClient();
      currentProxyToken = token;
      return token;
    } catch (err) {
      console.error('[accessToken] Failed to refresh via SharedWorker:', err);
      return currentProxyToken;
    }
  }

  // Node.js without custom function - return current token
  return currentProxyToken;
}

/**
 * Get proxy token, fetching if necessary.
 * In browser, uses SharedWorker. In CLI, uses custom function.
 */
export async function getProxyToken(): Promise<string> {
  const contextualToken = await getContextualProxyToken();
  if (contextualToken) {
    return contextualToken;
  }

  // If a custom refresh function is set (CLI), use it when needed
  if (customRefreshFn) {
    if (isTokenExpired(currentProxyToken)) {
      return refreshModuleProxyToken();
    }
    return currentProxyToken;
  }

  // In browser context, use SharedWorker via client
  if (isBrowserContext) {
    try {
      const token = await getProxyTokenFromClient();
      currentProxyToken = token;
      return token;
    } catch (err) {
      console.error('[accessToken] Failed to get token via SharedWorker:', err);
      return currentProxyToken;
    }
  }

  return currentProxyToken;
}

/**
 * Set the proxy token directly.
 * Used for initialization or when receiving token from external source.
 */
export function setProxyToken(token: string) {
  currentProxyToken = token;
  setToken(token);
}

/**
 * Set a custom token refresh function.
 * Used by CLI to provide its own refresh mechanism.
 */
export function setTokenRefreshFunction(fn: (() => Promise<string>) | null) {
  customRefreshFn = fn;
}

/**
 * Set a per-async-context token provider lookup.
 * Used by multi-tenant hosts such as exchange-server where a module-global
 * proxy token would leak one user's token into another user's exchange calls.
 */
export function setContextualProxyTokenProvider(fn: (() => (() => Promise<string> | string) | null) | null) {
  contextualTokenProviderFn = fn;
}

/**
 * Initialize the proxy token system.
 * Should be called early in app startup for browser contexts.
 */
export async function initializeProxyToken(): Promise<void> {
  if (isBrowserContext) {
    await initializeProxyTokenClient();
    syncToken();
  }
}

// Legacy aliases for backwards compatibility
/** @deprecated Use currentProxyToken instead */
export { currentProxyToken as currentAccessToken };
/** @deprecated Use refreshProxyToken instead */
export { refreshProxyToken as refreshToken };
/** @deprecated Use setProxyToken instead */
export { setProxyToken as setAccessToken };
