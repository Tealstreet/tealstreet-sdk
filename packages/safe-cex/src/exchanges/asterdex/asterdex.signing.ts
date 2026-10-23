import type { InternalAxiosRequestConfig } from 'axios';
import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';

import { signPayload } from '../../utils/payloadSigner';
import { markSkipRemoteSigning } from '../../remoteSigningAdapter';

const PUBLIC_FAPI_PREFIXES = [
  '/fapi/v1/ticker',
  '/fapi/v1/klines',
  '/fapi/v1/depth',
  '/fapi/v1/trades',
  '/fapi/v1/aggTrades',
  '/fapi/v1/exchangeInfo',
  '/fapi/v1/ping',
  '/fapi/v1/time',
  '/fapi/v3/ticker',
  '/fapi/v3/klines',
  '/fapi/v3/depth',
  '/fapi/v3/trades',
  '/fapi/v3/aggTrades',
  '/fapi/v3/exchangeInfo',
  '/fapi/v3/ping',
  '/fapi/v3/time',
] as const;

const SIGNING_FIELDS = ['signature', 'timestamp', 'recvWindow', 'user', 'signer', 'nonce'];
const RECV_WINDOW = 20000;

export const stripQueryString = (url: string): string => url.split('?')[0];

export const isAsterDEXPublicEndpoint = (url?: string): boolean => {
  if (!url) return false;
  const path = stripQueryString(url);
  return PUBLIC_FAPI_PREFIXES.some((prefix) => path.startsWith(prefix) || path.includes(prefix));
};

/** Insertion order, no URL encoding; undefined, null and empty values are dropped. */
export const buildAsterDEXQueryString = (params: Record<string, any>): string =>
  Object.entries(params)
    .filter(([_, value]) => value !== undefined && value !== null && value !== '')
    .map(([key, value]) => `${key}=${String(value)}`)
    .join('&');

/** A resend still carries the previous attempt's signing fields. */
export const withoutAsterDEXSigningFields = (params: unknown): Record<string, any> =>
  params && typeof params === 'object'
    ? Object.fromEntries(Object.entries(params).filter(([key]) => !SIGNING_FIELDS.includes(key)))
    : {};

/** The V1 request shape the TS venue sends: params with recvWindow and timestamp
 * last, an HMAC hex signature over their query string, and no body. Hosted
 * capture installs it; V3 wallet requests are refused, never HMAC-signed. */
export const createAsterDEXV1RequestInterceptor = (options: ExchangeOptions, now: () => number) =>
  async (config: InternalAxiosRequestConfig): Promise<InternalAxiosRequestConfig> => {
    const path = stripQueryString(config.url || '');
    if (isAsterDEXPublicEndpoint(path)) return markSkipRemoteSigning(config);
    if (path.includes('/fapi/v3/')) throw new Error('AsterDEX V3 requests are not HMAC-signed');
    const data = withoutAsterDEXSigningFields(config.data || config.params || {});
    const params: Record<string, any> = { ...data, recvWindow: RECV_WINDOW, timestamp: now() };
    const { signature, headers } = await signPayload(options, 'asterdex', buildAsterDEXQueryString(params), 'hex');
    params.signature = signature;
    const next = { ...config, params, headers: new AxiosHeaders({ ...config.headers, 'X-MBX-APIKEY': options.key, ...headers }) };
    delete next.data;
    return next as InternalAxiosRequestConfig;
  };
