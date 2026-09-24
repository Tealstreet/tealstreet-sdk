import type { HOWRegion } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';
import qs from 'qs';

import { createAxiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { markSkipRemoteSigning } from '../../remoteSigningAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { getBybitResponseMeta } from './bybit.response';
import { getBybitSiteHeaders } from './bybit.site';
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

const CLOCK_SYNC_RETRY_CONFIG_KEY = 'safeCexBybitClockSyncRetry';
export const BYBIT_ENV_PROBE_CONFIG_KEY = 'safeCexBybitEnvProbe';
export const BYBIT_ENV_MISMATCH_MESSAGE_PREFIX = 'Bybit API key environment mismatch';

type BybitEnvProbeResult =
  | { kind: 'mismatch'; diagnostic: string }
  | { kind: 'no-mismatch' }
  | { kind: 'probe-failed' };

// KLINE is fetched unsigned (headers are stripped below) and shares this
// client, so header presence is what distinguishes a credential-proving call.
const isSignedRequest = (config: { headers?: any } | undefined) => Boolean(config?.headers?.['X-BAPI-SIGN']);

// Public market data: tickers, instruments-info and kline. IP-metered by Bybit
// whether or not a signature rides along.
const isPublicEndpoint = (url?: string) => Boolean(url?.includes('/v5/market/'));

const isInstrumentsInfoRequest = (error: any) => error?.config?.url === ENDPOINTS.MARKETS;

const isBybitClockSyncRetMsg = (retMsg: string) => retMsg.toLowerCase().includes('recv');

const hasRetriedClockSync = (config: unknown) =>
  Boolean((config as Record<string, unknown> | undefined)?.[CLOCK_SYNC_RETRY_CONFIG_KEY]);

const isBybitEnvProbeRequest = (config: unknown) =>
  Boolean((config as Record<string, unknown> | undefined)?.[BYBIT_ENV_PROBE_CONFIG_KEY]);

export const shouldRetryBybitRequest = (error: any) => {
  const isNetwork = isNetworkError(error);
  const isErrSign = Boolean(
    error?.response?.data?.ret_msg?.includes('error sign!') || error?.response?.data?.retMsg?.includes('error sign!'),
  );
  if (!isNetwork) return isErrSign;
  return !isInstrumentsInfoRequest(error);
};

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const recvWindow = options?.extra?.recvWindow ?? RECV_WINDOW;
  const network = options.testnet ? 'testnet' : 'livenet';
  const bybitCompatibleExchange = exchange as BaseExchange & {
    getBybitRestBaseUrl?: (network: 'livenet' | 'testnet') => string;
  };
  const baseURL =
    bybitCompatibleExchange.getBybitRestBaseUrl?.(network) ?? (exchange.isDemo ? DEMO_URL[network] : BASE_URL[network]);
  const usesBybitSiteHeaders =
    exchange.name === 'bybit' || exchange.name === 'bybitinverse' || exchange.name === 'bybitspot';
  const bybitSiteHeaders = usesBybitSiteHeaders ? getBybitSiteHeaders(options) : {};
  const isConfiguredForDemo = exchange.name === 'bybitdemo' || exchange.isDemo;
  const expectedBybitBaseURL = isConfiguredForDemo ? DEMO_URL[network] : BASE_URL[network];
  const canProbeBybitEnvironment =
    network === 'livenet' &&
    (exchange.name === 'bybit' || exchange.name === 'bybitdemo') &&
    baseURL === expectedBybitBaseURL &&
    !(options as { bybitSiteId?: unknown }).bybitSiteId &&
    !options.remoteSigner &&
    Boolean(options.key && options.secret);
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
        ...bybitSiteHeaders,
      },
    },
    options,
    exchange,
  );
  let envProbePromise: Promise<BybitEnvProbeResult> | undefined;
  let envProbeResult: BybitEnvProbeResult | undefined;

  const getOppositeBybitEnvironmentProbe = () => {
    if (!canProbeBybitEnvironment) return undefined;

    if (isConfiguredForDemo) {
      return {
        baseURL: BASE_URL[network],
        keyEnvironment: 'live Bybit',
        configuredEnvironment: 'Bybit Demo Trading',
        remediation: 'Create a Demo Trading API key or switch the account to live Bybit.',
      };
    }

    return {
      baseURL: DEMO_URL[network],
      keyEnvironment: 'Bybit Demo Trading',
      configuredEnvironment: 'live Bybit',
      remediation: 'Create a live Bybit API key or switch the account to Bybit Demo Trading.',
    };
  };

  const probeOppositeBybitEnvironment = async (): Promise<BybitEnvProbeResult> => {
    const probe = getOppositeBybitEnvironmentProbe();
    if (!probe) return { kind: 'no-mismatch' };

    try {
      const { data } = await xhr.request({
        baseURL: probe.baseURL,
        url: ENDPOINTS.ACCOUNT,
        method: 'get',
        params: {},
        [BYBIT_ENV_PROBE_CONFIG_KEY]: true,
      } as any);
      const probeMeta = getBybitResponseMeta(data);

      if (!probeMeta.isSuccess) {
        return { kind: 'no-mismatch' };
      }

      return {
        kind: 'mismatch',
        diagnostic: `${BYBIT_ENV_MISMATCH_MESSAGE_PREFIX}: this key is valid for ${probe.keyEnvironment}, but this account is configured for ${probe.configuredEnvironment}. ${probe.remediation}`,
      };
    } catch {
      return { kind: 'probe-failed' };
    }
  };

  const getBybitEnvProbeResult = async () => {
    if (envProbeResult) return envProbeResult;
    envProbePromise ??= probeOppositeBybitEnvironment();
    envProbeResult = await envProbePromise;
    return envProbeResult;
  };

  const maybePrependBybitEnvMismatch = async (retCode: number | undefined, retMsg: string | undefined) => {
    if (retCode !== 10003 && retCode !== 401 && retCode !== 403) return retMsg;
    if (!retMsg || retMsg.includes(BYBIT_ENV_MISMATCH_MESSAGE_PREFIX)) return retMsg;

    const result = await getBybitEnvProbeResult();
    if (result.kind !== 'mismatch') return retMsg;

    return `${result.diagnostic} [${retMsg}]`;
  };

  // retry requests on network errors instead of throwing
  // and some signature errors
  retry(xhr, {
    retries: 3,
    retryCondition: shouldRetryBybitRequest,
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

    // Tag `nextConfig`, not `config` — every return below is built from it, so
    // a tag on `config` would be dropped by the spread above. Broader than the
    // kline check, which misses tickers and instruments-info; this only skips
    // the hosted signing round trip and never decides signing.
    if (isPublicEndpoint(nextConfig.url)) markSkipRemoteSigning(nextConfig);

    if (nextConfig.method === 'get' && nextConfig.url === ENDPOINTS.KLINE) {
      return { ...nextConfig, headers: new AxiosHeaders(bybitSiteHeaders) };
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
    const isEnvProbeResponse = isBybitEnvProbeRequest(response.config);

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
      if (isEnvProbeResponse) return response;
      if (isAuthStatus) {
        exchange.recordAuthFailure();
        response.data.retMsg = await maybePrependBybitEnvMismatch(response.status, response.data.retMsg);
      }
      return response;
    }

    if (isEnvProbeResponse) return response;

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

    if (responseMeta.retCode === 10003) {
      response.data.retMsg = await maybePrependBybitEnvMismatch(responseMeta.retCode, responseMeta.retMsg);
    }

    const finalResponseMeta = getBybitResponseMeta(response.data);
    if (response.data && finalResponseMeta.hasExplicitStatus && !finalResponseMeta.isSuccess) {
      const retMsg = finalResponseMeta.retMsg || '';
      if (isBybitClockSyncRetMsg(retMsg)) {
        if (!hasRetriedClockSync(response.config)) {
          await virtualClock.syncBybitClientServerTime(); // This method name stays the same as it's a common sync
          return xhr.request({
            ...response.config,
            [CLOCK_SYNC_RETRY_CONFIG_KEY]: true,
          } as typeof response.config);
        }
        exchange.emitAndLogError('Local time is out of sync after retry');
      } else if (finalResponseMeta.retCode !== undefined ? finalResponseMeta.retCode !== 0 : true) {
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
