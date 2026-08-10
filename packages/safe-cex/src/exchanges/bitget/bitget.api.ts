import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { axiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { v } from '../../utils/get-key';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { BASE_URL, BROKER_ID, ENDPOINTS, RECV_WINDOW } from './bitget.types';

// Bitget error codes that must NOT surface as user-facing toasts. The
// `_detectAccountType` probe in bitget.exchange.ts intentionally calls a
// v3 endpoint expecting `40084` ("Classic Account mode") on Classic
// accounts — that's the signal we use to set `isUta=false`, not a real
// failure.
const SILENT_ERROR_CODES = new Set<string>(['40084']);

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
        'ACCESS-KEY': options.key,
        'ACCESS-PASSPHRASE': options.passphrase,
        'X-CHANNEL-API-CODE': BROKER_ID,
        'Content-Type': 'application/json',
      },
    },
    options,
    exchange,
  );

  retry(xhr, { retries: 3, retryCondition: isNetworkError });

  xhr.interceptors.request.use((config) => {
    const nextConfig = { ...config };

    // Skip signing when remote signer handles it
    if (options.remoteSigner) {
      return nextConfig;
    }

    // Skip signing when no API key (public-only mode)
    if (!options.key || !options.secret) {
      return nextConfig;
    }

    const timestamp = virtualClock.getCurrentTimeMs();
    const method = config.method?.toUpperCase() ?? 'GET';

    // TODO: Audit whether HOW proxy re-serialization can break this signature (see bitmex.api.ts).
    let signingUrl = config.url;
    if (config.params) {
      if (config.params.symbol && !config.url?.includes(ENDPOINTS.KLINE)) {
        config.params.symbol = config.params.symbol.replace('$', '');
      }
      const signingQuery = qs.stringify(config.params, { encode: false });
      signingUrl = `${signingUrl}?${signingQuery}`;
    }

    const stringifiedBody = config.data
      ? toJsonString({
          ...config.data,
          ...(config.data.symbol ? { symbol: config.data.symbol.replace('$', '') } : {}),
        })
      : '';
    const toSign = [timestamp, method, signingUrl, stringifiedBody].join('');
    const signature = createHmac('sha256', options.secret).update(toSign).digest().toString('base64');

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'ACCESS-SIGN': signature,
      'ACCESS-TIMESTAMP': timestamp,
    });

    return { ...nextConfig, headers };
  });

  xhr.interceptors.response.use(async (response) => {
    if (response.data && !v(response.data, 'data') && v(response.data, 'code') !== '00000') {
      const code = String(v(response.data, 'code') ?? '');
      if (v(response.data, 'msg') && !SILENT_ERROR_CODES.has(code)) {
        exchange.emitAndLogError(v(response.data, 'msg'));
      }
    }
    return response;
  });

  if (options.tealstreetTunnel) {
    setInnerAdapterForRemoteSigning(xhr, axiosHttpOverWebsocketAdapter);
  }

  return xhr;
};

function toJsonString(obj: object): string | null {
  if (obj == null) {
    return null;
  }

  let json = JSON.stringify(obj);
  Object.keys(obj)
    .filter((key) => key[0] === '_')
    .forEach((key) => {
      json = json.replace(key, key.substring(1));
    });
  const reg = new RegExp('"_', 'g');
  return json.replace(reg, '"');
}
