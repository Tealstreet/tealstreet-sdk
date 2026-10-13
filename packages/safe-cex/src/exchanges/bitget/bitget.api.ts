import type { ExchangeOptions } from '../../types';
import type { IsSignatureRejection } from '../../utils/signatureRejection';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { safeCexAxiosCreate, installTransportAdapter } from '../../safeCexAxiosCreate';
import { v } from '../../utils/get-key';
import { appendHttpFailure } from '../../utils/httpFailure';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { BASE_URL, BROKER_ID, ENDPOINTS, RECV_WINDOW } from './bitget.types';
import { networkErrorOnRead } from '../../utils/retryReadsOnly';
import { resendOnSignatureRejection, signatureRejectedResponse } from '../../utils/signatureRejection';

// Bitget error codes that must NOT surface as raw user-facing toasts. The
// `_detectAccountType` probe in bitget.exchange.ts intentionally calls a
// v3 endpoint expecting `40084` ("Classic Account mode") on Classic
// accounts. `40085` is handled by callers as a routing signal: supported
// linear variants promote to UTA, while unsupported variants surface scoped
// variant guidance instead of the exchange's credential-looking envelope.
const SILENT_ERROR_CODES = new Set<string>(['40084', '40085']);

export const isBitgetUtaStrategyOrderNotFound = (message: unknown): boolean =>
  typeof message === 'string' &&
  message.toLowerCase().includes('corresponding strategy id could not be found');

const shouldSilenceBitgetResponseError = (code: string, message: unknown, url?: string): boolean =>
  SILENT_ERROR_CODES.has(code) ||
  (typeof message === 'string' && message.toLowerCase().includes('environment is incorrect')) ||
  (url?.includes(ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER) === true && isBitgetUtaStrategyOrderNotFound(message));

const shouldSuppressValidationErrorEmission = (exchange: BaseExchange): boolean =>
  (exchange as { suppressBitgetValidationErrorEmission?: boolean }).suppressBitgetValidationErrorEmission === true;

export const isBitgetSignatureRejection: IsSignatureRejection = (data) =>
  String(data?.code) === '40009' || /sign signature error/i.test(String(data?.msg ?? ''));

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const recvWindow = options?.extra?.recvWindow ?? RECV_WINDOW;

  const baseURL = BASE_URL;

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: recvWindow,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params),
      },
      headers: {
        'ACCESS-KEY': options.key,
        'ACCESS-PASSPHRASE': options.passphrase,
        'X-CHANNEL-API-CODE': BROKER_ID,
        'Content-Type': 'application/json',
      },
    },
    options,
    exchange,
  );

  retry(xhr, {
    retries: 3,
    retryCondition: (error) => networkErrorOnRead()(error) || signatureRejectedResponse(isBitgetSignatureRejection)(error),
  });

  xhr.interceptors.request.use((config) => {
    const nextConfig = { ...config };

    // Skip signing when remote signer handles it
    if (options.remoteSigner) {
      return nextConfig;
    }

    // Skip signing when no API key (public-only mode)
    if (!options.key || !options.secret) {
      return nextConfig;
    }

    const timestamp = virtualClock.getCurrentTimeMs();
    const method = config.method?.toUpperCase() ?? 'GET';

    // TODO: Audit whether HOW proxy re-serialization can break this signature (see bitmex.api.ts).
    let signingUrl = config.url;
    if (config.params) {
      if (config.params.symbol && !config.url?.includes(ENDPOINTS.KLINE)) {
        config.params.symbol = config.params.symbol.replace('$', '');
      }
      const signingQuery = qs.stringify(config.params, { encode: false });
      signingUrl = `${signingUrl}?${signingQuery}`;
    }

    // A replayed config carries the body axios already serialized: sign those bytes.
    const stringifiedBody =
      typeof config.data === 'string'
        ? config.data
        : config.data
          ? toJsonString({
              ...config.data,
              ...(config.data.symbol ? { symbol: config.data.symbol.replace('$', '') } : {}),
            })
          : '';
    const toSign = [timestamp, method, signingUrl, stringifiedBody].join('');
    const signature = createHmac('sha256', options.secret).update(toSign).digest().toString('base64');

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'ACCESS-SIGN': signature,
      'ACCESS-TIMESTAMP': timestamp,
    });

    return { ...nextConfig, headers };
  });

  const seenResponses = new WeakSet<object>();
  xhr.interceptors.response.use(async (response) => {
    const signatureResend = resendOnSignatureRejection(xhr, response, isBitgetSignatureRejection);
    if (signatureResend) return signatureResend;
    // An axios-retry resend reaches here twice (its chain, then the original's): emit once.
    if (seenResponses.has(response)) return response;
    seenResponses.add(response);

    if (response.data && !v(response.data, 'data') && v(response.data, 'code') !== '00000') {
      const code = String(v(response.data, 'code') ?? '');
      const message = v(response.data, 'msg');
      if (
        message &&
        !shouldSuppressValidationErrorEmission(exchange) &&
        !shouldSilenceBitgetResponseError(code, message, response.config?.url)
      ) {
        exchange.emitAndLogError(appendHttpFailure(String(message), response));
      }
    }
    return response;
  });

  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  return xhr;
};

function toJsonString(obj: object): string | null {
  if (obj == null) {
    return null;
  }

  let json = JSON.stringify(obj);
  Object.keys(obj)
    .filter((key) => key[0] === '_')
    .forEach((key) => {
      json = json.replace(key, key.substring(1));
    });
  const reg = new RegExp('"_', 'g');
  return json.replace(reg, '"');
}
