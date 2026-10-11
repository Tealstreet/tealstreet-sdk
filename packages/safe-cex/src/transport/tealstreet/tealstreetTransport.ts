import type { CreateAxiosDefaults } from 'axios';
import type { ExchangeName, ExchangeOptions } from '../../types';
import type { TransportProvider } from '../types';

import { isRelayUrl } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { getProxyToken } from '../../accessToken';
import {
  axiosHttpOverWebsocketAdapter,
  bigIntAxiosHttpOverWebsocketAdapter,
  createAxiosHttpOverWebsocketAdapter,
} from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { resolveExchangeRouting } from './routing/resolveExchangeRouting';
import { isNodeOrBunRuntime } from '../../utils/runtime';
import { getPublicCorsProxy, PRIVATE_PROXY_GRVT_TINY, PRIVATE_PROXY_JP } from './transportResolution';
import { markRestProxyHeader } from '../../utils/venueRestDiagnostics';

/** Headers the tradetunnel's CloudFront/WAF expects on a non-browser WebSocket upgrade. */
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

export const getProxyForAxios = (
  config: CreateAxiosDefaults,
  options: ExchangeOptions,
  exchange: ExchangeName,
): string =>
  config.baseURL?.includes('binance') &&
  (options.publicDataOnly || !Boolean((config?.headers as any)?.['X-MBX-APIKEY']))
    ? getPublicCorsProxy()
    : options.tealstreetProxy
      ? resolveExchangeRouting(exchange, options).restProxyHost
      : '';

/** Tealstreet's own proxy pools, tradetunnel and Caddy passthroughs. */
export const tealstreetTransport: TransportProvider = {
  name: 'tealstreet',

  rest: ({ exchange, options, config }) => {
    // GRVT and blofin cannot use the tunnel, so a tunnel request for them means the proxy.
    const effectiveOptions =
      (exchange === 'grvt' || exchange === 'blofin') && options.tealstreetTunnel
        ? { ...options, tealstreetProxy: true }
        : options;

    const privateProxyUrl =
      options.tealstreetTunnel && exchange !== 'blofin' && exchange !== 'grvt'
        ? ''
        : getProxyForAxios(config, effectiveOptions, exchange);
    const proxied = privateProxyUrl !== '';
    const baseURLWithProxy = proxied ? `${privateProxyUrl}/${config?.baseURL}` : config?.baseURL;

    return {
      baseURL: options.overrideProxyUrl ? `${options.overrideProxyUrl}/${config?.baseURL}` : baseURLWithProxy,
      proxied,
      authorize: async (request) => {
        // Get proxy token from SharedWorker (handles all fetching centrally)
        const token = await getProxyToken();

        // Use Tealstreet-Authorization when the venue has its own Authorization
        // semantics. MEXC futures uses Authorization for WEB/APP sessions, while
        // OPEN-API auth rides on ApiKey/Request-Time/Signature headers.
        // The proxy will check Tealstreet-Authorization first, then fall back to Authorization
        //
        // Only when there is something to send. `Bearer ` with nothing after it is
        // a MALFORMED credential, not an absent one, and it is strictly worse:
        // the edge reports it as "Authorization header is missing", which is then
        // the only string anyone downstream has to go on — and it points at the
        // venue rather than at us. A host can still hold '' here legitimately
        // (the browser SharedWorker and the CLI both return it while warming);
        // exchange-server cannot, since `getProxyToken` throws for it instead.
        markRestProxyHeader(request, token ? 'present' : 'empty');

        if (token) {
          if (exchange === 'grvt' || exchange === 'mexc') {
            request.headers['Tealstreet-Authorization'] = `Bearer ${token}`;
          } else {
            request.headers.Authorization = `Bearer ${token}`;
          }
        }

        // Add browser-like headers for CLI requests to proxy
        // Cloudflare Worker requires these headers for CORS validation
        if (isNodeOrBunRuntime()) {
          request.headers.Origin = 'https://trade.tealstreet.io';
          request.headers.Referer = 'https://trade.tealstreet.io/';
          request.headers['User-Agent'] =
            'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36';
          request.headers['sec-fetch-mode'] = 'cors';
          request.headers['sec-fetch-site'] = 'cross-site';
          request.headers['sec-fetch-dest'] = 'empty';
        }
      },
    };
  },

  restAdapter: ({ variant }) => {
    if (variant === 'default') return axiosHttpOverWebsocketAdapter;
    if (variant === 'bigInt') return bigIntAxiosHttpOverWebsocketAdapter;
    return createAxiosHttpOverWebsocketAdapter({ region: variant.region });
  },

  proxyPrefix: ({ exchange, options, config }) => getProxyForAxios(config, options, exchange),

  publicCorsProxy: () => getPublicCorsProxy(),

  privateProxy: (exchange) => {
    // GRVT's venue owns `Authorization`, so its proxy credential rides a separate header.
    const header = exchange === 'grvt' ? 'Tealstreet-Authorization' : 'Authorization';
    return {
      url: exchange === 'grvt' ? PRIVATE_PROXY_GRVT_TINY : PRIVATE_PROXY_JP,
      resolveHeaders: async () => {
        const token = await getProxyToken();
        return token ? { [header]: `Bearer ${token}` } : {};
      },
    };
  },

  serverProxyUrl: () => PRIVATE_PROXY_JP,

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
