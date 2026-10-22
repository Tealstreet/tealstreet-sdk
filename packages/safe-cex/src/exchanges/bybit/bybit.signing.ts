import type { InternalAxiosRequestConfig } from 'axios';
import type { ExchangeName, ExchangeOptions, PayloadSignerResult } from '../../types';

import { AxiosHeaders } from 'axios';
import qs from 'qs';
import { nanoid } from 'nanoid';
import { markSkipRemoteSigning } from '../../remoteSigningAdapter';
import { canSignPayload, signPayload } from '../../utils/payloadSigner';
import { virtualClock } from '../../utils/virtualClock';
import { getBybitSiteHeaders } from './bybit.site';
import { ENDPOINTS, RECV_WINDOW } from './bybit.types';

export const withBybitRequestId = (
  config: InternalAxiosRequestConfig,
  createRequestId: () => string,
): InternalAxiosRequestConfig => ({
  ...config,
  params: {
    ...config.params,
    requestId: createRequestId(),
  },
});

export type PreparedBybitRequest = {
  config: InternalAxiosRequestConfig;
  payload: string | undefined;
  timestamp: number;
  recvWindow: number;
};

/** Preserve serialized bodies, undefined payloads and qs's actual query grammar. */
export const prepareBybitRequest = (
  config: InternalAxiosRequestConfig,
  now: () => number,
  recvWindow: number,
): PreparedBybitRequest => {
  const body = typeof config.data === 'string' ? config.data : JSON.stringify(config.data);
  const payload = config.method === 'get' ? qs.stringify(config.params) : body;
  // Clock sampling follows getters/serialization, matching the TS interceptor.
  return { config, payload, timestamp: now(), recvWindow };
};

export const finishBybitRequest = (
  prepared: PreparedBybitRequest,
  key: string,
  { signature, headers: signerHeaders }: PayloadSignerResult,
): InternalAxiosRequestConfig => ({
  ...prepared.config,
  headers: new AxiosHeaders({
    ...prepared.config.headers,
    ...signerHeaders,
    'X-BAPI-SIGN': signature,
    'X-BAPI-API-KEY': key,
    'X-BAPI-TIMESTAMP': prepared.timestamp,
  }),
});

export const isBybitUnsignedSigningRequest = (method?: string, url?: string): boolean =>
  method === 'get' && url === ENDPOINTS.KLINE;

/** The complete request-only hook, independent of API clients and venue classes. */
export const createBybitRequestInterceptor = (
  options: ExchangeOptions,
  exchangeName: ExchangeName,
  now: () => number = () => virtualClock.getCurrentTimeMs(),
  siteHeaders: Record<string, string> = exchangeName === 'bybit' || exchangeName === 'bybitinverse' || exchangeName === 'bybitspot'
    ? getBybitSiteHeaders(options) : {},
) => {
  const recvWindow = options?.extra?.recvWindow ?? RECV_WINDOW;
  return async (config: InternalAxiosRequestConfig): Promise<InternalAxiosRequestConfig> => {
    // Keep ID generation and signing in one interceptor: Axios executes hooks LIFO.
    const nextConfig = withBybitRequestId(config, nanoid);
    if (nextConfig.url?.includes('/v5/market/')) markSkipRemoteSigning(nextConfig);
    if (isBybitUnsignedSigningRequest(nextConfig.method, nextConfig.url)) {
      return { ...nextConfig, headers: new AxiosHeaders(siteHeaders) };
    }
    if (options.remoteSigner || !options.key || !canSignPayload(options)) return nextConfig;
    // Replayed data may already be serialized by Axios; preserve its exact bytes.
    const prepared = prepareBybitRequest(nextConfig, now, recvWindow);
    const { signature, headers } = await signPayload(
      options,
      exchangeName,
      [prepared.timestamp, options.key, prepared.recvWindow, prepared.payload].join(''),
      'hex',
    );
    return finishBybitRequest(prepared, options.key, { signature, headers });
  };
};
