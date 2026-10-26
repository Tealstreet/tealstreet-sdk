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

import {
  getCurrentToken,
  getProxyToken as getProxyTokenFromClient,
  initializeProxyTokenClient,
  refreshProxyToken as refreshProxyTokenFromClient,
  setToken,
} from './proxyTokenClient';
import { isTokenExpired } from './proxyTokenExpiry';

export { isTokenExpired } from './proxyTokenExpiry';

/**
 * A contextual proxy-token provider was installed but could not produce a
 * usable token.
 *
 * This must never degrade to an empty string. A request sent with no
 * `Authorization` header reaches the venue as a bare 401, which reads to
 * everyone downstream — support, Sentry, the account-health badge — as a bad
 * API key rather than as our own auth having failed. That is what made
 * EXCHANGE-SERVER-27 look like an OKX problem for six days.
 */
const PROXY_TOKEN_ERROR_NAME = 'ProxyTokenUnavailableError';

export class ProxyTokenUnavailableError extends Error {
  constructor(message = 'Tealstreet proxy token unavailable') {
    super(message);
    this.name = PROXY_TOKEN_ERROR_NAME;
  }
}

// Optional custom refresh function (for CLI environments)
let customRefreshFn: (() => Promise<string>) | null = null;
type ProxyTokenProvider = () => Promise<string> | string;
let contextualTokenProviderFn: (() => ProxyTokenProvider | null) | null = null;
// Set only for the synchronous start of a lookup; see `withProxyTokenProvider`.
let scopedTokenProvider: ProxyTokenProvider | null = null;

// Detect if we're in a browser context (has SharedWorker support)
const isBrowserContext = typeof window !== 'undefined' || typeof self !== 'undefined';

// Legacy: direct access to current token (for synchronous access)
export let currentProxyToken = '';

// Sync the local variable with the client's cached token
function syncToken() {
  currentProxyToken = getCurrentToken();
}

async function getContextualProxyToken(): Promise<string | null> {
  const contextualTokenProvider = scopedTokenProvider ?? contextualTokenProviderFn?.();
  if (!contextualTokenProvider) return null;

  try {
    const token = await contextualTokenProvider();
    if (token && !isTokenExpired(token)) {
      return token;
    }
    // A provider is installed, so it OWNS this call's credential and there is
    // no safe fallback: the module global belongs to whoever set it last, which
    // on a multi-tenant host is somebody else's session.
    throw new ProxyTokenUnavailableError('proxy token missing or expired for this request context');
  } catch (err) {
    // `.name` as well as `instanceof`: today safe-cex resolves to a single
    // module in every bundle we ship, so the identity check holds — but if that
    // ever stops being true the failure mode is silent, and the symptom is this
    // error's reason being replaced by a generic one. One extra comparison
    // retires that whole class of bug.
    // The literal, not `ProxyTokenUnavailableError.name`: a minifier renames the
    // class binding but not the string the constructor assigns.
    if (err instanceof ProxyTokenUnavailableError || (err as { name?: unknown })?.name === PROXY_TOKEN_ERROR_NAME) {
      throw err;
    }
    // `err` is deliberately not logged or attached: a provider failure can
    // carry the token it failed to mint, and this path has no scrubber.
    throw new ProxyTokenUnavailableError('proxy token provider failed');
  }
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
 * The calling async context's proxy-token provider, so an account can keep
 * using it from work started outside that context (engine timers, sockets).
 */
export function captureProxyTokenProvider(): ProxyTokenProvider | null {
  return contextualTokenProviderFn?.() ?? null;
}

/**
 * Run `fn` with `provider` answering token lookups that start synchronously
 * inside it. `getProxyToken` reads the provider before its first await, so a
 * synchronous scope is enough and never leaks into another account's work.
 */
export function withProxyTokenProvider<T>(provider: ProxyTokenProvider | null, fn: () => T): T {
  if (!provider) return fn();
  const previous = scopedTokenProvider;
  scopedTokenProvider = provider;
  try {
    return fn();
  } finally {
    scopedTokenProvider = previous;
  }
}

/**
 * A token lookup bound to the effective provider at call time (scope, then async
 * context), for transports that read the token after awaits or from socket events.
 */
export function bindProxyTokenLookup(): () => Promise<string> {
  const provider = scopedTokenProvider ?? contextualTokenProviderFn?.() ?? null;
  return () => withProxyTokenProvider(provider, getProxyToken);
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
