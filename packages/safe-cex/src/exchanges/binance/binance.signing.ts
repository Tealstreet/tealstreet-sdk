import type { InternalAxiosRequestConfig } from 'axios';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import omit from 'lodash/omit';
import qs from 'qs';

import { canSignPayload, signPayload } from '../../utils/payloadSigner';
import { virtualClock } from '../../utils/virtualClock';

/**
 * Binance signs the query string and sends it as params, `signature` last.
 * Shared by USD-M, spot and portfolio margin, which differ only in host.
 */
export const signBinanceRequest = async (
  config: InternalAxiosRequestConfig,
  options: ExchangeOptions,
  exchangeName: ExchangeName,
  recvWindow: number,
): Promise<InternalAxiosRequestConfig> => {
  if (options.remoteSigner || !options.key || !canSignPayload(options)) return { ...config };

  const data = { ...(config.data || config.params || {}) };
  // A retry re-enters here with the previous attempt's signature still in params.
  delete data.signature;
  data.timestamp = virtualClock.getCurrentTimeMs();
  data.recvWindow = recvWindow;

  const { signature, headers } = await signPayload(
    options,
    exchangeName,
    qs.stringify(data, { arrayFormat: 'repeat' }),
    'hex',
  );
  data.signature = signature;

  const nextConfig = { ...config, params: data };
  if (headers) nextConfig.headers = new AxiosHeaders({ ...headers, ...config.headers });

  // Binance takes POST/PUT/DELETE data as query params.
  return omit(nextConfig, 'data') as InternalAxiosRequestConfig;
};
