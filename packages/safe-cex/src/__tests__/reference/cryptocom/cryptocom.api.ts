import type { ExchangeOptions } from '../../../types';
import type { CryptocomApiResponse } from './cryptocom.types';
import type { IsSignatureRejection } from '../../../utils/signatureRejection';

import retry, { isNetworkError } from 'axios-retry';

import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import { BaseExchange } from '../../../exchanges/base';
import { generateAuthenticatedRequest } from './cryptocom.signing';
import { resendOnSignatureRejection, signatureRejectedResponse } from '../../../utils/signatureRejection';
import { BASE_URLS, ENDPOINTS, PUBLIC_ENDPOINTS } from './cryptocom.types';

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const CRYPTOCOM_AUTHENTICATION_ERROR_CODE = '40101';

const stringifyResponseBody = (body: unknown): string => {
  if (typeof body === 'string') return body;
  try {
    return JSON.stringify(body);
  } catch (_err) {
    return String(body);
  }
};

const parseJsonObject = (value: string): Record<string, unknown> | undefined => {
  try {
    const parsed = JSON.parse(value);
    return isRecord(parsed) ? parsed : undefined;
  } catch (_err) {
    return undefined;
  }
};

const isAuthenticatedEnvelope = (value: unknown, method: string, apiKey: string): value is Record<string, unknown> => {
  if (!isRecord(value)) return false;
  return (
    value.method === method &&
    value.api_key === apiKey &&
    isRecord(value.params) &&
    typeof value.sig === 'string' &&
    typeof value.nonce === 'number'
  );
};

const NON_RETRYABLE_PRIVATE_WRITE_ENDPOINTS = [
  ENDPOINTS.CREATE_ORDER,
  ENDPOINTS.ADVANCED_CREATE_ORDER,
  ENDPOINTS.AMEND_ORDER,
  ENDPOINTS.ADVANCED_AMEND_ORDER,
  ENDPOINTS.CANCEL_ORDER,
  ENDPOINTS.ADVANCED_CANCEL_ORDER,
  ENDPOINTS.CANCEL_ALL_ORDERS,
  ENDPOINTS.CLOSE_POSITION,
  ENDPOINTS.CHANGE_LEVERAGE,
];

const isNonRetryablePrivateWrite = (url = ''): boolean =>
  NON_RETRYABLE_PRIVATE_WRITE_ENDPOINTS.some((endpoint) => url.includes(endpoint));

export const formatCryptocomApiError = (error: any): string | undefined => {
  const status = error?.response?.status;
  const body = error?.response?.data;
  if (body === undefined) return undefined;

  if (isRecord(body)) {
    const code = body.code;
    const message = typeof body.message === 'string' ? body.message : undefined;
    if (code !== undefined || message) {
      let msg = 'Crypto.com API Error';
      if (code !== undefined && code !== 0) msg += ` ${code}`;
      if (message) msg += `: ${message}`;
      return msg;
    }
  }

  const bodyString = stringifyResponseBody(body);
  if (status && status >= 500) {
    return `Crypto.com proxy/transport error (${status}): ${bodyString}`;
  }
  if (status) {
    return `Crypto.com HTTP error (${status}): ${bodyString}`;
  }
  return `Crypto.com API error: ${bodyString}`;
};

export const isCryptocomAuthenticationErrorCode = (code: unknown): boolean =>
  String(code) === CRYPTOCOM_AUTHENTICATION_ERROR_CODE;

export const isCryptocomAuthenticationFailureMessage = (message: unknown): boolean =>
  typeof message === 'string' && message.includes(`Crypto.com API Error ${CRYPTOCOM_AUTHENTICATION_ERROR_CODE}`);

const shouldSuppressValidationAuthErrorEmission = (exchange: BaseExchange, code: unknown): boolean =>
  isCryptocomAuthenticationErrorCode(code) &&
  (exchange as { suppressCryptocomValidationErrorEmission?: boolean }).suppressCryptocomValidationErrorEmission === true;

// "Not authenticated, or key/signature incorrect": a bad key costs one extra request.
export const isCryptocomSignatureRejection: IsSignatureRejection = (data) => Number(data?.code) === 40101;

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

  // Retry read requests on network errors. Do not retry private writes because
  // a network/CORS failure is ambiguous: the exchange may have accepted the
  // mutation even though the browser could not read the response.
  retry(xhr, {
    retries: 3,
    // A signature rejection is a refusal, so unlike a lost response it is safe to resend a write.
    retryCondition: (error) =>
      (isNetworkError(error) && !isNonRetryablePrivateWrite(error.config?.url)) ||
      signatureRejectedResponse(isCryptocomSignatureRejection)(error),
  });

  // Only use HTTP-over-WebSocket adapter when tealstreetTunnel is enabled (browser/production)
  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  // Request interceptor to handle authentication and request formatting
  xhr.interceptors.request.use(async (config) => {
    const nextConfig = { ...config };

    const endpoint = config.url || '';
    const requiresAuth = !isPublicEndpoint(endpoint);

    const method = endpoint.replace('/', '');
    const rawParams = typeof config.data === 'string' ? (parseJsonObject(config.data) ?? config.data) : config.data || {};

    if (requiresAuth && options.key && options.secret) {
      // Private: POST with signed JSON envelope
      nextConfig.method = 'POST';
      try {
        // A replay re-enters with the envelope it sent; re-sign its params with a
        // fresh nonce rather than resending the stale signature.
        const unsigned = isAuthenticatedEnvelope(rawParams, method, options.key) ? rawParams.params : rawParams;
        const params = isRecord(unsigned) ? unsigned : {};
        const authenticatedRequest = generateAuthenticatedRequest(method, params, options.key, options.secret);

        // Serialized here: axios-retry resends with an identity transformRequest, which
        // would otherwise send this object unserialized.
        nextConfig.data = JSON.stringify(authenticatedRequest);
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
      nextConfig.params = isRecord(rawParams) ? rawParams : {};
    }

    return nextConfig;
  });

  const unwrappedResponses = new WeakSet<object>();

  // Response interceptor to handle errors and unwrap results
  xhr.interceptors.response.use(
    async (response) => {
      // An axios-retry resend comes back through here twice (its own chain, then the
      // original's), and the second pass would read the unwrapped result as an error.
      if (unwrappedResponses.has(response)) return response;

      const signatureResend = resendOnSignatureRejection(xhr, response, isCryptocomSignatureRejection);
      if (signatureResend) return signatureResend;

      const data = response.data as CryptocomApiResponse<unknown>;

      // Check for API errors (code !== 0)
      if (data.code !== 0) {
        const msg = `Crypto.com API Error ${data.code}: ${data.message || 'Unknown error'}`;
        if (!shouldSuppressValidationAuthErrorEmission(exchange, data.code)) {
          exchange.emitAndLogError(msg);
        }
        throw new Error(msg);
      }

      // Unwrap the result from the response
      if (data.result !== undefined) {
        response.data = data.result;
      }
      unwrappedResponses.add(response);

      return response;
    },
    async (error) => {
      // Handle axios errors
      const msg = formatCryptocomApiError(error);
      if (msg) {
        if (!shouldSuppressValidationAuthErrorEmission(exchange, error?.response?.data?.code)) {
          exchange.emitAndLogError(msg);
        }
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
