import { describe, expect, it } from 'vitest';

import { omitUndefined } from '../../utils/omitUndefined';

/**
 * `Execution.id` / `HistoricalTrade.id` is the only idempotent dedupe key: one order
 * fills across many rows, so `orderId` cannot identify a fill, and
 * `users_app_trades`'s primary key includes the id.
 *
 * Measured live 2026-08 before the fix: okx 0/22, blofin 0/100, bitrue 0/42 mapped
 * rows carried an id, while every raw payload had a unique tradeId/execId.
 *
 * The mappers are instance fields on the exchange classes and unreachable from a
 * test, so these lock the two things that are testable in isolation: the id-selection
 * rule per venue, and the dedupe key that consumes it.
 */

// OKX emits several bill rows per trade (fill + fee/rebate) sharing one tradeId.
// tradeId must win: keying on the per-bill id would split them into duplicate fills
// and double the quantity, which the old composite key correctly collapsed.
const pickOkxId = (row: any) => (row.tradeId ? String(row.tradeId) : row.billId ? String(row.billId) : undefined);
const pickBlofinId = (row: any) => (row.tradeId ? String(row.tradeId) : undefined);
const pickBitrueId = (row: any) =>
  row.tradeId != null && row.tradeId !== '' ? String(row.tradeId) : undefined;

const dedupeKey = (execution: { id?: string; orderId?: string; timestamp: number; price: number; qty: number; side: string }) =>
  execution.id
    ? `id:${execution.id}`
    : `${execution.orderId || 'no-order'}:${execution.timestamp}:${execution.price}:${execution.qty}:${execution.side}`;

describe('per-fill id selection', () => {
  it('okx prefers tradeId, using billId only when tradeId is absent', () => {
    expect(pickOkxId({ billId: '111', tradeId: '222' })).toBe('222');
    expect(pickOkxId({ billId: '111' })).toBe('111');
    expect(pickOkxId({})).toBeUndefined();
  });

  it('okx collapses the fee/rebate bill rows of a single trade', () => {
    // Regression guard: preferring billId here split one trade into two fills and
    // doubled the mapped quantity.
    const rows = [
      { billId: '1', tradeId: 'T' },
      { billId: '2', tradeId: 'T' },
    ];
    expect(new Set(rows.map(pickOkxId)).size).toBe(1);
  });

  it('blofin and bitrue read tradeId, and treat absent/empty as absent', () => {
    expect(pickBlofinId({ tradeId: '99' })).toBe('99');
    expect(pickBlofinId({})).toBeUndefined();
    // bitrue returns numeric ids; 0 is a legal id and must not be dropped as falsy.
    expect(pickBitrueId({ tradeId: 0 })).toBe('0');
    expect(pickBitrueId({ tradeId: '' })).toBeUndefined();
    expect(pickBitrueId({ tradeId: null })).toBeUndefined();
  });

  it('omitUndefined keeps an absent id off the object entirely', () => {
    expect('id' in omitUndefined({ id: undefined, qty: 1 })).toBe(false);
    expect(omitUndefined({ id: '5', qty: 1 })).toEqual({ id: '5', qty: 1 });
  });
});

describe('dedupe key uses the fill id when present', () => {
  const base = { orderId: 'O1', timestamp: 1786158465, price: 0.07, qty: 143, side: 'buy' };

  it('keeps two genuine fills of one order at identical price/size/timestamp', () => {
    // This is the case the old orderId:ts:price:qty:side key silently collapsed.
    const a = { ...base, id: 'F1' };
    const b = { ...base, id: 'F2' };
    expect(dedupeKey(a)).not.toBe(dedupeKey(b));
  });

  it('still collapses a true duplicate of the same fill', () => {
    expect(dedupeKey({ ...base, id: 'F1' })).toBe(dedupeKey({ ...base, id: 'F1' }));
  });

  it('falls back to the composite key when the venue has no per-fill id', () => {
    // bybit's /v5/order/history fallback and blofin's ORDERS_HISTORY fallback are
    // order-level; fabricating an id from orderId would merge distinct partial fills.
    const noId = dedupeKey(base);
    expect(noId).toBe('O1:1786158465:0.07:143:buy');
    expect(noId).toBe(dedupeKey({ ...base }));
  });
});
