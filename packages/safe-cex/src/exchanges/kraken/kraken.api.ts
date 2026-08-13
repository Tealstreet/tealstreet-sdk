import type { ExchangeOptions } from '../../types';

import retry, { isNetworkError } from 'axios-retry';
import createHash from 'create-hash';
import createHmac from 'create-hmac';
import qs from 'qs';

import { axiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { markSkipRemoteSigning } from '../../remoteSigningAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { v } from '../../utils/get-key';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { BASE_URL, PUBLIC_ENDPOINTS } from './kraken.types';

// NOTE: Kraken Futures always expects form-encoded data for non-GET requests
// Signature: base64(HMAC_SHA512(SHA256(formData + nonce + path), base64Decode(secret)))

const sort = function alphabeticalSort(a: string, b: string) {
  return a.localeCompare(b);
};

const createKrakenApiError = (message: string, response?: unknown, cause?: unknown) => {
  const error = new Error(message, cause === undefined ? undefined : { cause });
  if (response !== undefined) {
    // Exchange consumers inspect HTTP status/body to distinguish invalid
    // credentials, permissions, and rate limits. Preserve axios' response
    // object when adding a Kraken-specific message.
    (error as Error & { response?: unknown }).response = response;
  }
  return error;
};

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  // Counter to ensure unique nonces even when requests are sent at the same millisecond
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

  retry(xhr, { retries: 3, retryCondition: isNetworkError });
  if (!options.publicDataOnly) {
    setInnerAdapterForRemoteSigning(xhr, axiosHttpOverWebsocketAdapter);
  }

  xhr.interceptors.request.use((config) => {
    const urlPath = (config.url || '').replace(/^\//, ''); // e.g. v3/openorders
    const isPublic = PUBLIC_ENDPOINTS.some((p) => (config?.url || '').includes(p));
    // Tag before the guards, on `isPublic` alone: `!options.key` is TRUE for
    // every request in hosted mode, where the keys live on the device, so
    // tagging there would mark genuinely private requests as needing no
    // signature. Kraken never signs these anyway, so the round trip the tag
    // removes was returning an unsigned config regardless.
    if (isPublic) markSkipRemoteSigning(config);
    if (isPublic || options.publicDataOnly || !options.key) return config;

    if (options.remoteSigner) return config;

    // Kraken Futures REST API Authentication:
    // According to Kraken Futures API docs:
    // 1. Construct authent string: postData + nonce + endpointPath
    // 2. SHA-256 hash the authent string
    // 3. HMAC-SHA512 sign the hash with base64-decoded secret
    // 4. Base64-encode the signature
    const method = (config.method || 'get').toLowerCase();

    // Build postData from params/body
    // Kraken Futures uses application/x-www-form-urlencoded which encodes spaces as +
    // qs.stringify defaults to RFC 3986 (%20 for spaces), so we use format: 'RFC1738' for + encoding
    // The SAME encoded string must be used for both signing and sending
    // TODO: Audit whether HOW proxy re-serialization can break this signature (see bitmex.api.ts).
    let postData = '';
    if (method === 'get' || method === 'delete') {
      if (config.params && Object.keys(config.params).length) {
        postData = qs.stringify(config.params, { arrayFormat: 'repeat', sort, format: 'RFC1738' });
      }
    } else if (method === 'post' || method === 'put' || method === 'patch') {
      const dataObj = config.data && typeof config.data === 'object' ? config.data : undefined;
      if (urlPath.endsWith('batchorder') && dataObj) {
        // Batch orders use special json= format
        const jsonStr = JSON.stringify(dataObj);
        postData = 'json=' + encodeURIComponent(jsonStr);
        (config as any).data = 'json=' + jsonStr;
      } else if (dataObj) {
        // Form-encode with + for spaces (RFC 1738 / application/x-www-form-urlencoded standard)
        postData = qs.stringify(dataObj, { arrayFormat: 'repeat', sort, format: 'RFC1738' });
        (config as any).data = postData;
      }
    }

    // Construct endpoint path for signing
    const endpoint = urlPath;
    // Generate nonce using millisecond timestamp + counter to ensure uniqueness
    const timestamp = virtualClock.getCurrentTimeMs();
    const nonce = String(timestamp * 1000 + (nonceCounter++ % 1000));

    // Kraken auth: SHA256(postData + nonce + endpointPath)
    const pathForSigning = '/api/' + endpoint;
    const authString = postData + nonce + pathForSigning;
    const hash = createHash('sha256').update(authString, 'utf8').digest();

    // Step 2: Base64-decode the secret
    const secret = Buffer.from(options.secret, 'base64');

    // Step 3: HMAC-SHA512 sign the hash
    const signature = createHmac('sha512', secret).update(hash).digest('base64');

    const headers = {
      ...(config.headers || {}),
      'Content-Type': 'application/x-www-form-urlencoded', // Kraken always expects form-encoded for non-GET
      Accept: 'application/json',
      APIKey: options.key,
      Authent: signature,
      Nonce: nonce, // Add nonce header (optional but helps with request ordering)
    } as any;

    return { ...config, headers };
  });

  xhr.interceptors.response.use(
    async (response) => {
      // Check if the response indicates an error
      if (response.data) {
        const result = v(response.data, 'result');
        const error = v(response.data, 'error');

        if (result === 'error' && error) {
          // Kraken returns errors like: { "result": "error", "error": "requiredArgumentMissing" }
          let errorMessage = `Kraken API Error: ${error}`;

          // Provide more specific error messages for common authentication issues
          if (error.toLowerCase().includes('authentication') || error.toLowerCase().includes('apikey')) {
            errorMessage = 'Authentication failed. Please check your API key and secret.';
          } else if (error.toLowerCase().includes('nonce')) {
            errorMessage = 'Invalid nonce. Please check your system time is synchronized.';
          } else if (error.toLowerCase().includes('permission')) {
            errorMessage =
              'API key lacks required permissions. Please ensure General API is set to "Full access" on your Kraken Futures API key.';
          } else if (error.toLowerCase().includes('signature')) {
            errorMessage = 'Invalid signature. Please verify your API secret is correct.';
          }

          throw createKrakenApiError(errorMessage, response);
        }

        // Also check for other error formats
        if (error && !result) {
          const errorMessage = `Kraken API Error: ${error}`;
          throw createKrakenApiError(errorMessage, response);
        }
      }
      return response;
    },
    async (error) => {
      // Handle HTTP errors (401, 403, etc.)
      if (error.response) {
        const status = error.response.status;
        const data = error.response.data;

        let errorMessage = `Kraken API Error (${status})`;

        if (status === 401) {
          errorMessage = 'Authentication failed (401). Please check your API key and secret.';
        } else if (status === 403) {
          errorMessage = 'Access forbidden (403). Your API key may lack required permissions or IP is not whitelisted.';
        } else if (status === 429) {
          errorMessage = 'Rate limit exceeded (429). Please try again later.';
        } else if (data?.error) {
          errorMessage = `Kraken API Error: ${data.error}`;
        }

        throw createKrakenApiError(errorMessage, error.response, error);
      }

      throw error;
    },
  );

  return xhr;
};
