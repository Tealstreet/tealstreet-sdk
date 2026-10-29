import axios, { AxiosAdapter, AxiosResponse, InternalAxiosRequestConfig } from 'axios';
import { nanoid } from 'nanoid';

import { decompress } from '@tealstreet/compression/src/decompress';
import { sleep } from '@tealstreet/sleep/src/sleep';
import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';

import { bindProxyToken, isTokenExpired, shouldSimulateHttpOverWebsocket } from './config';
import { tunnelEgressUrl } from './tunnelEndpoints';
import { isTokyoEgress } from './egressRegion';
import { HOWWebSocket } from './HOWWebSocket';
import { CreateAxiosHttpOverWebsocketAdapterOptions, HOWWebSocketAuthState, LatencyEntry } from './types';

const MAX_LOG_STRING_LENGTH = 500;
const MAX_LOG_ARRAY_LENGTH = 20;
const MAX_LOG_OBJECT_KEYS = 50;
const MAX_LOG_DEPTH = 4;
const REDACTED = '[REDACTED]';
const TRUNCATED = '[TRUNCATED]';
export const TRADE_TUNNEL_RPC_VERSION = 4;
const TRADE_TUNNEL_ACCEPT_ENCODING = ['gzip'] as const;
const TRADE_TUNNEL_GZIP_BASE64_ENCODING = 'gzip+base64';
const SENSITIVE_LOG_KEY_PATTERN =
  /authorization|cookie|password|passphrase|secret|token|signature|api[-_]?key|apikey|access[-_]?key|x-mbx-apikey/i;

type TradeTunnelRouteContext = {
  selectedRegion: 'JA' | 'SG';
  tunnelHost: 'tokyo' | 'singapore';
  forcedRegion?: 'JA' | 'SG';
  bypassProxy?: boolean;
  simulated?: boolean;
};

export class TradeTunnelTimeoutError extends Error {
  readonly config: any;
  readonly method: string;
  readonly tradeTunnelRoute: TradeTunnelRouteContext;
  readonly url: string;

  constructor(
    message: string,
    context: {
      config: any;
      method: string;
      route: TradeTunnelRouteContext;
      url: string;
    },
  ) {
    super(message);
    this.name = 'TradeTunnelTimeoutError';
    this.config = context.config;
    this.method = context.method;
    this.tradeTunnelRoute = context.route;
    this.url = context.url;
  }
}

export class TradeTunnelDisposedError extends Error {
  constructor(
    readonly config: unknown,
    readonly owner: object,
  ) {
    super('HTTP-over-WebSocket adapter disposed');
    this.name = 'TradeTunnelDisposedError';
  }
}

export class TradeTunnelCanceledError extends Error {
  constructor(
    readonly config: unknown,
    readonly owner: object,
  ) {
    super('HTTP-over-WebSocket request canceled');
    this.name = 'TradeTunnelCanceledError';
  }
}

function decodeBase64(encoded: string): ArrayBuffer {
  const binary = globalThis.atob(encoded);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return bytes.buffer;
}

async function decodeTradeTunnelRawBody(data: any): Promise<string | undefined> {
  if (data?.data_encoding === 'gzip' && data?.data_binary instanceof ArrayBuffer) {
    return decompress(data.data_binary, 'gzip');
  }

  if (data?.data_encoding === TRADE_TUNNEL_GZIP_BASE64_ENCODING && typeof data?.data_encoded === 'string') {
    return decompress(decodeBase64(data.data_encoded), 'gzip');
  }

  return typeof data?.data_raw === 'string' ? data.data_raw : undefined;
}

function isTradeTunnelVerboseLoggingEnabled(): boolean {
  if (typeof process === 'undefined') return false;
  return (
    process.env?.DEBUG === '1' ||
    process.env?.DEBUG === 'true' ||
    process.env?.NEXT_PUBLIC_LOG_TRADETUNNEL_TO_CONSOLE_LOG === '1'
  );
}

function logTradeTunnelVerbose(message: string, data: unknown): void {
  console.log(message, data);
}

