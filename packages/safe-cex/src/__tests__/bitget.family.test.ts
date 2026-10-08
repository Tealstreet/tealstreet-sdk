import { describe, expect, it } from 'vitest';

import {
  BITGET_LIKE_EXCHANGES,
  isBitgetLikeExchange,
  isBitgetVirtualAttachedTpSlOrder,
  isBitgetVirtualAttachedTpSlOrderId,
} from '../exchanges/bitget/bitget.family';

describe('bitget.family helpers', () => {
  it('recognises every currently-registered Bitget variant', () => {
    // Regression: extending the family requires updating this Set. If a new
    // variant is registered without being added here, bulk-cancel by side and
    // any other consumer of `isBitgetLikeExchange` will silently skip it.
    expect(BITGET_LIKE_EXCHANGES).toEqual(new Set(['bitget', 'bitgetusdc', 'bitgetinverse']));
    expect(isBitgetLikeExchange('bitget')).toBe(true);
    expect(isBitgetLikeExchange('bitgetusdc')).toBe(true);
    expect(isBitgetLikeExchange('bitgetinverse')).toBe(true);
  });

  it('rejects non-Bitget exchanges and non-string inputs', () => {
    expect(isBitgetLikeExchange('bybit')).toBe(false);
    expect(isBitgetLikeExchange('binance')).toBe(false);
    expect(isBitgetLikeExchange(undefined)).toBe(false);
    expect(isBitgetLikeExchange('')).toBe(false);
  });

  it('matches Tealstreet-synthesized virtual TP/SL order ids by suffix', () => {
    expect(isBitgetVirtualAttachedTpSlOrderId('123456__stop_loss')).toBe(true);
    expect(isBitgetVirtualAttachedTpSlOrderId('123456__take_profit')).toBe(true);
    expect(isBitgetVirtualAttachedTpSlOrderId('123456')).toBe(false);
    expect(isBitgetVirtualAttachedTpSlOrderId(undefined)).toBe(false);
  });

  it('combines family membership and id suffix for the composite predicate', () => {
    expect(isBitgetVirtualAttachedTpSlOrder({ id: '1__stop_loss' }, 'bitget')).toBe(true);
    expect(isBitgetVirtualAttachedTpSlOrder({ id: '1__take_profit' }, 'bitgetusdc')).toBe(true);
    // Right suffix, wrong exchange: leave the caller alone.
    expect(isBitgetVirtualAttachedTpSlOrder({ id: '1__stop_loss' }, 'bybit')).toBe(false);
    // Bitget exchange, but a real order id: not virtual.
    expect(isBitgetVirtualAttachedTpSlOrder({ id: '1' }, 'bitget')).toBe(false);
    // Missing exchange context (edge race during onboarding): safe fallback.
    expect(isBitgetVirtualAttachedTpSlOrder({ id: '1__stop_loss' }, undefined)).toBe(false);
  });
});
