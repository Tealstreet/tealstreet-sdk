import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import qs from 'qs';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
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
import { BASE_URL, ENDPOINTS, PUBLIC_ENDPOINTS, RECV_WINDOW } from './binanceinverse.types';

export const createPublicAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const baseURL = BASE_URL[options.testnet ? 'testnet' : 'livenet'];

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
      transformResponse: bigIntAsStringJsonParse,
    },
    options,
    exchange,
  );

  retry(xhr, { retries: 3, retryCondition: networkErrorOnRead(readsAndPosted(ENDPOINTS.LISTEN_KEY)) });

  xhr.interceptors.request.use((config) =>
    markSkipRemoteSigning({
      ...config,
      headers: AxiosHeaders.from({
        'Content-Type': 'application/json, charset=utf-8',
      }),
      timeout: 20_000,
    }),
  );

  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name, 'bigInt');
  }

  return xhr;
};

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const baseURL = BASE_URL[options.testnet ? 'testnet' : 'livenet'];

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
      transformResponse: bigIntAsStringJsonParse,
    },
    options,
    exchange,
  );

  retry(xhr, {
    retries: 3,
    retryCondition: (error) =>
      networkErrorOnRead(readsAndPosted(ENDPOINTS.LISTEN_KEY))(error) ||
      signatureRejectedResponse(isBinanceStyleSignatureRejection)(error),
  });

  xhr.interceptors.request.use((config) => {
    // COIN-M listen-key management is API-key authenticated, not signed.
    if (config.url === ENDPOINTS.LISTEN_KEY) {
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
    installTransportAdapter(xhr, options, exchange.name, 'bigInt');
  }

  const SILENT_ERROR_CODES = [
    -2011, // Unknown order sent - order already gone
    -2013, // Order does not exist - expected client-order lookup miss
    -4059, // No need to change position side
    -5022, // Post-only rejection during chaser retries
  ];

  xhr.interceptors.response.use(
    (response) => {
      const signatureResend = resendOnSignatureRejection(xhr, response, isBinanceStyleSignatureRejection);
      if (signatureResend) return signatureResend;

      if (response.data && typeof response.data === 'object' && 'code' in response.data && response.data.code < 0) {
        const errorCode = response.data.code;
        const errorMsg = response.data.msg || `Binance COIN-M Error: ${errorCode}`;
        if (!SILENT_ERROR_CODES.includes(errorCode)) {
          exchange.emitAndLogError(errorMsg);
        }
        const error = new Error(errorMsg);
        (error as any).code = errorCode;
        (error as any).response = response;
        (error as any).__binanceEmitted = true;
        return Promise.reject(error);
      }
      return response;
    },
    (error) => {
      const errorCode = error.response?.data?.code;
      if (error.response?.data?.msg && !SILENT_ERROR_CODES.includes(errorCode) && !error.__binanceEmitted) {
        error.__binanceEmitted = true;
        exchange.emitAndLogError(error.response.data.msg);
      }
      return Promise.reject(error);
    },
  );

  return xhr;
};
