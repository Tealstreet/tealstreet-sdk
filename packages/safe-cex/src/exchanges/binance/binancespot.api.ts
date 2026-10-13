import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import qs from 'qs';

import { markSkipRemoteSigning } from '../../remoteSigningAdapter';
import { safeCexAxiosCreate } from '../../safeCexAxiosCreate';
import { BaseExchange } from '../base';
import { signBinanceRequest } from './binance.signing';
import { BASE_URL, PUBLIC_ENDPOINTS, RECV_WINDOW } from './binancespot.types';
import { networkErrorOnRead } from '../../utils/retryReadsOnly';
import {
  isBinanceStyleSignatureRejection,
  resendOnSignatureRejection,
  signatureRejectedResponse,
} from '../../utils/signatureRejection';

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
    },
    options,
    exchange,
  );

  // retry requests on network errors instead of throwing
  retry(xhr, {
    retries: 3,
    retryCondition: (error) =>
      networkErrorOnRead()(error) || signatureRejectedResponse(isBinanceStyleSignatureRejection)(error),
  });

  xhr.interceptors.request.use((config) => {
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

  // Last, so a resend is not put through any later interceptor twice.
  xhr.interceptors.response.use(
    (response) => resendOnSignatureRejection(xhr, response, isBinanceStyleSignatureRejection) ?? response,
  );

  return xhr;
};
