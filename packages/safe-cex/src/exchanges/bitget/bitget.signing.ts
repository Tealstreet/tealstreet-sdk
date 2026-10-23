import type { InternalAxiosRequestConfig } from 'axios';

import { AxiosHeaders } from 'axios';
import qs from 'qs';

import { ENDPOINTS } from './bitget.types';

/** These serializers survive the TS venue class for hosted Rust callers. */
export const prepareBitgetRequest = (config: InternalAxiosRequestConfig, timestamp: number) => {
  const method = config.method?.toUpperCase() ?? 'GET';
  // TODO: Audit whether HOW proxy re-serialization can break this signature (see bitmex.api.ts).
  let signingUrl = config.url;
  if (config.params) {
    if (config.params.symbol && !config.url?.includes(ENDPOINTS.KLINE)) {
      config.params.symbol = config.params.symbol.replace('$', '');
    }
    const signingQuery = qs.stringify(config.params, { encode: false });
    signingUrl = `${signingUrl}?${signingQuery}`;
  }

  // A replayed config carries the body axios already serialized: sign those bytes.
  const stringifiedBody =
    typeof config.data === 'string'
      ? config.data
      : config.data
        ? toJsonString({
            ...config.data,
            ...(config.data.symbol ? { symbol: config.data.symbol.replace('$', '') } : {}),
          })
        : '';
  return { config, timestamp, payload: [timestamp, method, signingUrl, stringifiedBody].join('') };
};

export const finishBitgetRequest = (
  prepared: ReturnType<typeof prepareBitgetRequest>,
  key: string,
  signature: string,
  passphrase: string | undefined,
): InternalAxiosRequestConfig => ({
  ...prepared.config,
  headers: new AxiosHeaders({
    ...prepared.config.headers,
    'ACCESS-KEY': key,
    'ACCESS-SIGN': signature,
    'ACCESS-TIMESTAMP': prepared.timestamp,
    ...(passphrase === undefined ? {} : { 'ACCESS-PASSPHRASE': passphrase }),
  }),
});

function toJsonString(obj: object): string | null {
  if (obj == null) {
    return null;
  }

  let json = JSON.stringify(obj);
  Object.keys(obj)
    .filter((key) => key[0] === '_')
    .forEach((key) => {
      json = json.replace(key, key.substring(1));
    });
  const reg = new RegExp('"_', 'g');
  return json.replace(reg, '"');
}
