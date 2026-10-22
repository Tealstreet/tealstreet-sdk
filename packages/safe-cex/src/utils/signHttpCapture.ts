import type { AxiosInstance, AxiosRequestConfig } from 'axios';
import type { RemoteSignerRequest, RemoteSignerResponse } from '../types';

import { SIGNING_CAPTURE_FLAG } from './signingCapture';
import { tryParseJson } from './tryParseJson';

/** Run request-shape interceptors without ever invoking a network adapter. */
export const signHttpCapture = async (
  xhr: AxiosInstance,
  request: RemoteSignerRequest,
): Promise<RemoteSignerResponse> => {
  let captured: any = null;

  // Use per-request adapter to avoid corrupting xhr.defaults.adapter
  // (other requests on the same xhr instance would get the capture adapter during the await)
  const config: AxiosRequestConfig & { [SIGNING_CAPTURE_FLAG]: true } = {
    method: request.config!.method,
    url: request.config!.url,
    baseURL: request.config!.baseURL,
    params: tryParseJson(request.config!.params),
    data: tryParseJson(request.config!.data),
    headers: request.config!.headers,
    // Nothing leaves the browser here — the adapter below short-circuits with
    // a synthetic 200. Interceptors still run, so telemetry must be able to
    // tell this apart from a real request. See `utils/signingCapture.ts`.
    [SIGNING_CAPTURE_FLAG]: true,
    adapter: (config: any) => {
      captured = config;
      return Promise.resolve({ data: null, status: 200, statusText: 'OK', headers: {}, config });
    },
  };
  await xhr.request(config);

  const headers: Record<string, string> = {};
  if (captured?.headers?.toJSON) {
    for (const [k, v] of Object.entries(captured.headers.toJSON(true))) {
      if (v != null) headers[k] = String(v);
    }
  } else if (captured?.headers) {
    for (const [k, v] of Object.entries(captured.headers)) {
      if (v != null) headers[k] = String(v);
    }
  }

  return { id: request.id, config: { headers, url: captured?.url, params: captured?.params, data: captured?.data } };
};
