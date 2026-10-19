import type { HttpResponse, Transport } from '@tealstreet/safe-cex-wasm/host';
import type { InternalAxiosRequestConfig } from 'axios';
import type { ExchangeName, ExchangeOptions } from '../types';
import type { ResolvedRouting, ResolvedWs, RestTransport, TransportProvider } from './types';

import { AxiosHeaders } from 'axios';

import { SINGAPORE_HOW_EGRESS, TOKYO_HOW_EGRESS } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { socketMessages } from '@tealstreet/safe-cex-wasm/host';
import { getBybitSiteHeaders, isBybitSiteIdExchange } from '../exchanges/bybit/bybit.site';
import { WsDirectFallback } from '../utils/wsDirectFallback';
import { directTransport } from './direct';
import { getTransport } from './registry';
import {
  proxyAuthorizationHeader,
  TEALSTREET_REST_HEADERS,
  TEALSTREET_WS_HEADERS,
  tealstreetTransport,
} from './tealstreet/tealstreetTransport';
import { isWsRelayEnabled } from './tealstreet/wsRelay';

export interface EngineHttpRoute {
  mode: 'direct' | 'how';
  prefix: string;
  authorize: boolean;
  authorizationHeader: string;
  headers: Record<string, string>;
  tunnelUrl?: string;
}

export interface EngineWsRoute {
  host?: string;
  fallback?: string;
  relay?: string;
}

/** Serializable host policy; contains no venue credentials or proxy JWT. */
export interface EngineRoutingPlan {
  http: EngineHttpRoute;
  publicHttp?: EngineHttpRoute;
  publicHeader?: string;
  ws: { public: EngineWsRoute; private: EngineWsRoute; business: EngineWsRoute };
  wsHeaders: Record<string, string>;
  venueWsHeaders?: Record<string, string>;
  socketChannels: Array<{ contains: string; channel: 'public' | 'private' | 'business' }>;
}

type Request = Parameters<Transport['request']>[0];

const venueHeaders = (exchange: ExchangeName, options: ExchangeOptions): Record<string, string> =>
  isBybitSiteIdExchange(exchange) ? getBybitSiteHeaders(options) : {};

const socketChannels: EngineRoutingPlan['socketChannels'] = [
  { contains: '/business', channel: 'business' },
  { contains: '/ws-futures', channel: 'business' },
  { contains: '/private', channel: 'private' },
  { contains: '/user', channel: 'private' },
  { contains: '/account', channel: 'private' },
  { contains: 'listenKey=', channel: 'private' },
];

const wsRoute = (route: ResolvedWs): EngineWsRoute => ({
  ...(route.tunnelled && route.host ? { host: route.host } : {}),
  ...(route.fallback ? { fallback: route.fallback } : {}),
  ...(isWsRelayEnabled() && route.relay ? { relay: route.relay } : {}),
});

const resolved = (exchange: ExchangeName, options: ExchangeOptions, provider: TransportProvider) => {
  const routing = provider.wsRouting(exchange, options);
  const restRouting =
    (exchange === 'grvt' || exchange === 'blofin') && options.tealstreetTunnel && !options.tealstreetProxy
      ? provider.wsRouting(exchange, { ...options, tealstreetProxy: true })
      : routing;
  return { routing, restRouting };
};

