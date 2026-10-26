import type { InternalAxiosRequestConfig } from 'axios';
import type { ExchangeOptions } from '../../../types';
import type { ParadexExchange } from './paradex.exchange';

import retry from 'axios-retry';
import qs from 'qs';

import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import { LogSeverity } from '../../../types';
import { BaseExchange } from '../../../exchanges/base';
import { BASE_URL, RECV_WINDOW } from './paradex.types';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';

/**
 * Add JWT authentication to REST API requests
 *
 * REST API uses JWT tokens obtained from the /auth endpoint.
 * WebSocket connections also use JWT tokens for authentication.
 */
const authenticateRequest = async (
  config: InternalAxiosRequestConfig,
  exchange: ParadexExchange,
): Promise<InternalAxiosRequestConfig> => {
  // If JWT manager not ready yet, throw error - don't send unauthenticated request
  if (!exchange.jwtManager) {
    const errorMsg = 'JWT manager not initialized - cannot authenticate request';
    exchange.log(`[Paradex API] ${errorMsg}`, LogSeverity.Error);
    throw new Error(errorMsg);
  }

  try {
    // Get JWT token (will be cached and auto-refreshed by manager)
    const token = await exchange.jwtManager.getToken();

    if (!token || token.trim() === '') {
      const errorMsg = 'JWT token is empty - cannot authenticate request';
      exchange.log(`[Paradex API] ${errorMsg}`, LogSeverity.Error);
      throw new Error(errorMsg);
    }

    // Add JWT bearer token to Authorization header
    config.headers.set('Authorization', `Bearer ${token}`);
    config.headers.set('Content-Type', 'application/json');
    config.headers.set('Accept', 'application/json');
  } catch (error) {
    exchange.log(`[Paradex API] Failed to get JWT token: ${error}`, LogSeverity.Error);
    // Re-throw the error - don't send unauthenticated request
    throw error;
  }

  return config;
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
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
    },
    options,
    exchange,
  );

  // Retry on network errors
  retry(xhr, {
    retries: 3,
    retryCondition: networkErrorOnRead(),
    retryDelay: (retryCount) => retryCount * 1000,
  });

  // A payloadSigner caller controls the request's transport, so private requests go direct.
  if (!options.publicDataOnly && !options.payloadSigner) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  // Request interceptor for authentication
  xhr.interceptors.request.use(async (config) => {
    const paradexExchange = exchange as ParadexExchange;

    // Check if this is a public endpoint FIRST - before any async operations
    const url = config.url || '';

    // Public endpoints that don't require authentication
    // NOTE: old /ohlcv/{market} requires auth, but new /markets/klines is public
    const publicEndpoints = ['/trades', '/funding', '/summary', '/orderbook', '/tradingview'];
    const isPublic = publicEndpoints.some((endpoint) => url.includes(endpoint));

    // /markets is public when not accessing account-specific endpoints
    // This includes /markets/klines (OHLCV data)
    const isMarketsPublic = url.includes('/markets') && !url.includes('/account');

    if ((isPublic || isMarketsPublic) && !url.includes('/account')) {
      // Public endpoint - no authentication needed
      exchange.log(`Public endpoint detected: ${url}`, LogSeverity.Debug);
      return config;
    }

    // Add PARADEX-PARTNER header for all order placement requests
    if (config.method === 'post' && url.includes('/orders')) {
      config.headers.set('PARADEX-PARTNER', 'TEALSTREET');
      exchange.log(`Added PARADEX-PARTNER header for order request`, LogSeverity.Debug);
    }

    // Check if request already has signature headers (e.g., for onboarding)
    // Do this BEFORE waiting for SDK init to avoid deadlock during SDK initialization
    const hasSignatureHeaders = config.headers.has('PARADEX-STARKNET-SIGNATURE');
    if (hasSignatureHeaders) {
      // Already authenticated with signature headers - skip JWT
      // Don't wait for SDK init since this might BE the SDK init process
      return config;
    }

    // Wait for SDK initialization if in progress (but not for signature-auth or public requests)
    if (paradexExchange.sdkInitPromise) {
      await paradexExchange.sdkInitPromise;
    }

    // Private endpoint - add JWT authentication
    return authenticateRequest(config, paradexExchange);
  });

  // Response interceptor for error handling
  xhr.interceptors.response.use(
    (response) => {
      // Check if successful response contains an error (proxy case - returns 200 with error in body)
      if (response.data?.error) {
        const errorMessage =
          response.data.error && response.data.message
            ? `Paradex Error: ${response.data.error}: ${response.data.message}`
            : `Paradex Error: ${response.data.error}`;

        // Create an error object that looks like an AxiosError
        const error: any = new Error(errorMessage);
        error.isAxiosError = true;
        error.response = response;
        error.config = response.config;
        error.request = response.request;

        return Promise.reject(error);
      }
      return response;
    },
    (error) => {
      // Handle HTTP error responses with Paradex error details
      if (error.response?.data?.error) {
        const data = error.response.data;
        const errorMessage =
          data.error && data.message ? `Paradex Error: ${data.error}: ${data.message}` : `Paradex Error: ${data.error}`;

        // Create a new error with the Paradex error message
        // (Cannot modify error.message directly as it's a read-only property in AxiosError)
        const enhancedError = Object.create(error);
        Object.defineProperty(enhancedError, 'message', {
          value: errorMessage,
          writable: false,
          enumerable: true,
          configurable: true,
        });
        return Promise.reject(enhancedError);
      }
      return Promise.reject(error);
    },
  );

  return xhr;
};
