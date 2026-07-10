import axios, { AxiosAdapter } from 'axios';
import { nanoid } from 'nanoid';

import { decompress } from '@tealstreet/compression/src/decompress';
import { sleep } from '@tealstreet/sleep/src/sleep';
import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';

import { getProxyToken, isTokenExpired, shouldSimulateHttpOverWebsocket } from './config';
import { isTokyoEgress } from './egressRegion';
import { HOWWebSocket } from './HOWWebSocket';
import { CreateAxiosHttpOverWebsocketAdapterOptions, HOWWebSocketAuthState, LatencyEntry } from './types';

const MAX_LOG_STRING_LENGTH = 500;
const MAX_LOG_ARRAY_LENGTH = 20;
const MAX_LOG_OBJECT_KEYS = 50;
const MAX_LOG_DEPTH = 4;
const REDACTED = '[REDACTED]';
const TRUNCATED = '[TRUNCATED]';
export const TRADE_TUNNEL_RPC_VERSION = 3;
const TRADE_TUNNEL_ACCEPT_ENCODING = ['gzip'] as const;
const TRADE_TUNNEL_GZIP_BASE64_ENCODING = 'gzip+base64';
const SENSITIVE_LOG_KEY_PATTERN =
  /authorization|cookie|password|passphrase|secret|token|signature|api[-_]?key|apikey|access[-_]?key|x-mbx-apikey|x-bitmex/i;

type TradeTunnelRouteContext = {
  selectedRegion: 'JA' | 'SG';
  tunnelHost: 'tokyo' | 'singapore';
  forcedRegion?: 'JA' | 'SG';
  bypassProxy?: boolean;
  simulated?: boolean;
};

function decodeBase64(encoded: string): ArrayBuffer {
  const binary = globalThis.atob(encoded);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return bytes.buffer;
}

