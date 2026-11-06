import type { InternalAxiosRequestConfig } from 'axios';

import { AxiosHeaders } from 'axios';
import qs from 'qs';

/** BloFin's unsigned paths: the legacy interceptor never signed these. */
export const isBlofinUnsignedRequest = (url: string | undefined) =>
  Boolean(url?.includes('public') || url?.includes('market'));

/** These serializers survive the TS venue class for hosted Rust callers.
 * Preimage: path+query, METHOD, millisecond stamp, nonce, body. */
export const prepareBlofinRequest = (config: InternalAxiosRequestConfig, timestamp: number, nonce: string) => {
  const method = config.method?.toUpperCase() ?? 'GET';
  const params = config.params ? decodeURIComponent(qs.stringify(config.params)) : null;
  const url = `${config.url}${params ? `?${params}` : ''}`;
  // A replayed config carries the body axios already serialized: sign those bytes.
  const data = typeof config.data === 'string' ? config.data : config.data ? JSON.stringify(config.data) : '';
  return { config, timestamp, nonce, payload: `${url}${method}${timestamp}${nonce}${data}` };
};

export const finishBlofinRequest = (
  prepared: ReturnType<typeof prepareBlofinRequest>,
  key: string,
  signature: string,
  passphrase: string,
): InternalAxiosRequestConfig => ({
  ...prepared.config,
  headers: new AxiosHeaders({
    ...prepared.config.headers,
    'ACCESS-KEY': key,
    'ACCESS-SIGN': signature,
    'ACCESS-NONCE': prepared.nonce,
    'ACCESS-TIMESTAMP': prepared.timestamp,
    'ACCESS-PASSPHRASE': passphrase,
  }),
});
