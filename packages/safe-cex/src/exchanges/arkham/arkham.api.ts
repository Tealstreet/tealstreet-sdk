import type { AxiosRequestConfig } from 'axios';
import type { ExchangeOptions } from '../../types';

import { createHmac } from 'crypto';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import qs from 'qs';

import { axiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { BASE_URL, RECV_WINDOW } from './arkham.types';

const signRequest = (config: AxiosRequestConfig, options: ExchangeOptions) => {
  const timestamp = virtualClock.getCurrentTimeMs() * 1000; // Convert to microseconds
  const expiry = timestamp + 5 * 1000000; // 5 seconds in microseconds
  const method = config.method?.toUpperCase() || 'GET';

  // Build the message to sign according to Arkham's requirements (matching CCXT)
  let path = config.url || '';

  // Only include query string if params exist and have values
  let queryString = '';
  if (config.params && Object.keys(config.params).length > 0) {
    queryString = qs.stringify(config.params, { arrayFormat: 'comma' });
    if (queryString !== '') {
      path = path + '?' + queryString;
    }
  }

  const body = config.data ? JSON.stringify(config.data) : '';

  // Arkham signature format: api_key + expiry + method + path + body
  // Note: Our config.url already includes leading slash (e.g., '/orders')
  const message = `${options.key}${expiry}${method}${path}${body}`;

  // Decode secret from base64 and use directly (like CCXT)
  const decodedSecret = Buffer.from(options.secret, 'base64');

  // Create HMAC signature with decoded secret, output as base64
  const signature = createHmac('sha256', decodedSecret).update(message).digest('base64');

  const headers = new AxiosHeaders({
    ...config.headers,
    'Arkham-Api-Key': options.key, // Note: Capital 'A' in Api
    'Arkham-Signature': signature,
    'Arkham-Expires': expiry.toString(), // Note: Expires, not Expiry
    'Arkham-Broker-Id': '1002',
    'Content-Type': 'application/json',
    Accept: 'application/json',
  });

  // Remove data from GET/HEAD requests to prevent body errors
  if (method === 'GET' || method === 'HEAD') {
    const { data: _data, ...configWithoutData } = config;
    return { ...configWithoutData, headers };
  }

  return { ...config, headers };
};

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const xhr = safeCexAxiosCreate(
    {
      baseURL: BASE_URL[options.testnet ? 'testnet' : 'livenet'],
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { arrayFormat: 'comma' }),
      },
      headers: {
        'cache-control': 'no-cache',
      },
    },
    options,
    exchange,
  );

  // Retry on network errors
  retry(xhr, {
    retries: 3,
    retryCondition: isNetworkError,
    retryDelay: (retryCount) => retryCount * 1000,
  });

  // Only use HTTP-over-WebSocket adapter when not in test or public-only mode
  if (process.env.NODE_ENV !== 'test' && !options.publicDataOnly) {
    setInnerAdapterForRemoteSigning(xhr, axiosHttpOverWebsocketAdapter);
  }

  // Add request interceptor for signing
  xhr.interceptors.request.use((config) => {
    // Skip signing for public endpoints
    const publicEndpoints = ['/public/'];
    const isPublic = publicEndpoints.some((endpoint) => config.url?.includes(endpoint));

    if (isPublic) {
      return config;
    }

    if (options.remoteSigner) {
      return config;
    }

    // Sign private requests
    return signRequest(config, options);
  });

  // Add response interceptor for error handling
  xhr.interceptors.response.use(
    (response) => {
      // Arkham returns errors with successful HTTP status codes (200 OK)
      // Check if the response contains an error object with id, name, and message
      if (
        response.data &&
        typeof response.data === 'object' &&
        'id' in response.data &&
        'name' in response.data &&
        'message' in response.data
      ) {
        const error = new Error(`Arkham Error [${response.data.name}]: ${response.data.message}`);
        (error as any).code = response.data.id;
        (error as any).name = response.data.name;
        exchange.emitAndLogError(error.message);
        return Promise.reject(error);
      }
      return response;
    },
    (error) => {
      // Handle Arkham specific errors in error responses
      if (error.response?.data?.error) {
        const arkhamError = error.response.data.error;
        error.message = `Arkham Error: ${arkhamError.message || arkhamError.code || arkhamError}`;
      }
      return Promise.reject(error);
    },
  );

  return xhr;
};
