import type { WooV3Endpoint, WooV3HttpMethod } from './woo.v3.contracts';

import createHmac from 'create-hmac';
import qs from 'qs';

type QueryValue = string | number | boolean | null | undefined;

export type WooV3RequestAuth = Readonly<{
  apiKey: string;
  apiSecret: string;
  timestamp: number;
}>;

export type WooV3RequestInput = Readonly<{
  method: WooV3HttpMethod;
  path: WooV3Endpoint;
  query?: Readonly<Record<string, QueryValue | readonly QueryValue[]>>;
  body?: Readonly<Record<string, unknown>>;
}>;

export type WooV3PreparedRequest = Readonly<{
  method: WooV3HttpMethod;
  path: string;
  body?: string;
  headers: Readonly<Record<string, string>>;
  signaturePayload?: string;
}>;

const serializeQuery = (query: WooV3RequestInput['query']): string =>
  qs.stringify(query, {
    arrayFormat: 'repeat',
    sort: (a, b) => a.localeCompare(b),
  });

/**
 * Builds a deterministic, transport-neutral V3 request descriptor.
 * It does not own an Axios/fetch client and therefore cannot perform I/O.
 */
export const prepareWooV3Request = (input: WooV3RequestInput, auth?: WooV3RequestAuth): WooV3PreparedRequest => {
  const method = input.method.toUpperCase() as WooV3HttpMethod;
  const isQueryMethod = method === 'GET' || method === 'DELETE';

  if (!input.path.startsWith('/v3/')) {
    throw new Error('WOO V3 requests must use a /v3/ path');
  }
  if (isQueryMethod && input.body) {
    throw new Error(`${method} WOO V3 requests must place parameters in the query string`);
  }
  if (!isQueryMethod && input.query) {
    throw new Error(`${method} WOO V3 requests must place parameters in the JSON body`);
  }

  const query = input.query ? serializeQuery(input.query) : '';
  const path = query ? `${input.path}?${query}` : input.path;
  const body = input.body ? JSON.stringify(input.body) : undefined;
  const isPublic = input.path.startsWith('/v3/public/');

  if (isPublic) {
    const headers: Record<string, string> = {};
    if (body !== undefined) {
      headers['content-type'] = 'application/json';
    }

    return Object.freeze({
      method,
      path,
      ...(body === undefined ? {} : { body }),
      headers: Object.freeze(headers),
    });
  }
  if (!auth) {
    throw new Error('Authenticated WOO V3 requests require explicit credentials and timestamp');
  }

  const signaturePayload = `${auth.timestamp}${method}${path}${body ?? ''}`;
  const signature = createHmac('sha256', auth.apiSecret).update(signaturePayload).digest('hex');
  const headers: Record<string, string> = {
    'x-api-key': auth.apiKey,
    'x-api-timestamp': String(auth.timestamp),
    'x-api-signature': signature,
  };

  if (body !== undefined) {
    headers['content-type'] = 'application/json';
  }

  return Object.freeze({
    method,
    path,
    ...(body === undefined ? {} : { body }),
    headers: Object.freeze(headers),
    signaturePayload,
  });
};
