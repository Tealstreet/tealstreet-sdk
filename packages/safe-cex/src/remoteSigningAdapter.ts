import type { AxiosAdapter, AxiosResponse } from 'axios';
import type { ExchangeName, RemoteSigner, RemoteSignerRequest } from './types';

import axios, { AxiosHeaders, InternalAxiosRequestConfig } from 'axios';
import { nanoid } from 'nanoid';

import { tryParseJson } from './utils/tryParseJson';
import { markRestSigned } from './utils/venueRestDiagnostics';

/**
 * Set by an exchange's signing interceptor on a public market-data endpoint, to
 * tell the adapter not to round-trip to the client for a signature.
 *
 * It does NOT mean "send this unsigned". It says nothing about local signing:
 * a client that holds the key still runs its normal signing path afterwards, so
 * tag by falling through, never by returning early on the tag's behalf. That
 * distinction is load-bearing — many venues meter signed requests against a
 * per-account bucket and unsigned ones against a shared per-IP bucket, so
 * dropping a signature we can produce for free is a real cost. Only a keyless
 * hosted server, which cannot sign locally in the first place, skips.
 *
 * Without the flag the adapter cannot know: it runs AFTER every interceptor, so
 * the round trip is committed before the interceptor's own `isPublic` branch
 * has any say. Two cases end up here:
 *
 *  - The request already ends up unsigned regardless. Kraken's `PUBLIC_ENDPOINTS`
 *    list, OKX's `url.includes('public')`, and Bybit's GET-on-kline all
 *    early-return; Hyperliquid arrives here differently, since `createInfoAPI`
 *    registers no signing interceptor at all and the one on `createExchangeAPI`
 *    skips any body without an `action`. Either way the client replays the
 *    request through that same code, reaches that same outcome, and hands back
 *    a config with no signature on it. The round trip is pure latency for a
 *    guaranteed no-op, and tagging costs nothing anywhere.
 *  - The endpoint is public but currently signed — OKX `/api/v5/market/*` and
 *    Bybit's `/v5/market/` tickers and instruments-info, none of which match
 *    their exchange's narrow public test. Here the hosted request does go out
 *    unsigned where it used to be signed. Both venues meter public market data
 *    per-IP whether or not a signature rides along, so nothing is actually
 *    given up; on a venue that did credit these to the account instead, this is
 *    the case that would cost something, and the one to think twice about.
 *    Either way the fall-through leaves key-holding clients signing as today.
 *
 * Absence means "sign it", so an untagged exchange keeps today's behaviour
 * exactly. The way to get this wrong is to tag on a credential-shaped condition
 * instead of on the request. `!options.secret` is TRUE for every request in
 * hosted mode — the server is handed the key but never the secret — so any
 * guard OR'd with it (`if (!options.key || !options.secret) return`, the common
 * shape) would tag every private request as needing no signature, and the
 * server would ship real orders unauthenticated. `publicDataOnly` is the same
 * trap on a public-data instance. Tag only on the url or body.
 *
 * Measured on a hosted mobile session: four of the first six sign requests on
 * connect were public endpoints, and they gate the markets and tickers lanes.
 */
const SKIP_REMOTE_SIGNING = 'safeCexSkipRemoteSigning';

/**
 * Tag a public request so a keyless hosted server skips the signing round trip.
 * Mutates and returns the config; callers should fall through to the normal
 * signing path rather than returning this directly.
 */
export const markSkipRemoteSigning = <T>(config: T): T => {
  (config as Record<string, unknown>)[SKIP_REMOTE_SIGNING] = true;
  return config;
};

/**
 * Serialize AxiosHeaders to a plain Record<string, string> for transport.
 */
const serializeHeaders = (headers: InternalAxiosRequestConfig['headers']): Record<string, string> => {
  if (!headers) return {};
  if (headers instanceof AxiosHeaders) {
    const result: Record<string, string> = {};
    for (const [key, value] of Object.entries(headers.toJSON(true))) {
      if (value != null) result[key] = String(value);
    }
    return result;
  }
  const result: Record<string, string> = {};
  for (const [key, value] of Object.entries(headers)) {
    if (value != null) result[key] = String(value);
  }
  return result;
};

