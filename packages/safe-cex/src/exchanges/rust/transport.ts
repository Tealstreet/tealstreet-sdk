import type { Transport as EngineHostTransport, HttpResponse } from '@tealstreet/safe-cex-wasm/host';
import type { AxiosResponse, InternalAxiosRequestConfig } from 'axios';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';

import { directTransport } from '../../transport/direct';
import { getTransport } from '../../transport/registry';

type Request = Parameters<EngineHostTransport['request']>[0] & { signal?: AbortSignal };

const rawResponse = (response: AxiosResponse): HttpResponse => {
  const data: unknown = response.data;
  let body: Uint8Array;
  if (typeof data === 'string') body = new TextEncoder().encode(data);
  else if (data instanceof ArrayBuffer) body = new Uint8Array(data);
  else if (ArrayBuffer.isView(data)) body = new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  else throw new Error('Orderly Rust tunnel requires an unparsed response body');
  if (!Number.isInteger(response.status) || response.status < 100 || response.status > 599) {
    throw new Error('Orderly Rust tunnel returned an invalid HTTP status');
  }
  const headers =
    typeof response.headers === 'string'
      ? response.headers
      : Object.entries(response.headers instanceof AxiosHeaders ? response.headers.toJSON() : response.headers)
          .filter(([, value]) => value !== undefined && value !== null && value !== false)
          .flatMap(([name, value]) => (Array.isArray(value) ? value : [value]).map((item) => `${name}: ${item}`))
          .join('\n');
  return { status: response.status, headers, body };
};

/** Routes bytes already signed by Rust; intermediary authorization may add headers only. */
export const orderlyTransport = (
  name: ExchangeName,
  options: ExchangeOptions,
  base: EngineHostTransport,
): EngineHostTransport => {
  const provider = getTransport(options);
  const origin = options.testnet ? 'https://testnet-api-evm.orderly.org' : 'https://api-evm.orderly.org';
  const rest = provider.rest({ exchange: name, options, config: { baseURL: origin } });
  const routing = provider.wsRouting(name, options);
  if (
    options.tealstreetWsRoute ||
    [routing.wsPublic, routing.wsPrivate].some(
      (route) => route.tunnelled || route.host || route.fallback || route.relay || route.readonly,
    )
  ) {
    throw new Error(`[${name}] Orderly Rust WebSocket routing is not supported by this transport`);
  }
  const adapter = options.tealstreetTunnel
    ? provider.restAdapter({ exchange: name, options, variant: 'default' })
    : undefined;
  // The legacy HOW adapter parses JSON before RPC, changing signed bytes and large IDs.
  if (options.tealstreetTunnel && (!adapter || provider.name === 'tealstreet')) {
    throw new Error(
      `[${name}] Orderly Rust tunnel requires an adapter that preserves signed request and response bytes`,
    );
  }
  const routedOrigin = (rest.baseURL ?? origin).replace(/\/+$/, '');
  const directRest = routedOrigin === origin && !rest.proxied && !adapter;
  if (directRest && provider === directTransport) return base;

  return {
    webSocketHeaders: base.webSocketHeaders,
    now: base.now.bind(base),
    connect(url, handlers) {
      const connection = provider.wsConnection(url);
      if (!connection) return base.connect(url, handlers);
      if (connection.kind === 'relay' || !base.webSocketHeaders) {
        throw new Error(`[${name}] Orderly Rust WebSocket route requires unsupported relay or upgrade headers`);
      }
      const headers = AxiosHeaders.from(handlers.headers);
      for (const [key, value] of Object.entries(connection.headers)) {
        if (headers.has(key) && headers.get(key) !== value) {
          throw new Error(`[${name}] Orderly WebSocket transport changed an existing upgrade header`);
        }
        headers.set(key, value);
      }
      return base.connect(url, {
        ...handlers,
        headers: Object.fromEntries(Object.entries(headers.toJSON()).map(([key, value]) => [key, String(value)])),
      });
    },
    setTimeout: base.setTimeout.bind(base),
    clearTimeout: base.clearTimeout.bind(base),
    async request(request: Request) {
      request.signal?.throwIfAborted();
      if (directRest) return base.request(request);
      if (!request.url.startsWith(`${origin}/`)) {
        throw new Error(`[${name}] Orderly Rust request has an unexpected origin`);
      }
      const path = request.url.slice(origin.length);
      const config: InternalAxiosRequestConfig = {
        url: path,
        baseURL: routedOrigin,
        method: request.method,
        headers: AxiosHeaders.from(request.headers),
        data: request.body,
        signal: request.signal,
        timeout: options.extra?.recvWindow ?? 20000,
        responseType: 'arraybuffer',
        transformRequest: [(data: unknown) => data],
        transformResponse: [(data: unknown) => data],
        validateStatus: () => true,
      };
      if (rest.proxied) await rest.authorize?.(config);
      request.signal?.throwIfAborted();
      if (
        config.url !== path ||
        config.baseURL !== routedOrigin ||
        config.method !== request.method ||
        config.data !== request.body ||
        config.params !== undefined ||
        Object.entries(request.headers).some(([key, value]) => config.headers.get(key) !== value)
      ) {
        throw new Error(`[${name}] Orderly transport authorization changed signed request bytes or headers`);
      }
      if (!adapter) {
        const headers = Object.fromEntries(
          Object.entries(config.headers.toJSON()).map(([key, value]) => [key, String(value)]),
        );
        return base.request({ ...request, url: `${routedOrigin}${path}`, headers });
      }
      let response: AxiosResponse;
      try {
        response = await adapter(config);
      } catch (error) {
        const rejected = error as { response?: AxiosResponse };
        if (!rejected.response) throw error;
        response = rejected.response;
      }
      request.signal?.throwIfAborted();
      return rawResponse(response);
    },
  };
};
