import type { ExchangeOptions } from '../../types';

import retry, { isNetworkError } from 'axios-retry';
import qs from 'qs';

import { axiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { markSkipRemoteSigning } from '../../remoteSigningAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { createWalletFromPrivateKey } from '../../utils/ethereum/createWalletFromPrivateKey';
import { describeHttpFailure } from '../../utils/httpFailure';
import { isSigningCaptureRequest } from '../../utils/signingCapture';
import { BaseExchange } from '../base';
import {
  EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE,
  extractHyperliquidResponseError,
} from './hyperliquid.order-response';
import { signL1Action } from './hyperliquid.signing';
import { BASE_URLS, ENDPOINTS } from './hyperliquid.types';

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
      exchange.emitAndLogError(msg);
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

  retry(xhr, { retries: 3, retryCondition: isNetworkError });
  if (!options.publicDataOnly) {
    addSigningInterceptor(xhr, options, exchange);
    setInnerAdapterForRemoteSigning(xhr, axiosHttpOverWebsocketAdapter);
  }
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

  retry(xhr, { retries: 3, retryCondition: isNetworkError });

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
    setInnerAdapterForRemoteSigning(xhr, axiosHttpOverWebsocketAdapter);
  }
  addResponseInterceptor(xhr, exchange);

  return xhr;
};

// Legacy function for backwards compatibility - returns exchange API
export const createAPI = createExchangeAPI;
