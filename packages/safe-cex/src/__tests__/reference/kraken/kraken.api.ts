import type { ExchangeOptions } from '../../../types';

import retry from 'axios-retry';
import createHash from 'create-hash';
import createHmac from 'create-hmac';
import qs from 'qs';

import { markSkipRemoteSigning } from '../../../remoteSigningAdapter';
import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import { v } from '../../../utils/get-key';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { BASE_URL, HISTORY_BASE_URL, PUBLIC_ENDPOINTS } from './kraken.types';
import type { KrakenBaseUrl } from './kraken.types';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';

// NOTE: Kraken Futures always expects form-encoded data for non-GET requests
// Signature: base64(HMAC_SHA512(SHA256(formData + nonce + path), base64Decode(secret)))

const sort = function alphabeticalSort(a: string, b: string) {
  return a.localeCompare(b);
};

/**
 * Nonce counter shared by EVERY Kraken client, deliberately module-scoped.
 *
 * Kraken requires the nonce to strictly increase per API key, and one exchange builds three
 * clients — `xhr`, `unlimitedXHR` and `historyXhr`. A per-client counter means each starts
 * at 0, so a client that has been idle emits `ts*1000 + 0` while the busy tick loop has
 * already sent `ts*1000 + N` for the same millisecond. Kraken rejects the lower nonce, and
 * it does so as a 200 carrying `{"status":"unauthorized"}` rather than an HTTP error — so it
 * surfaces as an EMPTY page, not a failure.
 *
 * Measured: `history/v3/account-log` returned rows through a one-shot probe and
 * `unauthorized` through the same client once the tick loop was running.
 */
let nonceCounter = 0;


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

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange, baseUrlKind: KrakenBaseUrl = 'derivatives') => {

  const bases = baseUrlKind === 'history' ? HISTORY_BASE_URL : BASE_URL;

  const xhr = safeCexAxiosCreate(
    {
      baseURL: options.testnet ? bases.testnet : bases.livenet,
      timeout: 20_000,
      headers: {},
      /**
       * Kraken signs `qs.stringify(params, { sort, format: 'RFC1738' })` but axios would
       * otherwise serialize the query in INSERTION order with its own escaping. Every
       * signed Kraken GET shipped so far has been param-less, so the two never had to
       * agree; `history/v3/account-log` takes `since`/`before`/`count`/`sort` and is the
       * first that does. A mismatch is a 401, which reads as a dead credential rather than
       * as a serialization bug.
       */
    },
    options,
    exchange,
  );

  retry(xhr, { retries: 3, retryCondition: networkErrorOnRead() });
  if (!options.publicDataOnly) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  xhr.interceptors.request.use((config) => {
    const urlPath = (config.url || '').replace(/^\//, ''); // e.g. v3/openorders
    const isPublic = PUBLIC_ENDPOINTS.some((p) => (config?.url || '').includes(p));
    // Tag on `isPublic` alone, never on the guard it sits next to: a hosted
    // server is handed the key but no secret, so credential-shaped conditions
    // do not mean "public" there. Kraken never signs these anyway, so the round
    // trip the tag removes was returning an unsigned config regardless.
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
        (config as any).data = postData;
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
    // THE SIGNED STRING IS THE SENT STRING, by construction.
    //
    // Kraken signs `qs.stringify(params, { sort })` — alphabetical — while axios serializes
    // `config.params` in INSERTION order, and a `paramsSerializer` (bare function or
    // `{serialize}` object) did not reliably take effect through this stack. The result was
    // a request whose signature covered `count=...&since=...` while the wire carried
    // `since=...&count=...`, which Kraken answers with HTTP 200
    // `{"status":"unauthorized","reason":"You are not authorized to access this endpoint."}`.
    //
    // It authorized only when a caller happened to declare its params alphabetically, so
    // `history/v3/account-log` worked from a probe and failed from the income pager with a
    // byte-identical signature. Moving the query onto the URL removes the coupling entirely;
    // `woo.signV1` does the same thing for the same reason.
    if ((method === 'get' || method === 'delete') && postData) {
      const [pathOnly] = String(config.url || '').split('?');
      config.url = `${pathOnly}?${postData}`;
      config.params = undefined;
    }

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
