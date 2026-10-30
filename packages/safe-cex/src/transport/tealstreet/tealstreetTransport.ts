import type { CreateAxiosDefaults } from 'axios';
import type { ExchangeName, ExchangeOptions } from '../../types';
import type { ResolvedRouting, TransportProvider } from '../types';

import { isRelayUrl } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { bindProxyTokenLookup, getProxyToken, ProxyTokenUnavailableError } from '../../accessToken';
import {
  axiosHttpOverWebsocketAdapter,
  bigIntAxiosHttpOverWebsocketAdapter,
  createAxiosHttpOverWebsocketAdapter,
} from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { isNodeOrBunRuntime } from '../../utils/runtime';
import { markRestProxyHeader } from '../../utils/venueRestDiagnostics';
import { resolveExchangeRouting } from './routing/resolveExchangeRouting';
import { getPrivateProxyHost, getPublicCorsProxy } from './transportResolution';

/** Headers the tradetunnel's CloudFront/WAF expects on a non-browser WebSocket upgrade. */
export const TEALSTREET_WS_HEADERS = {
  Origin: 'https://preview.tealstreet.io',
  'User-Agent':
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36',
};

export const TEALSTREET_REST_HEADERS = {
  Origin: 'https://trade.tealstreet.io',
  Referer: 'https://trade.tealstreet.io/',
  'User-Agent':
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36',
  'sec-fetch-mode': 'cors',
  'sec-fetch-site': 'cross-site',
  'sec-fetch-dest': 'empty',
};

/** The edge's own refusal of our proxy authorization, as opposed to a venue's 401. */
export const isTealstreetProxyAuthFailureBody = (body: unknown): boolean => {
  const message =
    typeof body === 'string'
      ? body
      : body && typeof body === 'object' && 'message' in body && typeof body.message === 'string'
        ? body.message
        : '';

  return message === 'Authorization header is missing' || message === 'Invalid JWT';
};

/** How long a proxied request waits for a warming proxy token. */
export const PROXY_TOKEN_WAIT_MS = 15_000;
const PROXY_TOKEN_POLL_MS = 500;

const proxyAuthorizationMissingMessage = (exchange: ExchangeName): string =>
  `[${exchange}] Tealstreet proxy authorization missing: no proxy token within ${PROXY_TOKEN_WAIT_MS / 1000}s, so the private request was not sent`;

/** A non-empty proxy token, or `ProxyTokenUnavailableError` once the wait is over. */
const awaitProxyToken = async (exchange: ExchangeName, signal?: AbortSignal): Promise<string> => {
  // Bound before the first await: the polls below run outside the caller's token scope.
  const lookup = bindProxyTokenLookup();
  const deadline = Date.now() + PROXY_TOKEN_WAIT_MS;
  for (;;) {
    const token = await lookup();
    if (token) return token;
    if (signal?.aborted || Date.now() >= deadline)
      throw new ProxyTokenUnavailableError(proxyAuthorizationMissingMessage(exchange));
    await new Promise((resolve) => setTimeout(resolve, PROXY_TOKEN_POLL_MS));
  }
};

export const proxyAuthorizationHeader = (exchange: ExchangeName): string =>
  exchange === 'grvt' || exchange === 'mexc' || exchange === 'paradex' || exchange === 'breakoutprop'
    ? 'Tealstreet-Authorization'
    : 'Authorization';

const isTealstreetWebSocketUrl = (url: string): boolean => {
  try {
    const hostname = new URL(url).hostname;
    return hostname === 'tealstreet.io' || hostname.endsWith('.tealstreet.io');
  } catch {
    return false;
  }
};

export const getProxyForAxios = (
  config: CreateAxiosDefaults,
  options: ExchangeOptions,
  exchange: ExchangeName,
  routing?: ResolvedRouting,
  credentialIntent?: 'public' | 'primary' | 'secondary',
): string =>
  config.baseURL?.includes('binance') &&
  (credentialIntent !== undefined
    ? credentialIntent === 'public'
    : options.publicDataOnly ||
      !Object.entries(config.headers ?? {}).some(
        ([name, value]) => name.toLowerCase() === 'x-mbx-apikey' && Boolean(value),
      ))
    ? getPublicCorsProxy()
    : options.tealstreetProxy
      ? (routing ?? resolveExchangeRouting(exchange, options)).restProxyHost
      : '';

