import type { InternalAxiosRequestConfig } from 'axios';

import { AxiosHeaders } from 'axios';
import omit from 'lodash/omit';
import qs from 'qs';

import { tryParseJson } from '../../utils/tryParseJson';

const LISTEN_KEY = '/openApi/user/auth/userDataStream';
const sort = (a: string, b: string) => a.localeCompare(b);

/** The params serializer the legacy BingX client used for every request. */
export const serializeBingxParams = (params: unknown) => qs.stringify(params, { arrayFormat: 'repeat', sort });

/** Livenet listen-key requests carry only the API key. */
export const isBingxUnsignedRequest = (url: string | undefined, testnet: boolean | undefined) =>
  url === LISTEN_KEY && !testnet;

/** The HMAC preimage for hosted Rust callers after the TS venue class: the
 * request's params (minus any previous signature) plus `timestamp`, sorted with
 * `localeCompare` and serialized in repeat format, as the legacy interceptor did.
 */
export const prepareBingxRequest = (config: InternalAxiosRequestConfig, timestamp: number) => {
  // A replayed request arrives with the payload axios already serialized.
  const parsed = tryParseJson(config.data || config.params);
  const { signature: _previous, ...rest } = parsed && typeof parsed === 'object' ? (parsed as Record<string, any>) : {};
  const data: Record<string, any> = { ...rest, timestamp };
  const sorted = Object.fromEntries(Object.keys(data).sort(sort).map((key) => [key, data[key]]));
  return { config, sorted, payload: serializeBingxParams(sorted) };
};

/** GET-style requests send the signed params as the query; POST sends them as
 * the JSON body with `quantity` and `price` as numbers, as the legacy client did.
 */
export const finishBingxRequest = (
  prepared: ReturnType<typeof prepareBingxRequest>,
  key: string,
  signature: string,
): InternalAxiosRequestConfig => {
  const signed: Record<string, any> = { ...prepared.sorted, signature };
  const headers = new AxiosHeaders({ ...prepared.config.headers, 'X-BX-APIKEY': key });
  if (prepared.config.method !== 'post') {
    return { ...(omit(prepared.config, 'data') as InternalAxiosRequestConfig), params: signed, headers };
  }
  for (const field of ['quantity', 'price']) if (signed[field]) signed[field] = +signed[field];
  headers.set('Content-Type', 'application/json');
  return { ...prepared.config, data: JSON.stringify(signed), headers };
};
