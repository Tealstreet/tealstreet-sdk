import { DEFAULT_AMOUNT_PRECISION, DEFAULT_PRICE_PRECISION } from '@tealstreet/utils/src/currency/currencyUtils';

/**
 * The lot/tick step for a market, never zero and never undefined.
 *
 * `adjust(value, step)` computes `1 / Number(step)`. A step of `0` makes that
 * `Infinity` and `Math.round(v * Infinity) / Infinity` is **NaN**; a step of
 * `undefined` makes `Number(undefined)` NaN and the result NaN as well. Either
 * way the NaN is not caught anywhere downstream — `NaN > 0` and `NaN <= 0` are
 * both false, so the usual `size <= 0` guards wave it through and the order
 * reaches the venue with a NaN price and a NaN amount.
 *
 * So a market's precision is never read raw on an order path. `market` itself
 * is optional because most callers hold the result of a `markets.find(...)`,
 * and the structural parameter type keeps this reachable from callers holding
 * a partial market without dragging safe-cex's `Market` in.
 */

interface PrecisionCarrier {
  readonly precision?: {
    readonly price?: number;
    readonly amount?: number;
  };
}

/** Tick size for prices. Falls back to `DEFAULT_PRICE_PRECISION` (0.01). */
export const pricePrecisionOf = (market?: PrecisionCarrier): number =>
  market?.precision?.price || DEFAULT_PRICE_PRECISION;

/** Lot step for sizes. Falls back to `DEFAULT_AMOUNT_PRECISION` (1e-8). */
export const amountPrecisionOf = (market?: PrecisionCarrier): number =>
  market?.precision?.amount || DEFAULT_AMOUNT_PRECISION;