function sanitizeForTradeTunnelLog(value: unknown, depth = 0, seen = new WeakSet<object>()): unknown {
  if (value === null || value === undefined) return value;
  if (typeof value === 'number' || typeof value === 'boolean') return value;
  if (typeof value === 'bigint') return value.toString();
  if (typeof value === 'string') {
    if (value.length <= MAX_LOG_STRING_LENGTH) return value;
    return `${value.slice(0, MAX_LOG_STRING_LENGTH)}...${TRUNCATED}`;
  }
  if (typeof value !== 'object') return String(value);
  if (seen.has(value)) return '[CIRCULAR]';
  if (depth >= MAX_LOG_DEPTH) return TRUNCATED;

  seen.add(value);

  if (value instanceof URLSearchParams) {
    return sanitizeForTradeTunnelLog(Object.fromEntries(value.entries()), depth + 1, seen);
  }

  if (typeof FormData !== 'undefined' && value instanceof FormData) {
    const formObject: Record<string, unknown> = {};
    for (const [key, formValue] of value.entries()) {
      const maybeFile = formValue as any;
      formObject[key] =
        typeof File !== 'undefined' && maybeFile instanceof File
          ? { fileName: maybeFile.name, size: maybeFile.size }
          : formValue;
    }
    return sanitizeForTradeTunnelLog(formObject, depth + 1, seen);
  }

  if (Array.isArray(value)) {
    const values = value.slice(0, MAX_LOG_ARRAY_LENGTH).map((item) => sanitizeForTradeTunnelLog(item, depth + 1, seen));
    if (value.length > MAX_LOG_ARRAY_LENGTH) {
      values.push(`${TRUNCATED} ${value.length - MAX_LOG_ARRAY_LENGTH} items`);
    }
    return values;
  }

  const output: Record<string, unknown> = {};
  const entries = Object.entries(value as Record<string, unknown>);
  for (const [key, entryValue] of entries.slice(0, MAX_LOG_OBJECT_KEYS)) {
    output[key] = SENSITIVE_LOG_KEY_PATTERN.test(key)
      ? REDACTED
      : sanitizeForTradeTunnelLog(entryValue, depth + 1, seen);
  }
  if (entries.length > MAX_LOG_OBJECT_KEYS) {
    output.__truncatedKeys = entries.length - MAX_LOG_OBJECT_KEYS;
  }
  return output;
}

function summarizeTradeTunnelUrl(url: string): Record<string, unknown> {
  try {
    const parsed = new URL(url);
    return {
      origin: parsed.origin,
      pathname: parsed.pathname,
      queryKeys: Array.from(new Set(parsed.searchParams.keys())),
    };
  } catch {
    const [path] = url.split('?');
    return {
      value: path.length <= MAX_LOG_STRING_LENGTH ? path : `${path.slice(0, MAX_LOG_STRING_LENGTH)}...${TRUNCATED}`,
      hasQuery: url.includes('?'),
    };
  }
}

function hasLoggableContent(value: unknown): boolean {
  if (value === null || value === undefined) return false;
  if (typeof value === 'string') return value.length > 0;
  if (value instanceof URLSearchParams) return Array.from(value.keys()).length > 0;
  if (typeof FormData !== 'undefined' && value instanceof FormData) return Array.from(value.keys()).length > 0;
  if (typeof value === 'object') return Object.keys(value).length > 0;
  return true;
}

function formatTradeTunnelErrorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;

  if (error && typeof error === 'object') {
    const errorObject = error as Record<string, unknown>;
    for (const key of ['message', 'error', 'msg']) {
      const value = errorObject[key];
      if (typeof value === 'string' && value.length > 0) return value;
    }

    try {
      return JSON.stringify(error);
    } catch {
      return String(error);
    }
  }

  return String(error);
}

/**
 * Log trade tunnel request
 */
function logTradeTunnelRequest(
  request: { url: string; method: string; params?: any; data?: any },
  route?: TradeTunnelRouteContext,
) {
  if (!isTradeTunnelVerboseLoggingEnabled()) return;

  const logData: any = {
    url: summarizeTradeTunnelUrl(request.url),
    method: request.method,
    route,
  };

  if (hasLoggableContent(request.params)) {
    logData.params = sanitizeForTradeTunnelLog(request.params);
  }

  if (hasLoggableContent(request.data)) {
    logData.data = sanitizeForTradeTunnelLog(request.data);
  }

  logTradeTunnelVerbose('[TradeTunnel] REQUEST:', logData);
}

/**
 * Log trade tunnel response
 */
function logTradeTunnelResponse(
  request: { url: string; method: string },
  response: any,
  route?: TradeTunnelRouteContext,
) {
  if (!isTradeTunnelVerboseLoggingEnabled()) return;

  const logData: any = {
    url: summarizeTradeTunnelUrl(request.url),
    method: request.method,
    route,
  };

  if (response?.data !== undefined) {
    logData.data = sanitizeForTradeTunnelLog(response.data);
  } else if (response !== undefined) {
    logData.response = sanitizeForTradeTunnelLog(response);
  }

  logTradeTunnelVerbose('[TradeTunnel] RESPONSE:', logData);
}

