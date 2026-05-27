import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { bigIntAxiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { BASE_URL, BROKER_ID, RECV_WINDOW } from './bitrue.types';

const isErrorSignature = (error: any) => {
  const isNetwork = isNetworkError(error);
  const isErrSign =
    error?.response?.data?.ret_msg?.includes('error sign!') || error?.response?.data?.retMsg?.includes('error sign!');
  return isNetwork || isErrSign;
};

// only works with worker shim...
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
        // TODO: update referer
        'X-Referer': BROKER_ID,
        'Content-Type': 'application/json',
      },
    },
    options,
    exchange,
  );

  if (options.tealstreetTunnel) {
    setInnerAdapterForRemoteSigning(xhr, bigIntAxiosHttpOverWebsocketAdapter);
  }

  // retry requests on network errors instead of throwing
  // and some signature errors
  retry(xhr, {
    retries: 3,
    retryCondition: isErrorSignature,
  });

  xhr.interceptors.request.use((config) => {
    const nextConfig = {
      ...config,
    };

    if (options.remoteSigner) {
      return nextConfig;
    }

    // Skip signing when no API key (public-only mode)
    if (!options.key || !options.secret) {
      return nextConfig;
    }

    // proxy use with non base url
    if (nextConfig?.url?.startsWith('http')) {
      const parts = nextConfig.url.split('.com');
      nextConfig.baseURL = parts[0] + '.com';
      nextConfig.url = parts[1];
    }

    const method = nextConfig.method?.toUpperCase() || 'GET';
    const maybeAbsoluteUrl = `${nextConfig.baseURL || ''}${nextConfig.url || ''}`.toLowerCase();
    const isOpenApiRequest = maybeAbsoluteUrl.includes('open.bitrue.com');

    // open.bitrue.com listen-key endpoints use Binance-style query signing with X-MBX-APIKEY.
    if (isOpenApiRequest) {
      const timestamp = virtualClock.getCurrentTimeMs();
      const signParams = {
        ...(nextConfig.params || {}),
        timestamp,
        recvWindow,
      };
      const asString = qs.stringify(signParams);
      const signature = createHmac('sha256', options.secret).update(asString).digest('hex');

      const headers = new AxiosHeaders({
        ...nextConfig.headers,
        'Content-Type': 'application/x-www-form-urlencoded',
        'X-MBX-APIKEY': options.key,
      });

      const signedPayload = { ...signParams, signature };
      if (method === 'GET' || method === 'DELETE') {
        return { ...nextConfig, params: signedPayload, headers };
      }

      return { ...nextConfig, params: undefined, data: qs.stringify(signedPayload), headers };
    }

    const basePath = nextConfig.url;
    const body =
      nextConfig.data === undefined || nextConfig.data === null
        ? ''
        : typeof nextConfig.data === 'string'
          ? nextConfig.data
          : JSON.stringify(nextConfig.data);
    // TODO: Audit whether HOW proxy re-serialization can break this signature (see bitmex.api.ts).
    const path = basePath + (nextConfig.params ? '?' + qs.stringify(nextConfig.params) : '');
    const timestamp = virtualClock.getCurrentTimeMs();

    const signature = createHmac('sha256', options.secret)
      .update([timestamp, method, path, body].join(''))
      .digest('hex');

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'Content-Type': 'application/json',
      'X-CH-APIKEY': options.key,
      'X-CH-SIGN': signature,
      'X-CH-TS': timestamp,
      recvWindow: recvWindow,
    });

    return { ...nextConfig, headers };
  });

  return xhr;
};
