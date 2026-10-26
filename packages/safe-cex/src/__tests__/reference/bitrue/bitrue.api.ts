import type { ExchangeOptions } from '../../../types';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { installTransportAdapter, safeCexAxiosCreate } from '../../../safeCexAxiosCreate';
import { networkErrorOnRead, readsAndPosted } from '../../../utils/retryReadsOnly';
import {
  isBinanceStyleSignatureRejection,
  resendOnSignatureRejection,
  signatureRejectedResponse,
} from '../../../utils/signatureRejection';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { BASE_URL, BROKER_ID, ENDPOINTS, RECV_WINDOW } from './bitrue.types';

// Binance-style -1022; Bitrue sends codes as numbers or strings.
export const isBitrueSignatureRejection = isBinanceStyleSignatureRejection;

const isRetryable = (error: any) =>
  networkErrorOnRead(readsAndPosted(ENDPOINTS.LISTEN_KEY))(error) ||
  signatureRejectedResponse(isBitrueSignatureRejection)(error);

// only works with worker shim...
export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const recvWindow = options?.extra?.recvWindow ?? RECV_WINDOW;
  const baseURL = BASE_URL;
  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: recvWindow,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params),
      },
      headers: {
        // TODO: update referer
        'X-Referer': BROKER_ID,
        'Content-Type': 'application/json',
      },
    },
    options,
    exchange,
  );

  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name, 'bigInt');
  }

  // retry reads on network errors, and a signature rejection once
  retry(xhr, {
    retries: 3,
    retryCondition: isRetryable,
  });

  xhr.interceptors.request.use((config) => {
    const nextConfig = {
      ...config,
    };

    if (options.remoteSigner) {
      return nextConfig;
    }

    // Skip signing when no API key (public-only mode)
    if (!options.key || !options.secret) {
      return nextConfig;
    }

    // proxy use with non base url
    if (nextConfig?.url?.startsWith('http')) {
      const parts = nextConfig.url.split('.com');
      nextConfig.baseURL = parts[0] + '.com';
      nextConfig.url = parts[1];
    }

    const method = nextConfig.method?.toUpperCase() || 'GET';
    const basePath = nextConfig.url;
    const body =
      nextConfig.data === undefined || nextConfig.data === null
        ? ''
        : typeof nextConfig.data === 'string'
          ? nextConfig.data
          : JSON.stringify(nextConfig.data);
    // TODO: Audit whether HOW proxy re-serialization can break this signature (see bitmex.api.ts).
    const path = basePath + (nextConfig.params ? '?' + qs.stringify(nextConfig.params) : '');
    const timestamp = virtualClock.getCurrentTimeMs();

    const signature = createHmac('sha256', options.secret)
      .update([timestamp, method, path, body].join(''))
      .digest('hex');

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'Content-Type': 'application/json',
      'X-CH-APIKEY': options.key,
      'X-CH-SIGN': signature,
      'X-CH-TS': timestamp,
      recvWindow: recvWindow,
    });

    return { ...nextConfig, headers };
  });

  // The HOW tunnel resolves a 4xx, so a rejection over it never reaches axios-retry.
  xhr.interceptors.response.use(
    (response) => resendOnSignatureRejection(xhr, response, isBitrueSignatureRejection) ?? response,
  );

  return xhr;
};
