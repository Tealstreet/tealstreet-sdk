import type { InternalAxiosRequestConfig } from 'axios';

import { AxiosHeaders } from 'axios';

const PUBLIC = new Set([
  '/api/v1/time',
  '/api/v1/status',
  '/api/v1/assets',
  '/api/v1/markets',
  '/api/v1/market',
  '/api/v1/ticker',
  '/api/v1/tickers',
  '/api/v1/depth',
  '/api/v1/klines',
  '/api/v1/trades',
  '/api/v1/trades/history',
  '/api/v1/markPrices',
  '/api/v1/openInterest',
  '/api/v1/fundingRates',
]);
const INSTRUCTIONS: Record<string, string> = {
  'GET /api/v1/account': 'accountQuery',
  'PATCH /api/v1/account': 'accountUpdate',
  'GET /api/v1/position': 'positionQuery',
  'GET /api/v1/capital/collateral': 'collateralQuery',
  'GET /api/v1/orders': 'orderQueryAll',
  'POST /api/v1/order': 'orderExecute',
  'POST /api/v1/orders': 'orderExecute',
  'DELETE /api/v1/order': 'orderCancel',
  'DELETE /api/v1/orders': 'orderCancelAll',
  'GET /wapi/v1/history/fills': 'fillHistoryQueryAll',
  'GET /wapi/v1/history/orders': 'orderHistoryQueryAll',
  'GET /wapi/v1/history/funding': 'fundingHistoryQueryAll',
};
const refused = (): never => {
  throw new Error('Backpack signing request refused');
};
const row = (input: unknown): Record<string, unknown> => {
  if (!input || typeof input !== 'object' || Array.isArray(input)) return refused();
  return input as Record<string, unknown>;
};
const query = (input: Record<string, unknown>) =>
  Object.keys(input)
    .sort()
    .filter((key) => input[key] != null)
    .map((key) => {
      const value = input[key];
      if (
        !['string', 'number', 'boolean'].includes(typeof value) ||
        (typeof value === 'number' && !Number.isFinite(value))
      )
        return refused();
      return `${key}=${encodeURIComponent(String(value))}`;
    })
    .join('&');

/** Replayed Axios strings and native URL queries retain the complete signing
 * shape, including each row of POST /orders. No cryptographic material lives here. */
export const prepareBackpackRequest = (config: InternalAxiosRequestConfig, stamp: number, recvWindow?: number) => {
  const url = new URL(config.url ?? '', 'https://api.backpack.exchange');
  const method = (config.method ?? 'get').toUpperCase();
  if (url.origin !== 'https://api.backpack.exchange' || url.hash) return refused();
  if (method === 'GET' && PUBLIC.has(url.pathname)) return undefined;
  const instruction = INSTRUCTIONS[`${method} ${url.pathname}`];
  if (!instruction) return refused();
  const params: Record<string, unknown> = Object.create(null);
  for (const [key, value] of url.searchParams) {
    if (Object.hasOwn(params, key)) return refused();
    params[key] = value;
  }
  if (config.params !== undefined) {
    for (const [key, value] of Object.entries(row(config.params))) {
      if (Object.hasOwn(params, key)) return refused();
      params[key] = value;
    }
  }
  let data: unknown = config.data;
  if (typeof data === 'string' && data) data = JSON.parse(data);
  const batch = method === 'POST' && url.pathname === '/api/v1/orders';
  let rows: Record<string, unknown>[];
  if (method === 'GET') {
    if (data != null && data !== '') return refused();
    rows = [params];
  } else {
    if (Object.keys(params).length) return refused();
    rows = batch ? (Array.isArray(data) && data.length ? data.map(row) : refused()) : [row(data)];
  }
  const timestamp = Math.floor(stamp);
  if (!Number.isSafeInteger(timestamp) || timestamp < 0) return refused();
  const window =
    recvWindow === undefined || !Number.isFinite(recvWindow) || recvWindow <= 0
      ? 60000
      : Math.min(Math.floor(recvWindow), 60000);
  const payload =
    rows
      .map((value) => {
        const text = query(value);
        return `instruction=${instruction}${text ? `&${text}` : ''}`;
      })
      .join('&') + `&timestamp=${timestamp}&window=${window}`;
  return { config, payload, timestamp: String(timestamp), window: String(window) };
};

export const finishBackpackRequest = (
  prepared: NonNullable<ReturnType<typeof prepareBackpackRequest>>,
  credential: string,
  signature: string,
): InternalAxiosRequestConfig => {
  const headers = new AxiosHeaders(prepared.config.headers);
  for (const key of ['X-API-Key', 'X-Signature', 'X-Timestamp', 'X-Window', 'X-Broker-Id']) headers.delete(key);
  headers.set({
    'X-API-Key': credential,
    'X-Signature': signature,
    'X-Timestamp': prepared.timestamp,
    'X-Window': prepared.window,
    'X-Broker-Id': '1200',
    'Content-Type': 'application/json',
  });
  return { ...prepared.config, headers };
};
