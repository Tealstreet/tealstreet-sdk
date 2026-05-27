import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import {
  axiosHttpOverWebsocketAdapter,
  createAxiosHttpOverWebsocketAdapter,
} from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { BASE_URL, RECV_WINDOW, UDF_BASE_URL } from './bitmex.types';

let nonceCounter = 0;

const generateNonce = () => +virtualClock.getCurrentTimeMs() * 1000 + (nonceCounter++ % 1000);

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const xhr = safeCexAxiosCreate(
    {
      baseURL: options.publicDataOnly
        ? 'https://www.bitmex.com/api/v1'
        : BASE_URL[options.testnet ? 'testnet' : 'livenet'],
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { arrayFormat: 'repeat' }),
      },
      headers: {
        'api-key': options.key,
        'Content-Type': 'application/json',
      },
    },
    options,
    exchange,
  );

  if (!options.publicDataOnly) {
    // Pin the HOW egress region to the account's proxyRegion. BitMEX URLs
    // (api.direct.bitmex.com / www.bitmex.com) aren't in TOKYO_EGRESS_PATTERN,
    // so the default-region adapter would fall back to Singapore for both
    // old and new accounts. Region-specific adapters route via JA-only
    // accounts to Tokyo.
    const adapter =
      options.proxyRegion === 'JA'
        ? createAxiosHttpOverWebsocketAdapter({ region: 'JA' })
        : axiosHttpOverWebsocketAdapter;
    setInnerAdapterForRemoteSigning(xhr, adapter);
  }

  // retry requests on network errors instead of throwing
  retry(xhr, { retries: 3, retryCondition: isNetworkError });

  xhr.interceptors.request.use((config) => {
    const nextConfig = { ...config };

    if (options.remoteSigner) {
      return nextConfig;
    }

    // Skip signing when no API key (public-only mode)
    if (!options.key || !options.secret) {
      return nextConfig;
    }

    const method = config.method?.toUpperCase?.() || 'GET';
    const params = config.params ? qs.stringify(config.params) : null;
    const url = `${config.url}${params ? `?${params}` : ''}`;
    const data = config.data ? JSON.stringify(config.data) : null;

    // UDF endpoints (/api/udf/*) use www.bitmex.com instead of api.direct.bitmex.com
    if (url.startsWith('/udf/')) {
      nextConfig.baseURL = UDF_BASE_URL[options.testnet ? 'testnet' : 'livenet'];
      return {
        ...nextConfig,
        headers: {
          'Content-Type': 'application/json',
        },
      } as any;
    }

    const nonce = `${generateNonce()}`;
    const toSign = [method, '/api/v1' + url, nonce];

    if (data) {
      toSign.push(data);
    }

    const signature = createHmac('sha256', options.secret).update(toSign.join('')).digest('hex');

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'Content-Type': 'application/json',
      'api-key': options.key,
      'api-expires': nonce,
      'api-signature': signature,
    });

    // Bake params into URL so the HOW adapter sends the exact signed URL
    // instead of letting the proxy re-serialize params (which may differ)
    if (params) {
      nextConfig.url = url;
      nextConfig.params = undefined;
    }

    return { ...nextConfig, headers };
  });

  return xhr;
};
