import type { ExchangeOptions } from '../../../types';

import retry from 'axios-retry';
import qs from 'qs';

import { markSkipRemoteSigning } from '../../../remoteSigningAdapter';
import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import { createWalletFromPrivateKey } from '../../../utils/ethereum/createWalletFromPrivateKey';
import { describeHttpFailure } from '../../../utils/httpFailure';
import { isSigningCaptureRequest } from '../../../utils/signingCapture';
import { isHyperliquidRead } from './hyperliquidRetry';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';
import { BaseExchange } from '../../../exchanges/base';
import {
  EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE,
  extractHyperliquidResponseError,
  isOnlyHyperliquidTerminalCancelError,
} from './hyperliquid.order-response';
import { getHyperliquidRestRequestWeight, hyperliquidRestRateLimiter } from './hyperliquid.rateLimiter';
import { signL1Action } from './hyperliquid.signing';
import { BASE_URLS, ENDPOINTS } from '../../../exchanges/hyperliquid/hyperliquid.types';

const getRequestActionType = (requestData: unknown): string | undefined => {
  const data =
    typeof requestData === 'string'
      ? (() => {
          try {
            return JSON.parse(requestData);
          } catch {
            return undefined;
          }
        })()
      : requestData;

  if (typeof data !== 'object' || data === null || Array.isArray(data)) return undefined;
  const action = (data as { action?: unknown }).action;
  if (typeof action !== 'object' || action === null || Array.isArray(action)) return undefined;
  const type = (action as { type?: unknown }).type;
  return typeof type === 'string' ? type : undefined;
};

type HyperliquidOrderErrorTelemetryProvider = {
  getOrderErrorTelemetry?: (requestData: unknown, message: string) => Record<string, unknown> | undefined;
};

const getOrderErrorTelemetry = (
  exchange: BaseExchange,
  requestData: unknown,
  message: string,
): Record<string, unknown> | undefined => {
  const provider = exchange as BaseExchange & HyperliquidOrderErrorTelemetryProvider;
  return provider.getOrderErrorTelemetry?.(requestData, message);
};

// Common response interceptor for Hyperliquid API responses
const addResponseInterceptor = (xhr: ReturnType<typeof safeCexAxiosCreate>, exchange: BaseExchange) => {
  xhr.interceptors.response.use(async (response) => {
    // The HOW tunnel adapter resolves without axios' settle(), so a non-2xx
    // reaches this SUCCESS handler with its status intact but never inspected,
    // and callers report the body SHAPE instead of the failure. Read it here so
    // the status rides along — but only for `/exchange`. A blanket throw would
    // also hit `/info`, where Hyperliquid answers 422 for probe types we ask
    // about optionally (`perpDexs`, `userRole`, `userAbstraction`) and 429 under
    // load; every one of those is already swallowed by its caller, and turning
    // them into throws costs a full error toast per attempt across the startup
    // retries and the 120s HIP-3 sweep. `/info` failures also reach paged
    // collectors like `fetchHistoricalFills`, whose try/catch wraps the whole
    // window loop — a throw there discards windows already collected and the
    // ingestion coverage row closes over the gap for good.
    //
    // Only tunnelled requests get here: a direct axios request already rejects
    // non-2xx in validateStatus before any interceptor runs.
    const isExchangeAction = response.config?.url === ENDPOINTS.EXCHANGE;
    const httpFailed = isExchangeAction && typeof response.status === 'number' && response.status >= 400;

    // Append the status rather than choosing between it and the venue's text:
    // Hyperliquid can answer a 4xx with a perfectly good `{"status":"err"}`
    // body, and picking one drops half the diagnosis.
    const responseError = extractHyperliquidResponseError(response.data);
    if (responseError) {
      const msg = httpFailed ? `${responseError} (HTTP ${response.status})` : responseError;
      // Close-all / flatten then tries to cancel leftover TP/SL. Hyperliquid
      // already dropped those orders with the fill, so cancel comes back as
      // "never placed, already canceled, or filled" once per oid. That is the
      // desired end state — do not toast it. Still throw so callers can tell
      // a miss from a 2xx, and keep mixed batches (any other error) loud.
      const actionType = getRequestActionType(response.config?.data);
      const isTerminalCancelMiss =
        isExchangeAction &&
        (actionType === 'cancel' || actionType === 'cancelByCloid') &&
        isOnlyHyperliquidTerminalCancelError(responseError);
      if (!isTerminalCancelMiss) {
        exchange.emitAndLogError(msg, undefined, getOrderErrorTelemetry(exchange, response.config?.data, msg));
      }
      throw new Error(msg);
    }

    if (httpFailed) {
      const msg = describeHttpFailure('Hyperliquid', response);
      exchange.emitAndLogError(msg);
      throw new Error(msg);
    }

    // `signHttpRequest` captures a signed config by dispatching through a
    // per-request adapter that resolves `{data: null, status: 200}` without
    // sending anything. Interceptors still run, so without this guard every
    // hosted-mode signature of an `/exchange` action rejects inside the signer.
    if (isExchangeAction && response.data == null && !isSigningCaptureRequest(response.config)) {
      const msg = `${EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE} (HTTP ${response.status})`;
      exchange.emitAndLogError(msg);
      throw new Error(msg);
    }

    return response;
  });
};

