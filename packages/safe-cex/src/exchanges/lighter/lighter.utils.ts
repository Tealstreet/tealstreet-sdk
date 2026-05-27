/**
 * Utility functions for Lighter exchange
 */

import type { Market, Order } from '../../types';

/**
 * Generate a unique client order ID
 * @param accountIndex - Account index
 * @returns Unique client order ID
 */
export function generateLighterClientId(accountIndex: string | bigint): string {
  return `lighter-${accountIndex}-${Date.now()}-${Math.random().toString().slice(2, 8)}`;
}

/**
 * Get precision from a number
 * @param num - Number to get precision from
 * @returns Number of decimal places
 */
export function getPrecision(num: number | string): number {
  const val = Number(num);
  if (isNaN(val)) {
    return 0;
  }

  const str = String(num).toLowerCase();
  if (str.includes('e-')) {
    const exponent = Number(str.split('e-')[1]);
    return Number.isFinite(exponent) ? exponent : 0;
  }

  const strList = str.split('.');
  return strList.length === 2 ? strList[1].length : 0;
}

/**
 * Remove 0x prefix from a hash
 * @param hash - Hash with potential 0x prefix
 * @returns Hash without prefix
 */
export function dropPrefix(hash: string): string {
  return hash.replace(/^0x/, '');
}

/**
 * Add 0x prefix to a hash if not present
 * @param hash - Hash without prefix
 * @returns Hash with prefix
 */
export function addPrefix(hash: string): string {
  return hash.startsWith('0x') ? hash : `0x${hash}`;
}

/**
 * Convert string to BigInt safely
 * @param value - String value
 * @returns BigInt value
 */
export function stringToBigInt(value: string): bigint {
  return BigInt(value);
}

/**
 * Convert BigInt to string safely
 * @param value - BigInt value
 * @returns String value
 */
export function bigIntToString(value: bigint): string {
  return value.toString();
}

export type LighterCancelTarget = {
  orderId: string;
  marketId: number;
};

const DEFAULT_LIGHTER_DECIMALS = 18;
const MAX_SAFE_DECIMALS = 30;

function readNumber(
  source: Record<string, unknown> | undefined,
  keys: string[],
  opts: { allowZero?: boolean } = {},
): number | undefined {
  if (!source) {
    return undefined;
  }

  for (const key of keys) {
    const value = Number(source[key]);
    const isValid = opts.allowZero ? value >= 0 : value > 0;
    if (Number.isFinite(value) && isValid) {
      return value;
    }
  }

  return undefined;
}

function decimalsFromScale(scale: number | undefined): number | undefined {
  if (!Number.isFinite(scale) || !scale || scale <= 0) {
    return undefined;
  }

  const log10Scale = Math.log10(scale);
  if (!Number.isInteger(log10Scale)) {
    return undefined;
  }

  return log10Scale;
}

export function parseLighterApiKeyIndex(value: unknown): number | undefined {
  if (value === null || value === undefined) {
    return undefined;
  }

  if (typeof value === 'number' && Number.isFinite(value)) {
    return Math.trunc(value);
  }

  const rawValue = String(value).trim();
  if (!rawValue) {
    return undefined;
  }

  const match = rawValue.match(/-?\d+/);
  if (!match) {
    return undefined;
  }

  const parsed = Number.parseInt(match[0], 10);
  if (!Number.isFinite(parsed)) {
    return undefined;
  }

  return parsed;
}

export function toLighterApiKeyIndexString(value: unknown, fallback: string = '0'): string {
  const parsed = parseLighterApiKeyIndex(value);
  if (parsed === undefined) {
    return fallback;
  }

  return String(parsed);
}

export function parseLighterPositiveIntegerString(value: unknown): string | undefined {
  if (value === null || value === undefined) {
    return undefined;
  }

  const rawValue = String(value).trim();
  if (!rawValue) {
    return undefined;
  }

  if (!/^\d+$/.test(rawValue)) {
    return undefined;
  }

  const normalized = rawValue.replace(/^0+(?=\d)/, '');
  return normalized || '0';
}

