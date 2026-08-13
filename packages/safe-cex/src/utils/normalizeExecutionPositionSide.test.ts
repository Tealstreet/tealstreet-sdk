import { describe, expect, it } from 'vitest';

import { PositionSide } from '../types';
import { normalizeExecutionPositionSide } from './normalizeExecutionPositionSide';

describe('normalizeExecutionPositionSide', () => {
  it('maps every one-way spelling observed across venues to "both"', () => {
    // okx/blofin 'net', phemex 'Merged', binance 'BOTH', bitget 'net'.
    for (const raw of ['net', 'NET', 'Merged', 'BOTH', 'both', 'oneway', '']) {
      expect(normalizeExecutionPositionSide(raw)).toBe('both');
    }
  });

  it('maps hedge-mode leg names case-insensitively', () => {
    expect(normalizeExecutionPositionSide('long')).toBe(PositionSide.Long);
    expect(normalizeExecutionPositionSide('LONG')).toBe(PositionSide.Long);
    expect(normalizeExecutionPositionSide('Short')).toBe(PositionSide.Short);
  });

  it('keeps a hyperliquid close on the leg it closes, not the opposite one', () => {
    // `dir` describes the position leg. "Close Long" sells, but it belongs to
    // the long leg — treating it as short would file every close under the
    // wrong position.
    expect(normalizeExecutionPositionSide('Open Long')).toBe(PositionSide.Long);
    expect(normalizeExecutionPositionSide('Close Long')).toBe(PositionSide.Long);
    expect(normalizeExecutionPositionSide('Open Short')).toBe(PositionSide.Short);
    expect(normalizeExecutionPositionSide('Close Short')).toBe(PositionSide.Short);
  });

  it('refuses order-side words, which carry no leg information', () => {
    // A venue reporting only buy/sell has not told us the leg. Inferring one
    // would silently split a one-way account across two phantom positions.
    expect(normalizeExecutionPositionSide('buy')).toBe('both');
    expect(normalizeExecutionPositionSide('sell')).toBe('both');
  });

  it('falls back to "both" for nullish and unexpected values', () => {
    expect(normalizeExecutionPositionSide(undefined)).toBe('both');
    expect(normalizeExecutionPositionSide(null)).toBe('both');
    expect(normalizeExecutionPositionSide(0)).toBe('both');
    expect(normalizeExecutionPositionSide('Long > Short')).toBe('both');
  });
});