/**
 * Creates an axios adapter that delegates request signing to a remote client.
 *
 * Flow:
 * 1. Intercepts the unsigned request config (after non-signing interceptors have run)
 * 2. Sends config to client via remoteSigner.sign()
 * 3. Client runs it through its exchange's signing interceptors
 * 4. Client returns signed config (headers, params, data with signatures)
 * 5. This adapter fires the actual HTTP request with the signed config
 *
 * The `getInnerAdapter` callback resolves the inner adapter lazily at request time,
 * allowing exchange-specific adapters (e.g. HOW) to be set after this adapter is installed.
 *
 * @param remoteSigner - RemoteSigner implementation (e.g. RelayRemoteSigner)
 * @param exchangeName - Exchange identifier for routing on the client
 * @param getInnerAdapter - Callback to resolve inner adapter at request time
 */
export const createRemoteSigningAdapter = (
  remoteSigner: RemoteSigner,
  exchangeName: ExchangeName,
  getInnerAdapter?: () => AxiosAdapter | undefined,
  accountName?: string,
): AxiosAdapter => {
  const execute = (finalConfig: InternalAxiosRequestConfig): Promise<AxiosResponse> => {
    // Execute with inner adapter (e.g. HOW) or default HTTP adapter
    const innerAdapter = getInnerAdapter?.();
    const executeAdapter = innerAdapter || axios.defaults.adapter;
    if (!executeAdapter) {
      throw new Error('No axios adapter available for HTTP execution');
    }

    if (Array.isArray(executeAdapter)) {
      finalConfig.adapter = executeAdapter;
      return axios(finalConfig);
    }

    return (executeAdapter as AxiosAdapter)(finalConfig);
  };

  return async (config: InternalAxiosRequestConfig): Promise<AxiosResponse> => {
    // Public endpoint — the interceptor already established there is nothing to
    // sign, so go straight to the wire instead of round-tripping to the client.
    if ((config as unknown as Record<string, unknown>)[SKIP_REMOTE_SIGNING]) {
      return execute(config);
    }

    // The adapter runs AFTER axios transformRequest, which may have stringified
    // objects (e.g. JSON.stringify on data, qs.stringify on params). Parse them
    // back so the browser receives clean objects for its own signing interceptors.
    const request: RemoteSignerRequest = {
      id: nanoid(),
      type: 'http',
      exchangeName,
      accountName,
      config: {
        method: (config.method || 'GET').toUpperCase(),
        url: config.url || '',
        baseURL: config.baseURL || '',
        params: tryParseJson(config.params) as Record<string, unknown> | undefined,
        data: tryParseJson(config.data),
        headers: serializeHeaders(config.headers),
      },
    };

    const signStartedAt = Date.now();
    const response = await remoteSigner.sign(request);
    const signMs = Date.now() - signStartedAt;
    if (response.error) {
      throw new Error(`Remote signing failed: ${response.error}`);
    }

    // Merge signed config from client.
    // IMPORTANT: Use the browser's response values directly — don't fall back to config originals.
    // The signing interceptor may have intentionally modified or deleted fields (e.g. baking params
    // into the URL then deleting config.params). Falling back would undo that and break the signature.
    const signedConfig: InternalAxiosRequestConfig = {
      ...config,
      headers: new AxiosHeaders(response.config!.headers),
      ...(response.config!.url ? { url: response.config!.url } : {}),
      params: response.config!.params,
      data: response.config!.data,
    };

    // Integrity check: warn if the signed URL has params baked in but params object is also set
    if (signedConfig.url?.includes('?') && signedConfig.params && Object.keys(signedConfig.params).length > 0) {
      console.warn(
        `[remoteSigningAdapter] ${exchangeName}: URL has query params AND params object set — possible double-serialization`,
        { url: signedConfig.url, params: signedConfig.params },
      );
    }

    // Stamped after the merge so the signature age is read off the headers the
    // venue will actually receive, and timed here rather than at the response
    // so a request that never comes back still carries what signing cost.
    markRestSigned(signedConfig, signMs);

    return execute(signedConfig);
  };
};
