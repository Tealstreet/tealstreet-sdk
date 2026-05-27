import type { ExchangeOptions } from '../../types';

import retry, { isNetworkError } from 'axios-retry';
import qs from 'qs';

import { safeCexAxiosCreate } from '../../safeCexAxiosCreate';
import { BaseExchange } from '../base';
import { BASE_URLS } from './dydx.types';

/**
 * Creates an Axios instance for dYdX Indexer API
 * Note: This is primarily for REST API calls to the Indexer
 * Order placement and cancellation use the dYdX SDK directly
 */
export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const baseURL = BASE_URLS[options.testnet ? 'TESTNET' : 'PRODUCTION'];

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: options?.extra?.recvWindow || 30000,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { arrayFormat: 'repeat' }),
      },
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
    },
    options,
    exchange,
  );

  // Retry requests on network errors
  retry(xhr, { retries: 3, retryCondition: isNetworkError });

  // Only use HTTP over WebSocket adapter when tealstreetTunnel is enabled
  // if (options.tealstreetTunnel) {
  //   xhr.defaults.adapter = axiosHttpOverWebsocketAdapter;
  // }

  // Response interceptor for error handling
  xhr.interceptors.response.use(
    async (response) => {
      // dYdX API returns errors in different formats
      if (response.data?.errors && Array.isArray(response.data.errors)) {
        const errorMessages = response.data.errors.map((err: any) => err.msg || err.message || err).join(', ');
        exchange.emitAndLogError(errorMessages);
        throw new Error(errorMessages);
      }

      return response;
    },
    (error) => {
      // Handle axios errors
      if (error.response) {
        const errorMsg =
          error.response.data?.errors?.[0]?.msg ||
          error.response.data?.message ||
          error.response.statusText ||
          'Unknown API error';
        exchange.emitAndLogError(errorMsg);
        throw new Error(errorMsg);
      }
      throw error;
    },
  );

  return xhr;
};
