import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { bigIntAxiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import type { OKXExchange } from './okx.exchange';
import { BASE_URLS, RECV_WINDOW } from './okx.types';

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const { passphrase } = options;

  if (!passphrase && !options.publicDataOnly) {
    throw new Error('OKX requires a passphrase');
  }

  const region = (exchange as OKXExchange).region ?? 'okx';
  const baseURL = BASE_URLS[region];

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      headers: options.testnet ? { 'x-simulated-trading': 1 } : {},
    },
    options,
    exchange,
  );

  retry(xhr, { retries: 3, retryCondition: isNetworkError });

  // Check OKX API-level errors (non-zero code) and convert to thrown errors
  // so they surface properly instead of crashing on missing `data` field
  xhr.interceptors.response.use((response) => {
    const code = response.data?.code;
    if (code && code !== '0') {
      const msg = response.data?.msg || `OKX API error (code ${code})`;
      const err: any = new Error(msg);
      err.response = response;
      throw err;
    }
    return response;
  });

  xhr.interceptors.request.use((config) => {
    if (config?.url?.includes('public')) {
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

    const timestamp = virtualClock.getCurrentTime().toISOString();
    const toSign = [timestamp, method, url, data ? data : ''];

    const signature = createHmac('sha256', options.secret).update(toSign.join('')).digest('base64');

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'OK-ACCESS-KEY': options.key,
      'OK-ACCESS-SIGN': signature,
      'OK-ACCESS-TIMESTAMP': timestamp,
      'OK-ACCESS-PASSPHRASE': passphrase || '',
    });

    return { ...nextConfig, headers };
  });

  if (options.tealstreetTunnel) {
    setInnerAdapterForRemoteSigning(xhr, bigIntAxiosHttpOverWebsocketAdapter);
  }

  return xhr;
};
