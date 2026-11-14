import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest, RemoteSignerResponse } from '../../types';

import axios, { AxiosHeaders } from 'axios';
import qs from 'qs';

import { markSkipRemoteSigning } from '../../remoteSigningAdapter';
import { signHttpCapture } from '../../utils/signHttpCapture';

/** Preserve frozen native nonces; Kraken also permits REST requests without one.
 * This compatibility codec never creates a second nonce allocator beside Rust. */
export const signKrakenHttp = async (
  host: Pick<EngineHost, 'hostedSigning' | 'hostedSigningClock'>,
  options: ExchangeOptions,
  primaryComplete: boolean,
  request: RemoteSignerRequest,
): Promise<RemoteSignerResponse> => {
  const xhr = axios.create({
    baseURL: options.testnet ? 'https://demo-futures.kraken.com' : 'https://futures.kraken.com',
    paramsSerializer: {
      serialize: (params) =>
        qs.stringify(params, { arrayFormat: 'repeat', sort: (a, b) => a.localeCompare(b), format: 'RFC1738' }),
    },
  });
  xhr.interceptors.request.use(async (config) => {
    const uri = new URL(axios.getUri({ ...config, params: undefined }));
    const path = uri.pathname;
    const privatePath = path.startsWith('/derivatives/api/v3/') || path.startsWith('/api/history/');
    const publicEndpoint = [
      '/instruments',
      '/tickers',
      '/orderbook',
      '/history',
      '/historicalfundingrates',
      '/feeSchedules',
    ].some((suffix) => path.endsWith(suffix));
    if (!privatePath || publicEndpoint || options.publicDataOnly || !primaryComplete) {
      markSkipRemoteSigning(config);
      return config;
    }
    const method = config.method?.toUpperCase() ?? 'GET';
    let form = method === 'GET' ? uri.search.slice(1) : typeof config.data === 'string' ? config.data : '';
    if (method === 'GET' && config.params)
      form = qs.stringify(config.params, {
        arrayFormat: 'repeat',
        sort: (a, b) => a.localeCompare(b),
        format: 'RFC1738',
      });
    if (method !== 'GET' && config.data && typeof config.data === 'object') {
      form = path.endsWith('/batchorder')
        ? `json=${encodeURIComponent(JSON.stringify(config.data))}`
        : qs.stringify(config.data, { arrayFormat: 'repeat', sort: (a, b) => a.localeCompare(b), format: 'RFC1738' });
    }
    const headers = new AxiosHeaders(config.headers);
    const nonce = headers.get('Nonce');
    if (
      nonce !== undefined &&
      nonce !== null &&
      (typeof nonce !== 'string' ||
        !/^(0|[1-9][0-9]{0,19})$/.test(nonce) ||
        BigInt(nonce) > 18_446_744_073_709_551_615n)
    )
      throw new Error('Invalid Kraken signing nonce');
    const clock = host.hostedSigningClock();
    const output = await host.hostedSigning({
      kind: 'http',
      ...clock,
      payload: JSON.stringify({
        slot: 'primary',
        payload: `${form}${nonce ?? ''}${path.replace(/^\/derivatives/, '')}`,
      }),
    });
    if (!output.credential || !output.signature) throw new Error('Signing result is unavailable');
    headers.set({
      APIKey: output.credential,
      Authent: output.signature,
      'Content-Type': 'application/x-www-form-urlencoded',
    });
    if (method !== 'GET') config.data = form;
    if (method === 'GET') config.url = `${uri.origin}${path}${form ? `?${form}` : ''}`;
    delete config.params;
    return { ...config, headers };
  });
  return signHttpCapture(xhr, request);
};
