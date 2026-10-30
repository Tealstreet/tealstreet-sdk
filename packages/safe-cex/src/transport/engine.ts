import type { DisposableAxiosAdapter } from '@tealstreet/http-over-websocket/src/axiosHttpOverWebsocketAdapter';
import type { HttpResponse, PreparedHttp, Transport } from '@tealstreet/safe-cex-wasm/host';
import type { InternalAxiosRequestConfig } from 'axios';
import type { ExchangeName, ExchangeOptions } from '../types';
import type { ResolvedRouting, ResolvedWs, RestTransport, TransportProvider } from './types';

import { AxiosHeaders } from 'axios';

import { TradeTunnelTimeoutError } from '@tealstreet/http-over-websocket/src/axiosHttpOverWebsocketAdapter';
import { tunnelEgressUrl } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { HttpPreparationError, socketMessages } from '@tealstreet/safe-cex-wasm/host';
import { captureProxyTokenProvider, refreshProxyToken, withProxyTokenProvider } from '../accessToken';
import { getBybitSiteHeaders, isBybitSiteIdExchange } from '../exchanges/bybit/bybit.site';
import { WsDirectFallback } from '../utils/wsDirectFallback';
import { directTransport } from './direct';
import { getTransport } from './registry';
import {
  isTealstreetProxyAuthFailureBody,
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
  headerAliases?: Record<string, string>;
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
  readHttp?: EngineHttpRoute;
  readPublicHttp?: EngineHttpRoute;
  readHeader?: string;
  readPublic?: boolean;
  publicCorsHttp?: EngineHttpRoute;
  publicHeader?: string;
  ws: { public: EngineWsRoute; private: EngineWsRoute; business: EngineWsRoute };
  wsHeaders: Record<string, string>;
  venueWsHeaders?: Record<string, string>;
  socketChannels: Array<{ contains: string; channel: 'public' | 'private' | 'business' }>;
  /** Public sockets whose native URL names a product with its own route. */
  publicWsRoutes?: Array<{ contains: string; route: EngineWsRoute }>;
  /** Private sockets whose native URL has its own route (Bitget UTA). */
  privateWsRoutes?: Array<{ contains: string; route: EngineWsRoute }>;
}

type Request = Parameters<Transport['request']>[0];
type CredentialIntent = 'public' | 'primary' | 'secondary';
const orderlyExchanges = new Set<ExchangeName>(['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi']);
const coinbaseExchanges = new Set<ExchangeName>(['coinbase', 'coinbasespot']);

const httpHeaderAliases = (exchange: ExchangeName, proxied: boolean, how: boolean): Record<string, string> =>
  coinbaseExchanges.has(exchange) && proxied && !how ? { Authorization: 'Coinbase-Authorization' } : {};

const aliasHeaders = (headers: AxiosHeaders, aliases: Record<string, string>): void => {
  for (const [source, target] of Object.entries(aliases)) {
    const value = headers.get(source);
    if (value === undefined || source.toLowerCase() === target.toLowerCase()) continue;
    const previous = headers.get(target);
    if (typeof value !== 'string' || (previous !== undefined && previous !== value))
      throw new Error('Engine HTTP header alias conflicts with an existing header');
    headers.delete(source);
    headers.set(target, value);
  }
};

const secondaryReadHeaders: Partial<Record<ExchangeName, string>> = {
  binance: 'X-MBX-APIKEY',
  binancepm: 'X-MBX-APIKEY',
  binancespot: 'X-MBX-APIKEY',
  binanceinverse: 'X-MBX-APIKEY',
  asterdex: 'X-MBX-APIKEY',
  bybit: 'X-BAPI-API-KEY',
  bybitinverse: 'X-BAPI-API-KEY',
  bybitspot: 'X-BAPI-API-KEY',
  bybitdemo: 'X-BAPI-API-KEY',
  hyrotrader: 'X-BAPI-API-KEY',
  okx: 'OK-ACCESS-KEY',
  okxeu: 'OK-ACCESS-KEY',
  okxdemo: 'OK-ACCESS-KEY',
  okxeudemo: 'OK-ACCESS-KEY',
  phemex: 'X-PHEMEX-ACCESS-TOKEN',
  backpack: 'X-API-Key',
  kraken: 'APIKey',
  krakenspot: 'API-Key',
  bitget: 'ACCESS-KEY',
  bitgetusdc: 'ACCESS-KEY',
  bitgetinverse: 'ACCESS-KEY',
  kucoin: 'KC-API-KEY',
  kucoinspot: 'KC-API-KEY',
  mexc: 'ApiKey',
  bitunix: 'api-key',
  bingx: 'X-BX-APIKEY',
  bitrue: 'X-CH-APIKEY',
  weex: 'ACCESS-KEY',
  blofin: 'ACCESS-KEY',
  woo: 'x-api-key',
  coinbase: 'Authorization',
  coinbasespot: 'Authorization',
};

