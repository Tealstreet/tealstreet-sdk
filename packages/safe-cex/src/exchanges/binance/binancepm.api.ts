import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import omit from 'lodash/omit';
import qs from 'qs';

import { axiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import {
  safeCexAxiosCreate,
  safeCexAxiosCreateNoProxy,
  setInnerAdapterForRemoteSigning,
} from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { BASE_FAPI_URL, BASE_PAPI_URL, ENDPOINTS, PUBLIC_ENDPOINTS, RECV_WINDOW } from './binancepm.types';

export type ApiType = 'papi' | 'fapi';

export const createBinanceAPI = (options: ExchangeOptions, exchange: BaseExchange, apiType: ApiType = 'papi') => {
  const baseURL = (apiType === 'papi' ? BASE_PAPI_URL : BASE_FAPI_URL)[options.testnet ? 'testnet' : 'livenet'];

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
    // on livenet, don't sign listen key requests (they don't need it)
    if (config.url === ENDPOINTS.LISTEN_KEY && !options.testnet) {
      return config;
    }

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

  if (options.tealstreetTunnel) {
    setInnerAdapterForRemoteSigning(xhr, axiosHttpOverWebsocketAdapter);
  }

  return xhr;
};

// Maintain backward compatibility
export const createPAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  return createBinanceAPI(options, exchange, 'papi');
};

// New function for FAPI
export const createFAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  return createBinanceAPI(options, exchange, 'fapi');
};

export const createPublicAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const baseURL = BASE_FAPI_URL[options.testnet ? 'testnet' : 'livenet'];

  const xhr = safeCexAxiosCreateNoProxy(
    {
      baseURL,
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { arrayFormat: 'repeat' }),
      },
      headers: {
        'Content-Type': 'application/json, charset=utf-8',
      },
    },
    options,
    exchange,
  );

  // retry requests on network errors instead of throwing
  retry(xhr, { retries: 3, retryCondition: isNetworkError });

  xhr.interceptors.request.use((config) => {
    return {
      ...config,
      headers: AxiosHeaders.from({
        'Content-Type': 'application/json, charset=utf-8',
      }),
      timeout: 20_000,
    };
  });

  if (options.tealstreetTunnel) {
    xhr.defaults.adapter = axiosHttpOverWebsocketAdapter;
  }

  return xhr;
};
