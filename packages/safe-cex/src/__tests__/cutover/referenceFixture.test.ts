import { describe, expect, it } from 'vitest';

import { FrameStore } from '../../store/frameStore';
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