export const assertEngineAccountRouting = (options: ExchangeOptions, exchange?: ExchangeName): void => {
  if (
    options.secondary_key === undefined &&
    options.secondary_secret === undefined &&
    options.secondary_passphrase === undefined
  )
    return;
  if (!exchange || (!secondaryReadHeaders[exchange] && exchange !== 'cryptocom')) {
    throw new Error('Rust engine secondary signing keys are unsupported for this venue');
  }
  if (!options.secondary_key || !options.secondary_secret) {
    throw new Error('Rust engine secondary signing keys require a nonempty key and secret');
  }
  if (
    [
      'okx',
      'okxeu',
      'okxdemo',
      'okxeudemo',
      'bitget',
      'bitgetusdc',
      'bitgetinverse',
      'kucoin',
      'kucoinspot',
      'blofin',
      'weex',
    ].includes(exchange) &&
    !(options.secondary_passphrase ?? options.passphrase)
  ) {
    throw new Error('Rust engine secondary signing keys require a passphrase');
  }
};

const secondaryReadOptions = (options: ExchangeOptions, provider: TransportProvider): ExchangeOptions => ({
  ...options,
  key: options.secondary_key!,
  secret: options.secondary_secret!,
  passphrase: options.secondary_passphrase ?? options.passphrase,
  secondary_key: undefined,
  secondary_secret: undefined,
  secondary_passphrase: undefined,
  overrideProxyUrl: options.secondaryProxyUrl || provider.publicCorsProxy(),
});

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

// One Bybit account streams inverse and option products from their own native
// URLs; TS routes inverse through `bybitinverse`'s hosts. No option proxy exists.
const publicWsRoutes = (
  exchange: ExchangeName,
  options: ExchangeOptions,
  provider: TransportProvider,
): EngineRoutingPlan['publicWsRoutes'] =>
  exchange === 'bybit'
    ? [
        { contains: '/v5/public/inverse', route: wsRoute(provider.wsRouting('bybitinverse', options).wsPublic) },
        { contains: '/v5/public/spot', route: wsRoute(provider.wsRouting('bybitspot', options).wsPublic) },
        { contains: '/v5/public/option', route: {} },
      ]
    : undefined;

// The Rust Bitget venue opens classic v2 and UTA v3 private sockets on one
// channel; UTA takes TS #8093's privateUta route by its native URL.
const privateWsRoutes = (routing: ResolvedRouting): EngineRoutingPlan['privateWsRoutes'] =>
  routing.wsPrivateUta
    ? [
        {
          contains: '/v3/ws/private',
          // The same venue pong is not route traffic on either private socket.
          route: wsRoute({ ...routing.wsPrivateUta, ignoredMessages: routing.wsPrivate.ignoredMessages }),
        },
      ]
    : undefined;

