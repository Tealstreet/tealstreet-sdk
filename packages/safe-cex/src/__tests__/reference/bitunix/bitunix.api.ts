import type { ExchangeOptions } from '../../../types';
import type { IsSignatureRejection } from '../../../utils/signatureRejection';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import qs from 'qs';

import { uuid } from '@tealstreet/safe-cex/src/utils/uuid';
import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import { v } from '../../../utils/get-key';
import { appendHttpFailure } from '../../../utils/httpFailure';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { BASE_URL } from './bitunix.types';
import { sha256Hex } from './bitunix.utils';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';
import { resendOnSignatureRejection, signatureRejectedResponse } from '../../../utils/signatureRejection';

export const isBitunixSignatureRejection: IsSignatureRejection = (data) =>
  Number(data?.code) === 10007 || /sign signature error/i.test(String(data?.msg ?? ''));

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const xhr = safeCexAxiosCreate(
    {
      baseURL: BASE_URL.livenet,
      //timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      timeout: 20_000, // otherwise can't use leverage batch
      headers: options.testnet ? { 'x-simulated-trading': 1 } : {},
    },
    options,
    exchange,
  );

  retry(xhr, {
    retries: 3,
    retryCondition: (error) => networkErrorOnRead()(error) || signatureRejectedResponse(isBitunixSignatureRejection)(error),
  });

  if (!options.publicDataOnly) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  xhr.interceptors.request.use((config) => {
    if (config?.url?.includes('public')) {
      return config;
    }

    const nextConfig = { ...config };

    const params = config.params
      ? decodeURIComponent(
          qs.stringify(config.params, {
            arrayFormat: 'repeat',
            sort: (a, b) => a.localeCompare(b),
          }),
        )
      : null;
    // A replayed config carries the body axios already serialized: sign those bytes.
    const data = typeof config.data === 'string' ? config.data : config.data ? JSON.stringify(config.data) : null;

    const timestamp = +virtualClock.getCurrentTime();
    const nonce = uuid();
    const toSign = [nonce, timestamp, options.key, params?.replace(/[=&]/g, '') || ''];

    if (data) {
      toSign.push(data);
    }

    const digest = sha256Hex(toSign.join(''));

    // console.log('toSign', toSign.join(''));
    // console.log('digest', digest);

    const signature = sha256Hex(digest + options.secret);
    //console.log('signature', signature);

    // const signature = this.binaryToBase64(
    //   this.encode(this.hmac(this.encode(auth), this.encode(this.secret), 'sha256', 'hex')),
    // );

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'api-key': options.key,
      sign: signature,
      nonce: nonce,
      timestamp: timestamp,
    });

    return { ...nextConfig, headers };
  });

  const seenResponses = new WeakSet<object>();
  xhr.interceptors.response.use(async (response) => {
    const signatureResend = resendOnSignatureRejection(xhr, response, isBitunixSignatureRejection);
    if (signatureResend) return signatureResend;
    // An axios-retry resend reaches here twice (its chain, then the original's): emit once.
    if (seenResponses.has(response)) return response;
    seenResponses.add(response);

    const config = response?.config as any;
    const getHeaderValue = (name: string) => {
      const headers = config?.headers;
      if (!headers) {
        return undefined;
      }
      if (typeof headers.get === 'function') {
        return headers.get(name);
      }
      return headers[name] ?? headers[name.toLowerCase()] ?? headers[name.toUpperCase()];
    };

    const suppressExchangeError =
      config?.__suppressExchangeError === true ||
      String(getHeaderValue('x-tealstreet-suppress-exchange-error') ?? '').toLowerCase() === '1' ||
      String(getHeaderValue('x-tealstreet-suppress-exchange-error') ?? '').toLowerCase() === 'true';
    const suppressExchangeRateLimitError =
      config?.__suppressExchangeRateLimitError === true ||
      String(getHeaderValue('x-tealstreet-suppress-exchange-rate-limit-error') ?? '').toLowerCase() === '1' ||
      String(getHeaderValue('x-tealstreet-suppress-exchange-rate-limit-error') ?? '').toLowerCase() === 'true';

    if (suppressExchangeError) {
      return response;
    }

    if (response.data && !v(response.data, 'data')) {
      const message = v(response.data, 'msg');
      if (message) {
        const isRateLimitMessage = `${message}`.toLowerCase().includes('request too frequently');
        if (!(suppressExchangeRateLimitError && isRateLimitMessage)) {
          exchange.emitAndLogError(appendHttpFailure(`${message}`, response));
        }
      }
    }
    // if (response.data.code && response.data.code !== 0) {
    //   const msg = `[${response.data.code}] ${response.data.msg}`;
    //   exchange.emitAndLogError(msg);
    //   throw new Error(msg);
    // }
    return response;
  });

  return xhr;
};
