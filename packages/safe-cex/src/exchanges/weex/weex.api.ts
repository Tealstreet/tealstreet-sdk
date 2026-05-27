import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { safeCexAxiosCreate } from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { BASE_URL, PUBLIC_ENDPOINTS, RECV_WINDOW } from './weex.types';

export const serializeWeexParams = (params: unknown): string =>
  qs.stringify(params, {
    arrayFormat: 'repeat',
    sort: (a, b) => a.localeCompare(b),
  });

export const signWeexMessage = (secret: string, message: string): string =>
  createHmac('sha256', secret).update(message).digest('base64');

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  if (!options.passphrase && !options.publicDataOnly) {
    throw new Error('WEEX requires a passphrase');
  }

  const xhr = safeCexAxiosCreate(
    {
      baseURL: BASE_URL[options.testnet ? 'testnet' : 'livenet'],
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      paramsSerializer: {
        serialize: serializeWeexParams,
      },
      headers: {
        'Content-Type': 'application/json',
      },
      transformResponse: bigIntAsStringJsonParse,
    },
    options,
    exchange,
  );

  retry(xhr, { retries: 3, retryCondition: isNetworkError });

  xhr.interceptors.request.use((config) => {
    const url = config.url || '';
    const isPublicEndpoint = PUBLIC_ENDPOINTS.some((endpoint) => url.startsWith(endpoint));
    if (isPublicEndpoint || options.publicDataOnly) {
      return config;
    }

    const nextConfig = { ...config };

    if (options.remoteSigner || !options.key || !options.secret) {
      return nextConfig;
    }

    const method = config.method?.toUpperCase?.() || 'GET';
    const queryString = config.params ? serializeWeexParams(config.params) : '';
    const bodyString = config.data ? JSON.stringify(config.data) : '';
    const requestPath = `${url}${queryString ? `?${queryString}` : ''}`;
    const timestamp = `${virtualClock.getCurrentTimeMs()}`;
    const signature = signWeexMessage(options.secret, `${timestamp}${method}${requestPath}${bodyString}`);

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'ACCESS-KEY': options.key,
      'ACCESS-PASSPHRASE': options.passphrase || '',
      'ACCESS-TIMESTAMP': timestamp,
      'ACCESS-SIGN': signature,
      'Content-Type': 'application/json',
    });

    return { ...nextConfig, headers };
  });

  xhr.interceptors.response.use((response) => {
    const code = response.data?.code;
    if (code !== undefined && `${code}` !== '0' && `${code}` !== '200') {
      const msg = response.data?.msg || response.data?.message || `WEEX API error (code ${code})`;
      const err: any = new Error(msg);
      err.response = response;
      throw err;
    }

    return response;
  });

  return xhr;
};