// Exchanges that should bypass the proxy and connect directly
const BYPASS_PROXY_EXCHANGES = ['grvt'];

const BYPASS_PROXY_PATTERN = new RegExp(BYPASS_PROXY_EXCHANGES.join('|'), 'i');

export let howWebSocketSG = new HOWWebSocket(() => tunnelEgressUrl('sg'));
export let howWebSocketJA = new HOWWebSocket(() => tunnelEgressUrl('jp'));

// A Bad auth response is connection-scoped, but many HTTP requests can observe
// it concurrently. Elect one request to refresh/send the token and let the
// others await that same attempt instead of emitting duplicate rpc.auth frames.
const badAuthRefreshBySocket = new WeakMap<HOWWebSocket, Promise<string>>();

const refreshBadAuthOnce = (howWebSocket: HOWWebSocket, refresh: () => Promise<string>): Promise<string> => {
  const activeRefresh = badAuthRefreshBySocket.get(howWebSocket);
  if (activeRefresh) return activeRefresh;

  const refreshPromise = refresh();
  badAuthRefreshBySocket.set(howWebSocket, refreshPromise);

  const clearRefresh = () => {
    if (badAuthRefreshBySocket.get(howWebSocket) === refreshPromise) {
      badAuthRefreshBySocket.delete(howWebSocket);
    }
  };
  void refreshPromise.then(clearRefresh, clearRefresh);

  return refreshPromise;
};

function requestPacket(config: InternalAxiosRequestConfig, exact = false) {
  if (exact && (config.params !== undefined || (config.data !== undefined && typeof config.data !== 'string')))
    throw new TradeTunnelPreparationError('HTTP_PREPARATION_UNSUPPORTED');
  const newHeaders: { [key: string]: string } = {};

  // Handle AxiosHeaders object (axios v1.x) which has a toJSON() method
  // Otherwise fall back to Object.entries() for plain objects
  const headersToIterate = typeof config.headers?.toJSON === 'function' ? config.headers.toJSON() : config.headers;

  if (headersToIterate) {
    for (const [key, value] of Object.entries(headersToIterate as { [key: string]: string })) {
      // Skip undefined/null values but keep empty strings
      if (value !== undefined && value !== null) {
        newHeaders[key] = value;
      }
    }
  }

  // Handle GRVT custom auth properties (Cookie is a forbidden header in browsers)
  // These are set by the GRVT request interceptor to bypass AxiosHeaders restrictions
  if ((config as any).grvtCookie) {
    newHeaders['Cookie'] = (config as any).grvtCookie;
  }
  if ((config as any).grvtAccountId) {
    newHeaders['X-Grvt-Account-Id'] = (config as any).grvtAccountId;
  }

  // Check if this is a GRVT request
  const isGrvtRequest = config.baseURL && /grvt\.io/i.test(config.baseURL);

  // Build websocket request, conditionally including data field
  const websocketRequest: any = {
    headers: newHeaders,
    method: config.method?.toUpperCase() || '',
    url: config.url?.startsWith('http') ? config.url : (config.baseURL || '') + config.url,
    params: config.params || {},
  };

  // For GRVT: only include data field if it has actual data (omit if null/undefined)
  // For other exchanges: always include data field (use {} if empty)
  if (exact) {
    if (config.data !== undefined) websocketRequest.data = config.data;
  } else if (isGrvtRequest) {
    if (config.data !== null && config.data !== undefined) {
      websocketRequest.data = config.data;
    }
  } else {
    websocketRequest.data = config.data || {};
  }

  return websocketRequest;
}

export interface BoundTunnelRequest {
  ready(): boolean;
  /** Calls raw WebSocket.send synchronously, without route/authentication waits. */
  dispatch(): Promise<AxiosResponse>;
}
export interface PreparedTunnelRequest {
  bind(config: InternalAxiosRequestConfig): BoundTunnelRequest;
  cancel(): void;
}
export class TradeTunnelPreparationError extends Error {
  constructor(readonly code: 'HTTP_PREPARATION_STALE' | 'HTTP_PREPARATION_INVALID' | 'HTTP_PREPARATION_UNSUPPORTED') {
    super('HTTP tunnel preparation is no longer usable');
  }
}
export type DisposableAxiosAdapter = AxiosAdapter & {
  prepare(config: InternalAxiosRequestConfig): Promise<PreparedTunnelRequest>;
  /** Opaque TS/legacy requests only; never managed engine release proof. */
  prepareLegacy(config: InternalAxiosRequestConfig): Promise<PreparedTunnelRequest>;
  dispose(): void;
};

