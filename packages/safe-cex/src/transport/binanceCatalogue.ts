import type { HttpPacket } from '@tealstreet/safe-cex-wasm/host';
import type { CataloguePolicy, CatalogueRoute } from './sharedCatalogue';

import { SharedCatalogue } from './sharedCatalogue';

const ENDPOINTS: Record<string, string> = {
  'https://fapi.binance.com': '/fapi/v1/exchangeInfo',
  'https://demo-fapi.binance.com': '/fapi/v1/exchangeInfo',
  'https://dapi.binance.com': '/dapi/v1/exchangeInfo',
  'https://demo-dapi.binance.com': '/dapi/v1/exchangeInfo',
  'https://api.binance.com': '/api/v3/exchangeInfo',
  'https://testnet.binance.vision': '/api/v3/exchangeInfo',
};
const PUBLIC_QUERY = new Set(['symbol', 'symbols', 'permissions', 'showPermissionSets', 'symbolStatus']);
const MAX_BYTES = 8 * 1024 * 1024;

function keyFor(request: HttpPacket & { intent?: string }, route?: CatalogueRoute): string | undefined {
  if (request.intent !== 'public' || request.method !== 'GET' || request.body !== undefined || !route) return;
  if (
    Object.entries(request.headers).some(
      ([name, value]) =>
        !['accept', 'content-type'].includes(name.toLowerCase()) ||
        !['application/json', '*/*', 'application/json, text/plain, */*'].includes(String(value).toLowerCase()),
    )
  )
    return;
  try {
    const url = new URL(request.url);
    if (url.username || url.password || url.hash || request.url.length > 8192 || ENDPOINTS[url.origin] !== url.pathname)
      return;
    if ([...url.searchParams.keys()].some((name) => !PUBLIC_QUERY.has(name))) return;
    return `${url.origin}|${url.pathname}${url.search}|${route.network}|${route.route}`;
  } catch {
    return;
  }
}

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const positive = (value: unknown) =>
  typeof value === 'string' && value.trim() !== '' && Number.isFinite(Number(value)) && Number(value) > 0;

function valid(key: string, bytes: Uint8Array): boolean {
  if (bytes.length > MAX_BYTES) return false;
  try {
    const body = JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes));
    if (!record(body) || !Array.isArray(body.symbols) || body.symbols.length === 0) return false;
    const spot = key.includes('|/api/v3/');
    const seen = new Set<string>();
    let usable = 0;
    const complete = body.symbols.every((row: unknown) => {
      if (
        !record(row) ||
        !['symbol', 'baseAsset', 'quoteAsset'].every((field) => typeof row[field] === 'string' && row[field] !== '')
      )
        return false;
      if (
        ['status', 'contractStatus', 'contractType', 'marginAsset'].some(
          (field) => row[field] !== undefined && typeof row[field] !== 'string',
        )
      )
        return false;
      if (
        ['baseAssetPrecision', 'quoteAssetPrecision'].some(
          (field) =>
            row[field] !== undefined &&
            (!Number.isInteger(row[field]) || Number(row[field]) < 0 || Number(row[field]) > 18),
        )
      )
        return false;
      if (row.status !== undefined && row.contractStatus !== undefined) return false;
      if (seen.has(row.symbol as string)) return false;
      seen.add(row.symbol as string);
      if (
        !Array.isArray(row.filters) ||
        !row.filters.every(
          (filter: unknown) =>
            record(filter) &&
            typeof filter.filterType === 'string' &&
            ['tickSize', 'stepSize', 'minQty', 'maxQty', 'minNotional', 'notional'].every(
              (field) =>
                filter[field] === undefined ||
                (typeof filter[field] === 'string' && Number.isFinite(Number(filter[field]))),
            ),
        )
      )
        return false;
      if (
        !spot &&
        (!['PERPETUAL', 'TRADIFI_PERPETUAL'].includes(String(row.contractType)) ||
          (key.includes('|/dapi/')
            ? row.contractType !== 'PERPETUAL' || (row.status ?? row.contractStatus) !== 'TRADING'
            : !['USDT', 'USDC'].includes(String(row.marginAsset))))
      )
        return true;
      usable++;
      const price = row.filters.find((filter: Record<string, unknown>) => filter.filterType === 'PRICE_FILTER');
      const lot = row.filters.find((filter: Record<string, unknown>) => filter.filterType === 'LOT_SIZE');
      return positive(price?.tickSize) && positive(lot?.stepSize);
    });
    return complete && usable > 0;
  } catch {
    return false;
  }
}

function failureDelay(failures: number, headers: string, now: number): number {
  const nominal = Math.min(30_000, 1000 * 2 ** (Math.min(6, failures) - 1));
  const value = /^retry-after\s*:\s*([^\r\n]+)$/im.exec(headers.slice(0, 16_384))?.[1].trim().slice(0, 128);
  if (!value) return nominal;
  const delay = /^\d+(?:\.\d+)?$/.test(value) ? Number(value) * 1000 : Date.parse(value) - now;
  return Number.isFinite(delay) ? Math.max(nominal, Math.min(300_000, Math.max(0, delay))) : nominal;
}

export const binanceCataloguePolicy: CataloguePolicy = {
  database: 'safe-cex-binance-catalogue',
  ttlMs: 5000,
  leaseMs: 25_000,
  maxBytes: MAX_BYTES,
  maxEntries: 64,
  strict: true,
  keyFor,
  valid,
  failureDelay,
};
export const sharedBinanceCatalogue = new SharedCatalogue(binanceCataloguePolicy);
