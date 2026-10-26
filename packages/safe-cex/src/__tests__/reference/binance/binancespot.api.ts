import type { ExchangeOptions } from '../../../types';

import retry from 'axios-retry';
import qs from 'qs';

import { safeCexAxiosCreate } from '../../../safeCexAxiosCreate';
import { BaseExchange } from '../../../exchanges/base';
import { createBinanceRequestInterceptor } from '../../../exchanges/binance/binance.signing';
import { BASE_URL, RECV_WINDOW } from '../../../exchanges/binance/binancespot.types';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';
import {
  isBinanceStyleSignatureRejection,
  resendOnSignatureRejection,
  signatureRejectedResponse,
} from '../../../utils/signatureRejection';

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

  xhr.interceptors.request.use(createBinanceRequestInterceptor(options, exchange.name));

  // Last, so a resend is not put through any later interceptor twice.
  xhr.interceptors.response.use(
    (response) => resendOnSignatureRejection(xhr, response, isBinanceStyleSignatureRejection) ?? response,
  );

  return xhr;
};
