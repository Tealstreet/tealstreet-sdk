import type { ExchangeOptions } from '../../types';

import retry, { isNetworkError } from 'axios-retry';
import qs from 'qs';

import { axiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { BaseExchange } from '../base';
import { BASE_URLS } from './hyperliquid.types';

// Common response interceptor for Hyperliquid API responses
const addResponseInterceptor = (xhr: ReturnType<typeof safeCexAxiosCreate>, exchange: BaseExchange) => {
  xhr.interceptors.response.use(async (response) => {
    if (response.data?.status === 'err') {
      const msg = `${response.data?.response}`;
      exchange.emitAndLogError(msg);
      throw new Error(msg);
    }
    if (response.data?.response?.data?.status === 'err') {
      const msg = `${response.data?.response?.data?.response}`;
      exchange.emitAndLogError(msg);
      throw new Error(msg);
    }
    if (response.data?.statuses && response.data?.statuses.some((status: any) => Boolean(status.error))) {
      const errors = response.data?.statuses
        .filter((status: any) => Boolean(status.error))
        .map((status: any) => status.error)
        .join(', ');
      exchange.emitAndLogError(errors);
      throw new Error(errors);
    }
    if (
      response.data?.response?.data?.statuses &&
      response.data?.response?.data?.statuses.some((status: any) => Boolean(status.error))
    ) {
      const errors = response.data?.response?.data?.statuses
        .filter((status: any) => Boolean(status.error))
        .map((status: any) => status.error)
        .join(', ');
      exchange.emitAndLogError(errors);
      throw new Error(errors);
    }

    return response;
  });
};

// Create axios instance for /exchange endpoint (trading operations)
export const createExchangeAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const baseURL = BASE_URLS[options.testnet ? 'TESTNET' : 'PRODUCTION'];

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: options?.extra?.recvWindow,
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

  retry(xhr, { retries: 3, retryCondition: isNetworkError });
  if (!options.publicDataOnly) {
    setInnerAdapterForRemoteSigning(xhr, axiosHttpOverWebsocketAdapter);
  }
  addResponseInterceptor(xhr, exchange);

  return xhr;
};

// Create axios instance for /info endpoint (read-only operations)
export const createInfoAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const baseURL = BASE_URLS[options.testnet ? 'TESTNET' : 'PRODUCTION'];

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: options?.extra?.recvWindow,
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

  retry(xhr, { retries: 3, retryCondition: isNetworkError });
  if (!options.publicDataOnly) {
    setInnerAdapterForRemoteSigning(xhr, axiosHttpOverWebsocketAdapter);
  }
  addResponseInterceptor(xhr, exchange);

  return xhr;
};

// Legacy function for backwards compatibility - returns exchange API
export const createAPI = createExchangeAPI;
