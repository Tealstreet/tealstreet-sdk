import type { InternalAxiosRequestConfig } from 'axios';

import { AxiosHeaders } from 'axios';

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

// Crypto.com's canonical parameter string sorts object keys at up to three levels.
const parameterText = (value: unknown, level = 0): string => {
  if (level >= 3 || value === null) return String(value);
  if (Array.isArray(value)) return value.map((item) => parameterText(item, level + 1)).join('');
  if (record(value)) return Object.keys(value).sort().map((key) => `${key}${parameterText(value[key], level + 1)}`).join('');
  return String(value);
};

export const prepareCryptocomRequest = (config: InternalAxiosRequestConfig, key: string, stampMs: number) => {
  const method = (config.url ?? '').replace('/', '');
  let raw: unknown = config.data;
  if (typeof raw === 'string') {
    try { raw = JSON.parse(raw); } catch { raw = undefined; }
  }
  // A previously signed request contributes only its original params on re-signing.
  const unsigned = record(raw) && raw.method === method && raw.api_key === key
    && record(raw.params) && typeof raw.sig === 'string' && typeof raw.nonce === 'number' ? raw.params : raw;
  const params = record(unsigned) ? unsigned : {};
  return { params, method, stampMs, payload: `${method}${stampMs}${key}${parameterText(params)}${stampMs}` };
};

export const finishCryptocomRequest = (
  config: InternalAxiosRequestConfig,
  prepared: ReturnType<typeof prepareCryptocomRequest>,
  key: string,
  signature: string,
  metadata: [string, string][] = [],
): InternalAxiosRequestConfig => {
  const headers = new AxiosHeaders(config.headers);
  for (const [name, value] of metadata) headers.set(name, value);
  return {
    ...config,
    headers,
    method: 'POST',
    data: JSON.stringify({ id: prepared.stampMs, method: prepared.method, api_key: key,
      params: prepared.params, sig: signature, nonce: prepared.stampMs }),
  };
};