/** Tealstreet's own proxy pools, tradetunnel and Caddy passthroughs. */
export const tealstreetTransport: TransportProvider = {
  name: 'tealstreet',

  rest: ({ exchange, options, config, routing, serverHeaders, credentialIntent }) => {
    // GRVT and blofin cannot use the tunnel, so a tunnel request for them means the proxy.
    const effectiveOptions =
      (exchange === 'grvt' || exchange === 'blofin') && options.tealstreetTunnel
        ? { ...options, tealstreetProxy: true }
        : options;

    const privateProxyUrl =
      options.tealstreetTunnel && exchange !== 'blofin' && exchange !== 'grvt'
        ? ''
        : getProxyForAxios(config, effectiveOptions, exchange, routing, credentialIntent);
    const proxied = privateProxyUrl !== '';
    const baseURLWithProxy = proxied ? `${privateProxyUrl}/${config?.baseURL}` : config?.baseURL;

    return {
      baseURL: options.overrideProxyUrl ? `${options.overrideProxyUrl}/${config?.baseURL}` : baseURLWithProxy,
      proxied,
      authorize: async (request) => {
        // The browser SharedWorker and the CLI return '' while warming. A request
        // sent without the header gets the edge's 401, which reads as a refused
        // venue key, so it waits for a token or fails as our own authorization.
        const token = await awaitProxyToken(exchange, request.signal as AbortSignal | undefined);

        // Use Tealstreet-Authorization when the venue has its own Authorization
        // semantics. Breakout DXtrade and MEXC WEB/APP sessions own Authorization; MEXC
        // OPEN-API auth rides on ApiKey/Request-Time/Signature headers.
        // The proxy will check Tealstreet-Authorization first, then fall back to Authorization
        markRestProxyHeader(request);
        request.headers[proxyAuthorizationHeader(exchange)] = `Bearer ${token}`;

        // Add browser-like headers for CLI requests to proxy
        // Cloudflare Worker requires these headers for CORS validation
        if (serverHeaders || isNodeOrBunRuntime()) {
          Object.assign(request.headers, TEALSTREET_REST_HEADERS);
        }
      },
    };
  },

  restAdapter: ({ variant }) => {
    if (variant === 'default') return axiosHttpOverWebsocketAdapter;
    if (variant === 'bigInt') return bigIntAxiosHttpOverWebsocketAdapter;
    return createAxiosHttpOverWebsocketAdapter({
      region: variant.region,
      ...(variant.WebSocket ? { WebSocket: variant.WebSocket } : {}),
      ...(variant.raw ? { customParser: (body) => body } : {}),
    });
  },

  proxyPrefix: ({ exchange, options, config }) => getProxyForAxios(config, options, exchange),

  publicCorsProxy: () => getPublicCorsProxy(),

  privateProxy: (exchange) => {
    // GRVT's venue owns `Authorization`, so its proxy credential rides a separate header.
    const header = exchange === 'grvt' ? 'Tealstreet-Authorization' : 'Authorization';
    return {
      url: getPrivateProxyHost(exchange === 'grvt' ? 'tiny' : 'jp'),
      resolveHeaders: async () => {
        const token = await getProxyToken();
        return token ? { [header]: `Bearer ${token}` } : {};
      },
    };
  },

  serverProxyUrl: () => getPrivateProxyHost('jp'),

  // A tunnelled account already leaves through a whitelisted egress.
  requireProxy: (options) => ({ ...options, tealstreetProxy: !options.tealstreetTunnel }),

  wsRouting: (exchange, options) => resolveExchangeRouting(exchange, options),

  wsConnection: (url) => {
    // Checked before the generic tealstreet.io branch because a relay url IS a tealstreet.io host.
    if (isRelayUrl(url)) return { kind: 'relay', headers: TEALSTREET_WS_HEADERS };
    if (isTealstreetWebSocketUrl(url)) return { kind: 'headers', headers: TEALSTREET_WS_HEADERS };
    return undefined;
  },
};