/** Native launchers pass the venue's real REST base URL; never a routed URL. */
export const resolveEngineRoutingPlan = (
  exchange: ExchangeName,
  options: ExchangeOptions,
  baseURL: string,
): EngineRoutingPlan => {
  const provider = getTransport(options);
  if (provider !== directTransport && provider !== tealstreetTransport) {
    throw new Error('Custom transports must supply their own native host routing plan');
  }
  const { routing, restRouting } = resolved(exchange, options, provider);
  const http = (headers: Record<string, string>): EngineHttpRoute => {
    const route = restRoute(provider, exchange, options, restRouting, { method: 'GET', url: baseURL, headers });
    const target = route.baseURL ?? baseURL;
    if (target !== baseURL && !target.endsWith(`/${baseURL}`)) throw new Error('Unsupported native URL rewrite');
    const how =
      usesTunnel(exchange, options) && Boolean(provider.restAdapter({ exchange, options, variant: { raw: true } }));
    return {
      mode: how ? 'how' : 'direct',
      prefix: target === baseURL ? '' : target.slice(0, -baseURL.length - 1),
      authorize: route.proxied,
      authorizationHeader: proxyAuthorizationHeader(exchange),
      headers: {
        ...venueHeaders(exchange, options),
        ...(route.proxied && provider === tealstreetTransport ? TEALSTREET_REST_HEADERS : {}),
      },
      ...(how ? { tunnelUrl: routing.howEgress === 'jp' ? TOKYO_HOW_EGRESS : SINGAPORE_HOW_EGRESS } : {}),
    };
  };
  return {
    http: http(exchange.startsWith('binance') ? { 'X-MBX-APIKEY': options.key ?? '' } : {}),
    ...(exchange.startsWith('binance') ? { publicHeader: 'X-MBX-APIKEY', publicHttp: http({}) } : {}),
    ws: {
      public: wsRoute(routing.wsPublic),
      private: wsRoute(routing.wsPrivate),
      business: wsRoute(routing.wsBusiness),
    },
    wsHeaders: provider === tealstreetTransport ? { ...TEALSTREET_WS_HEADERS } : {},
    venueWsHeaders: venueHeaders(exchange, options),
    socketChannels,
  };
};

const hasHeader = (headers: Record<string, string>, name: string): boolean =>
  Object.keys(headers).some((key) => key.toLowerCase() === name.toLowerCase() && Boolean(headers[key]));

// Match the public Binance client, which deliberately bypasses ordinary proxy
// routing. Private/listen-key requests carry the key even when they are unsigned.
const publicBinanceRequest = (exchange: ExchangeName, request: Request): boolean =>
  exchange.startsWith('binance') && !hasHeader(request.headers, 'X-MBX-APIKEY');

const usesTunnel = (exchange: ExchangeName, options: ExchangeOptions): boolean =>
  exchange === 'bingx'
    ? !options.publicDataOnly && options.tealstreetTunnel !== false
    : exchange === 'weex' || exchange === 'grvt' || exchange === 'blofin'
      ? false
      : Boolean(options.tealstreetTunnel);

const restRoute = (
  provider: TransportProvider,
  exchange: ExchangeName,
  options: ExchangeOptions,
  routing: ResolvedRouting,
  request: Request,
): RestTransport => {
  if (publicBinanceRequest(exchange, request)) return { baseURL: request.url, proxied: false };
  return provider.rest({
    exchange,
    options,
    routing,
    config: { baseURL: request.url, headers: request.headers },
  });
};

const responseHeaders = (headers: unknown): string => {
  if (!headers || typeof headers !== 'object') return '';
  return Object.entries(headers)
    .map(([name, value]) => `${name}: ${String(value)}`)
    .join('\n');
};

const httpTransport = (
  exchange: ExchangeName,
  options: ExchangeOptions,
  transport: Transport,
  provider: TransportProvider,
  routing: ResolvedRouting,
): Transport['request'] => {
  const tunnel = usesTunnel(exchange, options);
  const adapter = tunnel
    ? provider.restAdapter({
        exchange,
        options,
        variant: { region: routing.howEgress === 'jp' ? 'JA' : 'SG', raw: true },
      })
    : undefined;
  return async (request): Promise<HttpResponse> => {
    const route = restRoute(provider, exchange, options, routing, request);
    const config = {
      method: request.method,
      url: route.baseURL ?? request.url,
      baseURL: request.url,
      headers: AxiosHeaders.from({ ...request.headers, ...venueHeaders(exchange, options) }),
      data: request.body,
      timeout: 20_000,
    } as InternalAxiosRequestConfig;
    if (route.proxied) await route.authorize?.(config);
    if (adapter) {
      const response = await adapter(config);
      // A raw tunnel response keeps native IDs and signed decimal lexemes intact.
      if (typeof response.data !== 'string') throw new Error('Engine tunnel requires a raw response body');
      return {
        status: response.status,
        headers: responseHeaders(response.headers),
        body: new TextEncoder().encode(response.data),
      };
    }
    return transport.request({
      ...request,
      url: config.url!,
      headers: config.headers.toJSON() as Record<string, string>,
    });
  };
};

