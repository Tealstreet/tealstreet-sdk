import type { ExchangeOptions } from '../../../types';
import type { BackpackSigningParams } from './backpack.signing';

import retry from 'axios-retry';
import qs from 'qs';

import { markSkipRemoteSigning } from '../../../remoteSigningAdapter';
import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import { appendHttpFailure } from '../../../utils/httpFailure';
import { BaseExchange } from '../../../exchanges/base';
import { generateAuthHeaders } from './backpack.signing';
import { BASE_URLS, INSTRUCTIONS } from './backpack.types';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';

const BACKPACK_MAX_AUTH_WINDOW_MS = 60_000;

export const resolveBackpackAuthWindow = (recvWindow?: number): number => {
  if (recvWindow === undefined || !Number.isFinite(recvWindow) || recvWindow <= 0) {
    return BACKPACK_MAX_AUTH_WINDOW_MS;
  }

  return Math.min(Math.floor(recvWindow), BACKPACK_MAX_AUTH_WINDOW_MS);
};

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const baseURL = BASE_URLS[options.testnet ? 'TESTNET' : 'PRODUCTION'];
  const authWindow = resolveBackpackAuthWindow(options?.extra?.recvWindow);

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: options?.extra?.recvWindow || 30000, // Increased to 30s for HOW adapter
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { arrayFormat: 'repeat' }),
      },
      headers: {
        'Content-Type': 'application/json',
        'X-Broker-Id': '1200', // Tealstreet broker ID
      },
    },
    options,
    exchange,
  );

  // Retry requests on network errors instead of throwing
  retry(xhr, { retries: 3, retryCondition: networkErrorOnRead() });

  // Only use HTTP-over-WebSocket adapter when tealstreetTunnel is enabled (browser/production)
  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  // Request interceptor to add authentication headers
  xhr.interceptors.request.use(async (config) => {
    const nextConfig = { ...config };

    // Check if this endpoint requires authentication
    const isPublic = isPublicEndpoint(config.url || '');
    if (isPublic) markSkipRemoteSigning(nextConfig);
    const requiresAuth = !isPublic;

    if (requiresAuth && options.key && options.secret) {
      try {
        // Determine the instruction based on the method and endpoint
        const instruction = getInstruction(config.method?.toUpperCase() || 'GET', config.url || '');

        // Preserve batch request bodies as arrays. Object-spreading an array
        // changes its signed shape into numeric keys and invalidates Backpack's
        // special POST /orders signing contract.
        const params = collectAuthParams(config.params, config.data);

        // Generate authentication headers
        const authHeaders = await generateAuthHeaders(
          instruction,
          params,
          options.key,
          options.secret,
          authWindow,
        );

        // Merge auth headers with existing headers
        Object.assign(nextConfig.headers || {}, authHeaders);
      } catch (error) {
        exchange.emitAndLogError(`Failed to generate auth headers: ${error}`);
        throw error;
      }
    }

    return nextConfig;
  });

  // Response interceptor to handle errors
  xhr.interceptors.response.use(
    async (response) => {
      // Check for error in response data
      if (response.data?.error) {
        const msg = appendHttpFailure(
          `Backpack API Error: ${response.data.error.msg || response.data.error}`,
          response,
        );
        exchange.emitAndLogError(msg);
        throw new Error(msg);
      }

      if (response.data?.code && response.data?.msg) {
        const msg = appendHttpFailure(`Backpack API Error ${response.data.code}: ${response.data.msg}`, response);
        exchange.emitAndLogError(msg);
        throw new Error(msg);
      }

      return response;
    },
    async (error) => {
      // Handle axios errors
      if (error.response?.data) {
        const errorData = error.response.data;
        let msg = 'Backpack API Error';
        let errorCode = '';

        // Extract error message - Backpack uses various formats
        if (errorData.error?.msg) {
          msg += `: ${errorData.error.msg}`;
        } else if (errorData.message) {
          // Format: { "code": "INVALID_CLIENT_REQUEST", "message": "Quantity decimal too long" }
          msg += `: ${errorData.message}`;
          errorCode = errorData.code ? ` [${errorData.code}]` : '';
        } else if (errorData.msg) {
          msg += `: ${errorData.msg}`;
        } else if (typeof errorData === 'string') {
          msg += `: ${errorData}`;
        } else {
          // Log the full error data for debugging
          msg += `: ${JSON.stringify(errorData)}`;
        }

        const fullMsg = `${msg}${errorCode}`;

        // Emit error to user so they see what went wrong
        exchange.emitter.emit('error', new Error(fullMsg));

        // Also log status code and request details for debugging
        exchange.emitAndLogError(`${fullMsg} (Status: ${error.response?.status}, URL: ${error.config?.url})`);
        throw new Error(fullMsg);
      }

      // If no response data, throw original error
      exchange.emitAndLogError(`Backpack API request failed: ${error.message}`);
      throw error;
    },
  );

  return xhr;
};

