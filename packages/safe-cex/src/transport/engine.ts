import type { DisposableAxiosAdapter, PreparedTunnelRequest } from '@tealstreet/http-over-websocket/src/axiosHttpOverWebsocketAdapter';
import type {
  AccountReconnectDiagnostics,
  ConnectionCloseDiagnostic,
  TimerLatenessBucket,
} from '@tealstreet/http-over-websocket/src/connectionDiagnostics';
import type { TunnelOutage } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import type { EngineEvent, HttpResponse, PreparedHttp, Transport } from '@tealstreet/safe-cex-wasm/host';
import type { InternalAxiosRequestConfig } from 'axios';
import type { ExchangeName, ExchangeOptions } from '../types';
import type { BybitPublicRateLimitNotice } from './bybitPublicRateLimitTelemetry';
import type { HttpTimingOutcome } from './httpTiming';
import type { CatalogueRead } from './sharedCatalogue';
import type { ResolvedRouting, ResolvedWs, RestTransport, TransportProvider } from './types';

import { AxiosHeaders } from 'axios';

import { TradeTunnelTimeoutError } from '@tealstreet/http-over-websocket/src/axiosHttpOverWebsocketAdapter';
import {
  ConnectionDiagnostics,
  HiddenTimerLateness,
  maximumTimerLateness,
} from '@tealstreet/http-over-websocket/src/connectionDiagnostics';
import { budgetDiagnostic, requestBudgetMs, requestClass } from '@tealstreet/http-over-websocket/src/requestBudget';
import { stripDiagnosticUrlQueries } from '@tealstreet/http-over-websocket/src/diagnosticUrl';
import {
  onTunnelFailing,
  tunnelEgressUrl,
  tunnelFailing,
  tunnelOutage,
} from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { HttpPreparationError, socketMessages } from '@tealstreet/safe-cex-wasm/host';
import { captureProxyTokenProvider, refreshProxyToken, withProxyTokenProvider } from '../accessToken';
import { getBybitSiteHeaders, isBybitSiteIdExchange } from '../exchanges/bybit/bybit.site';
import { SECONDARY_READ_KEYS_ENABLED } from '../secondaryReadKeys';
import { WsDirectFallback } from '../utils/wsDirectFallback';
import { sharedBinanceCatalogue } from './binanceCatalogue';
import { observeBybitPublicRateLimit } from './bybitPublicRateLimitTelemetry';
import { directTransport } from './direct';
import { HttpTiming, httpTimingKind, httpTimingStage, tunnelHostLabel, upstreamMsFromHeaders } from './httpTiming';
import { sharedHyperliquidCatalogue } from './hyperliquidCatalogue';
import { HyperliquidInfoBudget, warnHyperliquidInfoBudgetUnavailable } from './hyperliquidInfoBudget';
import { HyperliquidStartupFacts } from './hyperliquidStartupFacts';
import { payloadSignedViaPublicProxy, refuseSignedPublicProxy } from './publicProxyGuard';
import { getTransport } from './registry';
import { proxyFleetForwards } from './tealstreet/proxyFleetAllowlist';
import { PUBLIC_PROXY_READ_HOSTS, readsViaPublicProxy } from './tealstreet/publicCorsAllowlist';
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
  /** A bypassed read key's egress (secondaryReadKeys.ts): managed public GETs only. */
  publicReadHttp?: EngineHttpRoute;
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
const orderlyExchanges = new Set<ExchangeName>(['kodiakfi', 'mode', 'what', 'woofi']);
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

// Authorization can assign unnormalized scalars directly. Axios omits disabled
// values; fetch stringifies each remaining value, including arrays without spaces.
const wireHeaders = (headers: AxiosHeaders): Record<string, string> => Object.fromEntries(
  Object.entries(headers.toJSON()).map(([name, value]): [string, string] =>
    [name, String(value)]),
);

const isTunnelPreparation = (value: unknown): value is DisposableAxiosAdapter['prepare'] =>
  typeof value === 'function';

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
  bingxspot: 'X-BX-APIKEY',
  bitrue: 'X-CH-APIKEY',
  weex: 'ACCESS-KEY',
  blofin: 'ACCESS-KEY',
  woo: 'x-api-key',
  coinbase: 'Authorization',
  coinbasespot: 'Authorization',
};

/** Headers that carry a venue credential: never sent toward the public proxy. */
const CREDENTIAL_HEADERS: ReadonlySet<string> = new Set([
  'authorization',
  'x-ts-grvt-session',
  'x-grvt-account-id',
  ...Object.values(secondaryReadHeaders).map((name) => name!.toLowerCase()),
]);

/** The account without its read credentials while secondary read keys are bypassed (secondaryReadKeys.ts). */
export const withSecondaryReadKeyPolicy = (options: ExchangeOptions): ExchangeOptions => {
  if (SECONDARY_READ_KEYS_ENABLED) return options;
  const { secondary_key: _key, secondary_secret: _secret, secondary_passphrase: _passphrase, ...primary } = options;
  // Signer setup reads secondary identity metadata as a requested read identity.
  if (primary.localSigner?.secondary === undefined) return primary;
  const { secondary: _identity, ...localSigner } = primary.localSigner;
  return { ...primary, localSigner };
};