const resolved = (exchange: ExchangeName, options: ExchangeOptions, provider: TransportProvider) => {
  const declared = provider.wsRouting(exchange, options);
  // Bitunix's TS sockets choose their declared fallback on this explicit option.
  // Normalize the same policy for engine hosts without changing TS consumers.
  const forceBitunix = (route: ResolvedWs): ResolvedWs =>
    exchange === 'bitunix' && options.tealstreetWsRoute && route.fallback
      ? { tunnelled: true, host: route.fallback }
      : route;
  const routing = {
    ...declared,
    wsPublic: forceBitunix(declared.wsPublic),
    wsPrivate: forceBitunix(declared.wsPrivate),
    wsBusiness: forceBitunix(declared.wsBusiness),
  };
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
  assertEngineAccountRouting(options, exchange);
  const provider = getTransport(options);
  if (provider !== directTransport && provider !== tealstreetTransport) {
    throw new Error('Custom transports must supply their own native host routing plan');
  }
  const { routing, restRouting } = resolved(exchange, options, provider);
  const publicCorsProxy = ['asterdex', 'txflow'].includes(exchange) ? provider.publicCorsProxy() : undefined;
  const http = (credentialIntent: CredentialIntent): EngineHttpRoute => {
    const route = restRoute(
      provider,
      exchange,
      options,
      restRouting,
      { method: 'GET', url: baseURL, headers: {} },
      false,
      credentialIntent,
    );
    const target = route.baseURL ?? baseURL;
    if (target !== baseURL && !target.endsWith(`/${baseURL}`)) throw new Error('Unsupported native URL rewrite');
    const how =
      usesTunnel(exchange, options) && Boolean(provider.restAdapter({ exchange, options, variant: { raw: true } }));
    const headerAliases = httpHeaderAliases(exchange, route.proxied, how);
    return {
      mode: how ? 'how' : 'direct',
      prefix: target === baseURL ? '' : target.slice(0, -baseURL.length - 1),
      authorize: route.proxied,
      authorizationHeader: proxyAuthorizationHeader(exchange),
      headers: {
        ...venueHeaders(exchange, options),
        ...(route.proxied && provider === tealstreetTransport ? TEALSTREET_REST_HEADERS : {}),
      },
      ...(Object.keys(headerAliases).length ? { headerAliases } : {}),
      ...(how ? { tunnelUrl: tunnelEgressUrl(routing.howEgress) } : {}),
    };
  };
  const readPlan = options.secondary_key
    ? resolveEngineRoutingPlan(exchange, secondaryReadOptions(options, provider), baseURL)
    : undefined;
  const productRoutes = publicWsRoutes(exchange, options, provider);
  const privateRoutes = privateWsRoutes(routing);
  return {
    http: http(!options.publicDataOnly && options.key ? 'primary' : 'public'),
    ...(publicCorsProxy
      ? {
          publicCorsHttp: {
            mode: 'direct' as const,
            prefix: publicCorsProxy,
            authorize: false,
            authorizationHeader: proxyAuthorizationHeader(exchange),
            headers: {},
          },
        }
      : {}),
    ...(readPlan
      ? {
          readHeader: secondaryReadHeaders[exchange],
          readPublic: !['binance', 'binancepm', 'binanceinverse', 'asterdex'].includes(exchange),
          readHttp: readPlan.http,
          ...(exchange === 'binancespot' && { readPublicHttp: readPlan.publicHttp }),
        }
      : {}),
    ...(exchange.startsWith('binance') ? { publicHeader: 'X-MBX-APIKEY', publicHttp: http('public') } : {}),
    ws: {
      public: wsRoute(routing.wsPublic),
      private: wsRoute(routing.wsPrivate),
      business: wsRoute(routing.wsBusiness),
    },
    wsHeaders: provider === tealstreetTransport ? { ...TEALSTREET_WS_HEADERS } : {},
    venueWsHeaders: venueHeaders(exchange, options),
    socketChannels,
    ...(productRoutes ? { publicWsRoutes: productRoutes } : {}),
    ...(privateRoutes ? { privateWsRoutes: privateRoutes } : {}),
  };
};

const hasHeader = (headers: Record<string, string>, name: string): boolean =>
  Object.keys(headers).some((key) => key.toLowerCase() === name.toLowerCase() && Boolean(headers[key]));

// Futures profiles use a separate public client that bypasses account routing.
// Spot keeps its selected GET client and only removes public auth headers.
const publicBinanceRequest = (exchange: ExchangeName, request: Request): boolean =>
  ['binance', 'binancepm', 'binanceinverse'].includes(exchange) && !hasHeader(request.headers, 'X-MBX-APIKEY');