export const collectAuthParams = (queryParams: unknown, requestData: unknown): BackpackSigningParams => {
  const query = isRecord(queryParams) ? queryParams : {};

  if (Array.isArray(requestData)) {
    if (Object.keys(query).length > 0) {
      throw new Error('Backpack batch request signing does not support query parameters');
    }
    if (!requestData.every(isRecord)) {
      throw new Error('Backpack batch request body must contain only object rows');
    }
    return requestData;
  }

  return {
    ...query,
    ...(isRecord(requestData) ? requestData : {}),
  };
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

/**
 * Check if an endpoint is public (doesn't require authentication)
 */
function isPublicEndpoint(url: string): boolean {
  const publicEndpoints = [
    '/api/v1/time',
    '/api/v1/status',
    '/api/v1/assets',
    '/api/v1/markets',
    '/api/v1/market',
    '/api/v1/ticker',
    '/api/v1/tickers',
    '/api/v1/depth',
    '/api/v1/klines',
    '/api/v1/trades',
    '/api/v1/trades/history',
    '/api/v1/markPrices',
    '/api/v1/openInterest',
    '/api/v1/fundingRates',
  ];

  return publicEndpoints.some((endpoint) => url.includes(endpoint));
}

/**
 * Determine the instruction type based on the HTTP method and endpoint
 */
export function getInstruction(method: string, url: string): string {
  // Order endpoints
  if (url.includes('/api/v1/order') && !url.includes('/api/v1/orders')) {
    if (method === 'GET') return INSTRUCTIONS.ORDER_QUERY;
    if (method === 'POST') return INSTRUCTIONS.ORDER_EXECUTE;
    if (method === 'DELETE') return INSTRUCTIONS.ORDER_CANCEL;
  }

  if (url.includes('/api/v1/orders')) {
    if (method === 'GET') return INSTRUCTIONS.ORDER_QUERY_ALL;
    if (method === 'POST') return INSTRUCTIONS.ORDER_EXECUTE;
    if (method === 'DELETE') return INSTRUCTIONS.ORDER_CANCEL_ALL;
  }

  // Account endpoints
  if (url.includes('/api/v1/capital/collateral')) {
    return INSTRUCTIONS.COLLATERAL_QUERY;
  }

  if (url.includes('/api/v1/capital')) {
    return INSTRUCTIONS.BALANCE_QUERY;
  }

  if (url.includes('/api/v1/position')) {
    return INSTRUCTIONS.POSITION_QUERY;
  }

  if (url.includes('/api/v1/account')) {
    if (method === 'GET') return INSTRUCTIONS.ACCOUNT_QUERY;
    if (method === 'PATCH') return INSTRUCTIONS.ACCOUNT_UPDATE;
  }

  // History endpoints
  if (url.includes('/history/fills')) {
    return INSTRUCTIONS.FILL_HISTORY_QUERY_ALL;
  }

  if (url.includes('/history/orders')) {
    return INSTRUCTIONS.ORDER_HISTORY_QUERY_ALL;
  }

  if (url.includes('/wapi/v1/capital/deposits')) {
    return INSTRUCTIONS.DEPOSIT_QUERY_ALL;
  }

  if (url.includes('/wapi/v1/capital/withdrawals')) {
    return INSTRUCTIONS.WITHDRAWAL_QUERY_ALL;
  }

  if (url.includes('/wapi/v1/capital/deposit/address')) {
    return INSTRUCTIONS.DEPOSIT_ADDRESS_QUERY;
  }

  // Default to a generic query instruction
  return 'query';
}
