import type { ExchangeOptions } from '../../../types';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import qs from 'qs';

import { installTransportAdapter, safeCexAxiosCreate } from '../../../safeCexAxiosCreate';
import { appendHttpFailure } from '../../../utils/httpFailure';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { BASE_URL, ENDPOINTS, RECV_WINDOW } from './apexomni.types';
import { hmacSha256Base64 } from './apexomni.utils';

const serializeApexPayload = (payload: unknown) => {
  if (payload === undefined || payload === null) return '';
  if (typeof payload === 'string') return payload;

  return qs.stringify(payload, {
    filter(_prefix, value) {
      // Keep falsy values like 0/false because they must match the actual request payload.
      if (value === undefined || value === null) {
        return;
      }
      return value;
    },
    sort(a, b) {
      return a.localeCompare(b);
    },
  });
};

const serializeApexPayloadLegacy = (payload: unknown) =>
  qs.stringify(payload, {
    filter(_prefix, value) {
      if (value) {
        return value;
      }
      return;
    },
    sort(a, b) {
      return a.localeCompare(b);
    },
  });

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const recvWindow = options?.extra?.recvWindow ?? RECV_WINDOW;
  const baseURL = BASE_URL[options.testnet ? 'testnet' : 'livenet'];
  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: recvWindow,
      paramsSerializer: {
        serialize: (params) => serializeApexPayload(params),
      },
      headers: {
        'Content-Type': 'application/json, charset=utf-8',
      },
    },
    options,
    exchange,
  );

  // retry reads on network errors; a signature failure in a 200 envelope gets the legacy-signing resend below
  retry(xhr, {
    retries: 3,
    retryCondition: networkErrorOnRead(),
  });

  xhr.interceptors.request.use((config) => {
    // Retries receive transformed form bytes (or platform-specific FormData).
    // Retain the semantic payload so legacy signing is stable across runtimes.
    const originalData = Object.prototype.hasOwnProperty.call(config, '__apexOriginalData')
      ? (config as any).__apexOriginalData
      : config.data;
    const nextConfig = {
      ...config,
      __apexOriginalData: originalData,
      params: {
        ...config.params,
      },
    };

    if (nextConfig.method === 'get' && nextConfig.url === ENDPOINTS.KLINE) {
      return { ...nextConfig, headers: new AxiosHeaders({}) };
    }

    const method = `${nextConfig.method || 'get'}`.toUpperCase();
    const isQueryMethod = method === 'GET' || method === 'DELETE';
    const useLegacySigning = Boolean((nextConfig as any).__apexUseLegacySigning);

    const messageToSign = (() => {
      if (useLegacySigning) {
        const params = serializeApexPayloadLegacy(isQueryMethod ? nextConfig.params : originalData);
        const timestamp = new Date(new Date(virtualClock.getCurrentTimeMs()).toUTCString()).getTime();
        const path = nextConfig.url || '';
        (nextConfig as any).__apexTimestamp = timestamp;

        return [
          timestamp,
          method,
          '/api',
          path,
          isQueryMethod && params?.length > 0 ? '?' : '',
          params,
          path.includes(ENDPOINTS.CREATE_ORDER) ? originalData : '',
        ].join('');
      }

      const serializedPayload = serializeApexPayload(isQueryMethod ? nextConfig.params : nextConfig.data);
      const timestamp = `${virtualClock.getCurrentTimeMs()}`;
      const requestPath = `/api${nextConfig.url}${isQueryMethod && serializedPayload ? `?${serializedPayload}` : ''}`;
      (nextConfig as any).__apexTimestamp = timestamp;

      return `${timestamp}${method}${requestPath}${isQueryMethod ? '' : serializedPayload}`;
    })();

    const key = Buffer.from(options.secret).toString('base64');
    const signature = hmacSha256Base64(messageToSign, key);

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'APEX-SIGNATURE': signature,
      'APEX-API-KEY': options.key,
      'APEX-PASSPHRASE': `${options.passphrase}`,
      'APEX-TIMESTAMP': `${(nextConfig as any).__apexTimestamp}`,
    });

    if (nextConfig.url?.includes('/v3/set-initial-margin-rate')) {
      headers.setContentType('multipart/form-data');
    }

    return { ...nextConfig, headers };
  });

  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  xhr.interceptors.response.use(async (response) => {
    if (response.data.code && response.data.code !== 0) {
      const config = response.config as any;

      // Some Apex environments / proxy chains return signature failures with non-20016 codes
      // (HTTP 401 surfaced as code 401, generic 10002, etc.) and a message referencing
      // "sign"/"signature". Treat any of those as a signing-canonicalization failure and try
      // once with the legacy signing path before surfacing the error to the caller.
      const responseCode = Number(response.data.code);
      const responseMessage = `${response.data.msg ?? ''}`;
      const isSignCheckFailed = responseCode === 20016 || responseCode === 401 || /sign(ature)?/i.test(responseMessage);

      if (isSignCheckFailed && !config.__apexLegacyRetried) {
        return xhr.request({
          ...config,
          headers: new AxiosHeaders(config.headers || {}),
          __apexLegacyRetried: true,
          __apexUseLegacySigning: true,
        });
      }

      const msg = appendHttpFailure(`[${response.data.code}] ${response.data.msg}`, response);
      exchange.emitAndLogError(msg);
      throw new Error(msg);
    }

    return response;
  });

  return xhr;
};
