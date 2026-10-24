import type { AxiosAdapter, CreateAxiosDefaults, InternalAxiosRequestConfig } from 'axios';
import type { ExchangeName, ExchangeOptions } from '../types';

/**
 * One WebSocket channel's route. `tunnelled` carries an intermediary `host`;
 * `fallback`/`readonly` are candidates the channel's own runtime may switch to, and
 * `relay` is a relay url for a private channel. A direct route has none of them.
 */
export interface ResolvedWs {
  tunnelled: boolean;
  host?: string;
  fallback?: string;
  relay?: string;
  readonly?: string;
  /** Account for a venue's first-connect silence check before any reconnect. */
  initialSilentConnects?: number;
  /** Frames still reach the engine, but do not prove the current route works. */
  ignoredMessages?: string[];
  /** Exact upstream URLs which bypass every fallback and recovery policy. */
  directUrls?: string[];
}

export interface ResolvedRouting {
  /** '' when REST goes straight to the venue. */
  restProxyHost: string;
  howEgress: 'jp' | 'sg';
  wsPublic: ResolvedWs;
  wsPrivate: ResolvedWs;
  wsBusiness: ResolvedWs;
  /** Present only for venues that declare a separate UTA private socket (bitget). */
  wsPrivateUta?: ResolvedWs;
}

export interface RestTransportContext {
  exchange: ExchangeName;
  options: ExchangeOptions;
  config: CreateAxiosDefaults;
  /** Managed engine requests select routing before credential presentation. */
  credentialIntent?: 'public' | 'primary' | 'secondary';
  /** A host may resolve the account once and reuse it across requests. */
  routing?: ResolvedRouting;
  /** Server fetch runtimes must supply the browser headers expected by the proxy. */
  serverHeaders?: boolean;
}

export interface RestTransport {
  /** The axios baseURL to use: the venue's own, or a routed one. */
  baseURL?: string;
  /** True when requests leave through an intermediary rather than straight to the venue. */
  proxied: boolean;
  /** Adds whatever the intermediary needs (credentials, browser headers) to each request. */
  authorize?: (config: InternalAxiosRequestConfig) => Promise<void>;
}

/** Which flavour of request adapter a factory asks for; `bigInt` keeps 64-bit ids as strings. */
export type RestAdapterVariant =
  'default' | 'bigInt' | { region?: 'JA' | 'SG'; raw?: boolean; WebSocket?: typeof WebSocket };

export interface RestAdapterContext {
  exchange: ExchangeName;
  options: ExchangeOptions;
  variant: RestAdapterVariant;
}

export interface PrivateProxy {
  url: string;
  /** Credentials for the proxy; resolve once per operation. Empty means none available. */
  resolveHeaders(): Promise<Record<string, string>>;
}

/** How a WebSocket to `url` must be opened, when it is not a plain venue socket. */
export type WsConnection =
  { kind: 'relay'; headers: Record<string, string> } | { kind: 'headers'; headers: Record<string, string> };

/**
 * Decides where an exchange's REST and WebSocket traffic goes. Registered once per
 * runtime with `setDefaultTransport`; `ExchangeOptions.transport` overrides it for
 * one in-process instance only.
 */
export interface TransportProvider {
  readonly name: string;
  rest(context: RestTransportContext): RestTransport;
  /** The adapter a factory's tunnelled client should use, or undefined to stay on axios's own. */
  restAdapter(context: RestAdapterContext): AxiosAdapter | undefined;
  /** Prefix for a single absolute-URL request routed like `rest()`; '' means direct. */
  proxyPrefix(context: RestTransportContext): string;
  /** CORS-unblocking proxy for public reads a browser cannot make directly, if any. */
  publicCorsProxy(): string | undefined;
  /** Whitelisted proxy for out-of-band authenticated calls (builder approvals), if any. */
  privateProxy(exchange: ExchangeName): PrivateProxy | undefined;
  /** Options for a client that must go through the transport's proxy even on a direct account. */
  requireProxy(options: ExchangeOptions): ExchangeOptions;
  /** Proxy a keyless server uses for venues that only answer whitelisted IPs, if any. */
  serverProxyUrl(exchange: ExchangeName): string | undefined;
  wsRouting(exchange: ExchangeName, options: Partial<ExchangeOptions>): ResolvedRouting;
  wsConnection(url: string): WsConnection | undefined;
}
