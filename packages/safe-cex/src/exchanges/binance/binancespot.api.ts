import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import omit from 'lodash/omit';
import qs from 'qs';

import { safeCexAxiosCreate } from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { BASE_URL, PUBLIC_ENDPOINTS, RECV_WINDOW } from './binancespot.types';

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const baseURL = BASE_URL[options.testnet ? 'testnet' : 'livenet'];

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { arrayFormat: 'repeat' }),
      },
      headers: {
        'X-MBX-APIKEY': options.key,
        'Content-Type': 'application/json, charset=utf-8',
      },
    },
    options,
    exchange,
  );

  // retry requests on network errors instead of throwing
  retry(xhr, { retries: 3, retryCondition: isNetworkError });

  xhr.interceptors.request.use((config) => {
    const isPublicEndPoint = PUBLIC_ENDPOINTS.includes(`${config.url}`);
    if (isPublicEndPoint) {
      return {
        ...config,
        headers: AxiosHeaders.from({
          'Content-Type': 'application/json, charset=utf-8',
        }),
        timeout: 20_000,
      };
    }

    const nextConfig = { ...config };

    if (options.remoteSigner) {
      return nextConfig;
    }

    // Skip signing when no API key (public-only mode)
    if (!options.key || !options.secret) {
      return nextConfig;
    }

    const timestamp = virtualClock.getCurrentTimeMs();

    const data = config.data || config.params || {};
    data.timestamp = timestamp;
    data.recvWindow = options?.extra?.recvWindow ?? RECV_WINDOW;

    const asString = qs.stringify(data, { arrayFormat: 'repeat' });
    data.signature = createHmac('sha256', options.secret).update(asString).digest('hex');
    nextConfig.params = data;

    // remove data from POST/PUT/DELETE requests
    // Binance API takes data as query params
    return omit(nextConfig, 'data');
  });

  return xhr;
};
