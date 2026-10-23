import type { HOWRegion } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import type { IsSignatureRejection } from '../../utils/signatureRejection';
import type { ExchangeOptions } from '../../types';

import retry from 'axios-retry';
import qs from 'qs';

import { safeCexAxiosCreate, installTransportAdapter } from '../../safeCexAxiosCreate';
import { isNodeOrBunRuntime } from '../../utils/runtime';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { getBybitResponseMeta } from './bybit.response';
import { getBybitSiteHeaders } from './bybit.site';
import { createBybitRequestInterceptor } from './bybit.signing';
import { BASE_URL, BROKER_ID, DEMO_URL, ENDPOINTS, RECV_WINDOW } from './bybit.types';
import { networkErrorOnRead } from '../../utils/retryReadsOnly';
import { resendOnSignatureRejection, signatureRejectedResponse } from '../../utils/signatureRejection';

// Credential failures that will not self-heal on retry, so the tick backoff
// should treat them like a 401.
const BYBIT_AUTH_RET_CODES = new Set([
  10003, // API key is invalid
  // A wrong SECRET fails here, not at 10003 — the key is recognised and the
  // signature it produced is not. Without this the streak never moves, so the
  // CLI rendered a table of zeros and exited 0 on a mistyped secret. Measured
  // against api-demo.bybit.com with a truncated secret: retCode 10004.
  10004, // error sign / signature mismatch
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

const isInstrumentsInfoRequest = (error: any) => error?.config?.url === ENDPOINTS.MARKETS;

const isBybitClockSyncRetMsg = (retMsg: string) => retMsg.toLowerCase().includes('recv');

// Bybit names its own clock in the rejection ("server_timestamp[1791518966503]"),
// which is exact and needs no request.
export const parseBybitServerTimestamp = (retMsg: string): number | undefined => {
  const match = /server_timestamp\[(\d{12,})\]/.exec(retMsg);
  const timestamp = match ? Number(match[1]) : NaN;
  return Number.isFinite(timestamp) ? timestamp : undefined;
};

const hasRetriedClockSync = (config: unknown) =>
  Boolean((config as Record<string, unknown> | undefined)?.[CLOCK_SYNC_RETRY_CONFIG_KEY]);

// Bybit has said both "error sign!" and "Error sign, please check your signature
// generation algorithm"; 10004 is the code behind either.
const BYBIT_SIGN_REJECTION = /error sign/i;

export const isBybitSignRejection: IsSignatureRejection = (data) => {
  const { retCode, retMsg } = getBybitResponseMeta(data);
  return retCode === 10004 || BYBIT_SIGN_REJECTION.test(retMsg ?? '');
};

const isBybitEnvProbeRequest = (config: unknown) =>
  Boolean((config as Record<string, unknown> | undefined)?.[BYBIT_ENV_PROBE_CONFIG_KEY]);

export const shouldRetryBybitRequest = (error: any) => {
  const isNetwork = networkErrorOnRead()(error);
  if (!isNetwork) return signatureRejectedResponse(isBybitSignRejection)(error);
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
        'Content-Type': 'application/json',
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
  xhr.interceptors.request.use(createBybitRequestInterceptor(options, exchange.name, undefined, bybitSiteHeaders));

  if (options.tealstreetTunnel) {
    const region = options.proxyRegion as HOWRegion | undefined;
    installTransportAdapter(xhr, options, exchange.name, { region });
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
      // CloudFront refusing the egress address answers 403 before Bybit sees the
      // request: not the key, so it must not feed the auth backoff.
      // The tunnel wraps a non-JSON body as `{ raw_response }`; a direct adapter hands the string.
      const rawBody = typeof response.data === 'string' ? response.data : response.data?.raw_response;
      const blockedByCdn = typeof rawBody === 'string' && /cloudfront/i.test(rawBody);
      const isAuthStatus = !blockedByCdn && (response.status === 401 || response.status === 403);
      // Spell out the reason rather than a bare `HTTP 401`: the tunnel frame
      // carries no statusText, and both the health monitor's `auth_invalid`
      // patterns and the e2e toast assertion match on words like
      // "unauthorized"/"api key", not on the numeric status.
      const reason = blockedByCdn
        ? "Forbidden — blocked by Bybit's CDN before reaching Bybit"
        : isAuthStatus
          ? `${response.status === 401 ? 'Unauthorized' : 'Forbidden'} — API key rejected`
          : response.statusText;
      response.data = {
        // A string body (CloudFront's HTML page) would spread into one key per character.
        ...(response.data && typeof response.data === 'object' ? response.data : {}),
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

    // Re-signed once before it may count as a bad key: these arrive in bursts right
    // after a reconnect, on keys that work.
    const signatureResend = resendOnSignatureRejection(xhr, response, isBybitSignRejection);
    if (signatureResend) return signatureResend;

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
          const serverTimestamp = parseBybitServerTimestamp(retMsg);
          // On a server the clock is shared by every user's exchange; leave it alone there.
          if (serverTimestamp !== undefined && !isNodeOrBunRuntime()) virtualClock.syncFromServerTime(serverTimestamp);
          else await virtualClock.syncBybitClientServerTime();
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
        // An auth retCode is already surfaced by the credential path (the
        // streak, and the CLI's terminal message), so printing the body here
        // too gave the user the same sentence twice — "bybitdemo error API key
        // is invalid." followed by "API key is invalid.". Anything we do NOT
        // classify still gets dumped, because that is its only visibility.
        if (!BYBIT_AUTH_RET_CODES.has(responseMeta.retCode as number)) {
          console.error(`${exchange.name} error`, response.data);
        }
      }
    }
    return response;
  });

  return xhr;
};
