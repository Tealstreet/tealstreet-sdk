import type { AxiosAdapter, AxiosResponse } from 'axios';
import type { ExchangeName, RemoteSigner, RemoteSignerRequest } from './types';

import axios, { AxiosHeaders, InternalAxiosRequestConfig } from 'axios';
import { nanoid } from 'nanoid';

import { tryParseJson } from './utils/tryParseJson';

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
  return async (config: InternalAxiosRequestConfig): Promise<AxiosResponse> => {
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

    const response = await remoteSigner.sign(request);
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

    // Execute with inner adapter (e.g. HOW) or default HTTP adapter
    const innerAdapter = getInnerAdapter?.();
    const executeAdapter = innerAdapter || axios.defaults.adapter;
    if (!executeAdapter) {
      throw new Error('No axios adapter available for HTTP execution');
    }

    if (Array.isArray(executeAdapter)) {
      signedConfig.adapter = executeAdapter;
      return axios(signedConfig);
    }

    return (executeAdapter as AxiosAdapter)(signedConfig);
  };
};
