import { describe, expect, it } from 'vitest';

import { adjust } from '@tealstreet/safe-cex/src/utils/safe-math';
import { DEFAULT_AMOUNT_PRECISION, DEFAULT_PRICE_PRECISION } from '@tealstreet/utils/src/currency/currencyUtils';
import { amountPrecisionOf, pricePrecisionOf } from './marketPrecision';

describe('the failure these helpers exist to prevent', () => {
  it('adjust() returns NaN for a zero step', () => {
    expect(adjust(61000.27, 0)).toBeNaN();
  });

  it('adjust() returns NaN for a missing step', () => {
    expect(adjust(61000.27, undefined as unknown as number)).toBeNaN();
  });

  it('a NaN size passes both of the guards an order path applies to it', () => {
    // This is why the NaN is dangerous rather than merely wrong: nothing
    // downstream rejects it, so it goes to the venue.
    const size = adjust(1.234, 0);
    expect(size > 0).toBe(false);
    expect(size <= 0).toBe(false);
  });
});

describe('pricePrecisionOf', () => {
  it('passes a real tick size through untouched', () => {
    expect(pricePrecisionOf({ precision: { price: 0.5 } })).toBe(0.5);
  });

  it.each([
    ['zero', { precision: { price: 0 } }],
    ['missing precision.price', { precision: { amount: 0.001 } }],
    ['missing precision', {}],
    ['missing market', undefined],
  ])('falls back on %s', (_label, market) => {
    expect(pricePrecisionOf(market)).toBe(DEFAULT_PRICE_PRECISION);
  });

  it('always yields a step adjust() can use', () => {
    expect(adjust(61000.27, pricePrecisionOf(undefined))).toBe(61000.27);
  });
});

describe('amountPrecisionOf', () => {
  it('passes a real lot step through untouched', () => {
    expect(amountPrecisionOf({ precision: { amount: 0.001 } })).toBe(0.001);
  });

  it.each([
    ['zero', { precision: { amount: 0 } }],
    ['missing precision.amount', { precision: { price: 0.5 } }],
    ['missing precision', {}],
    ['missing market', undefined],
  ])('falls back on %s', (_label, market) => {
    expect(amountPrecisionOf(market)).toBe(DEFAULT_AMOUNT_PRECISION);
  });

  it('always yields a step adjust() can use', () => {
    expect(adjust(1.23456789, amountPrecisionOf(undefined))).toBe(1.23456789);
  });
});
