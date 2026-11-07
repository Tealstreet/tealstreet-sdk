import type { InternalAxiosRequestConfig } from 'axios';
import type { ExchangeName, ExchangeOptions, PayloadSignerResult } from '../../types';

import { AxiosHeaders } from 'axios';
import omit from 'lodash/omit';
import qs from 'qs';

import { canSignPayload, signPayload } from '../../utils/payloadSigner';
import { virtualClock } from '../../utils/virtualClock';
import { markSkipRemoteSigning } from '../../remoteSigningAdapter';
import { ENDPOINTS as LINEAR_ENDPOINTS, PUBLIC_ENDPOINTS as LINEAR_PUBLIC, RECV_WINDOW as LINEAR_WINDOW } from './binance.types';
import { ENDPOINTS as INVERSE_ENDPOINTS, PUBLIC_ENDPOINTS as INVERSE_PUBLIC, RECV_WINDOW as INVERSE_WINDOW } from './binanceinverse.types';
import { ENDPOINTS as PM_ENDPOINTS, PUBLIC_ENDPOINTS as PM_PUBLIC, RECV_WINDOW as PM_WINDOW } from './binancepm.types';
import { PUBLIC_ENDPOINTS as SPOT_PUBLIC, RECV_WINDOW as SPOT_WINDOW } from './binancespot.types';

export type PreparedBinanceRequest = {
  config: InternalAxiosRequestConfig;
  data: Record<string, unknown>;
  payload: string;
};

/** Keep the legacy JS property order and actual qs encoding for hosted capture. */
export const prepareBinanceRequest = (
  config: InternalAxiosRequestConfig,
  now: () => number,
  recvWindow: number,
): PreparedBinanceRequest => {
  const data = { ...(config.data || config.params || {}) };
  // Delete before assignment so the replacement signature remains last.
  delete data.signature;
  data.timestamp = now();
  data.recvWindow = recvWindow;
  return { config, data, payload: qs.stringify(data, { arrayFormat: 'repeat' }) };
};

export const finishBinanceRequest = (
  prepared: PreparedBinanceRequest,
  { signature, headers }: PayloadSignerResult,
): InternalAxiosRequestConfig => {
  prepared.data.signature = signature;
  const nextConfig = { ...prepared.config, params: prepared.data };
  // Last: a replayed config still carries the previous signature's headers.
  if (headers) nextConfig.headers = new AxiosHeaders({ ...prepared.config.headers, ...headers });
  return omit(nextConfig, 'data') as InternalAxiosRequestConfig;
};

/**
 * Binance signs the query string and sends it as params, `signature` last.
 * Shared by USD-M, spot and portfolio margin, which differ only in host.
 */
export const signBinanceRequest = async (
  config: InternalAxiosRequestConfig,
  options: ExchangeOptions,
  exchangeName: ExchangeName,
  recvWindow: number,
  now: () => number = () => virtualClock.getCurrentTimeMs(),
): Promise<InternalAxiosRequestConfig> => {
  if (options.remoteSigner || !options.key || !canSignPayload(options)) return { ...config };

  const prepared = prepareBinanceRequest(config, now, recvWindow);
  const result = await signPayload(
    options,
    exchangeName,
    prepared.payload,
    'hex',
  );
  // Binance takes POST/PUT/DELETE data as query params.
  return finishBinanceRequest(prepared, result);
};

const binanceSigningPolicy = (exchangeName: ExchangeName) => exchangeName === 'binanceoptions'
    ? { publicEndpoints: ['/eapi/v1/exchangeInfo', '/eapi/v1/ticker', '/eapi/v1/mark', '/eapi/v1/klines'], listenKey: '/eapi/v1/listenKey', unsignedTestnetListenKey: false, recvWindow: 5_000 }
    : exchangeName === 'binanceinverse'
    ? { publicEndpoints: INVERSE_PUBLIC, listenKey: INVERSE_ENDPOINTS.LISTEN_KEY, unsignedTestnetListenKey: true, recvWindow: INVERSE_WINDOW }
    : exchangeName === 'binancepm'
      ? { publicEndpoints: PM_PUBLIC, listenKey: PM_ENDPOINTS.LISTEN_KEY, unsignedTestnetListenKey: false, recvWindow: PM_WINDOW }
      : exchangeName === 'binancespot'
        ? { publicEndpoints: SPOT_PUBLIC, listenKey: undefined, unsignedTestnetListenKey: false, recvWindow: SPOT_WINDOW }
        : { publicEndpoints: LINEAR_PUBLIC, listenKey: LINEAR_ENDPOINTS.LISTEN_KEY, unsignedTestnetListenKey: false, recvWindow: LINEAR_WINDOW };

export const isBinancePublicSigningRequest = (exchangeName: ExchangeName, url?: string): boolean =>
  binanceSigningPolicy(exchangeName).publicEndpoints.includes(`${url}`);

// Request policy and canonical serialization survive independently of TS venue
// classes. Both the normal Axios client and hosted capture install this hook.
export const createBinanceRequestInterceptor = (
  options: ExchangeOptions,
  exchangeName: ExchangeName,
  now: () => number = () => virtualClock.getCurrentTimeMs(),
) => {
  const policy = binanceSigningPolicy(exchangeName);
  return (config: InternalAxiosRequestConfig) => {
    if (policy.listenKey !== undefined && config.url === policy.listenKey
      && (policy.unsignedTestnetListenKey || !options.testnet)) return markSkipRemoteSigning(config);
    if (isBinancePublicSigningRequest(exchangeName, config.url)) {
      return markSkipRemoteSigning({
        ...config,
        headers: AxiosHeaders.from({ 'Content-Type': 'application/json, charset=utf-8' }),
        timeout: 20_000,
      });
    }
    return signBinanceRequest(config, options, exchangeName, options?.extra?.recvWindow ?? policy.recvWindow, now);
  };
};
