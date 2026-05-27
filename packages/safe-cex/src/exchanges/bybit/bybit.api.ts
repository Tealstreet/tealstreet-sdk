import type { HOWRegion } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';
import qs from 'qs';

import { createAxiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { getBybitResponseMeta } from './bybit.response';
import { BASE_URL, BROKER_ID, DEMO_URL, ENDPOINTS, RECV_WINDOW } from './bybit.types';

const isErrorSignature = (error: any) => {
  const isNetwork = isNetworkError(error);
  const isErrSign =
    error?.response?.data?.ret_msg?.includes('error sign!') || error?.response?.data?.retMsg?.includes('error sign!');
  return isNetwork || isErrSign;
};

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const recvWindow = options?.extra?.recvWindow ?? RECV_WINDOW;
  const baseURL = exchange.name.includes('demo')
    ? DEMO_URL[options.testnet ? 'testnet' : 'livenet']
    : BASE_URL[options.testnet ? 'testnet' : 'livenet'];
  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: recvWindow,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params),
      },
      headers: {
        'X-Referer': BROKER_ID,
        'X-BAPI-RECV-WINDOW': recvWindow,
        'Content-Type': 'application/json, charset=utf-8',
      },
    },
    options,
    exchange,
  );

  // retry requests on network errors instead of throwing
  // and some signature errors
  retry(xhr, {
    retries: 3,
    retryCondition: isErrorSignature,
  });

  // Add requestId, skip auth for public endpoints, and sign requests.
  // Must be a SINGLE interceptor because axios runs request interceptors in
  // LIFO order — splitting into two would cause signing to run before requestId
  // is added, producing a signature mismatch.
  xhr.interceptors.request.use((config) => {
    const nextConfig = {
      ...config,
      params: {
        ...config.params,
        requestId: nanoid(),
      },
    };

    if (nextConfig.method === 'get' && nextConfig.url === ENDPOINTS.KLINE) {
      return { ...nextConfig, headers: new AxiosHeaders({}) };
    }

    // Skip signing when remote signer handles it
    if (options.remoteSigner) {
      return nextConfig;
    }

    // Skip signing when no API key (public-only mode)
    if (!options.key || !options.secret) {
      return nextConfig;
    }

    // TODO: Audit whether HOW proxy re-serialization can break this signature (see bitmex.api.ts).
    const data = nextConfig.method === 'get' ? qs.stringify(nextConfig.params) : JSON.stringify(nextConfig.data);

    const timestamp = virtualClock.getCurrentTimeMs();
    const signature = createHmac('sha256', options.secret)
      .update([timestamp, options.key, recvWindow, data].join(''))
      .digest('hex');

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'X-BAPI-SIGN': signature,
      'X-BAPI-API-KEY': options.key,
      'X-BAPI-TIMESTAMP': timestamp,
    });

    return { ...nextConfig, headers };
  });

  if (options.tealstreetTunnel) {
    const region = options.proxyRegion as HOWRegion | undefined;
    setInnerAdapterForRemoteSigning(xhr, createAxiosHttpOverWebsocketAdapter({ region }));
  }

  xhr.interceptors.response.use(async (response) => {
    const responseMeta = getBybitResponseMeta(response.data);
    if (response.data && responseMeta.hasExplicitStatus && !responseMeta.isSuccess) {
      const retMsg = responseMeta.retMsg || '';
      if (retMsg.includes('recv')) {
        exchange.emitAndLogError('Local time is out of sync, updating...');
        await virtualClock.syncBybitClientServerTime(); // This method name stays the same as it's a common sync
      } else if (responseMeta.retCode !== undefined ? responseMeta.retCode !== 0 : true) {
        if (response.data?.raw_response?.includes?.('There might be too much traffic or a configuration error')) {
          exchange.emitAndLogError('Server was unavailable for a request, retrying...');
        }
        console.error(`${exchange.name} error`, response.data);
      }
    }
    return response;
  });

  return xhr;
};
