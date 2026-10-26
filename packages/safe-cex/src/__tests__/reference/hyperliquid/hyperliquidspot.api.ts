import type { ExchangeOptions } from '../../../types';

import retry from 'axios-retry';
import qs from 'qs';

import { markSkipRemoteSigning } from '../../../remoteSigningAdapter';
import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import { isSigningCaptureRequest } from '../../../utils/signingCapture';
import { isHyperliquidRead } from './hyperliquidRetry';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';
import { BaseExchange } from '../../../exchanges/base';
import { getHyperliquidRestRequestWeight, hyperliquidRestRateLimiter } from './hyperliquid.rateLimiter';
import { BASE_URLS, ENDPOINTS } from './hyperliquidspot.types';

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const baseURL = BASE_URLS[options.testnet ? 'TESTNET' : 'PRODUCTION'];

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: options?.extra?.recvWindow,
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
  retry(xhr, { retries: 3, retryCondition: networkErrorOnRead(isHyperliquidRead) });

  if (!options.publicDataOnly) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  xhr.interceptors.request.use((config) => {
    const nextConfig = { ...config };
    // This interceptor never signs anything, so a hosted `/info` read costs a
    // round trip to the device for a signature that is never produced.
    if (nextConfig.url === ENDPOINTS.INFO) markSkipRemoteSigning(nextConfig);
    // const timestamp = virtualClock.getCurrentTimeMs();

    // const data = config.data || config.params || {};
    // data.timestamp = timestamp;
    // data.recvWindow = options?.extra?.recvWindow;
    //
    // const asString = qs.stringify(data, { arrayFormat: 'repeat' });
    // data.signature = createHmac('sha256', options.secret).update(asString).digest('hex');
    //nextConfig.params = data;

    // remove data from POST/PUT/DELETE requests
    // Binance API takes data as query params
    //return omit(nextConfig, 'data');
    return nextConfig;
  });

  xhr.interceptors.request.use(async (config) => {
    if (isSigningCaptureRequest(config)) return config;
    await hyperliquidRestRateLimiter.waitForToken(getHyperliquidRestRequestWeight(config));
    return config;
  });

  xhr.interceptors.response.use(async (response) => {
    if (response.data?.status === 'err') {
      const msg = `${response.data?.response}`;
      exchange.emitAndLogError(msg);
      throw new Error(msg);
    }
    if (response.data?.response?.data?.status === 'err') {
      const msg = `${response.data?.response?.data?.response}`;
      exchange.emitAndLogError(msg);
      throw new Error(msg);
    }
    if (response.data?.statuses && response.data?.statuses.some((status: any) => Boolean(status.error))) {
      const errors = response.data?.statuses
        .filter((status: any) => Boolean(status.error))
        .map((status: any) => status.error)
        .join(', ');
      exchange.emitAndLogError(errors);
      throw new Error(errors);
    }
    if (
      response.data?.response?.data?.statuses &&
      response.data?.response?.data?.statuses.some((status: any) => Boolean(status.error))
    ) {
      const errors = response.data?.response?.data?.statuses
        .filter((status: any) => Boolean(status.error))
        .map((status: any) => status.error)
        .join(', ');
      exchange.emitAndLogError(errors);
      throw new Error(errors);
    }

    return response;
  });

  return xhr;
};
