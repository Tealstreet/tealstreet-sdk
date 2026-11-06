import type { ExchangeOptions } from '../../../types';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { add, divide1 } from '@tealstreet/safe-cex/src/utils/safe-math';
import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { BASE_URL } from '../../../exchanges/phemex/phemex.types';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';

export { BROKER_ID } from '../../../exchanges/phemex/phemex.types';

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const baseURL = BASE_URL[options.testnet ? 'testnet' : 'livenet'];

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      //timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      timeout: 20_000, // otherwise can't use leverage batch
      withCredentials: false,
    },
    options,
    exchange,
  );

  retry(xhr, { retries: 3, retryCondition: networkErrorOnRead() });

  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  xhr.interceptors.request.use((config) => {
    const nextConfig = { ...config };

    if (options.remoteSigner) {
      return nextConfig;
    }

    // Skip signing when no API key (public-only mode)
    if (!options.key || !options.secret) {
      return nextConfig;
    }

    // TODO: Audit whether HOW proxy re-serialization can break this signature (see bitmex.api.ts).
    const params = config.params ? decodeURIComponent(qs.stringify(config.params)) : null;
    const data = config.data ? JSON.stringify(config.data) : null;

    const timestamp = +virtualClock.getCurrentTime();
    const expiryString = String(Math.round(divide1(add(timestamp, 60 * 1000), 1000)));
    const toSign = [config.url, params ? params : '', expiryString];

    if (data) {
      toSign.push(data);
    }

    const signature = createHmac('sha256', options.secret).update(toSign.join('')).digest('hex');

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'Content-Type': 'application/json',
      'X-PHEMEX-ACCESS-TOKEN': options.key,
      'X-PHEMEX-REQUEST-EXPIRY': expiryString,
      'X-PHEMEX-REQUEST-SIGNATURE': signature,
    });

    return { ...nextConfig, headers };
  });

  return xhr;
};
//['/g-accounts/accountPositions?currency=USDT', 1708872602]
