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
  initialSilentConnects?: number;
  ignoredMessages?: string[];
  directUrls?: string[];
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
const orderlyExchanges = new Set<ExchangeName>(['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi']);

export const assertEngineAccountRouting = (options: ExchangeOptions): void => {
  if (options.secondary_key || options.secondary_secret) {
    throw new Error('Rust engine secondary signing keys are unsupported; use a primary-key account');
  }
};

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
  ...(route.initialSilentConnects !== undefined ? { initialSilentConnects: route.initialSilentConnects } : {}),
  ...(route.ignoredMessages ? { ignoredMessages: route.ignoredMessages } : {}),
  ...(route.directUrls ? { directUrls: route.directUrls } : {}),
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
  assertEngineAccountRouting(options);
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

// Factory adapter gates are distinct from saved-account metadata defaults.
const usesTunnel = (exchange: ExchangeName, options: ExchangeOptions): boolean =>
  exchange === 'hyperliquid' || exchange === 'hyperliquidspot'
    ? !options.publicDataOnly
    : exchange === 'bingx'
      ? !options.publicDataOnly && options.tealstreetTunnel !== false
      : !['weex', 'grvt', 'blofin', 'mexc'].includes(exchange) && Boolean(options.tealstreetTunnel);

const restRoute = (
  provider: TransportProvider,
  exchange: ExchangeName,
  options: ExchangeOptions,
  routing: ResolvedRouting,
  request: Request,
  serverHeaders = false,
): RestTransport => {
  if (publicBinanceRequest(exchange, request)) return { baseURL: request.url, proxied: false };
  // Factories resolve a base URL, never a signed query. Keep request data out
  // of host selection and append its exact bytes after the provider rewrites it.
  const baseURL = request.url.match(/^https?:\/\/[^/?#]+/i)?.[0];
  if (!baseURL) throw new Error('Engine HTTP requires an absolute venue URL');
  const route = provider.rest({
    exchange,
    options,
    routing,
    serverHeaders,
    config: { baseURL, headers: request.headers },
  });
  return { ...route, baseURL: `${route.baseURL ?? baseURL}${request.url.slice(baseURL.length)}` };
};

const responseHeaders = (headers: unknown): string => {
  if (!headers || typeof headers !== 'object') return '';
  return Object.entries(headers)
    .map(([name, value]) => `${name}: ${String(value)}`)
    .join('\n');
};

const untilAborted = <T>(promise: Promise<T>, signal: AbortSignal): Promise<T> =>
  new Promise((resolve, reject) => {
    if (signal.aborted) {
      void promise.catch(() => undefined);
      reject(signal.reason);
      return;
    }
    const onAbort = () => reject(signal.reason);
    signal.addEventListener('abort', onAbort, { once: true });
    promise.then(
      (value) => {
        signal.removeEventListener('abort', onAbort);
        resolve(value);
      },
      (error) => {
        signal.removeEventListener('abort', onAbort);
        reject(error);
      },
    );
  });

const httpTransport = (
  exchange: ExchangeName,
  options: ExchangeOptions,
  transport: Transport,
  provider: TransportProvider,
  routing: ResolvedRouting,
): { request: Transport['request']; dispose(): void } => {
  const tunnel = usesTunnel(exchange, options);
  const adapter = tunnel
    ? provider.restAdapter({
        exchange,
        options,
        variant: { region: routing.howEgress === 'jp' ? 'JA' : 'SG', raw: true, WebSocket: transport.WebSocket },
      })
    : undefined;
  let disposed = false;
  const pending = new Set<AbortController>();
  const request: Transport['request'] = async (request): Promise<HttpResponse> => {
    if (disposed) throw new Error('Engine transport disposed');
    const controller = new AbortController();
    pending.add(controller);
    const signal = request.signal ? AbortSignal.any([request.signal, controller.signal]) : controller.signal;
    const started = Date.now();
    const cleanup = () => {
      clearTimeout(timer);
      pending.delete(controller);
      signal.removeEventListener('abort', cleanup);
    };
    const timer = setTimeout(() => {
      controller.abort(new Error('Engine transport request timed out'));
      pending.delete(controller);
    }, 20_000);
    signal.addEventListener('abort', cleanup, { once: true });
    try {
      signal.throwIfAborted();
      const signed = orderlyExchanges.has(exchange)
        ? { url: request.url, method: request.method, body: request.body, headers: Object.entries(request.headers) }
        : undefined;
      const route = restRoute(provider, exchange, options, routing, request, transport.serverHeaders);
      const config = {
        method: request.method,
        url: route.baseURL ?? request.url,
        baseURL: request.url,
        headers: AxiosHeaders.from({ ...request.headers, ...venueHeaders(exchange, options) }),
        data: request.body,
        timeout: 20_000,
        signal,
      } as InternalAxiosRequestConfig;
      const originalUrl = config.url;
      if (route.proxied && route.authorize) await untilAborted(route.authorize(config), signal);
      signal.throwIfAborted();
      // ED25519 covers Orderly's exact method, path/query and JSON bytes. Proxy
      // authorization may add headers, but must never rewrite a signed packet.
      if (signed && (
        request.url !== signed.url || request.method !== signed.method || request.body !== signed.body ||
        config.url !== originalUrl || config.baseURL !== signed.url ||
        config.method !== signed.method || config.data !== signed.body ||
        config.params !== undefined || config.signal !== signal ||
        signed.headers.some(([name, value]) => config.headers.get(name) !== value)
      )) throw new Error('Orderly transport authorization changed signed request bytes or headers');
      config.timeout = Math.max(1, 20_000 - (Date.now() - started));
      if (adapter) {
        const response = await untilAborted(adapter(config), signal);
        // A raw tunnel response keeps native IDs and signed decimal lexemes intact.
        if (typeof response.data !== 'string') throw new Error('Engine tunnel requires a raw response body');
        cleanup();
        return {
          status: response.status,
          headers: responseHeaders(response.headers),
          body: new TextEncoder().encode(response.data),
        };
      }
      const response = await untilAborted(
        transport.request({
          ...request,
          url: config.url!,
          headers: config.headers.toJSON() as Record<string, string>,
          signal,
        }),
        signal,
      );
      if (response.body instanceof Uint8Array) {
        cleanup();
        return response;
      }
      const body = response.body;
      return {
        ...response,
        body: (async function* () {
          const iterator = body[Symbol.asyncIterator]();
          try {
            for (;;) {
              const next = await untilAborted(iterator.next(), signal);
              if (next.done) return;
              yield next.value;
            }
          } finally {
            cleanup();
            void iterator.return?.().catch(() => undefined);
          }
        })(),
      };
    } catch (error) {
      cleanup();
      throw error;
    }
  };
  return {
    request,
    dispose: () => {
      disposed = true;
      for (const controller of pending) controller.abort(new Error('Engine transport disposed'));
      pending.clear();
      (adapter as { dispose?(): void } | undefined)?.dispose?.();
    },
  };
};

/** Account options are already normalized by accountToExchangeOptions. */
export const createEngineTransport = (
  exchange: ExchangeName,
  options: ExchangeOptions,
  transport: Transport,
): Transport => {
  assertEngineAccountRouting(options);
  const provider = getTransport(options);
  const { routing, restRouting } = resolved(exchange, options, provider);
  const routes = {
    public: wsRoute(routing.wsPublic),
    private: wsRoute(routing.wsPrivate),
    business: wsRoute(routing.wsBusiness),
  };
  const requireSocketHeaders = (url: string) => {
    const connection = provider.wsConnection(url);
    // Tealstreet's identification headers are supplied by the browser itself.
    // A custom provider's headers may authenticate the upgrade; dropping them
    // silently would route an Orderly account through an unauthenticated socket.
    if (orderlyExchanges.has(exchange) && provider !== tealstreetTransport && !transport.webSocketHeaders &&
        connection && Object.keys(connection.headers).length) {
      throw new Error('Orderly custom WebSocket routing requires upgrade header support');
    }
    return connection;
  };
  if (orderlyExchanges.has(exchange)) {
    const prefix = options.testnet ? 'testnet-' : '';
    const account = options.applicationId ?? 'OqdphuyCtYWxwzhxyLLjOWNdFP7sQt8RPWzmb5xY';
    const publicUrl = `wss://${prefix}ws-evm.orderly.org/ws/stream/${account}`;
    const urls = [[publicUrl, routes.public]] as Array<[string, EngineWsRoute]>;
    if (!options.publicDataOnly && options.key && options.secret && options.applicationId) {
      urls.push([`wss://${prefix}ws-private-evm.orderly.org/v2/ws/private/stream/${options.applicationId}`, routes.private]);
    }
    for (const [url, route] of urls) {
      const candidates = route.directUrls?.includes(url) ? [url] : [route.host ?? url, route.relay, route.fallback];
      for (const candidate of candidates) if (candidate) requireSocketHeaders(candidate);
    }
  }
  const fallbacks = new Map<string, WsDirectFallback>();
  const http = httpTransport(exchange, options, transport, provider, restRouting);
  return {
    // Protocol negotiation must use the same resolved route as connect(),
    // including hosts (such as Workers) which construct the engine themselves.
    engineOptions: {
      ...transport.engineOptions,
      ...(exchange === 'grvt' ? { grvtSessionProxy: Boolean(routes.private.host) } : {}),
    },
    webSocketHeaders: transport.webSocketHeaders,
    now: () => transport.now(),
    setTimeout: (fn, ms) => transport.setTimeout(fn, ms),
    clearTimeout: (handle) => transport.clearTimeout(handle),
    request: http.request,
    dispose: () => {
      http.dispose();
      transport.dispose?.();
    },
    connect: (url, handlers) => {
      const channel =
        handlers.channel ?? socketChannels.find((rule) => url.includes(rule.contains))?.channel ?? 'public';
      const declared = routes[channel];
      const bypass = declared.directUrls?.includes(url);
      const route: EngineWsRoute = bypass ? {} : declared;
      const key = `${channel}:${handlers.context?.feed ?? ''}:${url.split('?')[0]}`;
      let fallback = bypass ? undefined : fallbacks.get(key);
      if (!bypass && !fallback) {
        fallback = new WsDirectFallback(2, route.initialSilentConnects);
        fallbacks.set(key, fallback);
      }
      if (handlers.context?.recover && route.fallback) fallback?.preferFallback();
      const target = fallback?.resolveUrl(route.host ?? url, route.relay, route.fallback) ?? url;
      const connection = requireSocketHeaders(target);
      const upgrade = AxiosHeaders.from(handlers.headers ?? {});
      if (transport.webSocketHeaders) {
        upgrade.set(venueHeaders(exchange, options));
        if (connection) upgrade.set(connection.headers);
      }
      const headers = upgrade.toJSON() as Record<string, string>;
      const receive = (text: string) => {
        if (!route.ignoredMessages?.includes(text)) fallback?.onMessage();
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
          const relayOptions = {
            headers: { ...connection.headers, ...headers },
            ALPNProtocols: ['http/1.1'],
          };
          const relay = transport.WebSocket
            ? new RelayWebSocket(target, [], relayOptions, transport.WebSocket)
            : new RelayWebSocket(target, [], relayOptions);
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
                // Browser close() forbids protocol error codes.
                relay.close();
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
