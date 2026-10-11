import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import qs from 'qs';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { markSkipRemoteSigning } from '../../remoteSigningAdapter';
import { installTransportAdapter, safeCexAxiosCreate, safeCexAxiosCreateNoProxy } from '../../safeCexAxiosCreate';
import { networkErrorOnRead, readsAndPosted } from '../../utils/retryReadsOnly';
import { BaseExchange } from '../base';
import { signBinanceRequest } from './binance.signing';
import { BASE_URL, ENDPOINTS, PUBLIC_ENDPOINTS, RECV_WINDOW } from './binance.types';

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
  retry(xhr, { retries: 3, retryCondition: networkErrorOnRead(readsAndPosted(ENDPOINTS.LISTEN_KEY)) });

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
    installTransportAdapter(xhr, options, exchange.name, 'bigInt');
  }

  // Add response interceptor for error handling
  // Binance can return errors with 200 status code (e.g., in batch order responses)
  // Error codes that are handled silently by calling code (don't emit to UI)
  const SILENT_ERROR_CODES = [
    -2011, // "Unknown order sent" - order already cancelled, handled in cancelOrders
    -4059, // "No need to change position side" - idempotent position-mode change
    -5022, // Post-only rejection ("order would execute as taker") - expected during chaser retries
  ];

  xhr.interceptors.response.use(
    (response) => {
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
        return Promise.reject(error);
      }
      return response;
    },
    (error) => {
      // Handle errors from non-200 responses
      const errorCode = error.response?.data?.code;
      if (error.response?.data?.msg && !SILENT_ERROR_CODES.includes(errorCode)) {
        exchange.emitAndLogError(error.response.data.msg);
      }
      return Promise.reject(error);
    },
  );

  return xhr;
};