/** Account options are already normalized by accountToExchangeOptions. */
export const createEngineTransport = (
  exchange: ExchangeName,
  options: ExchangeOptions,
  transport: Transport,
): Transport => {
  const provider = getTransport(options);
  const { routing, restRouting } = resolved(exchange, options, provider);
  const routes = {
    public: wsRoute(routing.wsPublic),
    private: wsRoute(routing.wsPrivate),
    business: wsRoute(routing.wsBusiness),
  };
  const fallbacks = new Map<string, WsDirectFallback>();
  return {
    webSocketHeaders: transport.webSocketHeaders,
    now: () => transport.now(),
    setTimeout: (fn, ms) => transport.setTimeout(fn, ms),
    clearTimeout: (handle) => transport.clearTimeout(handle),
    request: httpTransport(exchange, options, transport, provider, restRouting),
    connect: (url, handlers) => {
      const channel =
        handlers.channel ?? socketChannels.find((rule) => url.includes(rule.contains))?.channel ?? 'public';
      const route = routes[channel];
      const key = `${channel}:${url.split('?')[0]}`;
      let fallback = fallbacks.get(key);
      if (!fallback) {
        fallback = new WsDirectFallback();
        fallbacks.set(key, fallback);
      }
      const target = fallback.resolveUrl(route.host ?? url, route.relay, route.fallback);
      const connection = provider.wsConnection(target);
      const upgrade = AxiosHeaders.from(handlers.headers ?? {});
      if (transport.webSocketHeaders) {
        upgrade.set(venueHeaders(exchange, options));
        if (connection) upgrade.set(connection.headers);
      }
      const headers = upgrade.toJSON() as Record<string, string>;
      const receive = (text: string) => {
        fallback.onMessage();
        handlers.onMessage(text);
      };
      if (connection?.kind !== 'relay') return transport.connect(target, { ...handlers, headers, onMessage: receive });
      let socket: { send(text: string): void; close(): void } | undefined;
      let closed = false;
      let opened = false;
      const pending: string[] = [];
      void import('@tealstreet/http-over-websocket/src/RelayWebSocket')
        .then(({ RelayWebSocket }) => {
          if (closed) return;
          const relay = new RelayWebSocket(target, [], {
            headers: { ...connection.headers, ...headers },
            ALPNProtocols: ['http/1.1'],
          });
          socket = relay;
          relay.addEventListener('open', () => {
            if (!closed) {
              opened = true;
              for (const text of pending) relay.send(text);
              pending.length = 0;
              handlers.onOpen();
            }
          });
          const decode = socketMessages(
            (text) => {
              if (!closed) receive(text);
            },
            () => {
              if (!closed) {
                closed = true;
                pending.length = 0;
                relay.close(1007);
                handlers.onClose(1007);
              }
            },
          );
          relay.addEventListener('message', (event) => {
            if (!closed) decode((event as MessageEvent).data);
          });
          relay.addEventListener('close', (event) => {
            if (!closed) {
              closed = true;
              handlers.onClose((event as CloseEvent).code);
            }
          });
        })
        .catch(() => {
          if (!closed) {
            closed = true;
            handlers.onClose(1006);
          }
        });
      return {
        send: (text) => {
          if (closed) return;
          if (socket && opened) socket.send(text);
          else {
            if (pending.length >= 100) {
              closed = true;
              pending.length = 0;
              socket?.close();
              handlers.onClose(1006);
            } else pending.push(text);
          }
        },
        close: () => {
          closed = true;
          pending.length = 0;
          socket?.close();
        },
      };
    },
  };
};
