import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';

import qs from 'qs';

import { isNodeOrBunRuntime } from '../../utils/runtime';
import { uuid } from '../../utils/uuid';

const signedBeforeAbort = <T>(promise: Promise<T>, signal: AbortSignal): Promise<T> =>
  new Promise((resolve, reject) => {
    const abort = () => reject(signal.reason);
    signal.addEventListener('abort', abort, { once: true });
    promise.then(resolve, reject).finally(() => signal.removeEventListener('abort', abort));
    if (signal.aborted) abort();
  });

/** The TS Paradex JWT manager uses fetch; only its API client uses remoteSigner. */
export const paradexSigningTransport = (options: ExchangeOptions, transport: Transport): Transport => {
  const signer = options.remoteSigner;
  if (!signer) return transport;
  return {
    ...transport,
    async request(request) {
      const url = new URL(request.url);
      if (url.pathname === '/v1/auth' || url.pathname.startsWith('/v1/auth/')) return transport.request(request);
      const started = Date.now();
      const timeoutMs = Math.max(1, request.timeoutMs ?? 20_000);
      const deadline = AbortSignal.timeout(timeoutMs);
      const signal = request.signal ? AbortSignal.any([request.signal, deadline]) : deadline;
      signal.throwIfAborted();
      const id = uuid();
      let data: unknown = request.body;
      try {
        if (request.body) data = JSON.parse(request.body);
      } catch {
        // The signing contract also accepts raw bodies.
      }
      const response = await signedBeforeAbort(signer.sign({
        id,
        type: 'http',
        exchangeName: 'paradex',
        accountName: signer.accountName,
        config: {
          method: request.method,
          baseURL: `${url.origin}/v1`,
          url: url.pathname.replace(/^\/v1(?=\/|$)/, '') || '/',
          params: url.search ? qs.parse(url.search.slice(1)) : undefined,
          data,
          headers: { ...request.headers },
        },
      }), signal);
      signal.throwIfAborted();
      if (response.id !== id) throw new Error('Remote signing returned a different request ID');
      if (response.error) throw new Error(`Remote signing failed: ${response.error}`);
      const config = response.config;
      if (!config || !config.headers || typeof config.headers !== 'object') {
        throw new Error('Remote signing returned no request headers');
      }
      const headers: Record<string, string> = {};
      for (const [name, value] of Object.entries(config.headers)) {
        if (typeof value !== 'string') throw new Error('Remote signing returned an invalid header');
        headers[name.toLowerCase()] = value;
      }
      for (const [name, value] of Object.entries(request.headers)) {
        const key = name.toLowerCase();
        if (!['authorization', 'tealstreet-authorization'].includes(key)) continue;
        if (!headers[key]?.startsWith('Bearer ') || headers[key].length <= 7) {
          if (value.startsWith('Bearer ') && value.length > 7) headers[key] = value;
          else delete headers[key];
        }
      }
      let next: URL;
      if (config.url?.startsWith('//') && transport.httpMode !== 'how') {
        // A desktop renderer can expose Node while Axios still uses the page.
        const browser = typeof window !== 'undefined' && typeof document !== 'undefined';
        const base = browser ? window.location.href : !isNodeOrBunRuntime() ? globalThis.location?.href : undefined;
        next = new URL(config.url, base);
      } else {
        next = config.url
          ? new URL(config.url.startsWith('/') ? `/v1${config.url}` : config.url, `${url.origin}/v1/`)
          : new URL(url.pathname, url.origin);
      }
      if (config.params) {
        const query = qs.stringify(config.params, { arrayFormat: 'comma' });
        if (query) next.search += `${next.search ? '&' : '?'}${query}`;
      }
      return transport.request({
        ...request,
        signal,
        timeoutMs: Math.max(1, timeoutMs - (Date.now() - started)),
        url: next.toString(),
        headers,
        body: config.data === undefined ? undefined : typeof config.data === 'string' ? config.data : JSON.stringify(config.data),
      });
    },
  };
};