// Actions the browser will sign on the exchange-server's behalf. Deliberately
// excludes usdSend/withdraw3/vaultTransfer: a transferable signature for those
// moves funds, and nothing in the hosted flow needs to ask for one.
const RELAY_SIGNABLE_ACTION_TYPES = new Set([
  'order',
  'cancel',
  'cancelByCloid',
  'modify',
  'batchModify',
  'updateLeverage',
  'updateIsolatedMargin',
  'scheduleCancel',
]);

// Hyperliquid signs in the method body rather than in an interceptor, so the
// exchange-server — which holds no secret — cannot produce a signature and used
// to throw before the request went out. It now sends the action unsigned; the
// browser's replay of that POST (BaseExchange.signHttpRequest) re-runs this
// interceptor with the real key and fills the signature in.
const addSigningInterceptor = (
  xhr: ReturnType<typeof safeCexAxiosCreate>,
  options: ExchangeOptions,
  exchange: BaseExchange,
) => {
  xhr.interceptors.request.use(async (config) => {
    // Above the `!options.secret` guard on purpose: that guard is the branch
    // every hosted request takes, so a tag below it would never run where it
    // matters. Keyed on the url, which is credential-independent.
    if (config.url === ENDPOINTS.INFO) markSkipRemoteSigning(config);

    const body = config.data as any;
    if (!options.secret || !body || typeof body !== 'object') return config;
    if (body.signature || !body.action || body.nonce === undefined) return config;

    const actionType = String(body.action?.type ?? '');
    if (!RELAY_SIGNABLE_ACTION_TYPES.has(actionType)) {
      exchange.emitAndLogError(`Refusing to sign unexpected Hyperliquid action "${actionType}"`);
      return config;
    }

    const signature = await signL1Action(
      createWalletFromPrivateKey(options.secret),
      body.action,
      body.vaultAddress || null,
      body.nonce,
      !options.testnet,
    );

    return { ...config, data: { ...body, signature } };
  });
};

const addRateLimitInterceptor = (xhr: ReturnType<typeof safeCexAxiosCreate>) => {
  xhr.interceptors.request.use(async (config) => {
    if (isSigningCaptureRequest(config)) return config;
    await hyperliquidRestRateLimiter.waitForToken(getHyperliquidRestRequestWeight(config));
    return config;
  });
};

// Create axios instance for /exchange endpoint (trading operations)
export const createExchangeAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
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

  retry(xhr, { retries: 3, retryCondition: networkErrorOnRead(isHyperliquidRead) });
  if (!options.publicDataOnly) {
    addSigningInterceptor(xhr, options, exchange);
    installTransportAdapter(xhr, options, exchange.name);
  }
  addRateLimitInterceptor(xhr);
  addResponseInterceptor(xhr, exchange);

  return xhr;
};

// Create axios instance for /info endpoint (read-only operations)
export const createInfoAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
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

  retry(xhr, { retries: 3, retryCondition: networkErrorOnRead(isHyperliquidRead) });

  // This instance has no signing interceptor at all — every request through it
  // is unsigned by construction — yet it still carries the remote-signing
  // adapter, so without this tag each `/info` read costs a round trip to the
  // device for a signature nothing was ever going to produce. Registered
  // unconditionally, like every other exchange's: a public-data instance can
  // still be handed a remoteSigner, and the tag is inert without one. Guarded
  // on the url rather than set in the instance defaults, so an `/exchange`
  // action posted here would still be signed.
  xhr.interceptors.request.use((config) => {
    if (config.url === ENDPOINTS.INFO) markSkipRemoteSigning(config);
    return config;
  });

  if (!options.publicDataOnly) {
    installTransportAdapter(xhr, options, exchange.name);
  }
  addRateLimitInterceptor(xhr);
  addResponseInterceptor(xhr, exchange);

  return xhr;
};

// Legacy function for backwards compatibility - returns exchange API
export const createAPI = createExchangeAPI;
