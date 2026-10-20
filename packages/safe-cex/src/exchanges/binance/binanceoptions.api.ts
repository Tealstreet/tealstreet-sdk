import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import qs from 'qs';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { markSkipRemoteSigning } from '../../remoteSigningAdapter';
import { safeCexAxiosCreate } from '../../safeCexAxiosCreate';
import { networkErrorOnRead } from '../../utils/retryReadsOnly';
import {
  isBinanceStyleSignatureRejection,
  resendOnSignatureRejection,
  signatureRejectedResponse,
} from '../../utils/signatureRejection';
import { BaseExchange } from '../base';
import { signBinanceRequest } from './binance.signing';
import { BASE_URL, PUBLIC_ENDPOINTS, RECV_WINDOW } from './binanceoptions.types';

// -2011 "Unknown order sent" and -2013 "Order does not exist" are expected answers to a cancel or lookup.
const SILENT_ERROR_CODES = [-2011, -2013];

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const xhr = safeCexAxiosCreate(
    {
      baseURL: BASE_URL[options.testnet ? 'testnet' : 'livenet'],
      timeout: 20_000,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { arrayFormat: 'repeat' }),
      },
      headers: {
        'X-MBX-APIKEY': options.key,
        'Content-Type': 'application/json, charset=utf-8',
      },
      // Option order IDs exceed Number.MAX_SAFE_INTEGER.
      transformResponse: bigIntAsStringJsonParse,
    },
    options,
    exchange,
  );

  retry(xhr, {
    retries: 3,
    retryCondition: (error) =>
      networkErrorOnRead()(error) || signatureRejectedResponse(isBinanceStyleSignatureRejection)(error),
  });

  xhr.interceptors.request.use((config) => {
    if (PUBLIC_ENDPOINTS.includes(`${config.url}`)) {
      return markSkipRemoteSigning({
        ...config,
        headers: AxiosHeaders.from({ 'Content-Type': 'application/json, charset=utf-8' }),
      });
    }
    return signBinanceRequest(config, options, exchange.name, options?.extra?.recvWindow ?? RECV_WINDOW);
  });

  xhr.interceptors.response.use(
    (response) => {
      const signatureResend = resendOnSignatureRejection(xhr, response, isBinanceStyleSignatureRejection);
      if (signatureResend) return signatureResend;

      const code = Number(response.data?.code);
      if (response.data && typeof response.data === 'object' && code < 0) {
        const message = response.data.msg || `Binance Options error ${code}`;
        if (!SILENT_ERROR_CODES.includes(code)) exchange.emitAndLogError(message);
        const error = new Error(message);
        Object.assign(error, { code, response, __binanceEmitted: true });
        return Promise.reject(error);
      }
      return response;
    },
    (error) => {
      const code = error.response?.data?.code;
      if (error.response?.data?.msg && !SILENT_ERROR_CODES.includes(code) && !error.__binanceEmitted) {
        error.__binanceEmitted = true;
        exchange.emitAndLogError(error.response.data.msg);
      }
      return Promise.reject(error);
    },
  );

  return xhr;
};
