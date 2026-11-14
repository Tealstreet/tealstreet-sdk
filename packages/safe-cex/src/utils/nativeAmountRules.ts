import type { Market } from '../types';

import { roundToNearestMultiple } from '@tealstreet/math/src/roundToNearestMultiple';
import { DEFAULT_AMOUNT_PRECISION } from '@tealstreet/utils/src/currency/currencyUtils';

/** Only the sizing fields are needed; preview contexts need not invent a full Market. */
export type NativeAmountMarket = Pick<Market, 'symbol' | 'contractSize' | 'isInverse'> & {
  precision?: { amount?: number };
  limits?: { amount?: { max?: number } };
};

/** Whether amount precision and limits need base-to-native conversion for this venue.
 * MEXC publishes base units while placement takes contracts; market validity is checked separately.
 * Missing or unrecognised names retain the existing native-rails default. */
// Adding another base-amount-rail venue requires widening this type predicate and
// auditing hard-coded MEXC messages in callers, e.g. tealstreet-next's
// chartBracketAmount.ts: 'Cannot place MEXC bracket'.
export const venueUsesBaseAmountRails = (exchangeName?: string): exchangeName is 'mexc' => exchangeName === 'mexc';

/** MEXC publishes base-sized amount rails while placement takes native contracts. */
export const nativeAmountRules = (
  market: NativeAmountMarket,
  exchangeName?: string,
): {
  step: number;
  basePerContract?: number;
  maximum?: number;
} => {
  if (!venueUsesBaseAmountRails(exchangeName)) return { step: market.precision?.amount || DEFAULT_AMOUNT_PRECISION };
  const size = market.contractSize;
  if (size === undefined || !Number.isFinite(size) || size <= 0 || market.isInverse) {
    throw new Error(`MEXC order units are unknown for '${market.symbol}'`);
  }
  const step = (market.precision?.amount ?? NaN) / size;
  if (!Number.isFinite(step) || step <= 0) {
    throw new Error(`MEXC native lot step is unknown for '${market.symbol}'`);
  }
  const maximum = market.limits?.amount?.max;
  return {
    step,
    basePerContract: size,
    maximum: maximum !== undefined && Number.isFinite(maximum) && maximum > 0 ? maximum / size : undefined,
  };
};

export const sameNativeAmount = (a: number, b: number): boolean =>
  Number.isFinite(a) &&
  Number.isFinite(b) &&
  Math.abs(a - b) <= Number.EPSILON * Math.max(1, Math.abs(a), Math.abs(b)) * 8;

/** A reduce must preserve the requested native quantity or refuse before sending. */
export const assertNativeReduceAmount = (amount: number, market: NativeAmountMarket, exchangeName?: string): void => {
  if (!venueUsesBaseAmountRails(exchangeName)) return;
  const { step, maximum } = nativeAmountRules(market, exchangeName);
  if (!Number.isFinite(amount) || amount <= 0) {
    throw new Error(`Invalid MEXC native order size for '${market.symbol}'`);
  }
  if (maximum !== undefined && amount > maximum && !sameNativeAmount(amount, maximum)) {
    throw new Error(`MEXC native order size ${amount} exceeds maximum ${maximum} for '${market.symbol}'`);
  }
  if (!sameNativeAmount(amount, roundToNearestMultiple(amount, step))) {
    throw new Error(`MEXC native order size ${amount} is not a multiple of lot step ${step} for '${market.symbol}'`);
  }
};
