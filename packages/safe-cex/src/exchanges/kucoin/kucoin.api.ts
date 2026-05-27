import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { axiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { BASE_URL, BROKER_ID, RECV_WINDOW } from './kucoin.types';

const stringifyQuery = (params: Record<string, any>, encode: boolean) => qs.stringify(params, { encode });

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const recvWindow = options?.extra?.recvWindow ?? RECV_WINDOW;
  const secret = options.secret || '';
  const passphrase = options.passphrase || '';
  const shouldAuthenticate = !options.publicDataOnly;

  const baseURL = BASE_URL;

  // For API key-V2.0, encrypt passphrase with HMAC-sha256 via API-Secret and then base64 encode
  const encryptedPassphrase = createHmac('sha256', secret).update(passphrase).digest('base64');

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: recvWindow,
      paramsSerializer: {
        serialize: (params) => stringifyQuery(params, true),
      },
      headers: {
        'X-CHANNEL-API-CODE': BROKER_ID,
        'Content-Type': 'application/json',
        ...(shouldAuthenticate
          ? {
              'KC-API-KEY': options.key,
              'KC-API-PASSPHRASE': encryptedPassphrase,
              'KC-API-KEY-VERSION': '2',
            }
          : {}),
      },
    },
    options,
    exchange,
  );

  retry(xhr, { retries: 3, retryCondition: isNetworkError });

  if (shouldAuthenticate) {
    setInnerAdapterForRemoteSigning(xhr, axiosHttpOverWebsocketAdapter);
  }

  xhr.interceptors.request.use((config) => {
    const nextConfig = { ...config };

    if (!shouldAuthenticate || options.remoteSigner) {
      return nextConfig;
    }

    const timestamp = virtualClock.getCurrentTimeMs();
    const method = config.method?.toUpperCase() ?? 'GET';

    let url = config.url;
    if (config.params) {
      const query = stringifyQuery(config.params, false);
      url = `${url}?${query}`;
    }

    const stringifiedBody = config.data ? JSON.stringify(config.data) : '';
    const toSign = [timestamp, method, url, stringifiedBody].join('');

    const signature = createHmac('sha256', secret).update(toSign).digest('base64');

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'KC-API-SIGN': signature,
      'KC-API-TIMESTAMP': timestamp,
      'KC-API-KEY-VERSION': '2',
    });

    return { ...nextConfig, headers };
  });

  return xhr;
};
