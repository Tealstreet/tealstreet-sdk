import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { axiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import {
  KUCOIN_SPOT_BASE_URL,
  KUCOIN_SPOT_BROKER_ID,
  KUCOIN_SPOT_PUBLIC_ENDPOINTS,
  KUCOIN_SPOT_RECV_WINDOW,
} from './kucoinspot.types';

const isPublicEndpoint = (url?: string) => {
  if (!url) return false;
  return KUCOIN_SPOT_PUBLIC_ENDPOINTS.some((endpoint) => url.startsWith(endpoint));
};

export const createKucoinSpotAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const recvWindow = options?.extra?.recvWindow ?? KUCOIN_SPOT_RECV_WINDOW;
  const secret = options.secret || '';
  const passphrase = options.passphrase || '';
  const encryptedPassphrase = createHmac('sha256', secret).update(passphrase).digest('base64');

  const xhr = safeCexAxiosCreate(
    {
      baseURL: KUCOIN_SPOT_BASE_URL,
      timeout: recvWindow,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params),
      },
      headers: {
        'X-CHANNEL-API-CODE': KUCOIN_SPOT_BROKER_ID,
        'Content-Type': 'application/json',
      },
    },
    options,
    exchange,
  );

  retry(xhr, { retries: 3, retryCondition: isNetworkError });

  if (!options.publicDataOnly) {
    setInnerAdapterForRemoteSigning(xhr, axiosHttpOverWebsocketAdapter);
  }

  xhr.interceptors.request.use((config) => {
    const nextConfig = { ...config };
    if (isPublicEndpoint(config.url) || options.publicDataOnly) {
      return nextConfig;
    }

    if (options.remoteSigner) {
      return nextConfig;
    }

    const timestamp = virtualClock.getCurrentTimeMs();
    const method = config.method?.toUpperCase() ?? 'GET';

    let url = config.url;
    if (config.params) {
      const query = qs.stringify(config.params);
      url = `${url}?${query}`;
    }

    const stringifiedBody = config.data ? JSON.stringify(config.data) : '';
    const signature = createHmac('sha256', secret)
      .update([timestamp, method, url, stringifiedBody].join(''))
      .digest('base64');

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'KC-API-KEY': options.key,
      'KC-API-SIGN': signature,
      'KC-API-TIMESTAMP': timestamp,
      'KC-API-PASSPHRASE': encryptedPassphrase,
      'KC-API-KEY-VERSION': '2',
    });

    return { ...nextConfig, headers };
  });

  return xhr;
};