const createAxiosHttpOverWebsocketAdapter = (
  options?: CreateAxiosHttpOverWebsocketAdapterOptions | ((str: string) => any),
): DisposableAxiosAdapter => {
  // Support legacy signature: createAxiosHttpOverWebsocketAdapter(customParser)
  const opts: CreateAxiosHttpOverWebsocketAdapterOptions =
    typeof options === 'function' ? { customParser: options } : options || {};
  const { customParser, region, WebSocket: WebSocketCtor } = opts;
  const ownedSockets = new Map<'JA' | 'SG', HOWWebSocket>();
  const abort = new AbortController();
  let disposed = false;
  const owner = {};
  const disposedError = (config: unknown) => new TradeTunnelDisposedError(config, owner);

  const prepare = (initialConfig: InternalAxiosRequestConfig, legacy = false): Promise<PreparedTunnelRequest> => {
    let config = initialConfig;
    // Bound before any await: token reads below run after the socket and auth waits.
    const getProxyToken = bindProxyToken();
    const requestOwner = {};
    const cancellation = new AbortController();
    const canceledError = () => new TradeTunnelCanceledError(config, requestOwner);
    if (disposed) return Promise.reject(disposedError(config));
    if (config.signal?.aborted) return Promise.reject(canceledError());
    const addAbort = (listener: () => void) => {
      abort.signal.addEventListener('abort', listener, { once: true });
      cancellation.signal.addEventListener('abort', listener, { once: true });
      config.signal?.addEventListener?.('abort', listener, { once: true });
    };
    const removeAbort = (listener: () => void) => {
      abort.signal.removeEventListener('abort', listener);
      cancellation.signal.removeEventListener('abort', listener);
      config.signal?.removeEventListener?.('abort', listener);
    };
    const abortError = () => (disposed ? disposedError(config) : canceledError());
    let badAuthRetries = 0;
    const selectedRegion = region ?? (config.baseURL && isTokyoEgress(config.baseURL) ? 'JA' : 'SG');
    let howWebSocket: HOWWebSocket;
    if (WebSocketCtor) {
      let owned = ownedSockets.get(selectedRegion);
      if (!owned) {
        owned = new HOWWebSocket(tunnelEgressUrl(selectedRegion === 'JA' ? 'jp' : 'sg'), WebSocketCtor);
        ownedSockets.set(selectedRegion, owned);
      }
      howWebSocket = owned;
    } else {
      howWebSocket = selectedRegion === 'JA' ? howWebSocketJA : howWebSocketSG;
    }
    const baseRouteContext: TradeTunnelRouteContext = {
      selectedRegion,
      tunnelHost: selectedRegion === 'JA' ? 'tokyo' : 'singapore',
      ...(region ? { forcedRegion: region } : {}),
    };
    return (async () => {
      let websocketRequest = requestPacket(config, !legacy);
      let bound = false;
      let canceled = false;
      const original = new URL(websocketRequest.url);
      const originalMethod = websocketRequest.method;
      const bind = (finalConfig: InternalAxiosRequestConfig) => {
        if (bound || canceled) throw new TradeTunnelPreparationError('HTTP_PREPARATION_INVALID');
        const packet = requestPacket(finalConfig, !legacy);
        if (
          new URL(packet.url).origin !== original.origin ||
          packet.method !== originalMethod ||
          finalConfig.signal !== initialConfig.signal
        )
          throw new TradeTunnelPreparationError('HTTP_PREPARATION_INVALID');
        config = finalConfig;
        websocketRequest = packet;
        bound = true;
      };
      const timeoutMs = typeof config.timeout === 'number' && config.timeout > 0 ? config.timeout : 20_000;
      const deadline = Date.now() + timeoutMs;
      const createTimeoutError = (route: TradeTunnelRouteContext = baseRouteContext) =>
        new TradeTunnelTimeoutError(`WSoverHTTP request timed out: ${config.url}`, {
          config,
          method: websocketRequest.method,
          route,
          url: websocketRequest.url,
        });
      const remainingTime = () => {
        if (disposed) throw disposedError(config);
        if (canceled) throw canceledError();
        if (config.signal?.aborted) throw canceledError();
        return deadline - Date.now();
      };
      const awaitWithinDeadline = <T>(promise: Promise<T>, route?: TradeTunnelRouteContext): Promise<T> =>
        new Promise<T>((resolve, reject) => {
          const remaining = remainingTime();
          if (remaining <= 0) {
            reject(createTimeoutError(route));
            return;
          }

          const onAbort = () => {
            clearTimeout(timer);
            removeAbort(onAbort);
            reject(abortError());
          };
          const timer = setTimeout(() => {
            removeAbort(onAbort);
            reject(createTimeoutError(route));
          }, remaining);
          addAbort(onAbort);
          promise.then(
            (value) => {
              clearTimeout(timer);
              removeAbort(onAbort);
              resolve(value);
            },
            (error) => {
              clearTimeout(timer);
              removeAbort(onAbort);
              reject(error);
            },
          );
        });
      const sleepWithinDeadline = (milliseconds: number, route?: TradeTunnelRouteContext) =>
        awaitWithinDeadline(sleep(milliseconds), route);

      if (shouldSimulateHttpOverWebsocket()) {
        if (!legacy) throw new TradeTunnelPreparationError('HTTP_PREPARATION_UNSUPPORTED');
        return {
          cancel() {
            canceled = true;
            cancellation.abort();
          },
          bind(finalConfig) {
            bind(finalConfig);
            let sent = false;
            return {
              ready: () => !sent && !canceled && remainingTime() > 0,
              dispatch() {
                if (sent || canceled || remainingTime() <= 0)
                  throw createTimeoutError({ ...baseRouteContext, simulated: true });
                sent = true;
                const response = axios({
                  headers: websocketRequest.headers,
                  method: websocketRequest.method,
                  url: websocketRequest.url,
                  data: websocketRequest.data,
                  params: websocketRequest.params,
                  timeout: remainingTime(),
                  signal: config.signal,
                });
                logTradeTunnelRequest(websocketRequest, { ...baseRouteContext, simulated: true });
                return awaitWithinDeadline(response, { ...baseRouteContext, simulated: true }).then((result) => {
                  logTradeTunnelResponse(websocketRequest, result, { ...baseRouteContext, simulated: true });
                  return { ...result, config };
                });
              },
            };
          },
        };
      } else {
        const MAX_TOKEN_RETRIES = 120; // Max retries for getting a valid token (120 * 500ms = 60s max)

        // Helper to get a valid (non-empty, non-expired) token with retries.
        //
        // The retry window is for hosts where "no token yet" is transient and
        // resolves itself — the browser SharedWorker's cold start, and the CLI
        // before its refresh function has produced one. Both return `''` while
        // warming, which is what the loop waits out.
        //
        // A host that supplies a token per async context (exchange-server) is
        // different in kind: its provider THROWS rather than returning `''`,
        // and it will keep throwing for the whole window, so waiting buys a
        // minute of latency before the identical failure. There is no branch
        // for that here — the throw simply propagates out of the loop, which is
        // the behaviour we want. Do not wrap this in a catch to "handle" it.
        //
        // Propagating also skips the post-loop `getProxyToken()` below. That
        // call is a last-chance re-read for the warming hosts, not a cache, and
        // for a throwing provider it would only throw again.
        const getValidToken = async (): Promise<string> => {
          for (let i = 0; i < MAX_TOKEN_RETRIES; i++) {
            const token = await awaitWithinDeadline(getProxyToken());
            if (token && !isTokenExpired(token)) {
              return token;
            }
            if (i < MAX_TOKEN_RETRIES - 1) {
              // Log every 10 attempts (5 seconds) to avoid spam
              if (i % 10 === 0) {
                console.log(`[HOW] Waiting for valid token... (attempt ${i + 1}/${MAX_TOKEN_RETRIES})`);
              }
              await sleepWithinDeadline(500);
            }
          }
          // Return whatever we have after max retries
          const finalToken = await awaitWithinDeadline(getProxyToken());
          if (!finalToken) {
            console.error('[HOW] Failed to get valid token after 60s - backend may be down');
          }
          return finalToken;
        };

        // Helper to safely send token with retry on WebSocket state issues
        const safeSendToken = async (token: string, retries = 0): Promise<void> => {
          if (remainingTime() <= 0) throw createTimeoutError();
          // Never send an empty token
          if (!token) {
            console.error('[HOW] Attempted to send empty token, skipping');
            return;
          }

          const MAX_RETRIES = 5;
          try {
            // Wait for WebSocket to be ready
            while (!howWebSocket.isOpen || !howWebSocket.isTimeSynced) {
              await sleepWithinDeadline(25);
            }
            // Use runtime globals that webpack/SWC don't constant-fold.
            // `typeof window` is statically replaced in Next.js client bundles
            // (including worker chunks), so it always evaluated to 'object' and
            // mis-tagged browser workers as 'frontend'.
            // `importScripts` is a worker-only global; main thread doesn't have it.
            // Accessing it via `self` (cast to any) avoids TS needing the webworker lib.
            const isBrowserWorker = typeof self !== 'undefined' && typeof (self as any).importScripts === 'function';
            const isNodeOrBunServer =
              typeof process !== 'undefined' && (Boolean(process.versions?.node) || Boolean((process as any).isBun));
            const source = isBrowserWorker || isNodeOrBunServer ? 'worker' : 'frontend';
            if (remainingTime() <= 0) throw createTimeoutError();
            howWebSocket.sendToken({ type: 'rpc.auth', token, source });
          } catch (err: any) {
            if (err?.name === 'WebSocketNotReadyError' && retries < MAX_RETRIES) {
              // WebSocket closed between our check and send, wait for reconnect
              console.log(
                `[HOW] Token send failed (state: ${err.webSocketState}), waiting for reconnect... (retry ${retries + 1}/${MAX_RETRIES})`,
              );
              await sleepWithinDeadline(500);
              return safeSendToken(token, retries + 1);
            }
            throw err;
          }
        };

        if (!howWebSocket.isInitialized) {
          howWebSocket.connectAndSubscribe();
        }
        while (!howWebSocket.isOpen || !howWebSocket.isTimeSynced) {
          await sleepWithinDeadline(25);
        }

        // IP-whitelisted connections are authenticated by TradeTunnel itself,
        // which sends rpc.auth success immediately after the WebSocket opens.
        // Only fetch/send a JWT when the server has not already authenticated
        // this connection.
        let token = '';
        if (howWebSocket.authState !== HOWWebSocketAuthState.Good) {
          token = await getValidToken();

          if (!howWebSocket.isTokenSent && token) {
            await safeSendToken(token);
          }
        }

        while (howWebSocket.authState !== HOWWebSocketAuthState.Good) {
          await sleepWithinDeadline(25);
          if (howWebSocket.authState === HOWWebSocketAuthState.Bad) {
            await sleepWithinDeadline(500);

            // If token is NOT expired but auth still fails, it means user auth is broken
            // (not just an expired token issue), so increment retries to eventually fail out
            if (token && !isTokenExpired(token)) {
              badAuthRetries++;
            }

            // if user auth is the problem, fail out after 10 attempts completely
            if (badAuthRetries > 10) {
              throw new Error('Cannot refresh auth token, please reload the terminal');
            }

            try {
              token = await awaitWithinDeadline(
                refreshBadAuthOnce(howWebSocket, async () => {
                  const refreshedToken = await getValidToken();
                  if (refreshedToken) {
                    await safeSendToken(refreshedToken);
                  }
                  return refreshedToken;
                }),
              );
            } catch (error) {
              // An owner's deadline or disposal must not cancel other accounts
              // sharing this connection. Survivors elect a fresh token attempt.
              const foreignTimeout = error instanceof TradeTunnelTimeoutError && error.config !== config;
              const foreignDisposal = error instanceof TradeTunnelDisposedError && error.owner !== owner;
              const foreignCancellation = error instanceof TradeTunnelCanceledError && error.owner !== requestOwner;
              if (foreignTimeout || foreignDisposal || foreignCancellation) continue;
              throw error;
            }
          }
        }

        const epoch = howWebSocket.epoch;
        const usable = () =>
          !canceled &&
          !disposed &&
          !config.signal?.aborted &&
          howWebSocket.isOpen &&
          howWebSocket.isTimeSynced &&
          howWebSocket.authState === HOWWebSocketAuthState.Good &&
          howWebSocket.epoch === epoch;
        return {
          cancel() {
            canceled = true;
            cancellation.abort();
          },
          bind(finalConfig) {
            if (remainingTime() <= 0) throw createTimeoutError();
            if (!usable()) throw new TradeTunnelPreparationError('HTTP_PREPARATION_STALE');
            bind(finalConfig);
            const nonce = nanoid();
            const shouldBypassProxy = config.baseURL && BYPASS_PROXY_PATTERN.test(config.baseURL);
            const routeContext: TradeTunnelRouteContext = {
              ...baseRouteContext,
              bypassProxy: Boolean(shouldBypassProxy),
            };

            const data = legacy
              ? parseWebsocketData(customParser || JSON.parse, websocketRequest)
              : websocketRequest.data;
            const framePacket = {
              type: 'rpc.http',
              rpc_version: TRADE_TUNNEL_RPC_VERSION,
              ...(!legacy || customParser
                ? { format: 'raw', accept_encoding: TRADE_TUNNEL_ACCEPT_ENCODING, accept_binary: true }
                : {}),
              ...(shouldBypassProxy ? { bypass_proxy: true } : {}),
              nonce,
              arg: { ...websocketRequest, data },
            };
            // Legacy Axios remains opaque/unsealed. Managed callers validate
            // and serialize before claim, then use the same raw send executor.
            const frame = legacy ? undefined : JSON.stringify(framePacket);
            let sent = false;
            return {
              ready: () => {
                const remaining = remainingTime();
                if (!usable()) throw new TradeTunnelPreparationError('HTTP_PREPARATION_STALE');
                return !sent && remaining > 0;
              },
              dispatch() {
                if (sent || remainingTime() <= 0) throw createTimeoutError(routeContext);
                if (!usable()) throw new TradeTunnelPreparationError('HTTP_PREPARATION_STALE');
                sent = true;
                return new Promise((resolve, reject) => {
                  const responseTimeoutMs = remainingTime();
                  if (responseTimeoutMs <= 0) {
                    reject(createTimeoutError(routeContext));
                    return;
                  }
                  const onAbort = () => {
                    clearTimeout(timeoutTimer);
                    howWebSocket.off('sub:' + nonce);
                    removeAbort(onAbort);
                    reject(abortError());
                  };
                  const timeoutTimer = setTimeout(() => {
                    removeAbort(onAbort);
                    howWebSocket.off('sub:' + nonce);
                    reject(createTimeoutError(routeContext));
                  }, responseTimeoutMs);
                  addAbort(onAbort);

                  howWebSocket.once('sub:' + nonce, async (data: any) => {
                    try {
                      // Log response before processing
                      logTradeTunnelResponse(websocketRequest, data, routeContext);

                      if (data && typeof data === 'object' && data.error) {
                        // Create a proper Error object with URL context for better Sentry tracking
                        const tunnelError = new Error(
                          `${formatTradeTunnelErrorMessage(data.error)} (URL: ${websocketRequest.url})`,
                        );
                        tunnelError.name = 'TradeTunnelError';
                        (tunnelError as any).url = websocketRequest.url;
                        (tunnelError as any).method = websocketRequest.method;
                        (tunnelError as any).originalError = data.error;
                        (tunnelError as any).tradeTunnelRoute = routeContext;
                        // Attach the caller's config so error-path interceptors (e.g.
                        // `error?.config?.__suppressExchangeError` in Lighter) can see
                        // request-time suppression flags through the tunnel.
                        (tunnelError as any).config = config;
                        reject(tunnelError);
                        return;
                      }

                      if (!legacy || customParser) {
                        const rawBody = await decodeTradeTunnelRawBody(data);
                        if (rawBody !== undefined) {
                          data.data = legacy ? customParser!(rawBody) : rawBody;
                        }
                      }
                      // Attach the original adapter config so response interceptors see
                      // the caller's custom keys/headers (e.g. __suppressExchangeError).
                      // Tunnel frames carry only {data, status, headers}, so without this
                      // the response config is empty and per-request suppression flags
                      // are lost.
                      if (remainingTime() <= 0) throw createTimeoutError(routeContext);
                      resolve({ ...data, config });
                    } catch (error) {
                      if (
                        error instanceof TradeTunnelTimeoutError ||
                        error instanceof TradeTunnelDisposedError ||
                        error instanceof TradeTunnelCanceledError
                      ) {
                        reject(error);
                        return;
                      }
                      const tunnelError = error instanceof Error ? error : new Error(String(error));
                      tunnelError.name = 'TradeTunnelResponseDecodeError';
                      (tunnelError as any).url = websocketRequest.url;
                      (tunnelError as any).method = websocketRequest.method;
                      (tunnelError as any).tradeTunnelRoute = routeContext;
                      (tunnelError as any).config = config;
                      reject(tunnelError);
                    } finally {
                      clearTimeout(timeoutTimer);
                      removeAbort(onAbort);
                    }
                  });

                  try {
                    if (remainingTime() <= 0) throw createTimeoutError(routeContext);
                    if (frame === undefined) howWebSocket.send(framePacket);
                    else howWebSocket.sendPrepared(frame, websocketRequest.url);
                    // Diagnostics follow the physical API handoff; never sit between claim and send.
                    logTradeTunnelRequest(
                      {
                        url: websocketRequest.url,
                        method: websocketRequest.method,
                        params: websocketRequest.params,
                        data,
                      },
                      routeContext,
                    );
                  } catch (error) {
                    clearTimeout(timeoutTimer);
                    removeAbort(onAbort);
                    howWebSocket.off('sub:' + nonce);
                    reject(error);
                  }
                });
              },
            };
          },
        };
      }
    })();
  };
  const adapter: AxiosAdapter = (config) => prepare(config, true).then((prepared) => prepared.bind(config).dispatch());
  return Object.assign(adapter, {
    prepare: (config: InternalAxiosRequestConfig) => prepare(config),
    prepareLegacy: (config: InternalAxiosRequestConfig) => prepare(config, true),
    dispose: () => {
      if (disposed) return;
      disposed = true;
      abort.abort();
      for (const socket of ownedSockets.values()) socket.dispose();
      ownedSockets.clear();
    },
  });
};

