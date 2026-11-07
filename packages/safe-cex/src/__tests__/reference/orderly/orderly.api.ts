import type { AxiosRequestConfig } from 'axios';
import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import qs from 'qs';
import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import type { ExchangeOptions } from '../../../types';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { orderlySign } from './orderly.encryption';
import { BASE_URL, BROKER_ID, ENDPOINTS, RECV_WINDOW, TESTNET_BROKER_ID } from './orderly.types';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';

const signRequest = (c: AxiosRequestConfig, options: ExchangeOptions) => {
  // we need to uppercase the method and default to GET
  const nextConfig = { ...c, method: c.method?.toUpperCase?.() || 'GET' };

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
    nextConfig.data && Object.keys(nextConfig.data).length > 0 ? JSON.stringify(nextConfig.data) : '',
  ].join('');

  const orderlySignature = orderlySign(textSign, options.secret);

  const headers = new AxiosHeaders({
    ...nextConfig.headers,
    'orderly-timestamp': timestamp,
    'orderly-signature': orderlySignature,
    'orderly-textsign': textSign,
    'orderly-key': options.key,
    'Content-Type':
      nextConfig.method === 'GET' || nextConfig.method === 'DELETE'
        ? 'application/x-www-form-urlencoded'
        : 'application/json',
    Accept: '*/*',
  });

  return { ...nextConfig, headers };
};

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const xhr = safeCexAxiosCreate(
    {
      baseURL: BASE_URL[options.testnet ? 'testnet' : 'livenet'],
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { sort: (a, b) => a.localeCompare(b) }),
      },
      headers: {
        'cache-control': 'no-cache',
        'orderly-account-id': `${options.applicationId}`,
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
      if (config.url === ENDPOINTS.PLACE_ORDER) {
         
        config.data.broker_id = options.testnet ? TESTNET_BROKER_ID : BROKER_ID;
      }
      if (config.url === ENDPOINTS.ALGO_ORDER) {
         
        config.data.brokerId = options.testnet ? TESTNET_BROKER_ID : BROKER_ID;
      }
    }

    return signRequest(config, options);
  });

  return xhr;
};
