import type { HOWRegion } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';
import qs from 'qs';

import { createAxiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { getBybitResponseMeta } from './bybit.response';
import { BASE_URL, BROKER_ID, DEMO_URL, ENDPOINTS, RECV_WINDOW } from './bybit.types';

// Credential failures that will not self-heal on retry, so the tick backoff
// should treat them like a 401. Deliberately excludes 10004 ("error sign"),
// which is usually clock drift and is already retried.
const BYBIT_AUTH_RET_CODES = new Set([
  10003, // API key is invalid
  10005, // permission denied for current apikey
  10010, // unmatched IP
  33004, // api key has expired
]);

// KLINE is fetched unsigned (headers are stripped below) and shares this
// client, so header presence is what distinguishes a credential-proving call.
const isSignedRequest = (config: { headers?: any } | undefined) => Boolean(config?.headers?.['X-BAPI-SIGN']);

const isErrorSignature = (error: any) => {
  const isNetwork = isNetworkError(error);
  const isErrSign =
    error?.response?.data?.ret_msg?.includes('error sign!') || error?.response?.data?.retMsg?.includes('error sign!');
  return isNetwork || isErrSign;
};

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const recvWindow = options?.extra?.recvWindow ?? RECV_WINDOW;
  const baseURL = exchange.name.includes('demo')
    ? DEMO_URL[options.testnet ? 'testnet' : 'livenet']
    : BASE_URL[options.testnet ? 'testnet' : 'livenet'];
  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: recvWindow,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params),
      },
      headers: {
        'X-Referer': BROKER_ID,
        'X-BAPI-RECV-WINDOW': recvWindow,
        'Content-Type': 'application/json, charset=utf-8',
      },
    },
    options,
    exchange,
  );

  // retry requests on network errors instead of throwing
  // and some signature errors
  retry(xhr, {
    retries: 3,
    retryCondition: isErrorSignature,
  });

  // Add requestId, skip auth for public endpoints, and sign requests.
  // Must be a SINGLE interceptor because axios runs request interceptors in
  // LIFO order — splitting into two would cause signing to run before requestId
  // is added, producing a signature mismatch.
  xhr.interceptors.request.use((config) => {
    const nextConfig = {
      ...config,
      params: {
        ...config.params,
        requestId: nanoid(),
      },
    };

    if (nextConfig.method === 'get' && nextConfig.url === ENDPOINTS.KLINE) {
      return { ...nextConfig, headers: new AxiosHeaders({}) };
    }

    // Skip signing when remote signer handles it
    if (options.remoteSigner) {
      return nextConfig;
    }

    // Skip signing when no API key (public-only mode)
    if (!options.key || !options.secret) {
      return nextConfig;
    }

    // TODO: Audit whether HOW proxy re-serialization can break this signature (see bitmex.api.ts).
    const data = nextConfig.method === 'get' ? qs.stringify(nextConfig.params) : JSON.stringify(nextConfig.data);

    const timestamp = virtualClock.getCurrentTimeMs();
    const signature = createHmac('sha256', options.secret)
      .update([timestamp, options.key, recvWindow, data].join(''))
      .digest('hex');

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'X-BAPI-SIGN': signature,
      'X-BAPI-API-KEY': options.key,
      'X-BAPI-TIMESTAMP': timestamp,
    });

    return { ...nextConfig, headers };
  });

  if (options.tealstreetTunnel) {
    const region = options.proxyRegion as HOWRegion | undefined;
    setInnerAdapterForRemoteSigning(xhr, createAxiosHttpOverWebsocketAdapter({ region }));
  }

  xhr.interceptors.response.use(async (response) => {
    const responseMeta = getBybitResponseMeta(response.data);

    // The HOW adapter resolves non-2xx without axios' settle(), so an auth
    // failure arrives here as a *success* carrying `{"raw_response":""}` — no
    // retCode, no retMsg. Callers then hit `v(data,'retMsg') !== 'OK'` and
    // emitAndLogError(undefined), which base.ts renders as the contentless
    // 'Unknown Exchange Error' (89 of them in one 30s e2e window). Synthesize
    // the status so every caller reports something real and the health
    // monitor's auth_invalid classification can actually match.
    if (!responseMeta.hasExplicitStatus && response.status >= 400) {
      const isAuthStatus = response.status === 401 || response.status === 403;
      // Spell out the reason rather than a bare `HTTP 401`: the tunnel frame
      // carries no statusText, and both the health monitor's `auth_invalid`
      // patterns and the e2e toast assertion match on words like
      // "unauthorized"/"api key", not on the numeric status.
      const reason = isAuthStatus
        ? `${response.status === 401 ? 'Unauthorized' : 'Forbidden'} — API key rejected`
        : response.statusText;
      response.data = {
        ...(response.data ?? {}),
        retCode: response.status,
        retMsg: `HTTP ${response.status}${reason ? ` ${reason}` : ''}`,
      };
      if (isAuthStatus) exchange.recordAuthFailure();
      return response;
    }

    // Bybit also returns credential failures inside a normal envelope, both on
    // 4xx and on HTTP 200, so status alone under-counts the streak.
    if (responseMeta.retCode !== undefined && BYBIT_AUTH_RET_CODES.has(responseMeta.retCode)) {
      exchange.recordAuthFailure();
    } else if (responseMeta.isSuccess && isSignedRequest(response.config)) {
      // Only a signed request proves the credentials work — KLINE is
      // deliberately unsigned and shares this client, so a chart fetch during
      // backoff must not reset the streak.
      exchange.recordAuthSuccess();
    }

    if (response.data && responseMeta.hasExplicitStatus && !responseMeta.isSuccess) {
      const retMsg = responseMeta.retMsg || '';
      if (retMsg.includes('recv')) {
        exchange.emitAndLogError('Local time is out of sync, updating...');
        await virtualClock.syncBybitClientServerTime(); // This method name stays the same as it's a common sync
      } else if (responseMeta.retCode !== undefined ? responseMeta.retCode !== 0 : true) {
        if (response.data?.raw_response?.includes?.('There might be too much traffic or a configuration error')) {
          exchange.emitAndLogError('Server was unavailable for a request, retrying...');
        }
        console.error(`${exchange.name} error`, response.data);
      }
    }
    return response;
  });

  return xhr;
};