export function isLighterL1Address(value: unknown): boolean {
  if (value === null || value === undefined) {
    return false;
  }

  return /^0x[a-fA-F0-9]{40}$/.test(String(value).trim());
}

/**
 * Resolve cancellable order targets from current order objects.
 * This is computed before store mutation to avoid race conditions.
 */
export function resolveLighterCancelTargets(
  orders: Order[],
  marketsBySymbol: Record<string, Market>,
): LighterCancelTarget[] {
  return orders.flatMap((order) => {
    const market = marketsBySymbol[order.symbol];
    if (!market) {
      return [];
    }

    const parsedMarketId = Number.parseInt(market.id, 10);
    if (!Number.isFinite(parsedMarketId)) {
      return [];
    }

    return [{ orderId: order.id, marketId: parsedMarketId }];
  });
}

function clampDecimals(decimals: number): number {
  if (!Number.isFinite(decimals)) {
    return DEFAULT_LIGHTER_DECIMALS;
  }

  return Math.max(0, Math.min(MAX_SAFE_DECIMALS, Math.trunc(decimals)));
}

export function getLighterAmountDecimals(market: Market): number {
  const info = market.info as Record<string, unknown> | undefined;
  const amountDecimals = readNumber(
    info,
    ['amountDecimals', 'quantityDecimals', 'baseDecimals', 'sizeDecimals', 'sizePrecision'],
    { allowZero: true },
  );

  if (amountDecimals !== undefined) {
    return clampDecimals(amountDecimals);
  }

  const amountScale = readNumber(info, ['amountScale', 'quantityScale', 'baseScale', 'sizeScale']);
  const amountDecimalsFromScale = decimalsFromScale(amountScale);
  if (amountDecimalsFromScale !== undefined) {
    return clampDecimals(amountDecimalsFromScale);
  }

  return clampDecimals(getPrecision(market.precision.amount));
}

export function getLighterPriceDecimals(market: Market): number {
  const info = market.info as Record<string, unknown> | undefined;
  const priceDecimals = readNumber(info, ['priceDecimals', 'quoteDecimals', 'tickDecimals', 'pricePrecision'], {
    allowZero: true,
  });

  if (priceDecimals !== undefined) {
    return clampDecimals(priceDecimals);
  }

  const priceScale = readNumber(info, ['priceScale', 'quoteScale', 'tickScale']);
  const priceDecimalsFromScale = decimalsFromScale(priceScale);
  if (priceDecimalsFromScale !== undefined) {
    return clampDecimals(priceDecimalsFromScale);
  }

  return clampDecimals(getPrecision(market.precision.price));
}

function normalizeNonNegativeNumber(value: unknown): number {
  const parsed = Number.parseFloat(String(value ?? ''));
  if (!Number.isFinite(parsed) || parsed < 0) {
    return 0;
  }
  return parsed;
}

export type LighterOrderMinimums = {
  minBaseAmount: number;
  minQuoteAmount: number;
};

export function getLighterOrderMinimums(market: Market): LighterOrderMinimums {
  const info = market.info as Record<string, unknown> | undefined;
  return {
    minBaseAmount: normalizeNonNegativeNumber(info?.min_base_amount ?? info?.minBaseAmount),
    minQuoteAmount: normalizeNonNegativeNumber(info?.min_quote_amount ?? info?.minQuoteAmount),
  };
}

export function toLighterScaledInt(value: number, decimals: number): bigint {
  if (!Number.isFinite(value)) {
    throw new Error(`Cannot scale non-finite value: ${value}`);
  }

  const safeDecimals = clampDecimals(decimals);
  const scale = 10 ** safeDecimals;

  if (!Number.isFinite(scale)) {
    throw new Error(`Cannot compute scale for decimals: ${safeDecimals}`);
  }

  return BigInt(Math.round(value * scale));
}
