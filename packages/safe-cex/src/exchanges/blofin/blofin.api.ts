import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { uuid } from '@tealstreet/safe-cex/src/utils/uuid';
import { safeCexAxiosCreate } from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { BASE_URL } from './blofin.types';

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const { passphrase } = options;

  if (!passphrase && !options.publicDataOnly) {
    throw new Error('Blofin requires a passphrase');
  }

  const baseURL = BASE_URL;

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      //timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      timeout: 20_000, // otherwise can't use leverage batch
      headers: options.testnet ? { 'x-simulated-trading': 1 } : {},
    },
    options,
    exchange,
  );

  retry(xhr, { retries: 3, retryCondition: isNetworkError });

  // if (options.tealstreetTunnel) {
  //   xhr.defaults.adapter = axiosHttpOverWebsocketAdapter;
  // }

  xhr.interceptors.request.use((config) => {
    if (config?.url?.includes('public') || config?.url?.includes('market') || options.publicDataOnly) {
      return config;
    }

    const nextConfig = { ...config };

    if (options.remoteSigner) {
      return nextConfig;
    }

    // Skip signing when no API key (public-only mode)
    if (!options.key || !options.secret) {
      return nextConfig;
    }

    const method = config.method?.toUpperCase?.() || 'GET';
    // TODO: Audit whether HOW proxy re-serialization can break this signature (see bitmex.api.ts).
    const params = config.params ? decodeURIComponent(qs.stringify(config.params)) : null;
    const url = `${config.url}${params ? `?${params}` : ''}`;
    const data = config.data ? JSON.stringify(config.data) : null;

    const timestamp = +virtualClock.getCurrentTime();
    const nonce = uuid();
    const toSign = [url, method, timestamp, nonce];

    if (data) {
      toSign.push(data);
    }

    const signature = btoa(createHmac('sha256', options.secret).update(toSign.join('')).digest('hex'));

    // const signature = this.binaryToBase64(
    //   this.encode(this.hmac(this.encode(auth), this.encode(this.secret), 'sha256', 'hex')),
    // );

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'ACCESS-KEY': options.key,
      'ACCESS-SIGN': signature,
      'ACCESS-NONCE': nonce,
      'ACCESS-TIMESTAMP': timestamp,
      'ACCESS-PASSPHRASE': passphrase || '',
    });

    return { ...nextConfig, headers };
  });

  return xhr;
};
