import type { AxiosRequestConfig } from 'axios';
import type { ExchangeOptions } from '../../../types';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import { getTransport } from '../../../transport/registry';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { BASE_URL, BROKER_ID, ENDPOINTS, RECV_WINDOW, TESTNET_BROKER_ID } from './woo.types';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';
import { parseWooJson } from './woo.json';

const signV1 = (config: AxiosRequestConfig, options: ExchangeOptions) => {
  const nextConfig = { ...config };
  const method = nextConfig.method?.toUpperCase?.() || 'GET';
  const isMutation = method === 'DELETE' || method === 'POST' || method === 'PUT';

  // proxy use with non base url
  if (nextConfig?.url?.startsWith('http')) {
    const privateProxyUrl = getTransport(options).proxyPrefix({ exchange: 'woo', options, config: nextConfig });
    const isPrivateProxyEnabled = privateProxyUrl !== '';
    nextConfig.url = isPrivateProxyEnabled ? `${privateProxyUrl}/${nextConfig.url}` : nextConfig.url;
  }

  const data = config.data || config.params || {};
  const asString = qs.stringify(data, {
    arrayFormat: 'repeat',
    sort: (a, b) => a.localeCompare(b),
  });

  if (asString && !isMutation) {
    nextConfig.url = `${nextConfig.url}?${asString}`;
    delete nextConfig.params;
  }

  const timestamp = virtualClock.getCurrentTimeMs();
  const signature = createHmac('sha256', options.secret).update(`${asString}|${timestamp}`).digest('hex');

  const headers = new AxiosHeaders({
    ...nextConfig.headers,
    'x-api-timestamp': timestamp,
    'x-api-signature': signature,
  });

  if (isMutation) {
    nextConfig.data = asString;
    delete nextConfig.params;
    headers.setContentType('application/x-www-form-urlencoded');
  }

  return { ...nextConfig, headers };
};

const signV3 = (c: AxiosRequestConfig, options: ExchangeOptions) => {
  // we need to uppercase the method and default to GET
  const nextConfig = { ...c, method: c.method?.toUpperCase?.() || 'GET' };

  // proxy use with non base url
  if (nextConfig?.url?.startsWith('http')) {
    const privateProxyUrl = getTransport(options).proxyPrefix({ exchange: 'woo', options, config: nextConfig });
    const isPrivateProxyEnabled = privateProxyUrl !== '';
    nextConfig.url = isPrivateProxyEnabled ? `${privateProxyUrl}/${nextConfig.url}` : nextConfig.url;
  }

  // we do the serialization of params once here
  // because we need it in the signature
  if (nextConfig.params) {
    nextConfig.url = `${nextConfig.url}?${qs.stringify(nextConfig.params)}`;
    delete nextConfig.params;
  }

  const timestamp = virtualClock.getCurrentTimeMs();
  const textSign = [
    timestamp,
    nextConfig.method,
    nextConfig.url,
    nextConfig.data ? JSON.stringify(nextConfig.data) : '',
  ].join('');

  const signature = createHmac('sha256', options.secret).update(textSign).digest('hex');

  const headers = new AxiosHeaders({
    ...nextConfig.headers,
    'x-api-timestamp': timestamp,
    'x-api-signature': signature,
  });

  return { ...nextConfig, headers };
};

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const xhr = safeCexAxiosCreate(
    {
      baseURL: BASE_URL[options.testnet ? 'testnet' : 'livenet'],
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      transformResponse: parseWooJson,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { sort: (a, b) => a.localeCompare(b) }),
      },
      headers: {
        'cache-control': 'no-cache',
        'x-api-key': options.key,
      },
    },
    options,
    exchange,
  );

  // retry requests on network errors instead of throwing
  retry(xhr, { retries: 3, retryCondition: networkErrorOnRead() });

  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  xhr.interceptors.request.use((config) => {
    // don't sign public endpoints requests
    if (config.url?.includes?.('/public/')) {
      return config;
    }

    if (config.method?.toUpperCase?.() === 'POST') {
      // breaks retry
      try {
        if (config.url === ENDPOINTS.PLACE_ORDER) {
          config.data.broker_id = options.testnet ? TESTNET_BROKER_ID : BROKER_ID;
        }
        if (config.url === ENDPOINTS.ALGO_ORDER) {
          config.data.brokerId = options.testnet ? TESTNET_BROKER_ID : BROKER_ID;
        }
      } catch (_e) {
        return config;
        // do nothing
      }
    }

    if (options.remoteSigner) {
      return config;
    }

    // Skip signing when no API key (public-only mode)
    if (!options.key || !options.secret) {
      return config;
    }

    // sign v1 endpoints requests
    if (config.url?.includes('v1')) {
      return signV1(config, options);
    }

    // sign v3 endpoints requests
    return signV3(config, options);
  });

  return xhr;
};
