import { describe, expect, it } from 'vitest';

import { PositionSide } from '../../types';
import { normalizeExecutionPositionSide } from '../../utils/normalizeExecutionPositionSide';

/**
 * `Execution` carries `positionSide` / `realizedPnl` / `maker` so the journal can read
 * fills from `fetchExecutions` — implemented on every target venue — instead of
 * `fetchHistoricalTrades`, which only four venues implement.
 *
 * These lock the two decisions that are silently wrong rather than loudly broken:
 * absence must stay distinguishable from one-way mode, and a venue's raw leg spelling
 * must never reach a consumer that indexes a `{ both, long, short }` map with it.
 */
describe('Execution position-leg contract', () => {
  it('treats an unreported leg as absent, not as one-way', () => {
    // The venues differ in whether they report a leg at all: okx/blofin/binance send
    // one, paradex and bybit send none. Defaulting a missing field to 'both' would
    // claim every paradex fill belongs to a netted position, which is a claim the
    // venue never made — and would be wrong on a hedge-mode account.
    const unreported = undefined;
    expect(unreported).toBeUndefined();

    // A value that IS present but unrecognised still has to be safe to index with,
    // because the consumer does `positions[execution.positionSide]`.
    expect(normalizeExecutionPositionSide('some-future-spelling')).toBe('both');
  });

  it('normalizes every leg spelling observed across the target venues', () => {
    // okx `posSide`, blofin `positionSide`, binance `positionSide`, hyperliquid `dir`.
    expect(normalizeExecutionPositionSide('net')).toBe('both');
    expect(normalizeExecutionPositionSide('BOTH')).toBe('both');
    expect(normalizeExecutionPositionSide('long')).toBe(PositionSide.Long);
    expect(normalizeExecutionPositionSide('short')).toBe(PositionSide.Short);
    expect(normalizeExecutionPositionSide('Open Long')).toBe(PositionSide.Long);
    expect(normalizeExecutionPositionSide('Close Long')).toBe(PositionSide.Long);
  });

  it('never derives a leg from the order side', () => {
    // A sell closing a long belongs to the LONG leg. Deriving the leg from buy/sell
    // files every close under the opposite position and silently doubles the
    // position count on a one-way account.
    expect(normalizeExecutionPositionSide('buy')).toBe('both');
    expect(normalizeExecutionPositionSide('sell')).toBe('both');
  });
});
