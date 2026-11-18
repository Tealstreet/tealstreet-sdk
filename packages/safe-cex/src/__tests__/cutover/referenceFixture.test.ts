import { describe, expect, it, vi } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { nativeTicker } from './nativeTicker';
import { applyReferenceFixture } from './referenceFixture';

describe('retained protocol producer completion', () => {
  it('retains independent completion reports until every native account section arrives', () => {
    const store = new FrameStore();
    applyReferenceFixture(store, { orders: [], loaded: { orders: true } });
    expect(store.loaded.orders).toBe(false);
    applyReferenceFixture(store, { positions: [], loaded: { positions: true } });
    expect(store.loaded.positions).toBe(false);
    applyReferenceFixture(store, {
      balance: { total: 100, free: 90, used: 10, upnl: 0 },
      loaded: { balance: true },
    });
    expect(store.loaded).toMatchObject({ orders: true, positions: true, balance: true });
  });
});

it('emits native sparse ticker deltas and keeps unchanged row identities', () => {
  const store = new FrameStore();
  applyReferenceFixture(store, {
    tickers: [
      nativeTicker({ id: 'BTC', symbol: 'BTC', last: 100 }),
      nativeTicker({ id: 'ETH', symbol: 'ETH', last: 200 }),
    ],
  });
  const previous = store.tickers;
  const apply = vi.spyOn(store, 'apply');
  applyReferenceFixture(store, { tickers: [{ ...previous[0], last: 101 }, previous[1]] });
  const frame = apply.mock.calls[0][0];
  expect(frame.tickers()).toBeUndefined();
  expect(Array.from(frame.tickerDeltas() ?? [])).toEqual([0, 4, 101]);
  expect(store.tickers[0].last).toBe(101);
  expect(previous[0].last).toBe(100);
  expect(store.tickers[1]).toBe(previous[1]);
});
