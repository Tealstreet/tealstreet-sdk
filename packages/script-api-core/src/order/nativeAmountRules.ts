import type { Market } from '@tealstreet/safe-cex/src/types';

import { roundToNearestMultiple } from '@tealstreet/math/src/roundToNearestMultiple';
import { DEFAULT_AMOUNT_PRECISION } from '@tealstreet/utils/src/currency/currencyUtils';
import { ScriptValidationError } from './ScriptValidationError';

/** MEXC publishes base-sized amount rails while placement takes native contracts. */
export const nativeAmountRules = (
  market: Market,
  exchangeName?: string,
): {
  step: number;
  basePerContract?: number;
  maximum?: number;
} => {
  if (exchangeName !== 'mexc') return { step: market.precision?.amount || DEFAULT_AMOUNT_PRECISION };
  const size = market.contractSize;
  if (size === undefined || !Number.isFinite(size) || size <= 0 || market.isInverse) {
    throw new ScriptValidationError(`MEXC order units are unknown for '${market.symbol}'`);
  }
  const step = market.precision?.amount / size;
  if (!Number.isFinite(step) || step <= 0) {
    throw new ScriptValidationError(`MEXC native lot step is unknown for '${market.symbol}'`);
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
export const assertNativeReduceAmount = (amount: number, market: Market, exchangeName?: string): void => {
  if (exchangeName !== 'mexc') return;
  const { step, maximum } = nativeAmountRules(market, exchangeName);
  if (!Number.isFinite(amount) || amount <= 0) {
    throw new ScriptValidationError(`Invalid MEXC native order size for '${market.symbol}'`);
  }
  if (maximum !== undefined && amount > maximum && !sameNativeAmount(amount, maximum)) {
    throw new ScriptValidationError(
      `MEXC native order size ${amount} exceeds maximum ${maximum} for '${market.symbol}'`,
    );
  }
  if (!sameNativeAmount(amount, roundToNearestMultiple(amount, step))) {
    throw new ScriptValidationError(
      `MEXC native order size ${amount} is not a multiple of lot step ${step} for '${market.symbol}'`,
    );
  }
};
