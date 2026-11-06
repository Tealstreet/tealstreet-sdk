import type { InternalAxiosRequestConfig } from 'axios';

import { AxiosHeaders } from 'axios';
import qs from 'qs';

/** Public market paths are sent unsigned. */
export const isBitunixUnsignedRequest = (url: string | undefined) => Boolean(url?.includes('public'));

/** The REST preimage `nonce ‖ timestamp ‖ apiKey ‖ query ‖ body`, kept for hosted
 * Rust callers after the TS venue class: sorted repeat-format params with their
 * `=` and `&` removed, then the exact body bytes.
 */
export const prepareBitunixRequest = (
  config: InternalAxiosRequestConfig,
  key: string,
  timestamp: number,
  nonce: string,
) => {
  const params = config.params
    ? decodeURIComponent(
        qs.stringify(config.params, {
          arrayFormat: 'repeat',
          sort: (a, b) => a.localeCompare(b),
        }),
      )
    : null;
  // A replayed config carries the body axios already serialized: sign those bytes.
  const data = typeof config.data === 'string' ? config.data : config.data ? JSON.stringify(config.data) : null;
  const toSign = [nonce, timestamp, key, params?.replace(/[=&]/g, '') || ''];
  if (data) toSign.push(data);
  return { config, timestamp, nonce, payload: toSign.join('') };
};

export const finishBitunixRequest = (
  prepared: ReturnType<typeof prepareBitunixRequest>,
  key: string,
  signature: string,
): InternalAxiosRequestConfig => ({
  ...prepared.config,
  headers: new AxiosHeaders({
    ...prepared.config.headers,
    'api-key': key,
    sign: signature,
    nonce: prepared.nonce,
    timestamp: prepared.timestamp,
  }),
});