async function decodeTradeTunnelRawBody(data: any): Promise<string | undefined> {
  if (
    data?.data_encoding === TRADE_TUNNEL_GZIP_BASE64_ENCODING &&
    typeof data?.data_encoded === 'string'
  ) {
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

const TUNNEL_PATH =
  typeof process !== 'undefined' && process.env?.NEXT_PUBLIC_TRADETUNNEL_STAGING === '1'
    ? '/tunnel-staging'
    : '/tunnel';

const SIGNAPORE_HOW_EGRESS = `wss://fsjnpy143pbz.asia.tealstreet.io${TUNNEL_PATH}`;
const TOKYO_HOW_EGRESS = `wss://fsjnpy143pbz.jp.tealstreet.io${TUNNEL_PATH}`;

// Exchanges that should bypass the proxy and connect directly
const BYPASS_PROXY_EXCHANGES = ['grvt'];

const BYPASS_PROXY_PATTERN = new RegExp(BYPASS_PROXY_EXCHANGES.join('|'), 'i');

export let howWebSocketSG = new HOWWebSocket(SIGNAPORE_HOW_EGRESS);
export let howWebSocketJA = new HOWWebSocket(TOKYO_HOW_EGRESS);

const createAxiosHttpOverWebsocketAdapter = (
  options?: CreateAxiosHttpOverWebsocketAdapterOptions | ((str: string) => any),
): AxiosAdapter => {
  // Support legacy signature: createAxiosHttpOverWebsocketAdapter(customParser)
  const opts: CreateAxiosHttpOverWebsocketAdapterOptions =
    typeof options === 'function' ? { customParser: options } : options || {};
  const { customParser, region } = opts;

  return (config) => {
    let badAuthRetries = 0;
    // Use forced region if provided, otherwise auto-detect from URL
    let howWebSocket: HOWWebSocket;
    if (region === 'JA') {
      howWebSocket = howWebSocketJA;
    } else if (region === 'SG') {
      howWebSocket = howWebSocketSG;
    } else {
      howWebSocket = config.baseURL && isTokyoEgress(config.baseURL) ? howWebSocketJA : howWebSocketSG;
    }
    const selectedRegion = howWebSocket === howWebSocketJA ? 'JA' : 'SG';
    const baseRouteContext: TradeTunnelRouteContext = {
      selectedRegion,
      tunnelHost: selectedRegion === 'JA' ? 'tokyo' : 'singapore',
      ...(region ? { forcedRegion: region } : {}),
    };
    return new Promise(async (resolve, reject) => {
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
      if (isGrvtRequest) {
        if (config.data !== null && config.data !== undefined) {
          websocketRequest.data = config.data;
        }
      } else {
        websocketRequest.data = config.data || {};
      }

      if (shouldSimulateHttpOverWebsocket()) {
        // Log request before sending (simulated mode)
        logTradeTunnelRequest(
          {
            url: websocketRequest.url,
            method: websocketRequest.method,
            params: websocketRequest.params,
            data: websocketRequest.data,
          },
          { ...baseRouteContext, simulated: true },
        );

        const response = await axios({
          headers: websocketRequest.headers,
          method: websocketRequest.method,
          url: websocketRequest.url,
          data: websocketRequest.data,
          params: websocketRequest.params,
        });

        // Log response (simulated mode)
        logTradeTunnelResponse(websocketRequest, response, { ...baseRouteContext, simulated: true });

        // Attach the original adapter config so response interceptors see the
        // caller's custom keys/headers (e.g. __suppressExchangeError). The
        // inner axios call attached its own config; replace it with ours.
        return resolve({ ...response, config });
      } else {
        const MAX_TOKEN_RETRIES = 120; // Max retries for getting a valid token (120 * 500ms = 60s max)

        // Helper to get a valid (non-empty, non-expired) token with retries
        const getValidToken = async (): Promise<string> => {
          for (let i = 0; i < MAX_TOKEN_RETRIES; i++) {
            const token = await getProxyToken();
            if (token && !isTokenExpired(token)) {
              return token;
            }
            if (i < MAX_TOKEN_RETRIES - 1) {
              // Log every 10 attempts (5 seconds) to avoid spam
              if (i % 10 === 0) {
                console.log(`[HOW] Waiting for valid token... (attempt ${i + 1}/${MAX_TOKEN_RETRIES})`);
              }
              await sleep(500);
            }
          }
          // Return whatever we have after max retries
          const finalToken = await getProxyToken();
          if (!finalToken) {
            console.error('[HOW] Failed to get valid token after 60s - backend may be down');
          }
          return finalToken;
        };

        // Helper to safely send token with retry on WebSocket state issues
        const safeSendToken = async (token: string, retries = 0): Promise<void> => {
          // Never send an empty token
          if (!token) {
            console.error('[HOW] Attempted to send empty token, skipping');
            return;
          }

          const MAX_RETRIES = 5;
          try {
            // Wait for WebSocket to be ready
            while (!howWebSocket.isOpen || !howWebSocket.isTimeSynced) {
              await sleep(25);
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
            howWebSocket.sendToken({ type: 'rpc.auth', token, source });
          } catch (err: any) {
            if (err?.name === 'WebSocketNotReadyError' && retries < MAX_RETRIES) {
              // WebSocket closed between our check and send, wait for reconnect
              console.log(
                `[HOW] Token send failed (state: ${err.webSocketState}), waiting for reconnect... (retry ${retries + 1}/${MAX_RETRIES})`,
              );
              await sleep(500);
              return safeSendToken(token, retries + 1);
            }
            throw err;
          }
        };

        if (!howWebSocket.isInitialized) {
          howWebSocket.connectAndSubscribe();
        }
        while (!howWebSocket.isOpen || !howWebSocket.isTimeSynced) {
          await sleep(25);
        }

        // Get proxy token from SharedWorker (handles all fetching centrally)
        // Wait for a valid token before proceeding
        let token = await getValidToken();

        if (!howWebSocket.isTokenSent && token) {
          await safeSendToken(token);
        }

        while (howWebSocket.authState !== HOWWebSocketAuthState.Good) {
          await sleep(25);
          if (howWebSocket.authState === HOWWebSocketAuthState.Bad) {
            await sleep(500);

            // If token is NOT expired but auth still fails, it means user auth is broken
            // (not just an expired token issue), so increment retries to eventually fail out
            if (token && !isTokenExpired(token)) {
              badAuthRetries++;
            }

            // if user auth is the problem, fail out after 10 attempts completely
            if (badAuthRetries > 10) {
              throw new Error('Cannot refresh auth token, please reload the terminal');
            }

            // Get fresh valid token from SharedWorker
            token = await getValidToken();

            if (token) {
              await safeSendToken(token);
            }
          }
        }

        const nonce = nanoid();
        const shouldBypassProxy = config.baseURL && BYPASS_PROXY_PATTERN.test(config.baseURL);
        const routeContext: TradeTunnelRouteContext = {
          ...baseRouteContext,
          bypassProxy: Boolean(shouldBypassProxy),
        };

        let timeoutTimer = setTimeout(() => {
          howWebSocket.off('sub:' + nonce);
          const timeoutError = new Error(`WSoverHTTP request timed out: ${config.url}`);
          timeoutError.name = 'TradeTunnelTimeoutError';
          (timeoutError as any).url = websocketRequest.url;
          (timeoutError as any).method = websocketRequest.method;
          (timeoutError as any).tradeTunnelRoute = routeContext;
          reject(timeoutError);
        }, 20_000);

        howWebSocket.once('sub:' + nonce, async (data: any) => {
          clearTimeout(timeoutTimer);

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

            if (customParser) {
              const rawBody = await decodeTradeTunnelRawBody(data);
              if (rawBody !== undefined) {
                data.data = customParser(rawBody);
              }
            }
            // Attach the original adapter config so response interceptors see
            // the caller's custom keys/headers (e.g. __suppressExchangeError).
            // Tunnel frames carry only {data, status, headers}, so without this
            // the response config is empty and per-request suppression flags
            // are lost.
            resolve({ ...data, config });
          } catch (error) {
            const tunnelError = error instanceof Error ? error : new Error(String(error));
            tunnelError.name = 'TradeTunnelResponseDecodeError';
            (tunnelError as any).url = websocketRequest.url;
            (tunnelError as any).method = websocketRequest.method;
            (tunnelError as any).tradeTunnelRoute = routeContext;
            (tunnelError as any).config = config;
            reject(tunnelError);
          }
        });

        let data: any;

        data = parseWebsocketData(customParser || JSON.parse, websocketRequest);

        // Log request before sending
        logTradeTunnelRequest(
          {
            url: websocketRequest.url,
            method: websocketRequest.method,
            params: websocketRequest.params,
            data: data,
          },
          routeContext,
        );

        howWebSocket.send({
          type: 'rpc.http',
          rpc_version: TRADE_TUNNEL_RPC_VERSION,
          ...(customParser
            ? { format: 'raw', accept_encoding: TRADE_TUNNEL_ACCEPT_ENCODING }
            : {}),
          ...(shouldBypassProxy ? { bypass_proxy: true } : {}),
          nonce,
          arg: { ...websocketRequest, data },
        });
      }
    });
  };
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
