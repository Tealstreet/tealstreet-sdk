import type { ExchangeOptions } from '../../../types';
import type { IsSignatureRejection } from '../../../utils/signatureRejection';

import retry from 'axios-retry';
import createHash from 'create-hash';
import createHmac from 'create-hmac';
import qs from 'qs';

import { safeCexAxiosCreate } from '../../../safeCexAxiosCreate';
import { networkErrorOnRead, readsAndPosted } from '../../../utils/retryReadsOnly';
import { resendOnSignatureRejection, signatureRejectedResponse } from '../../../utils/signatureRejection';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { BASE_URL, ENDPOINTS, PUBLIC_ENDPOINTS } from './krakenspot.types';

// Kraken Spot REST API Authentication:
// Signature: base64(HMAC_SHA512(base64_decode(secret), URI_path + SHA256(nonce + POST_body)))
// This differs from Kraken Futures which signs: SHA256(postData + nonce + '/api/' + endpoint)

const unsignedFormParams = (data: unknown): Record<string, unknown> => {
  if (typeof data === 'string') {
    const { nonce: _previous, ...rest } = qs.parse(data);
    return rest;
  }
  return data && typeof data === 'object' ? (data as Record<string, unknown>) : {};
};

export const isKrakenSpotSignatureRejection: IsSignatureRejection = (data) =>
  Array.isArray(data?.error) && data.error.some((error: unknown) => String(error).includes('EAPI:Invalid signature'));

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  let nonceCounter = 0;

  const xhr = safeCexAxiosCreate(
    {
      baseURL: options.testnet ? BASE_URL.testnet : BASE_URL.livenet,
      timeout: 20_000,
      headers: {},
    },
    options,
    exchange,
  );

  retry(xhr, {
    retries: 3,
    retryCondition: (error) =>
      networkErrorOnRead(
        readsAndPosted(
          ENDPOINTS.BALANCE,
          ENDPOINTS.EXTENDED_BALANCE,
          ENDPOINTS.TRADE_BALANCE,
          ENDPOINTS.OPEN_ORDERS,
          ENDPOINTS.CLOSED_ORDERS,
          ENDPOINTS.TRADES_HISTORY,
          ENDPOINTS.OPEN_POSITIONS,
          ENDPOINTS.WS_TOKEN,
        ),
      )(error) || signatureRejectedResponse(isKrakenSpotSignatureRejection)(error),
  });

  xhr.interceptors.request.use((config) => {
    const isPublic = PUBLIC_ENDPOINTS.some((p) => (config?.url || '').includes(p));
    if (isPublic || options.publicDataOnly || !options.key) return config;

    if (options.remoteSigner) return config;

    const method = (config.method || 'get').toLowerCase();

    // Kraken Spot private endpoints are always POST
    // Build the POST body as form-encoded data
    const nonce = String(virtualClock.getCurrentTimeMs() * 1000 + (nonceCounter++ % 1000));

    // A replayed config carries the form body it sent: re-sign its params under a new nonce.
    const dataObj = unsignedFormParams(config.data);
    const postBody = qs.stringify({ ...dataObj, nonce }, { arrayFormat: 'repeat' });

    // Kraken Spot auth:
    // 1. SHA-256 hash of (nonce + postBody)
    // 2. HMAC-SHA512 of (urlPath + sha256hash) using base64-decoded secret
    const urlPath = config.url || '';
    const sha256Hash = createHash('sha256')
      .update(nonce + postBody, 'utf8')
      .digest();

    const secret = Buffer.from(options.secret, 'base64');
    const hmacPayload = Buffer.concat([Buffer.from(urlPath, 'utf8'), sha256Hash]);
    const signature = createHmac('sha512', secret).update(hmacPayload).digest('base64');

    const headers = {
      ...(config.headers || {}),
      'Content-Type': 'application/x-www-form-urlencoded',
      Accept: 'application/json',
      'API-Key': options.key,
      'API-Sign': signature,
    } as any;

    // Set the form-encoded body (includes nonce)
    if (method === 'post') {
      (config as any).data = postBody;
    }

    return { ...config, headers };
  });

  xhr.interceptors.response.use(
    async (response) => {
      const signatureResend = resendOnSignatureRejection(xhr, response, isKrakenSpotSignatureRejection);
      if (signatureResend) return signatureResend;

      if (response.data) {
        const errors = response.data.error;
        if (errors && Array.isArray(errors) && errors.length > 0) {
          // Kraken Spot returns errors as: { error: ["EGeneral:Invalid arguments", ...], result: {} }
          const errorStr = errors.join('; ');

          let errorMessage = `Kraken Spot API Error: ${errorStr}`;

          if (errorStr.includes('EAPI:Invalid key')) {
            errorMessage = 'Authentication failed. Please check your API key.';
          } else if (errorStr.includes('EAPI:Invalid nonce')) {
            errorMessage = 'Invalid nonce. Please check your system time is synchronized.';
          } else if (errorStr.includes('EAPI:Invalid signature')) {
            errorMessage = 'Invalid signature. Please verify your API secret is correct.';
          } else if (errorStr.includes('EGeneral:Permission denied')) {
            errorMessage = 'API key lacks required permissions.';
          } else if (errorStr.includes('EOrder:Rate limit exceeded')) {
            errorMessage = 'Order rate limit exceeded. Please try again.';
          } else if (errorStr.includes('EOrder:Insufficient funds')) {
            errorMessage = 'Insufficient funds for this order.';
          }

          throw new Error(errorMessage);
        }
      }
      return response;
    },
    async (error) => {
      if (error.response) {
        const status = error.response.status;
        const data = error.response.data;

        let errorMessage = `Kraken Spot API Error (${status})`;

        if (status === 401 || status === 403) {
          errorMessage = `Authentication failed (${status}). Please check your API key and secret.`;
        } else if (status === 429) {
          errorMessage = 'Rate limit exceeded (429). Please try again later.';
        } else if (data?.error?.length) {
          errorMessage = `Kraken Spot API Error: ${data.error.join('; ')}`;
        }

        throw new Error(errorMessage);
      }

      throw error;
    },
  );

  return xhr;
};
