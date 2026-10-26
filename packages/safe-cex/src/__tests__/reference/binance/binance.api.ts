import type { ExchangeOptions } from '../../../types';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import qs from 'qs';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { markSkipRemoteSigning } from '../../../remoteSigningAdapter';
import { installTransportAdapter, safeCexAxiosCreate, safeCexAxiosCreateNoProxy } from '../../../safeCexAxiosCreate';
import { networkErrorOnRead, readsAndPosted } from '../../../utils/retryReadsOnly';
import {
  isBinanceStyleSignatureRejection,
  resendOnSignatureRejection,
  signatureRejectedResponse,
} from '../../../utils/signatureRejection';
import { BaseExchange } from '../../../exchanges/base';
import { createBinanceRequestInterceptor } from '../../../exchanges/binance/binance.signing';
import { BASE_URL, ENDPOINTS, RECV_WINDOW } from '../../../exchanges/binance/binance.types';

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
      // Binance uses big ints (orderId in particular). We need to replace them with strings to avoid JS rounding errors
      transformResponse: bigIntAsStringJsonParse,
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
      // Binance uses big ints (orderId in particular). We need to replace them with strings to avoid JS rounding errors
      transformResponse: bigIntAsStringJsonParse,
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

  xhr.interceptors.request.use(createBinanceRequestInterceptor(options, exchange.name));

  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name, 'bigInt');
  }

  // Add response interceptor for error handling
  // Binance can return errors with 200 status code (e.g., in batch order responses)
  // Error codes that are handled silently by calling code (don't emit to UI)
  const SILENT_ERROR_CODES = [
    -2011, // "Unknown order sent" - order already cancelled, handled in cancelOrders
    -2013, // "Order does not exist" - an expected answer to findOrderByClientOrderId
    -4059, // "No need to change position side" - idempotent position-mode change
    -5022, // Post-only rejection ("order would execute as taker") - expected during chaser retries
  ];

  xhr.interceptors.response.use(
    (response) => {
      const signatureResend = resendOnSignatureRejection(xhr, response, isBinanceStyleSignatureRejection);
      if (signatureResend) return signatureResend;

      // Check if response contains an error code (Binance error format: { code: negative number, msg: string })
      // Negative codes are errors (e.g., -2011 "Unknown order sent", -4130 "closePosition order exists")
      if (response.data && typeof response.data === 'object' && 'code' in response.data && response.data.code < 0) {
        const errorCode = response.data.code;
        const errorMsg = response.data.msg || `Binance Error: ${errorCode}`;
        // Only emit error to UI if it's not in the silent list
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
      // Handle errors from non-200 responses. A retried request's rejection comes back
      // through this handler twice (its own chain, then the original's): emit it once.
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
