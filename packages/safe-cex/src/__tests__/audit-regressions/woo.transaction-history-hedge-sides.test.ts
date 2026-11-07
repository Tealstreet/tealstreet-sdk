import { describe, expect, it } from 'vitest';

import { mapWooHedgeTransactionSide } from '../reference/woo/woo.utils';

// `/v3/trade/transactionHistory` reports `side` in two vocabularies depending
// on the account's position mode. Captured verbatim from a live WOO account
// (2026-08) in hedge mode: 210 of the 500 returned rows used the hedge
// vocabulary, and the mapper — which only understood BUY/SELL — returned null
// for every one of them, so 42% of that account's fills never reached the
// trade history or the position reconstruction built on top of it.
describe('woo hedge-mode transaction side mapping', () => {
  it.each([
    ['OPEN LONG', 'buy', 'long'],
    ['CLOSE LONG', 'sell', 'long'],
    ['OPEN SHORT', 'sell', 'short'],
    ['CLOSE SHORT', 'buy', 'short'],
  ])('maps %s to side=%s positionSide=%s', (raw, side, positionSide) => {
    expect(mapWooHedgeTransactionSide(raw)).toEqual({ side, positionSide });
  });

  it('normalizes separator and case variants', () => {
    // The endpoint is not consistent about spacing across API generations.
    for (const variant of ['open long', 'OPEN_LONG', ' Open  Long ', 'open_long']) {
      expect(mapWooHedgeTransactionSide(variant)).toEqual({ side: 'buy', positionSide: 'long' });
    }
  });

  it('returns undefined for one-way rows so the caller keeps its BUY/SELL path', () => {
    expect(mapWooHedgeTransactionSide('BUY')).toBeUndefined();
    expect(mapWooHedgeTransactionSide('SELL')).toBeUndefined();
  });

  it('returns undefined rather than guessing on unknown input', () => {
    for (const raw of ['', 'LONG', 'OPEN', 'CLOSE', 'OPEN MIDDLE', undefined as unknown as string]) {
      expect(mapWooHedgeTransactionSide(raw)).toBeUndefined();
    }
  });

  it('does not treat a hedge side as a plain direction', () => {
    // Regression guard: a naive `includes('LONG') -> buy` shortcut would make
    // CLOSE LONG a buy, doubling the position instead of flattening it.
    expect(mapWooHedgeTransactionSide('CLOSE LONG')!.side).toBe('sell');
    expect(mapWooHedgeTransactionSide('OPEN SHORT')!.side).toBe('sell');
  });
});
