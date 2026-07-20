import type { ExchangeOptions } from '../../types';
import type { CryptocomApiResponse } from './cryptocom.types';

import retry, { isNetworkError } from 'axios-retry';

import { axiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { BaseExchange } from '../base';
import { generateAuthenticatedRequest } from './cryptocom.signing';
import { BASE_URLS, PUBLIC_ENDPOINTS } from './cryptocom.types';

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const baseURL = BASE_URLS[options.testnet ? 'TESTNET' : 'PRODUCTION'];

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: options?.extra?.recvWindow || 30000,
      headers: {
        'Content-Type': 'application/json',
      },
    },
    options,
    exchange,
  );

  // Retry requests on network errors instead of throwing
  retry(xhr, { retries: 3, retryCondition: isNetworkError });

  // Only use HTTP-over-WebSocket adapter when tealstreetTunnel is enabled (browser/production)
  if (options.tealstreetTunnel) {
    setInnerAdapterForRemoteSigning(xhr, axiosHttpOverWebsocketAdapter);
  }

  // Request interceptor to handle authentication and request formatting
  xhr.interceptors.request.use(async (config) => {
    const nextConfig = { ...config };

    const endpoint = config.url || '';
    const requiresAuth = !isPublicEndpoint(endpoint);

    const method = endpoint.replace('/', '');
    const params = config.data || {};

    if (requiresAuth && options.key && options.secret) {
      // Private: POST with signed JSON envelope
      nextConfig.method = 'POST';
      try {
        const authenticatedRequest = generateAuthenticatedRequest(method, params, options.key, options.secret);

        nextConfig.data = authenticatedRequest;
      } catch (error) {
        exchange.emitAndLogError(`Failed to generate auth request: ${error}`);
        throw error;
      }
    } else {
      // Public: GET with params as query string. Crypto.com's public REST
      // endpoints started rejecting the old POST envelope shape with
      // 500 / code 50001 — only GET is accepted now.
      nextConfig.method = 'GET';
      nextConfig.data = undefined;
      nextConfig.params = params && typeof params === 'object' ? params : {};
    }

    return nextConfig;
  });

  // Response interceptor to handle errors and unwrap results
  xhr.interceptors.response.use(
    async (response) => {
      const data = response.data as CryptocomApiResponse<unknown>;

      // Check for API errors (code !== 0)
      if (data.code !== 0) {
        const msg = `Crypto.com API Error ${data.code}: ${data.message || 'Unknown error'}`;
        exchange.emitAndLogError(msg);
        throw new Error(msg);
      }

      // Unwrap the result from the response
      if (data.result !== undefined) {
        response.data = data.result;
      }

      return response;
    },
    async (error) => {
      // Handle axios errors
      if (error.response?.data) {
        const errorData = error.response.data as CryptocomApiResponse<unknown>;
        let msg = 'Crypto.com API Error';

        if (errorData.code !== undefined && errorData.code !== 0) {
          msg += ` ${errorData.code}`;
        }

        if (errorData.message) {
          msg += `: ${errorData.message}`;
        }

        exchange.emitAndLogError(msg);
        throw new Error(msg);
      }

      throw error;
    },
  );

  return xhr;
};

/**
 * Check if an endpoint is public (doesn't require authentication)
 */
function isPublicEndpoint(url: string): boolean {
  return PUBLIC_ENDPOINTS.some((endpoint) => url.includes(endpoint));
}