// Factory adapter gates are distinct from saved-account metadata defaults.
const usesTunnel = (exchange: ExchangeName, options: ExchangeOptions): boolean =>
  exchange === 'paradex'
    ? !options.publicDataOnly && !options.payloadSigner
    : ['hyperliquid', 'hyperliquidspot', 'kucoin', 'kucoinspot'].includes(exchange)
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
  credentialIntent?: CredentialIntent,
): RestTransport => {
  if (
    (credentialIntent === 'public' && ['binance', 'binancepm', 'binanceinverse'].includes(exchange)) ||
    (credentialIntent === undefined && publicBinanceRequest(exchange, request))
  )
    return { baseURL: request.url, proxied: false };
  // Factories resolve a base URL, never a signed query. Keep request data out
  // of host selection and append its exact bytes after the provider rewrites it.
  const baseURL = request.url.match(/^https?:\/\/[^/?#]+/i)?.[0];
  if (!baseURL) throw new Error('Engine HTTP requires an absolute venue URL');
  const route = provider.rest({
    exchange,
    options,
    routing,
    serverHeaders,
    credentialIntent,
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

/** Reports a proxy-authorization failure once per streak; a proxy-accepted response ends it. */
interface ProxyAuthorizationNotice {
  failed(message: string): void;
  accepted(): void;
}

const readBody = async (body: HttpResponse['body']): Promise<Uint8Array> => {
  if (body instanceof Uint8Array) return body;
  const chunks: Uint8Array[] = [];
  for await (const chunk of body) chunks.push(chunk);
  const bytes = new Uint8Array(chunks.reduce((length, chunk) => length + chunk.length, 0));
  let at = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, at);
    at += chunk.length;
  }
  return bytes;
};

const PROXY_AUTHORIZATION_MISSING = 'Tealstreet proxy authorization missing';

const httpTransport = (
  exchange: ExchangeName,
  options: ExchangeOptions,
  transport: Transport,
  provider: TransportProvider,
  routing: ResolvedRouting,
  publicPrefix?: string,
  proxyAuthorization?: ProxyAuthorizationNotice,
): {
  mode: 'direct' | 'how';
  request: Transport['request'];
  prepareHttp: NonNullable<Transport['prepareHttp']>;
  dispose(): void;
} => {
  const tunnel = publicPrefix === undefined && usesTunnel(exchange, options);
  const adapter = tunnel
    ? provider.restAdapter({
        exchange,
        options,
        variant: { region: routing.howEgress === 'jp' ? 'JA' : 'SG', raw: true, WebSocket: transport.WebSocket },
      })
    : undefined;
  let disposed = false;
  const pending = new Set<AbortController>();
  // Multi-tenant hosts bind proxy tokens to the account's async context. Engine
  // requests start from timers, sockets and shared public feeds, so keep the
  // provider of the context that created this account's transport.
  const proxyTokenProvider = captureProxyTokenProvider();
  // The edge's 401 is our authorization, not the venue's verdict on the key: the
  // engine would classify it as a refused key, so it fails as a transport error
  // (retried by the venue's read policy) after asking for a fresh token.
  const checkProxyAuthorization = async (response: HttpResponse): Promise<HttpResponse> => {
    if (response.status !== 401) {
      proxyAuthorization?.accepted();
      return response;
    }
    const body = await readBody(response.body);
    const text = new TextDecoder().decode(body);
    if (!isTealstreetProxyAuthFailureBody(text)) return { ...response, body };
    void withProxyTokenProvider(proxyTokenProvider, refreshProxyToken).catch(() => undefined);
    const message = `[${exchange}] Tealstreet proxy refused our proxy authorization (HTTP 401: ${text}); refreshing the proxy token and retrying`;
    proxyAuthorization?.failed(message);
    throw new Error(message);
  };
  const prepare = async (request: Request, intent?: CredentialIntent): Promise<PreparedHttp> => {
    if (disposed) throw new Error('Engine transport disposed');
    const controller = new AbortController();
    pending.add(controller);
    const signal = request.signal ? AbortSignal.any([request.signal, controller.signal]) : controller.signal;
    const started = Date.now();
    const deadlineMs = Math.max(1, request.timeoutMs ?? 20_000);
    const remaining = () => Math.max(0, deadlineMs - (Date.now() - started));
    let prepared: PreparedHttp | Awaited<ReturnType<DisposableAxiosAdapter['prepare']>> | undefined;
    let bound = false;
    let cancelBound: (() => void) | undefined;
    const cleanup = () => {
      clearTimeout(timer);
      pending.delete(controller);
      signal.removeEventListener('abort', onAbort);
    };
    let boundReleased = false;
    let preparedReleased = false;
    const releaseResources = () => {
      if (cancelBound && !boundReleased) {
        boundReleased = true;
        try {
          cancelBound();
        } catch {
          /* Continue releasing the preparation. */
        }
      }
      if (prepared && !preparedReleased) {
        preparedReleased = true;
        try {
          prepared.cancel?.();
        } catch {
          /* Cleanup cannot strand this request. */
        }
      }
    };
    const onAbort = () => {
      releaseResources();
      cleanup();
    };
    const timer = setTimeout(() => controller.abort(new HttpPreparationError('timedOut')), deadlineMs);
    signal.addEventListener('abort', onAbort, { once: true });
    const cancel = () => {
      controller.abort();
      releaseResources();
      cleanup();
    };
    const active = () => {
      signal.throwIfAborted();
      if (disposed) throw new Error('Engine transport disposed');
      if (remaining() <= 0) throw new HttpPreparationError('timedOut');
    };
    const beforeRelease = <T>(operation: () => T): T => {
      try {
        active();
        const result = operation();
        active();
        return result;
      } catch (error) {
        cancel();
        if (intent !== undefined && error instanceof TradeTunnelTimeoutError)
          throw new HttpPreparationError('timedOut');
        throw error;
      }
    };
    const wrap = (response: HttpResponse): HttpResponse => {
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
    };
    try {
      signal.throwIfAborted();
      const original = new URL(request.url);
      const signed =
        intent !== undefined || orderlyExchanges.has(exchange) || coinbaseExchanges.has(exchange)
          ? { url: request.url, method: request.method, body: request.body, headers: Object.entries(request.headers) }
          : undefined;
      const route =
        publicPrefix === undefined
          ? restRoute(provider, exchange, options, routing, request, transport.serverHeaders, intent)
          : { baseURL: publicPrefix ? `${publicPrefix}/${request.url}` : request.url, proxied: false };
      const target = route.baseURL ?? request.url;
      const prefix = target.endsWith(request.url) ? target.slice(0, -request.url.length) : undefined;
      if (intent !== undefined && prefix === undefined) throw new HttpPreparationError('unsupported');
      const aliases = httpHeaderAliases(exchange, route.proxied, Boolean(adapter));
      const hostHeaders = AxiosHeaders.from(venueHeaders(exchange, options));
      const config = {
        method: request.method,
        url: target,
        baseURL: request.url,
        headers: AxiosHeaders.from({ ...request.headers, ...venueHeaders(exchange, options) }),
        data: request.body,
        timeout: remaining(),
        signal,
      } as InternalAxiosRequestConfig;
      aliasHeaders(config.headers, aliases);
      const before = AxiosHeaders.from(config.headers.toJSON());
      if (route.proxied && route.authorize)
        await untilAborted(
          withProxyTokenProvider(proxyTokenProvider, () => route.authorize!(config)),
          signal,
        );
      signal.throwIfAborted();
      if (remaining() <= 0) throw new HttpPreparationError('timedOut');
      if (
        signed &&
        (request.url !== signed.url ||
          request.method !== signed.method ||
          request.body !== signed.body ||
          signed.headers.some(
            ([name, value]) =>
              config.headers.get(
                Object.entries(aliases).find(([source]) => source.toLowerCase() === name.toLowerCase())?.[1] ?? name,
              ) !== value,
          ) ||
          config.url !== target ||
          config.baseURL !== request.url ||
          config.method !== request.method ||
          config.data !== request.body ||
          config.params !== undefined ||
          config.signal !== signal ||
          Object.entries(before.toJSON()).some(([name, value]) => config.headers.get(name) !== value))
      )
        throw new Error('Engine transport authorization changed signed request bytes or headers');
      for (const [name, value] of Object.entries(config.headers.toJSON()))
        if (before.get(name) !== value) hostHeaders.set(name, value);
      config.timeout = remaining();
      const waitForPrepared = async <T extends { cancel?(): void }>(promise: Promise<T>): Promise<T> => {
        void promise.then(
          (value) => {
            if (signal.aborted) {
              try {
                value?.cancel?.();
              } catch {
                /* Late resources remain isolated. */
              }
            }
          },
          () => undefined,
        );
        return untilAborted(promise, signal);
      };
      const how = adapter as Partial<DisposableAxiosAdapter> | undefined;
      if (adapter) {
        const prepareTunnel = intent === undefined ? how?.prepareLegacy : how?.prepare;
        if (prepareTunnel)
          prepared = await waitForPrepared(
            withProxyTokenProvider(proxyTokenProvider, () => prepareTunnel.call(adapter, config)),
          );
        else if (intent !== undefined) throw new HttpPreparationError('unsupported');
      } else if (intent !== undefined) {
        if (!transport.prepareHttp) throw new HttpPreparationError('unsupported');
        prepared = await waitForPrepared(
          Promise.resolve(
            transport.prepareHttp({
              ...request,
              intent,
              route: 'direct',
              url: target,
              headers: config.headers.toJSON() as Record<string, string>,
              timeoutMs: remaining(),
              signal,
            }),
          ),
        );
      }
      signal.throwIfAborted();
      if (intent !== undefined && (!prepared || typeof prepared.bind !== 'function'))
        throw new HttpPreparationError('invalid');
      const validateBound = <T extends { ready(): boolean; dispatch(): unknown; cancel?(): void }>(
        inner: T | undefined,
      ): T | undefined => {
        if (inner && typeof inner.cancel === 'function') cancelBound = () => inner.cancel!();
        if (
          intent !== undefined &&
          (!inner || typeof inner.ready !== 'function' || typeof inner.dispatch !== 'function')
        )
          throw new HttpPreparationError('invalid');
        return inner;
      };
      return {
        ...(adapter ? {} : { expiresAt: (prepared as PreparedHttp | undefined)?.expiresAt }),
        cancel,
        bind(packet) {
          beforeRelease(() => {
            if (bound) throw new HttpPreparationError('invalid');
          });
          if (
            packet.request !== request.request ||
            packet.method !== request.method ||
            new URL(packet.url).origin !== original.origin ||
            packet.route !== request.route ||
            packet.signal !== request.signal
          ) {
            cancel();
            throw new HttpPreparationError('invalid');
          }
          const headers = AxiosHeaders.from(
            intent === undefined ? { ...packet.headers, ...venueHeaders(exchange, options) } : packet.headers,
          );
          aliasHeaders(headers, aliases);
          for (const [name, value] of Object.entries(hostHeaders.toJSON())) {
            const previous = headers.get(name);
            if (previous !== undefined && previous !== value) {
              cancel();
              throw new HttpPreparationError('invalid');
            }
            headers.set(name, value);
          }
          const url = intent === undefined ? target : `${prefix}${packet.url}`;
          const finalConfig = { ...config, url, headers, data: packet.body, timeout: remaining(), signal };
          const network: Request = {
            ...packet,
            ...(intent === undefined ? {} : { route: 'direct' as const }),
            url,
            headers: headers.toJSON() as Record<string, string>,
            timeoutMs: remaining(),
            signal,
          };
          if (intent === undefined) delete network.timeoutMs;
          let send: () => Promise<HttpResponse>;
          let ready: () => boolean;
          if (adapter) {
            const tunnel = prepared as Awaited<ReturnType<DisposableAxiosAdapter['prepare']>> | undefined;
            const inner = beforeRelease(() => validateBound(tunnel?.bind(finalConfig)));
            ready = inner ? () => inner.ready() : () => true;
            send = () => {
              const response = inner
                ? inner.dispatch()
                : withProxyTokenProvider(proxyTokenProvider, () => adapter(finalConfig));
              return untilAborted(response, signal).then((value) => {
                if (typeof value.data !== 'string') throw new Error('Engine tunnel requires a raw response body');
                return {
                  status: value.status,
                  headers: responseHeaders(value.headers),
                  body: new TextEncoder().encode(value.data),
                };
              });
            };
          } else {
            const inner = beforeRelease(() => validateBound((prepared as PreparedHttp | undefined)?.bind(network)));
            ready = inner ? () => inner.ready() : () => true;
            send = inner ? () => inner.dispatch() : () => transport.request(network);
          }
          bound = true;
          let sent = false;
          return {
            ready() {
              return beforeRelease(() => !sent && ready());
            },
            dispatch() {
              beforeRelease(() => {
                if (sent) throw new HttpPreparationError('invalid');
              });
              sent = true;
              try {
                const response = send();
                return untilAborted(response, signal)
                  .then((value) =>
                    route.proxied && !adapter ? untilAborted(checkProxyAuthorization(value), signal) : value,
                  )
                  .then(wrap)
                  .catch((error) => {
                    cancel();
                    throw error;
                  });
              } catch (error) {
                cancel();
                throw error;
              }
            },
            cancel,
          };
        },
      };
    } catch (error) {
      cancel();
      if ((error as { name?: unknown })?.name === 'ProxyTokenUnavailableError') {
        const reason = (error as Error).message;
        proxyAuthorization?.failed(
          reason.includes(PROXY_AUTHORIZATION_MISSING) ? reason : `[${exchange}] ${PROXY_AUTHORIZATION_MISSING}: ${reason}`,
        );
      }
      if (intent !== undefined && error instanceof TradeTunnelTimeoutError) throw new HttpPreparationError('timedOut');
      throw error;
    }
  };
  const request: Transport['request'] = async (request) => (await prepare(request)).bind(request).dispatch();
  return {
    mode: adapter ? 'how' : 'direct',
    request,
    prepareHttp: (request) => prepare(request, request.intent),
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
  credentials?: { hasSecondary: boolean },
  onProxyAuthorizationFailure?: (message: string) => void,
): Transport => {
  assertEngineAccountRouting(options, exchange);
  const provider = getTransport(options);
  const { routing, restRouting } = resolved(exchange, options, provider);
  const routes = {
    public: wsRoute(routing.wsPublic),
    private: wsRoute(routing.wsPrivate),
    business: wsRoute(routing.wsBusiness),
  };
  const productRoutes = publicWsRoutes(exchange, options, provider) ?? [];
  const privateRoutes = privateWsRoutes(routing) ?? [];
  // Relay sockets read the proxy token on open; see `httpTransport`.
  const proxyTokenProvider = captureProxyTokenProvider();
  const requireSocketHeaders = (url: string) => {
    const connection = provider.wsConnection(url);
    // Tealstreet's identification headers are supplied by the browser itself.
    // A custom provider's headers may authenticate the upgrade; dropping them
    // silently would route an Orderly account through an unauthenticated socket.
    if (
      orderlyExchanges.has(exchange) &&
      provider !== tealstreetTransport &&
      !transport.webSocketHeaders &&
      connection &&
      Object.keys(connection.headers).length
    ) {
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
      urls.push([
        `wss://${prefix}ws-private-evm.orderly.org/v2/ws/private/stream/${options.applicationId}`,
        routes.private,
      ]);
    }
    for (const [url, route] of urls) {
      const candidates = route.directUrls?.includes(url) ? [url] : [route.host ?? url, route.relay, route.fallback];
      for (const candidate of candidates) if (candidate) requireSocketHeaders(candidate);
    }
  }
  const fallbacks = new Map<string, WsDirectFallback>();
  let proxyAuthorizationFailing = false;
  const proxyAuthorization: ProxyAuthorizationNotice = {
    failed: (message) => {
      if (proxyAuthorizationFailing) return;
      proxyAuthorizationFailing = true;
      try {
        onProxyAuthorizationFailure?.(message);
      } catch {
        /* A reporting listener cannot fail the request. */
      }
    },
    accepted: () => {
      proxyAuthorizationFailing = false;
    },
  };
  const http = httpTransport(exchange, options, transport, provider, restRouting, undefined, proxyAuthorization);
  // Explicit public requests bypass account proxy/tunnel policy. URLs stay in
  // the host; venues choose only the typed route name.
  const directHttp = httpTransport(exchange, options, transport, provider, restRouting, '');
  const publicCorsProxy = provider.publicCorsProxy();
  const publicCorsHttp = publicCorsProxy
    ? httpTransport(exchange, options, transport, provider, restRouting, publicCorsProxy)
    : undefined;
  const readOptions = credentials?.hasSecondary
    ? { ...options, overrideProxyUrl: options.secondaryProxyUrl || provider.publicCorsProxy() }
    : options.secondary_key
      ? secondaryReadOptions(options, provider)
      : undefined;
  const readHttp = readOptions
    ? httpTransport(
        exchange,
        readOptions,
        transport,
        provider,
        resolved(exchange, readOptions, provider).restRouting,
        undefined,
        proxyAuthorization,
      )
    : undefined;
  return {
    // Protocol negotiation must use the same resolved route as connect(),
    // including hosts (such as Workers) which construct the engine themselves.
    httpMode: http.mode,
    engineOptions: {
      ...transport.engineOptions,
      ...(exchange === 'grvt' ? { grvtSessionProxy: Boolean(routes.private.host) } : {}),
    },
    webSocketHeaders: transport.webSocketHeaders,
    now: () => transport.now(),
    setTimeout: (fn, ms) => transport.setTimeout(fn, ms),
    clearTimeout: (handle) => transport.clearTimeout(handle),
    prepareHttp: (request) => {
      const { route, intent, ...packet } = request;
      if (route === 'primary' && intent === 'secondary') throw new HttpPreparationError('invalid');
      if (route === 'secondary' && intent !== 'secondary') throw new HttpPreparationError('invalid');
      const selected =
        route === 'primary'
          ? http
          : route === 'direct'
            ? directHttp
            : route === 'publicCors'
              ? publicCorsHttp
              : intent === 'secondary'
                ? readHttp
                : intent === 'public' &&
                    readHttp &&
                    !['binance', 'binancepm', 'binanceinverse', 'asterdex'].includes(exchange)
                  ? readHttp
                  : http;
      if (!selected) throw new HttpPreparationError('unsupported');
      return selected.prepareHttp({ ...packet, route, intent });
    },
    request: (request) => {
      const { route, ...nativeRequest } = request;
      if (route === 'primary') return http.request(nativeRequest);
      // A declared read-key request; same fallback as an inferred read.
      if (route === 'secondary') return (readHttp ?? http).request(nativeRequest);
      if (route === 'direct') return directHttp.request(nativeRequest);
      if (route === 'publicCors')
        return publicCorsHttp
          ? publicCorsHttp.request(nativeRequest)
          : Promise.reject(new Error('Public CORS request routing is not configured'));
      return readHttp &&
        request.method.toUpperCase() === 'GET' &&
        (!['binance', 'binancepm', 'binanceinverse', 'asterdex'].includes(exchange) ||
          hasHeader(request.headers, secondaryReadHeaders[exchange]!))
        ? readHttp.request(nativeRequest)
        : http.request(nativeRequest);
    },
    dispose: () => {
      http.dispose();
      readHttp?.dispose();
      directHttp.dispose();
      publicCorsHttp?.dispose();
      transport.dispose?.();
    },
    connect: (url, handlers) => {
      const channel =
        handlers.channel ?? socketChannels.find((rule) => url.includes(rule.contains))?.channel ?? 'public';
      const scoped = channel === 'public' ? productRoutes : channel === 'private' ? privateRoutes : [];
      const declared = scoped.find((rule) => url.includes(rule.contains))?.route ?? routes[channel];
      const bypass = declared.directUrls?.includes(url);
      const route: EngineWsRoute = bypass ? {} : declared;
      const key = `${channel}:${handlers.context?.feed ?? ''}:${url.split('?')[0]}`;
      let fallback = bypass ? undefined : fallbacks.get(key);
      if (!bypass && !fallback) {
        fallback = new WsDirectFallback(2, route.initialSilentConnects);
        fallbacks.set(key, fallback);
      }
      if (handlers.context?.rejectRoute) {
        // The venue refused this egress: advance exactly one route (not recover's
        // jump to the last), or report exhaustion after connect returns, never dialling.
        if (!fallback?.rejectCurrent()) {
          let live = true;
          transport.setTimeout(() => {
            if (live) handlers.onRouteExhausted?.();
          }, 0);
          return {
            send: () => {},
            close: () => {
              live = false;
            },
          };
        }
      } else if (handlers.context?.recover && route.fallback) fallback?.preferFallback();
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
      const receiveBinary = (data: Uint8Array) => {
        fallback?.onMessage();
        handlers.onBinary?.(data);
      };
      if (connection?.kind !== 'relay')
        return transport.connect(target, { ...handlers, headers, onMessage: receive, onBinary: receiveBinary });
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
          const relay = withProxyTokenProvider(proxyTokenProvider, () =>
            transport.WebSocket
              ? new RelayWebSocket(target, [], relayOptions, transport.WebSocket)
              : new RelayWebSocket(target, [], relayOptions),
          );
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
            (data) => {
              if (!closed) {
                if (typeof data === 'string') receive(data);
                else receiveBinary(data);
              }
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
            handlers.binary,
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
