import type { HttpPacket } from '@tealstreet/safe-cex-wasm/host';

import { SharedCatalogue } from './sharedCatalogue';

const MAX_BYTES = 1_048_576;
function keyFor(request: HttpPacket & { intent?: string }): string | undefined {
  if (request.intent !== 'public' || request.method !== 'POST') return;
  let url: URL;
  try {
    url = new URL(request.url);
  } catch {
    return;
  }
  if (url.pathname !== '/info' || url.search || url.hash || url.protocol !== 'https:' || url.username || url.password)
    return;
  if (Object.keys(request.headers).some((name) => !['content-type', 'accept'].includes(name.toLowerCase()))) return;
  try {
    const body = JSON.parse(request.body ?? 'null');
    if (!body || !['metaAndAssetCtxs', 'spotMetaAndAssetCtxs', 'perpDexs'].includes(body.type)) return;
    if (Object.keys(body).some((name) => name !== 'type' && !(name === 'dex' && body.type === 'metaAndAssetCtxs')))
      return;
    if (body.dex !== undefined && (typeof body.dex !== 'string' || !/^[a-zA-Z0-9_-]{0,64}$/.test(body.dex))) return;
    return `${url.origin}|${body.type}|${body.dex ?? ''}`;
  } catch {
    return;
  }
}
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const decimals = (value: unknown): boolean => Number.isInteger(value) && Number(value) >= 0 && Number(value) <= 18;
const index = (value: unknown): boolean =>
  Number.isInteger(value) && Number(value) >= 0 && Number(value) <= 0xffff_ffff;
const numeric = (value: unknown): boolean =>
  (typeof value === 'number' || (typeof value === 'string' && value.trim() !== '')) && Number.isFinite(Number(value));
// Cache eligibility is conservative; native decoding still owns catalogue semantics.
const valid = (key: string, bytes: Uint8Array): boolean => {
  if (bytes.length > MAX_BYTES) return false;
  try {
    const value = JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes));
    if (key.includes('|perpDexs|'))
      return (
        Array.isArray(value) &&
        value[0] === null &&
        value.slice(1).every((row) => record(row) && typeof row.name === 'string')
      );
    if (
      !Array.isArray(value) ||
      value.length !== 2 ||
      !value[0] ||
      !Array.isArray(value[0].universe) ||
      !Array.isArray(value[1])
    )
      return false;
    const spot = key.includes('|spotMetaAndAssetCtxs|');
    if (
      spot &&
      (!Array.isArray(value[0].tokens) ||
        !value[0].tokens.every(
          (row: unknown) => record(row) && typeof row.name === 'string' && index(row.index) && decimals(row.szDecimals),
        ))
    )
      return false;
    return (
      value[0].universe.length === value[1].length &&
      value[0].universe.every(
        (row: unknown) =>
          record(row) &&
          typeof row.name === 'string' &&
          (spot
            ? index(row.index) && Array.isArray(row.tokens) && row.tokens.length === 2 && row.tokens.every(index)
            : decimals(row.szDecimals) && (row.maxLeverage === undefined || numeric(row.maxLeverage))),
      ) &&
      value[1].every(
        (row: unknown) =>
          record(row) &&
          ['markPx', 'oraclePx', 'prevDayPx', 'dayNtlVlm', 'dayBaseVlm', 'openInterest', 'funding'].every(
            (field) => row[field] === undefined || numeric(row[field]),
          ) &&
          (row.midPx == null || numeric(row.midPx)),
      )
    );
  } catch {
    return false;
  }
};

export const hyperliquidCataloguePolicy = {
  database: 'safe-cex-hyperliquid-catalogue',
  ttlMs: 5000,
  leaseMs: 3000,
  maxBytes: MAX_BYTES,
  maxEntries: 64,
  keyFor,
  valid,
};
export const sharedHyperliquidCatalogue = new SharedCatalogue(hyperliquidCataloguePolicy);
