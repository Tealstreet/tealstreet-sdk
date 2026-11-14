import type { InternalAxiosRequestConfig } from 'axios';
import { AxiosHeaders } from 'axios';
import qs from 'qs';

export const serializeApexOmni = (value: unknown): string => typeof value === 'string' ? value
  : value === undefined || value === null ? '' : qs.stringify(value, {
    sort: (a, b) => a.localeCompare(b),
    filter: (_key, item) => item === null ? undefined : item,
  });

const invalidBody = (): never => { throw new Error('Invalid ApeX signing body'); };
const canonicalFields = (fields: Iterable<[string, unknown]>): string => Array.from(fields)
  .filter(([, value]) => value !== undefined && value !== null)
  .sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)
  .map(([key, value]) => {
    if (!['string', 'number', 'boolean'].includes(typeof value)) return invalidBody();
    return qs.stringify({ [key]: value });
  }).join('&');

const multipartFields = (body: string, contentType: string): [string, string][] => {
  const match = /^multipart\/form-data\s*;\s*boundary=(?:"([^"\r\n]+)"|([^;\s]+))\s*$/i.exec(contentType);
  const boundary = match?.[1] ?? match?.[2];
  if (!boundary || boundary.length > 70 || /[^\x20-\x7e]/.test(boundary)) return invalidBody();
  const parts = body.split(`--${boundary}`);
  if (parts.shift() !== '' || !['--\r\n', '--'].includes(parts.pop() ?? '')) return invalidBody();
  return parts.map((part) => {
    const field = /^\r\nContent-Disposition: form-data; name="([^"\r\n]+)"\r\n\r\n([\s\S]*)\r\n$/i.exec(part);
    if (!field) return invalidBody();
    return [field[1], field[2]];
  });
};

/** Native forms and multipart bodies sign sorted semantic fields, including
 * existing zkLink signatures; transport boundaries and field order are unsigned. */
const canonicalPostBody = (body: unknown, headers: InternalAxiosRequestConfig['headers']): string => {
  if (body === undefined || body === null) return '';
  if (typeof body === 'string') {
    const contentType = String(new AxiosHeaders(headers).getContentType() ?? '');
    if (/^multipart\/form-data\b/i.test(contentType)) return canonicalFields(multipartFields(body, contentType));
    if (!body) return '';
    const fields: [string, string][] = body.split('&').map((part) => {
      const equals = part.indexOf('=');
      if (equals < 0) return invalidBody();
      const decode = (value: string) => decodeURIComponent(value.replace(/\+/g, ' '));
      return [decode(part.slice(0, equals)), decode(part.slice(equals + 1))];
    });
    return canonicalFields(fields);
  }
  if (body instanceof URLSearchParams) return canonicalFields(body.entries());
  if (typeof FormData !== 'undefined' && body instanceof FormData) return canonicalFields(body.entries());
  if (typeof body !== 'object' || Array.isArray(body)
    || ![Object.prototype, null].includes(Object.getPrototypeOf(body))) return invalidBody();
  return canonicalFields(Object.entries(body));
};

export const prepareApexOmniRequest = (config: InternalAxiosRequestConfig, stampMs: number) => {
  const method = (config.method ?? 'GET').toUpperCase();
  const url = config.url ?? '';
  const path = url.startsWith('/api/') ? url.slice(4) : url;
  if (!path.startsWith('/v3/') || path.includes('?')) throw new Error('Invalid ApeX signing path');
  if (method === 'GET' && path === '/v3/klines') return undefined;
  const queryMethod = method === 'GET' || method === 'DELETE';
  const data = queryMethod ? serializeApexOmni(config.params) : canonicalPostBody(config.data, config.headers);
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