/** A bypassed read key's egress, kept for the account's unsigned public GETs only. */
const publicReadOptions = (options: ExchangeOptions): ExchangeOptions | undefined =>
  SECONDARY_READ_KEYS_ENABLED || !options.secondaryProxyUrl
    ? undefined
    : { ...options, secondaryProxyUrl: undefined, overrideProxyUrl: options.secondaryProxyUrl };

export const assertEngineAccountRouting = (options: ExchangeOptions, exchange?: ExchangeName): void => {
  options = withSecondaryReadKeyPolicy(options);
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
  options = withSecondaryReadKeyPolicy(options);
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
  // A signed read never goes to the public proxy (publicProxyGuard): the native host
  // gets no read route there, so its secondary reads fail closed before I/O.
  const readHttp = readPlan && readPlan.http.prefix !== provider.publicCorsProxy() ? readPlan.http : undefined;
  const publicReadOnly = publicReadOptions(options);
  const publicReadPlan = publicReadOnly && resolveEngineRoutingPlan(exchange, publicReadOnly, baseURL);
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
    ...(readPlan && readHttp
      ? {
          readHeader: secondaryReadHeaders[exchange],
          readPublic: !hasOwnPublicReadRoute(exchange),
          readHttp,
          ...(exchange === 'binancespot' && { readPublicHttp: readPlan.publicHttp }),
        }
      : {}),
    ...(publicReadPlan && publicReadEligible(exchange)
      ? {
          publicReadHttp:
            exchange === 'binancespot' ? (publicReadPlan.publicHttp ?? publicReadPlan.http) : publicReadPlan.http,
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

const isPublicBinanceRead = (exchange: ExchangeName, request: Request, credentialIntent?: CredentialIntent) =>
  (credentialIntent === 'public' && ['binance', 'binancepm', 'binanceinverse'].includes(exchange)) ||
  (credentialIntent === undefined && publicBinanceRequest(exchange, request));

/** The account's tunnel setting under the venue's default: BingX tunnels unless switched off. */
const tunnelFlagOn = (exchange: ExchangeName, options: ExchangeOptions): boolean =>
  exchange === 'bingx' || exchange === 'bingxspot'
    ? options.tealstreetTunnel !== false
    : Boolean(options.tealstreetTunnel);

// Factory adapter gates are distinct from saved-account metadata defaults.
const usesTunnel = (exchange: ExchangeName, options: ExchangeOptions): boolean =>
  exchange === 'paradex'
    ? !options.publicDataOnly && !options.payloadSigner
    : ['hyperliquid', 'hyperliquidspot', 'kucoin', 'kucoinspot'].includes(exchange)
      ? !options.publicDataOnly
      : exchange === 'bingx' || exchange === 'bingxspot'
        ? !options.publicDataOnly && tunnelFlagOn(exchange, options)
        : !['weex', 'grvt', 'blofin', 'mexc'].includes(exchange) && tunnelFlagOn(exchange, options);

// A browser engine on public data only keeps the account's tunnel flag but not the
// tunnel; the flag must not suppress the proxy, or a venue whose public REST refuses
// browser origins (requiresCors) is dialed browser-direct. Native hosts have no CORS.
// GRVT and blofin are exempt: their provider already turns the tunnel flag into the proxy.
const restOptionsFor = (exchange: ExchangeName, options: ExchangeOptions): ExchangeOptions =>
  options.publicDataOnly &&
  options.tealstreetTunnel &&
  exchange !== 'grvt' &&
  exchange !== 'blofin' &&
  !usesTunnel(exchange, options)
    ? { ...options, tealstreetTunnel: false }
    : options;

const restRoute = (
  provider: TransportProvider,
  exchange: ExchangeName,
  options: ExchangeOptions,
  routing: ResolvedRouting,
  request: Request,
  serverHeaders = false,
  credentialIntent?: CredentialIntent,
): RestTransport => {
  if (isPublicBinanceRead(exchange, request, credentialIntent)) return { baseURL: request.url, proxied: false };
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

/** Reports a relay or proxy-authorization failure once per streak; an accepted response ends it. */
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

// Hyperliquid's public API is CORS-open. Its account traffic takes the tunnel,
// but market data must not wait on it: the TS adapter read it from a public-only instance.
const DIRECT_PUBLIC_READS: ReadonlySet<ExchangeName> = new Set(['hyperliquid', 'hyperliquidspot']);

/** Venues whose public reads may take a bypassed read key's egress; the rest keep their own route. */
/** Binance and AsterDEX public reads keep their own route, never a read key's egress. */
const hasOwnPublicReadRoute = (exchange: ExchangeName): boolean =>
  ['binance', 'binancepm', 'binanceinverse', 'asterdex'].includes(exchange);

const publicReadEligible = (exchange: ExchangeName): boolean =>
  !DIRECT_PUBLIC_READS.has(exchange) && !hasOwnPublicReadRoute(exchange);

/** The one method read routing reads: only a GET may take a read or public-read egress. */
const isGet = (method: string): boolean => method.toUpperCase() === 'GET';

/** REST left (or could not leave) an unreachable tunnel for the HTTP proxy; once per account and outage. */
export interface TunnelProxyFallbackNotice {
  outage: TunnelOutage;
  /** Why this venue's REST stayed on the tunnel: no proxy route, or a host the fleet refuses. */
  skipped?: 'no_proxy_route' | 'proxy_not_allowlisted';
}

const relayFailed = (exchange: ExchangeName, error: Error) =>
  `[${exchange}] The connection to the Tealstreet relay failed (${stripDiagnosticUrlQueries(error.message)}); this account's requests through it fail and keep retrying until it reconnects`;

const httpTransport = (
  exchange: ExchangeName,
  options: ExchangeOptions,
  transport: Transport,
  provider: TransportProvider,
  routing: ResolvedRouting,
  publicPrefix?: string,
  proxyAuthorization?: ProxyAuthorizationNotice,
  onTunnelProxyFallback?: (notice: TunnelProxyFallbackNotice) => void,
  onBybitPublicRateLimit?: (notice: BybitPublicRateLimitNotice) => void,
  infoFacts?: HyperliquidStartupFacts,
  onHttpTiming?: (event: EngineEvent) => void,
): {
  mode: 'direct' | 'how';
  invalidateCatalogue(): void;
  connectionDiagnostic(): ConnectionCloseDiagnostic | undefined;
  hiddenTimerLateness(): TimerLatenessBucket;
  request: Transport['request'];
  prepareHttp: NonNullable<Transport['prepareHttp']>;
  dispose(): void;
} => {
  const tunnel = publicPrefix === undefined && usesTunnel(exchange, options);
  const restOptions = restOptionsFor(exchange, options);
  const restRouting = restOptions === options ? routing : resolved(exchange, restOptions, provider).restRouting;
  const adapter = tunnel
    ? provider.restAdapter({
        exchange,
        options,
        variant: { region: routing.howEgress === 'jp' ? 'JA' : 'SG', raw: true, WebSocket: transport.WebSocket },
      })
    : undefined;
  let disposed = false;
  const infoBudget =
    exchange === 'hyperliquid' || exchange === 'hyperliquidspot' ? new HyperliquidInfoBudget() : undefined;
  const timing =
    infoBudget && onHttpTiming
      ? new HttpTiming(
          tunnelHostLabel(adapter ? tunnelEgressUrl(routing.howEgress) : undefined),
          () => (adapter as Partial<DisposableAxiosAdapter> | undefined)?.tunnelLatency?.(),
          onHttpTiming,
        )
      : undefined;
  const catalogueOrigins = new Set<string>();
  const pending = new Set<AbortController>();
  // While this realm's tunnel is unreachable, REST goes through the HTTP proxy, which
  // egresses from the same fleet. Decided per request at prepare: a prepared request keeps its path.
  const proxyOptions = { ...options, tealstreetTunnel: false, tealstreetProxy: true };
  let proxyRouting: ResolvedRouting | undefined;
  const reportedOutages = new Set<string>();
  const tunnelProxyRoute = (
    request: Request,
    intent?: CredentialIntent,
    failing = false,
  ): RestTransport | undefined => {
    const outage = tunnelOutage();
    if (!adapter || (!outage && !failing)) return undefined;
    proxyRouting ??= resolved(exchange, proxyOptions, provider).restRouting;
    const route = restRoute(provider, exchange, proxyOptions, proxyRouting, request, transport.serverHeaders, intent);
    const skipped =
      !route.proxied && !isPublicBinanceRead(exchange, request, intent)
        ? 'no_proxy_route'
        : route.proxied && !proxyFleetForwards(request.url)
          ? 'proxy_not_allowlisted'
          : undefined;
    const reportKey = outage && `${outage.episode}:${skipped ?? 'proxy'}`;
    if (outage && reportKey && !reportedOutages.has(reportKey)) {
      reportedOutages.add(reportKey);
      onTunnelProxyFallback?.({ outage, ...(skipped ? { skipped } : {}) });
    }
    return skipped ? undefined : route;
  };
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
  /** `rerouted` re-prepares an abandoned tunnel preparation on the proxy, within its original deadline. */
  type Reroute = { abandon?: AbortSignal; rerouted?: { startedAt: number } };
  const prepare = async (request: Request, intent?: CredentialIntent, reroute: Reroute = {}): Promise<PreparedHttp> => {
    if (disposed) throw new Error('Engine transport disposed');
    let proxyRoute: RestTransport | undefined;
    try {
      proxyRoute = tunnelProxyRoute(request, intent, reroute.rerouted !== undefined);
    } catch {
      proxyRoute = undefined;
    }
    const tunnelAdapter = proxyRoute ? undefined : adapter;
    const controller = new AbortController();
    pending.add(controller);
    const signal = AbortSignal.any(
      [request.signal, controller.signal, reroute.abandon].filter((s): s is AbortSignal => s !== undefined),
    );
    const started = reroute.rerouted?.startedAt ?? Date.now();
    const kind = requestClass(request.method, request.url, request.requestClass);
    const deadlineMs = requestBudgetMs(kind, request.timeoutMs);
    const timeoutError = () => Object.assign(new HttpPreparationError('timedOut'), {
      message: `HTTP request timed out (${budgetDiagnostic(kind, deadlineMs, Date.now() - started)})`,
      requestClass: kind, budgetMs: deadlineMs, elapsedMs: Math.max(0, Date.now() - started),
    });
    const remaining = () => Math.max(0, deadlineMs - (Date.now() - started));
    type Preparation =
      | { kind: 'cached'; response: HttpResponse }
      | { kind: 'tunnel'; adapter: NonNullable<typeof tunnelAdapter>; resource?: PreparedTunnelRequest }
      | { kind: 'direct'; resource?: PreparedHttp };
    let preparation: Preparation | undefined;
    let infoRead: ReturnType<HyperliquidStartupFacts['begin']> | undefined;
    let catalogueRead: CatalogueRead | undefined;
    let physicalRequest = false;
    const settleDeadline = (deadlineFired = false) => {
      if (
        physicalRequest &&
        (deadlineFired || remaining() <= 0) &&
        !disposed &&
        !request.signal?.aborted &&
        !reroute.abandon?.aborted
      )
        catalogueRead?.timedOut?.();
    };
    let bound = false;
    let cancelBound: (() => void) | undefined;
    const cleanup = () => {
      catalogueRead?.cancel();
      infoRead?.close();
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
      const resource = preparation?.kind === 'cached' ? undefined : preparation?.resource;
      if (resource && !preparedReleased) {
        preparedReleased = true;
        try {
          resource.cancel?.();
        } catch {
          /* Cleanup cannot strand this request. */
        }
      }
    };
    const onAbort = () => {
      settleDeadline();
      releaseResources();
      cleanup();
    };
    const timer = setTimeout(() => {
      settleDeadline(true);
      controller.abort(timeoutError());
    }, remaining());
    signal.addEventListener('abort', onAbort, { once: true });
    const cancel = () => {
      settleDeadline();
      controller.abort();
      releaseResources();
      cleanup();
    };
    const active = () => {
      signal.throwIfAborted();
      if (disposed) throw new Error('Engine transport disposed');
      if (remaining() <= 0) throw timeoutError();
    };
    const beforeRelease = <T>(operation: () => T): T => {
      try {
        active();
        const result = operation();
        active();
        return result;
      } catch (error) {
        cancel();
        if (error instanceof TradeTunnelTimeoutError) proxyAuthorization?.failed(relayFailed(exchange, error));
        if (intent !== undefined && error instanceof TradeTunnelTimeoutError)
          throw timeoutError();
        throw error;
      }
    };
    const wrap = (response: HttpResponse): HttpResponse => {
      // An answer through the relay ends a relay outage's one notice.
      if (tunnelAdapter) proxyAuthorization?.accepted();
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
      const preparedUrl = request.url;
      const original = new URL(preparedUrl);
      const signed =
        intent !== undefined || orderlyExchanges.has(exchange) || coinbaseExchanges.has(exchange)
          ? { url: request.url, method: request.method, body: request.body, headers: Object.entries(request.headers) }
          : undefined;
      const route =
        publicPrefix === undefined
          ? (proxyRoute ??
            restRoute(provider, exchange, restOptions, restRouting, request, transport.serverHeaders, intent))
          : { baseURL: publicPrefix ? `${publicPrefix}/${request.url}` : request.url, proxied: false };
      if (infoFacts && request.method === 'POST' && original.pathname === '/info')
        infoRead = infoFacts.begin(tunnelAdapter ? 'how' : route.proxied ? 'proxy' : 'direct', deadlineMs);
      const target = route.baseURL ?? request.url;
      const publicProxy = provider.publicCorsProxy();
      const payloadSigned = payloadSignedViaPublicProxy(exchange, options);
      const testnet = options.testnet;
      refuseSignedPublicProxy({
        exchange,
        method: request.method,
        testnet,
        publicProxy,
        target,
        url: request.url,
        headers: request.headers,
        intent,
        credentialHeaders: CREDENTIAL_HEADERS,
        payloadSigned,
      });
      const prefix = target.endsWith(request.url) ? target.slice(0, -request.url.length) : undefined;
      if (intent !== undefined && prefix === undefined) throw new HttpPreparationError('unsupported');
      const aliases = httpHeaderAliases(exchange, route.proxied, Boolean(tunnelAdapter));
      const hostHeaders = AxiosHeaders.from(venueHeaders(exchange, options));
      const config: InternalAxiosRequestConfig & { requestClass: typeof kind } = {
        method: request.method,
        requestClass: kind,
        url: target,
        baseURL: request.url,
        headers: AxiosHeaders.from({ ...request.headers, ...venueHeaders(exchange, options) }),
        data: request.body,
        timeout: remaining(),
        signal,
      };
      aliasHeaders(config.headers, aliases);
      const before = AxiosHeaders.from(config.headers.toJSON());
      if (route.proxied && route.authorize)
        await untilAborted(
          withProxyTokenProvider(proxyTokenProvider, () => route.authorize!(config)),
          signal,
        );
      signal.throwIfAborted();
      if (remaining() <= 0) throw timeoutError();
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
      if (infoBudget) {
        catalogueRead = await untilAborted(
          sharedHyperliquidCatalogue.prepare({ ...request, intent }, signal, started + deadlineMs),
          signal,
        );
        if (catalogueRead) catalogueOrigins.add(original.origin);
      } else if (['binance', 'binancepm', 'binanceinverse', 'binancespot'].includes(exchange)) {
        // Only resolved public routing facts enter the anonymous catalogue key.
        const routeEndpoint = tunnelAdapter
          ? new URL(tunnelEgressUrl(routing.howEgress))
          : prefix
            ? new URL(prefix)
            : undefined;
        if (
          !routeEndpoint ||
          !(routeEndpoint.username || routeEndpoint.password || routeEndpoint.search || routeEndpoint.hash)
        ) {
          catalogueRead = await untilAborted(
            sharedBinanceCatalogue.prepare(
              { ...request, intent },
              signal,
              started + deadlineMs,
              {
                network: options.testnet ? 'testnet' : 'mainnet',
                route: tunnelAdapter
                  ? `how:${routing.howEgress}:${routeEndpoint!.href}`
                  : routeEndpoint
                    ? `proxy:${routeEndpoint.href}`
                    : 'direct',
              },
              () => !disposed,
            ),
            signal,
          );
        }
      }
      const cached = catalogueRead?.cached;
      const cacheFresh = () =>
        !cached || (Date.now() >= catalogueRead!.cachedAt! && Date.now() < catalogueRead!.expiresAt!);
      const leaseFresh = () =>
        catalogueRead?.leaseExpiresAt === undefined ||
        (Date.now() >= catalogueRead.leaseStartedAt! && Date.now() < catalogueRead.leaseExpiresAt);
      let prepared: Preparation;
      if (cached) {
        prepared = { kind: 'cached', response: cached };
      } else if (tunnelAdapter) {
        const candidate: unknown = Reflect.get(tunnelAdapter, intent === undefined ? 'prepareLegacy' : 'prepare');
        const prepareTunnel = isTunnelPreparation(candidate) ? candidate : undefined;
        prepared = {
          kind: 'tunnel',
          adapter: tunnelAdapter,
          resource: prepareTunnel
            ? await waitForPrepared(
                withProxyTokenProvider(proxyTokenProvider, () => prepareTunnel.call(tunnelAdapter, config)),
              )
            : undefined,
        };
        if (!prepareTunnel && intent !== undefined) throw new HttpPreparationError('unsupported');
      } else {
        const prepareDirect = transport.prepareHttp;
        if (intent !== undefined && !prepareDirect) throw new HttpPreparationError('unsupported');
        prepared = {
          kind: 'direct',
          resource: intent !== undefined && prepareDirect
            ? await waitForPrepared(
                Promise.resolve(
                  prepareDirect.call(transport, {
                    ...request,
                    intent,
                    route: 'direct',
                    url: target,
                    headers: wireHeaders(config.headers),
                    timeoutMs: remaining(),
                    signal,
                  }),
                ),
              )
            : undefined,
        };
      }
      preparation = prepared;
      signal.throwIfAborted();
      type Admission = {
        budget: HyperliquidInfoBudget;
        read: NonNullable<typeof infoRead>;
        generation: Awaited<ReturnType<HyperliquidInfoBudget['wait']>>;
      };
      let pendingAdmission: Admission | undefined;
      if (!cached && infoBudget && request.method === 'POST' && original.pathname === '/info') {
        const budget = infoBudget;
        const read = infoRead;
        if (!read) throw new HttpPreparationError('invalid');
        const generation = await untilAborted(
          read.wait(() => budget.wait(original.origin, request.body, started + deadlineMs, signal, request.readClass)),
          signal,
        );
        pendingAdmission = { budget, read, generation };
      }
      const admission = pendingAdmission;
      const resource = prepared.kind === 'cached' ? undefined : prepared.resource;
      if (!cached && intent !== undefined && (!resource || typeof resource.bind !== 'function'))
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
        ...(tunnelAdapter ? {} : { expiresAt: prepared.kind === 'direct' ? prepared.resource?.expiresAt : undefined }),
        cancel,
        bind(packet) {
          beforeRelease(() => {
            if (bound) throw new HttpPreparationError('invalid');
          });
          if (
            packet.request !== request.request ||
            packet.method !== request.method ||
            packet.readClass !== request.readClass ||
            new URL(packet.url).origin !== original.origin ||
            packet.route !== request.route ||
            packet.signal !== request.signal ||
            (catalogueRead &&
              (packet.url !== signed!.url ||
                packet.body !== signed!.body ||
                JSON.stringify(Object.entries(packet.headers)) !== JSON.stringify(signed!.headers)))
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
          beforeRelease(() => refuseSignedPublicProxy({
            exchange,
            method: packet.method,
            testnet,
            publicProxy,
            target: url,
            url: intent === undefined ? preparedUrl : packet.url,
            headers: packet.headers,
            intent,
            credentialHeaders: CREDENTIAL_HEADERS,
            payloadSigned,
          }));
          beforeRelease(() => {
            if (intent === undefined && packet.url !== preparedUrl) throw new HttpPreparationError('invalid');
          });
          const finalConfig = { ...config, url, headers, data: packet.body, timeout: remaining(), signal };
          const network: Request = {
            ...packet,
            ...(intent === undefined ? {} : { route: 'direct' as const }),
            url,
            headers: wireHeaders(headers),
            timeoutMs: remaining(),
            signal,
          };
          if (intent === undefined) delete network.timeoutMs;
          let send: () => Promise<HttpResponse>;
          let ready: () => boolean;
          if (prepared.kind === 'cached') {
            ready = cacheFresh;
            send = () => {
              if (!cacheFresh()) throw timeoutError();
              return Promise.resolve(prepared.response);
            };
          } else if (prepared.kind === 'tunnel') {
            const inner = beforeRelease(() => validateBound(prepared.resource?.bind(finalConfig)));
            ready = inner ? () => inner.ready() : () => true;
            send = () => {
              const response = inner
                ? inner.dispatch()
                : withProxyTokenProvider(proxyTokenProvider, () => prepared.adapter(finalConfig));
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
            const inner = beforeRelease(() => validateBound(prepared.resource?.bind(network)));
            ready = inner ? () => inner.ready() : () => true;
            send = inner ? () => inner.dispatch() : () => transport.request(network);
          }
          const timingStage = httpTimingStage(packet.url, packet.body);
          bound = true;
          let sent = false;
          let admitting = false;
          let admitted = false;
          const admitDispatch = admission ? async <T>(handoff: () => T): Promise<Awaited<T>> => {
            beforeRelease(() => {
              if (sent || admitting) throw new HttpPreparationError('invalid');
            });
            admitting = true;
            try {
              const result = await untilAborted(
                admission.read.wait(() =>
                  admission.budget.admitDispatch(
                    original.origin,
                    admission.generation,
                    started + deadlineMs,
                    signal,
                    () => {
                      active();
                      admitted = true;
                      try {
                        return { value: handoff() };
                      } finally {
                        admitted = false;
                      }
                    },
                    () => sent,
                  ),
                ),
                signal,
              );
              return await result.value;
            } catch (error) {
              cancel();
              throw error;
            } finally {
              admitting = false;
            }
          } : undefined;
          return {
            ...(admitDispatch ? { admitDispatch } : {}),
            ready() {
              return beforeRelease(() => {
                if (!leaseFresh()) throw new HttpPreparationError('stale');
                return !sent && ready();
              });
            },
            dispatch() {
              if (admitDispatch && !admitted) return admitDispatch(() => this.dispatch());
              beforeRelease(() => {
                if (sent) throw new HttpPreparationError('invalid');
                if (!leaseFresh()) throw new HttpPreparationError('stale');
              });
              sent = true;
              if (!cached) infoRead?.sent();
              const dispatchedAt = Date.now();
              const sample = (outcome: HttpTimingOutcome, response?: HttpResponse) =>
                timing?.record({
                  route: tunnelAdapter ? 'tunnel' : route.proxied ? 'proxy' : 'direct',
                  kind: httpTimingKind(request.url),
                  stage: timingStage,
                  outcome,
                  queuedMs: dispatchedAt - started,
                  wireMs: Date.now() - dispatchedAt,
                  upstreamMs: tunnelAdapter ? upstreamMsFromHeaders(response?.headers) : undefined,
                  status: response?.status,
                });
              try {
                const response = send();
                physicalRequest = true;
                return untilAborted(response, signal)
                  .then(
                    (value) => {
                      if (!cached) sample('ok', value);
                      return value;
                    },
                    (error) => {
                      // A caller or host calling the request off is not a network outcome.
                      if (!cached && !disposed && !request.signal?.aborted && !reroute.abandon?.aborted)
                        sample(
                          error instanceof TradeTunnelTimeoutError ||
                            (error instanceof HttpPreparationError && error.code === 'timedOut') ||
                            remaining() <= 0
                            ? 'timeout'
                            : 'error',
                        );
                      throw error;
                    },
                  )
                  .then((value) =>
                    route.proxied && !tunnelAdapter ? untilAborted(checkProxyAuthorization(value), signal) : value,
                  )
                  .then(async (value) => {
                    if (!cached) infoRead?.response(value.status);
                    if (!cached && infoBudget && request.method === 'POST' && original.pathname === '/info') {
                      const reportFailure = () => {
                        if (!signal.aborted) warnHyperliquidInfoBudgetUnavailable();
                      };
                      const recorded = Promise.resolve()
                        .then(() => infoBudget.response(original.origin, value.status, value.headers ?? ''))
                        .catch(reportFailure);
                      // Only a 429 must publish its pause before Rust can enqueue its retry.
                      if (value.status === 429) await untilAborted(recorded, signal).catch(reportFailure);
                    }
                    return catalogueRead && !cached ? untilAborted(catalogueRead.complete(value), signal) : value;
                  })
                  .then((value) => wrap(observeBybitPublicRateLimit(packet, value, onBybitPublicRateLimit)))
                  .catch((error) => {
                    const report = (): never => {
                      // No answer through the relay, and nobody called it off: the relay is down.
                      if (tunnelAdapter && !disposed && !request.signal?.aborted && !packet.signal?.aborted)
                        proxyAuthorization?.failed(
                          relayFailed(exchange, error instanceof Error ? error : new Error(String(error))),
                        );
                      cancel();
                      throw error;
                    };
                    const failed = catalogueRead?.failed?.();
                    return failed ? untilAborted(failed, signal).then(report, report) : report();
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
          reason.includes(PROXY_AUTHORIZATION_MISSING)
            ? reason
            : `[${exchange}] ${PROXY_AUTHORIZATION_MISSING}: ${reason}`,
        );
      }
      // The relay never prepared it, and nobody called it off: the relay is down.
      if (tunnelAdapter && (preparation?.kind === 'cached' || !preparation?.resource) && !disposed && !request.signal?.aborted && !reroute.abandon?.aborted)
        proxyAuthorization?.failed(relayFailed(exchange, error instanceof Error ? error : new Error(String(error))));
      if (intent !== undefined && error instanceof TradeTunnelTimeoutError) throw timeoutError();
      throw error;
    }
  };
  // An unsigned public read has nothing an egress IP vouches for, so once the tunnel is
  // visibly failing it takes the HTTP proxy: at once, or by abandoning a tunnel
  // preparation still in readiness. Nothing has been sent before a preparation resolves.
  // Not async: off the tunnel path the preparation resolves exactly as an
  // unwrapped prepare, so the engine's dispatch order stays its own.
  const preparePublic = (request: Request): Promise<PreparedHttp> => {
    if (!adapter || publicPrefix !== undefined) return prepare(request, 'public');
    const startedAt = Date.now();
    if (tunnelFailing()) return prepare(request, 'public', { rerouted: { startedAt } });
    const abandon = new AbortController();
    const stop = onTunnelFailing(() => abandon.abort());
    return prepare(request, 'public', { abandon: abandon.signal }).then(
      (prepared) => {
        stop();
        return prepared;
      },
      (error) => {
        stop();
        if (!abandon.signal.aborted || disposed || request.signal?.aborted) throw error;
        return prepare(request, 'public', { rerouted: { startedAt } });
      },
    );
  };
  const request: Transport['request'] = async (request) => (await prepare(request)).bind(request).dispatch();
  return {
    mode: adapter ? 'how' : 'direct',
    invalidateCatalogue: () => {
      for (const origin of catalogueOrigins) void sharedHyperliquidCatalogue.invalidateOrigin(origin);
    },
    connectionDiagnostic: () => (adapter as Partial<DisposableAxiosAdapter> | undefined)?.connectionDiagnostic?.(),
    hiddenTimerLateness: () =>
      (adapter as Partial<DisposableAxiosAdapter> | undefined)?.hiddenTimerLateness?.() ?? 'unknown',
    request,
    prepareHttp: (request) => (request.intent === 'public' ? preparePublic(request) : prepare(request, request.intent)),
    dispose: () => {
      timing?.flush();
      disposed = true;
      for (const controller of pending) controller.abort(new Error('Engine transport disposed'));
      pending.clear();
      (adapter as { dispose?(): void } | undefined)?.dispose?.();
    },
  };
};

export type EngineTransport = Transport & {
  reconnectDiagnostics(): AccountReconnectDiagnostics | undefined;
  startupDiagnostics(): Record<string, string>;
  invalidateCatalogue(): void;
};

/** Account options are already normalized by accountToExchangeOptions. */
export const createEngineTransport = (
  exchange: ExchangeName,
  options: ExchangeOptions,
  transport: Transport,
  credentials?: { hasSecondary: boolean },
  onProxyAuthorizationFailure?: (message: string) => void,
  onTunnelProxyFallback?: (notice: TunnelProxyFallbackNotice) => void,
  onBybitPublicRateLimit?: (notice: BybitPublicRateLimitNotice) => void,
  onHttpTiming?: (event: EngineEvent) => void,
): EngineTransport => {
  options = withSecondaryReadKeyPolicy(options);
  assertEngineAccountRouting(options, exchange);
  const infoFacts =
    exchange === 'hyperliquid' || exchange === 'hyperliquidspot' ? new HyperliquidStartupFacts() : undefined;
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
  const diagnostics: Partial<Pick<AccountReconnectDiagnostics, 'public' | 'private' | 'business'>> = {};
  const timerLateness = new HiddenTimerLateness();
  let cachedDiagnostic: AccountReconnectDiagnostics | undefined;
  let cachedDiagnosticKey = '';
  const reconnectDiagnostics = (): AccountReconnectDiagnostics | undefined => {
    const how = http.connectionDiagnostic();
    const value = {
      ...diagnostics,
      ...(how ? { how } : {}),
      hiddenTimerLateness: maximumTimerLateness(
        timerLateness.bucket,
        http.hiddenTimerLateness(),
        readHttp?.hiddenTimerLateness() ?? 'unknown',
      ),
    };
    const key = JSON.stringify(value);
    if (key !== cachedDiagnosticKey) {
      cachedDiagnosticKey = key;
      cachedDiagnostic =
        Object.keys(diagnostics).length || how || value.hiddenTimerLateness !== 'unknown' ? value : undefined;
    }
    return cachedDiagnostic;
  };
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
  const http = httpTransport(
    exchange,
    options,
    transport,
    provider,
    restRouting,
    undefined,
    proxyAuthorization,
    onTunnelProxyFallback,
    onBybitPublicRateLimit,
    infoFacts,
    onHttpTiming,
  );
  // Explicit public requests bypass account proxy/tunnel policy. URLs stay in
  // the host; venues choose only the typed route name.
  const directHttp = httpTransport(
    exchange,
    options,
    transport,
    provider,
    restRouting,
    '',
    undefined,
    undefined,
    onBybitPublicRateLimit,
    infoFacts,
  );
  const publicCorsProxy = provider.publicCorsProxy();
  const publicCorsHttp = publicCorsProxy
    ? httpTransport(
        exchange,
        options,
        transport,
        provider,
        restRouting,
        publicCorsProxy,
        undefined,
        undefined,
        onBybitPublicRateLimit,
        infoFacts,
      )
    : undefined;
  // A keyless public-data engine (a mirror's chart source) on an opted-in venue holds no
  // credential, so its reads to allowlisted hosts leave through the public CORS proxy, off the
  // trading fleet's IPs, unless the account is plain direct; the signed-packet guard still
  // refuses anything credentialed.
  const keylessPublicReads =
    publicCorsHttp !== undefined &&
    PUBLIC_PROXY_READ_HOSTS[exchange] !== undefined &&
    options.publicDataOnly === true &&
    !options.key &&
    !options.secret &&
    !options.passphrase &&
    (options.tealstreetProxy === true || tunnelFlagOn(exchange, options)) &&
    !DIRECT_PUBLIC_READS.has(exchange);
  const readOptions =
    SECONDARY_READ_KEYS_ENABLED && credentials?.hasSecondary
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
        onTunnelProxyFallback,
        onBybitPublicRateLimit,
        infoFacts,
      )
    : undefined;
  const publicReadOnly = publicReadOptions(options);
  const publicReadHttp = publicReadOnly
    ? httpTransport(
        exchange,
        publicReadOnly,
        transport,
        provider,
        resolved(exchange, publicReadOnly, provider).restRouting,
        undefined,
        proxyAuthorization,
        onTunnelProxyFallback,
        onBybitPublicRateLimit,
        infoFacts,
      )
    : undefined;
  return {
    // Protocol negotiation must use the same resolved route as connect(),
    // including hosts (such as Workers) which construct the engine themselves.
    startupDiagnostics: () => infoFacts?.tags() ?? {},
    invalidateCatalogue: () => {
      http.invalidateCatalogue();
      directHttp.invalidateCatalogue();
      publicCorsHttp?.invalidateCatalogue();
      readHttp?.invalidateCatalogue();
      publicReadHttp?.invalidateCatalogue();
    },
    httpMode: http.mode,
    reconnectDiagnostics,
    engineOptions: {
      ...transport.engineOptions,
      ...(exchange === 'grvt' ? { grvtSessionProxy: Boolean(routes.private.host) } : {}),
    },
    webSocketHeaders: transport.webSocketHeaders,
    now: () => transport.now(),
    setTimeout: (fn, ms) => {
      const dueAt = Date.now() + Math.max(0, ms);
      return transport.setTimeout(() => {
        timerLateness.observe(dueAt);
        fn();
      }, ms);
    },
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
                : intent === 'public' && DIRECT_PUBLIC_READS.has(exchange)
                  ? directHttp
                  : intent === 'public' && keylessPublicReads && readsViaPublicProxy(exchange, packet.url)
                    ? publicCorsHttp
                    : intent === 'public' &&
                      readHttp &&
                      !['binance', 'binancepm', 'binanceinverse', 'asterdex'].includes(exchange)
                    ? readHttp
                    : intent === 'public' && publicReadHttp && isGet(packet.method) && publicReadEligible(exchange)
                      ? publicReadHttp
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
        isGet(request.method) &&
        (!['binance', 'binancepm', 'binanceinverse', 'asterdex'].includes(exchange) ||
          hasHeader(request.headers, secondaryReadHeaders[exchange]!))
        ? readHttp.request(nativeRequest)
        : keylessPublicReads && readsViaPublicProxy(exchange, request.url)
          ? publicCorsHttp!.request(nativeRequest)
          : http.request(nativeRequest);
    },
    dispose: () => {
      http.dispose();
      readHttp?.dispose();
      publicReadHttp?.dispose();
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
      const diagnostic = new ConnectionDiagnostics(
        connection?.kind === 'relay' ? 'relay' : target === url ? 'direct' : 'proxy',
      );
      diagnostic.dial(target);
      let locallyClosed = false;
      const recordClose = (code?: number) => {
        diagnostics[channel] = diagnostic.closed(
          locallyClosed ? 'local' : code === undefined || code === 1006 ? 'unknown' : 'remote',
        );
      };
      if (connection?.kind !== 'relay') {
        const native = transport.connect(target, {
          ...handlers,
          headers,
          onMessage: receive,
          onBinary: receiveBinary,
          onOpen: () => {
            diagnostic.opened();
            handlers.onOpen();
          },
          onClose: (code) => {
            recordClose(code);
            handlers.onClose(code);
          },
        });
        return {
          send: (text) => native.send(text),
          close: () => {
            locallyClosed = true;
            recordClose();
            native.close();
          },
        };
      }
      let socket: { send(text: string): void; close(): void } | undefined;
      let relayDiagnostics: ConnectionDiagnostics | undefined;
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
          relayDiagnostics = relay.diagnostics;
          relay.addEventListener('open', () => {
            if (!closed) {
              opened = true;
              diagnostic.opened();
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
                diagnostics[channel] = relay.diagnostics?.lastClose ?? diagnostic.closed('local');
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
              diagnostics[channel] = relay.diagnostics?.lastClose ?? diagnostic.closed('unknown');
              handlers.onClose((event as CloseEvent).code);
            }
          });
        })
        .catch(() => {
          if (!closed) {
            closed = true;
            recordClose(1006);
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
              locallyClosed = true;
              diagnostics[channel] = (relayDiagnostics ?? diagnostic).closed('local');
              socket?.close();
              handlers.onClose(1006);
            } else pending.push(text);
          }
        },
        close: () => {
          locallyClosed = true;
          diagnostics[channel] = (relayDiagnostics ?? diagnostic).closed('local');
          closed = true;
          pending.length = 0;
          socket?.close();
        },
      };
    },
  };
};
