import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import qs from 'qs';

import { markSkipRemoteSigning } from '../../remoteSigningAdapter';
import { installTransportAdapter, safeCexAxiosCreate, safeCexAxiosCreateNoProxy } from '../../safeCexAxiosCreate';
import { networkErrorOnRead, readsAndPosted } from '../../utils/retryReadsOnly';
import {
  isBinanceStyleSignatureRejection,
  resendOnSignatureRejection,
  signatureRejectedResponse,
} from '../../utils/signatureRejection';
import { BaseExchange } from '../base';
import { signBinanceRequest } from './binance.signing';
import { BASE_FAPI_URL, BASE_PAPI_URL, ENDPOINTS, PUBLIC_ENDPOINTS, RECV_WINDOW } from './binancepm.types';

export type ApiType = 'papi' | 'fapi';

export const createBinanceAPI = (options: ExchangeOptions, exchange: BaseExchange, apiType: ApiType = 'papi') => {
  const baseURL = (apiType === 'papi' ? BASE_PAPI_URL : BASE_FAPI_URL)[options.testnet ? 'testnet' : 'livenet'];

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { arrayFormat: 'repeat' }),
      },
      headers: {
        'X-MBX-APIKEY': options.key,
        'Content-Type': 'application/json, charset=utf-8',
      },
    },
    options,
    exchange,
  );

  // retry requests on network errors instead of throwing
  retry(xhr, {
    retries: 3,
    retryCondition: (error) =>
      networkErrorOnRead(readsAndPosted(ENDPOINTS.LISTEN_KEY))(error) ||
      signatureRejectedResponse(isBinanceStyleSignatureRejection)(error),
  });

  xhr.interceptors.request.use((config) => {
    // on livenet, don't sign listen key requests (they don't need it)
    if (config.url === ENDPOINTS.LISTEN_KEY && !options.testnet) {
      return markSkipRemoteSigning(config);
    }

    const isPublicEndPoint = PUBLIC_ENDPOINTS.includes(`${config.url}`);
    if (isPublicEndPoint) {
      return markSkipRemoteSigning({
        ...config,
        headers: AxiosHeaders.from({
          'Content-Type': 'application/json, charset=utf-8',
        }),
        timeout: 20_000,
      });
    }

    return signBinanceRequest(config, options, exchange.name, options?.extra?.recvWindow ?? RECV_WINDOW);
  });

  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  // Last, so a resend is not put through any later interceptor twice.
  xhr.interceptors.response.use(
    (response) => resendOnSignatureRejection(xhr, response, isBinanceStyleSignatureRejection) ?? response,
  );

  return xhr;
};

// Maintain backward compatibility
export const createPAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  return createBinanceAPI(options, exchange, 'papi');
};

// New function for FAPI
export const createFAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  return createBinanceAPI(options, exchange, 'fapi');
};

export const createPublicAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const baseURL = BASE_FAPI_URL[options.testnet ? 'testnet' : 'livenet'];

  const xhr = safeCexAxiosCreateNoProxy(
    {
      baseURL,
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { arrayFormat: 'repeat' }),
      },
      headers: {
        'Content-Type': 'application/json, charset=utf-8',
      },
    },
    options,
    exchange,
  );

  // retry requests on network errors instead of throwing
  retry(xhr, { retries: 3, retryCondition: networkErrorOnRead(readsAndPosted(ENDPOINTS.LISTEN_KEY)) });

  // Every request on this client is public; a hosted server need not ask the device.
  xhr.interceptors.request.use((config) => {
    return markSkipRemoteSigning({
      ...config,
      headers: AxiosHeaders.from({
        'Content-Type': 'application/json, charset=utf-8',
      }),
      timeout: 20_000,
    });
  });

  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  return xhr;
};
