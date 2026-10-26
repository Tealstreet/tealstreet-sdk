import { describe, expect, it } from 'vitest';

import { resolveBybitExecution } from './reference/bybit/bybit.fill-event';

/**
 * The numbers here are the ones a user actually saw (Discord, 2026-09-29):
 * KERNELUSDT trading around $0.0519, a market buy reported at $0.057 and three
 * reduce-only sells reported at $0.047 — roughly 10% adverse in both
 * directions, which is bybit's slippage-protection band rather than anything
 * that traded.
 */
describe('resolveBybitExecution', () => {
  it('reports the execution price, not the order price', () => {
    const marketBuy = { price: '0.057', avgPrice: '0.05192', qty: '1927', cumExecQty: '1927' };
    expect(resolveBybitExecution(marketBuy)).toEqual({ price: 0.05192, amount: 1927 });
  });

  it('reports the filled amount on a partial fill, not the order size', () => {
    const partial = { price: '0.051124', avgPrice: '0.051124', qty: '1927', cumExecQty: '400' };
    expect(resolveBybitExecution(partial)).toMatchObject({ amount: 400 });
  });

  // `avgPrice` is an empty string until something fills, and parseFloat('') is
  // NaN — which renders as a fill at no price at all.
  it('falls back to the order price when avgPrice is empty, never NaN', () => {
    const unfilled = { price: '0.051124', avgPrice: '', qty: '1927', cumExecQty: '0' };
    const result = resolveBybitExecution(unfilled);
    expect(result.price).toBe(0.051124);
    expect(Number.isNaN(result.price)).toBe(false);
  });

  it('survives a frame missing the execution fields entirely', () => {
    expect(resolveBybitExecution({ price: '100', qty: '2' })).toEqual({ price: 100, amount: 2 });
    expect(resolveBybitExecution({})).toEqual({ price: 0, amount: 0 });
  });

  it('leaves a limit order that filled at its limit unchanged', () => {
    const limit = { price: '50000', avgPrice: '50000', qty: '0.5', cumExecQty: '0.5' };
    expect(resolveBybitExecution(limit)).toEqual({ price: 50_000, amount: 0.5 });
  });
});
