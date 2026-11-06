import type { ExchangeOptions } from '../../../types';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { installTransportAdapter, safeCexAxiosCreate } from '../../../safeCexAxiosCreate';
import { networkErrorOnRead, readsAndPosted } from '../../../utils/retryReadsOnly';
import { resendOnSignatureRejection, signatureRejectedResponse } from '../../../utils/signatureRejection';
import { isSigningCaptureRequest } from '../../../utils/signingCapture';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { isKucoinSignatureRejection } from './kucoin.api';
import { addKucoinBrokerHeaders, getKucoinRequestTimestamp } from './kucoin.broker';
import {
  KUCOIN_SPOT_BASE_URL,
  KUCOIN_SPOT_BROKER_ID,
  KUCOIN_SPOT_ENDPOINTS,
  KUCOIN_SPOT_PUBLIC_ENDPOINTS,
  KUCOIN_SPOT_RECV_WINDOW,
} from './kucoinspot.types';

const isPublicEndpoint = (url?: string) => {
  if (!url) return false;
  return KUCOIN_SPOT_PUBLIC_ENDPOINTS.some((endpoint) => url.startsWith(endpoint));
};

export const createKucoinSpotAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const recvWindow = options?.extra?.recvWindow ?? KUCOIN_SPOT_RECV_WINDOW;
  const secret = options.secret || '';
  const passphrase = options.passphrase || '';
  const encryptedPassphrase = createHmac('sha256', secret).update(passphrase).digest('base64');

  const xhr = safeCexAxiosCreate(
    {
      baseURL: KUCOIN_SPOT_BASE_URL,
      timeout: recvWindow,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params),
      },
      headers: {
        'X-CHANNEL-API-CODE': KUCOIN_SPOT_BROKER_ID,
        'Content-Type': 'application/json',
      },
    },
    options,
    exchange,
  );

  retry(xhr, {
    retries: 3,
    retryCondition: (error) =>
      networkErrorOnRead(
        readsAndPosted(KUCOIN_SPOT_ENDPOINTS.GET_PUBLIC_WS_TOKEN, KUCOIN_SPOT_ENDPOINTS.GET_PRIVATE_WS_TOKEN),
      )(error) || signatureRejectedResponse(isKucoinSignatureRejection)(error),
  });

  if (!options.publicDataOnly) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  xhr.interceptors.request.use((config) => {
    const nextConfig = { ...config };
    if (isPublicEndpoint(config.url) || options.publicDataOnly) {
      return nextConfig;
    }

    // Only the device's signing pass keeps the server's timestamp, which the broker
    // signature was made over; a replay must not reuse the previous attempt's.
    const incomingTimestamp = isSigningCaptureRequest(config) ? getKucoinRequestTimestamp(nextConfig.headers) : undefined;
    const timestamp = incomingTimestamp ?? `${virtualClock.getCurrentTimeMs()}`;
    const headersWithBroker = addKucoinBrokerHeaders(new AxiosHeaders(nextConfig.headers), options, timestamp);
    // Set even without a broker config, so a hosted replay cannot hand the device the old one.
    headersWithBroker.set('KC-API-TIMESTAMP', timestamp);

    if (options.remoteSigner) {
      return { ...nextConfig, headers: headersWithBroker };
    }

    const method = config.method?.toUpperCase() ?? 'GET';

    let url = config.url;
    if (config.params) {
      const query = qs.stringify(config.params);
      url = `${url}?${query}`;
    }

    const stringifiedBody = typeof config.data === 'string' ? config.data : config.data ? JSON.stringify(config.data) : '';
    const signature = createHmac('sha256', secret)
      .update([timestamp, method, url, stringifiedBody].join(''))
      .digest('base64');

    const headers = addKucoinBrokerHeaders(headersWithBroker, options, timestamp);
    headers.set('KC-API-KEY', options.key);
    headers.set('KC-API-SIGN', signature);
    headers.set('KC-API-TIMESTAMP', timestamp);
    headers.set('KC-API-PASSPHRASE', encryptedPassphrase);
    headers.set('KC-API-KEY-VERSION', '2');

    return { ...nextConfig, headers };
  });

  // Last, and over the tunnel any status resolves.
  xhr.interceptors.response.use(
    (response) => resendOnSignatureRejection(xhr, response, isKucoinSignatureRejection) ?? response,
  );

  return xhr;
};
