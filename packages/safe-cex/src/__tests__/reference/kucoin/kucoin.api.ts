import type { ExchangeOptions } from '../../../types';
import type { IsSignatureRejection } from '../../../utils/signatureRejection';

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
import { addKucoinBrokerHeaders, getKucoinRequestTimestamp } from './kucoin.broker';
import { BASE_URL, BROKER_ID, ENDPOINTS, RECV_WINDOW } from './kucoin.types';

const stringifyQuery = (params: Record<string, any>, encode: boolean) => qs.stringify(params, { encode });

// Shared with KuCoin spot.
export const isKucoinSignatureRejection: IsSignatureRejection = (data) =>
  String(data?.code) === '400005' || /invalid kc-api-sign/i.test(String(data?.msg ?? ''));

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const recvWindow = options?.extra?.recvWindow ?? RECV_WINDOW;
  const secret = options.secret || '';
  const passphrase = options.passphrase || '';
  const shouldAuthenticate = !options.publicDataOnly;

  const baseURL = BASE_URL;

  // For API key-V2.0, encrypt passphrase with HMAC-sha256 via API-Secret and then base64 encode
  const encryptedPassphrase = createHmac('sha256', secret).update(passphrase).digest('base64');

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: recvWindow,
      paramsSerializer: {
        serialize: (params) => stringifyQuery(params, true),
      },
      headers: {
        'X-CHANNEL-API-CODE': BROKER_ID,
        'Content-Type': 'application/json',
        ...(shouldAuthenticate
          ? {
              'KC-API-KEY': options.key,
              'KC-API-PASSPHRASE': encryptedPassphrase,
              'KC-API-KEY-VERSION': '2',
            }
          : {}),
      },
    },
    options,
    exchange,
  );

  retry(xhr, {
    retries: 3,
    retryCondition: (error) =>
      networkErrorOnRead(readsAndPosted(ENDPOINTS.GET_PUBLIC_WS_TOKEN, ENDPOINTS.GET_PRIVATE_WS_TOKEN))(error) ||
      signatureRejectedResponse(isKucoinSignatureRejection)(error),
  });

  if (shouldAuthenticate) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  xhr.interceptors.request.use((config) => {
    const nextConfig = { ...config };

    if (!shouldAuthenticate) {
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
      const query = stringifyQuery(config.params, false);
      url = `${url}?${query}`;
    }

    const stringifiedBody = typeof config.data === 'string' ? config.data : config.data ? JSON.stringify(config.data) : '';
    const toSign = [timestamp, method, url, stringifiedBody].join('');

    const signature = createHmac('sha256', secret).update(toSign).digest('base64');

    const headers = addKucoinBrokerHeaders(headersWithBroker, options, timestamp);
    headers.set('KC-API-SIGN', signature);
    headers.set('KC-API-TIMESTAMP', timestamp);
    headers.set('KC-API-KEY-VERSION', '2');

    return { ...nextConfig, headers };
  });

  // Last, and over the tunnel any status resolves.
  xhr.interceptors.response.use(
    (response) => resendOnSignatureRejection(xhr, response, isKucoinSignatureRejection) ?? response,
  );

  return xhr;
};
