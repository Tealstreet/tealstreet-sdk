import type { InternalAxiosRequestConfig } from 'axios';
import { AxiosHeaders } from 'axios';
import qs from 'qs';

export const serializeApexOmni = (value: unknown): string => typeof value === 'string' ? value
  : value === undefined || value === null ? '' : qs.stringify(value, {
    sort: (a, b) => a.localeCompare(b),
    filter: (_key, item) => item === null ? undefined : item,
  });

/** Native order bodies already contain their zkLink signatures. This capture
 * signs their exact serialized bytes; contract generation uses the batch codec. */
export const prepareApexOmniRequest = (config: InternalAxiosRequestConfig, stampMs: number) => {
  const method = (config.method ?? 'GET').toUpperCase();
  const url = config.url ?? '';
  const path = url.startsWith('/api/') ? url.slice(4) : url;
  if (!path.startsWith('/v3/') || path.includes('?')) throw new Error('Invalid ApeX signing path');
  if (method === 'GET' && path === '/v3/klines') return undefined;
  const queryMethod = method === 'GET' || method === 'DELETE';
  const data = serializeApexOmni(queryMethod ? config.params : config.data);
  const timestamp = String(Math.floor(stampMs));
  const payload = `${timestamp}${method}/api${path}${queryMethod && data ? '?' : ''}${data}`;
  return { config, timestamp, payload };
};

export const finishApexOmniRequest = (
  prepared: NonNullable<ReturnType<typeof prepareApexOmniRequest>>, key: string, signature: string, passphrase: string,
): InternalAxiosRequestConfig => {
  const headers = new AxiosHeaders(prepared.config.headers);
  for (const name of Object.keys(headers)) if (/^APEX-/i.test(name)) headers.delete(name);
  headers.set({ 'APEX-API-KEY': key, 'APEX-SIGNATURE': signature,
    'APEX-PASSPHRASE': passphrase, 'APEX-TIMESTAMP': prepared.timestamp });
  return { ...prepared.config, headers };
};