function parseWebsocketData(parser: (str: string) => any, websocketRequest: any) {
  let rawData = websocketRequest.data;
  let data;

  if (rawData instanceof FormData) {
    const formObject: { [key: string]: any } = {};
    for (const [key, value] of rawData.entries()) {
      formObject[key] = value;
    }
    return formObject;
  }

  // If rawData is already an object, we can just keep it
  if (typeof rawData !== 'string') {
    return rawData;
  }

  // Try parsing as JSON
  try {
    data = parser(rawData);
    return data;
  } catch {
    // JSON parse failed, so check if it looks like URL-encoded form data
    // A quick/naive test is to check that the string
    // has at least one '=' and uses the 'key=value' pattern repeatedly.
    // You could refine this with a better regex or a content-type check.
    const formDataPattern = /^[^=&]+=[^=&]+(?:&[^=&]+=[^=&]+)*$/;
    if (formDataPattern.test(rawData)) {
      // Convert it via URLSearchParams
      const params = new URLSearchParams(rawData);
      let formObject: { [key: string]: any } = {};
      for (const [key, value] of params) {
        formObject[key] = value;
      }
      return formObject;
    } else {
      // Otherwise, treat it as just a raw string
      return rawData;
    }
  }
}

export { createAxiosHttpOverWebsocketAdapter };
export const axiosHttpOverWebsocketAdapter = createAxiosHttpOverWebsocketAdapter();
export const bigIntAxiosHttpOverWebsocketAdapter = createAxiosHttpOverWebsocketAdapter({
  customParser: bigIntAsStringJsonParse,
});

