import type { InternalAxiosRequestConfig } from 'axios';

import { AxiosHeaders } from 'axios';
import qs from 'qs';

/** These serializers survive the TS venue class for hosted Rust callers. */
export const isOkxUnsignedSigningRequest = (url?: string): boolean => Boolean(url?.includes('public'));
export const isOkxPublicSigningRequest = (url?: string): boolean =>
  Boolean(url && (url.includes('/public/') || url.includes('/market/')));

export const prepareOkxRequest = (config: InternalAxiosRequestConfig, timestamp: string) => {
  const params = config.params ? decodeURIComponent(qs.stringify(config.params)) : null;
  const path = `${config.url}${params ? `?${params}` : ''}`;
  const hasBody = config.data !== undefined && config.data !== null;
  const data = hasBody ? (typeof config.data === 'string' ? config.data : JSON.stringify(config.data) || '') : '';
  return { config, timestamp, hasBody, data,
    payload: `${timestamp}${config.method?.toUpperCase?.() || 'GET'}${path}${data}` };
};

export const finishOkxRequest = (
  prepared: ReturnType<typeof prepareOkxRequest>,
  key: string,
  signature: string,
  passphrase: string,
): InternalAxiosRequestConfig => ({
  ...prepared.config,
  ...(prepared.hasBody ? { data: prepared.data } : {}),
  headers: new AxiosHeaders({
    ...prepared.config.headers,
    'OK-ACCESS-KEY': key,
    'OK-ACCESS-SIGN': signature,
    'OK-ACCESS-TIMESTAMP': prepared.timestamp,
    'OK-ACCESS-PASSPHRASE': passphrase,
  }),
});
