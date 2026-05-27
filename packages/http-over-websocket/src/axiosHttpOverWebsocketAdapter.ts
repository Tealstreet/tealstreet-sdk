import axios, { AxiosAdapter } from 'axios';
import { nanoid } from 'nanoid';

import { sleep } from '@tealstreet/sleep/src/sleep';
import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';

import { getProxyToken, isTokenExpired, shouldSimulateHttpOverWebsocket } from './config';
import { HOWWebSocket } from './HOWWebSocket';
import { CreateAxiosHttpOverWebsocketAdapterOptions, HOWWebSocketAuthState, LatencyEntry } from './types';

// Check if trade tunnel logging is enabled via environment variable
const LOG_TRADETUNNEL =
  typeof process !== 'undefined' && process.env?.NEXT_PUBLIC_LOG_TRADETUNNEL_TO_CONSOLE_LOG === '1';

/**
 * Strip proxy URL from the full URL to show just the exchange endpoint
 */
function stripProxyUrl(url: string): string {
  // Remove common proxy patterns like wss://xxx.tealstreet.io/tunnel or similar
  // The actual exchange URL is typically after the proxy forwards it
  return url;
}

/**
 * Log trade tunnel request
 */
function logTradeTunnelRequest(request: { url: string; method: string; params?: any; data?: any }) {
  if (!LOG_TRADETUNNEL) return;

  const logData: any = {
    url: stripProxyUrl(request.url),
    method: request.method,
  };

  if (request.params && Object.keys(request.params).length > 0) {
    logData.params = request.params;
  }

  if (request.data && Object.keys(request.data).length > 0) {
    logData.data = request.data;
  }

  console.log('[TradeTunnel] REQUEST:', logData);
}

/**
 * Log trade tunnel response
 */
function logTradeTunnelResponse(request: { url: string; method: string }, response: any) {
  if (!LOG_TRADETUNNEL) return;

  const logData: any = {
    url: stripProxyUrl(request.url),
    method: request.method,
  };

  if (response?.data !== undefined) {
    logData.data = response.data;
  } else if (response !== undefined) {
    logData.response = response;
  }

  console.log('[TradeTunnel] RESPONSE:', logData);
}

const TUNNEL_PATH =
  typeof process !== 'undefined' && process.env?.NEXT_PUBLIC_TRADETUNNEL_STAGING === '1'
    ? '/tunnel-staging'
    : '/tunnel';

const SIGNAPORE_HOW_EGRESS = `wss://fsjnpy143pbz.asia.tealstreet.io${TUNNEL_PATH}`;
const TOKYO_HOW_EGRESS = `wss://fsjnpy143pbz.jp.tealstreet.io${TUNNEL_PATH}`;

// Exchanges that should use Tokyo egress server
const TOKYO_EGRESS_EXCHANGES = [
  // url pattern, so both arkham & arkm
  'arkham',
  'arkm',
  'hlnode',
  'hyperliquid',
  'hyperliquidspot',
  'bitunix',
  'bitrue',
  'bitget',
  'binance',
  'kucoin',
  'kraken',
  'breakout',
  'aster',
  'paradex',
  'grvt',
  'backpack',
];

// Exchanges that should bypass the proxy and connect directly
const BYPASS_PROXY_EXCHANGES = ['grvt'];

// Create regex pattern for Tokyo egress exchanges
const TOKYO_EGRESS_PATTERN = new RegExp(TOKYO_EGRESS_EXCHANGES.join('|'), 'i');
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
      howWebSocket = config.baseURL && TOKYO_EGRESS_PATTERN.test(config.baseURL) ? howWebSocketJA : howWebSocketSG;
    }
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
        logTradeTunnelRequest({
          url: websocketRequest.url,
          method: websocketRequest.method,
          params: websocketRequest.params,
          data: websocketRequest.data,
        });

        const response = await axios({
          headers: websocketRequest.headers,
          method: websocketRequest.method,
          url: websocketRequest.url,
          data: websocketRequest.data,
          params: websocketRequest.params,
        });

        // Log response (simulated mode)
        logTradeTunnelResponse(websocketRequest, response);

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

        let timeoutTimer = setTimeout(() => {
          howWebSocket.off('sub:' + nonce);
          const timeoutError = new Error(`WSoverHTTP request timed out: ${config.url}`);
          timeoutError.name = 'TradeTunnelTimeoutError';
          (timeoutError as any).url = websocketRequest.url;
          (timeoutError as any).method = websocketRequest.method;
          reject(timeoutError);
        }, 20_000);

        howWebSocket.once('sub:' + nonce, (data: any) => {
          clearTimeout(timeoutTimer);

          // Log response before processing
          logTradeTunnelResponse(websocketRequest, data);

          if (data && typeof data === 'object' && data.error) {
            // Create a proper Error object with URL context for better Sentry tracking
            const tunnelError = new Error(`${data.error} (URL: ${websocketRequest.url})`);
            tunnelError.name = 'TradeTunnelError';
            (tunnelError as any).url = websocketRequest.url;
            (tunnelError as any).method = websocketRequest.method;
            (tunnelError as any).originalError = data.error;
            // Attach the caller's config so error-path interceptors (e.g.
            // `error?.config?.__suppressExchangeError` in Lighter) can see
            // request-time suppression flags through the tunnel.
            (tunnelError as any).config = config;
            reject(tunnelError);
            return;
          }

          if (customParser && data.data_raw) {
            data.data = customParser(data.data_raw);
          }
          // Attach the original adapter config so response interceptors see
          // the caller's custom keys/headers (e.g. __suppressExchangeError).
          // Tunnel frames carry only {data, status, headers}, so without this
          // the response config is empty and per-request suppression flags
          // are lost.
          resolve({ ...data, config });
        });

        let data: any;

        data = parseWebsocketData(customParser || JSON.parse, websocketRequest);

        // Check if this exchange should bypass the proxy
        const shouldBypassProxy = config.baseURL && BYPASS_PROXY_PATTERN.test(config.baseURL);

        // Log request before sending
        logTradeTunnelRequest({
          url: websocketRequest.url,
          method: websocketRequest.method,
          params: websocketRequest.params,
          data: data,
        });

        howWebSocket.send({
          type: 'rpc.http',
          ...(customParser ? { format: 'raw' } : {}),
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
  return TOKYO_EGRESS_PATTERN.test(exchangeName) ? howWebSocketJA : howWebSocketSG;
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