/**
 * Get the HOWWebSocket instance for a given exchange name
 */
export const getHOWWebSocketForExchange = (exchangeName: string): HOWWebSocket => {
  return isTokyoEgress(exchangeName) ? howWebSocketJA : howWebSocketSG;
};

/**
 * Get latency for a specific domain from the appropriate HOW instance
 */
export const getLatencyForDomain = (exchangeName: string, domain: string): LatencyEntry | undefined => {
  const howWebSocket = getHOWWebSocketForExchange(exchangeName);
  return howWebSocket.getLatencyForDomain(domain);
};

/**
 * Get all server-side latencies from both HOW WebSocket instances
 */
export const getAllServerLatencies = (): {
  singapore: Map<string, LatencyEntry>;
  tokyo: Map<string, LatencyEntry>;
} => {
  return {
    singapore: howWebSocketSG.getAllLatencies(),
    tokyo: howWebSocketJA.getAllLatencies(),
  };
};

/**
 * Get tunnel latency (client <-> tradetunnel round trip) for a given exchange
 * Returns undefined if not yet measured
 */
export const getTunnelLatencyForExchange = (exchangeName: string): number | undefined => {
  const howWebSocket = getHOWWebSocketForExchange(exchangeName);
  return howWebSocket.tunnelLatency;
};

/**
 * Get all tunnel latencies from both HOW WebSocket instances
 */
export const getAllTunnelLatencies = (): {
  singapore: number | undefined;
  tokyo: number | undefined;
} => {
  return {
    singapore: howWebSocketSG.tunnelLatency,
    tokyo: howWebSocketJA.tunnelLatency,
  };
};

export const disposeHOWWebSockets = (): void => {
  howWebSocketSG.dispose();
  howWebSocketJA.dispose();
};
