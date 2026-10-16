import { describe, expect, it } from 'vitest';

import { InProcessFallbackMarketLock } from './lock';

describe('InProcessFallbackMarketLock', () => {
  it('serializes acquisitions for the same key', async () => {
    const lock = new InProcessFallbackMarketLock();
    const first = await lock.acquire('bybit');
    const secondPromise = lock.acquire('bybit', { retryDelayMs: 1 });

    let secondResolved = false;
    secondPromise.then(() => {
      secondResolved = true;
    });
    await Promise.resolve();
    expect(secondResolved).toBe(false);

    first.release();
    const second = await secondPromise;

    expect(second.acquired).toBe(true);
    second.release();
  });

  it('times out waiting callers without releasing the active lock', async () => {
    const lock = new InProcessFallbackMarketLock();
    const first = await lock.acquire('bybit');
    const second = await lock.acquire('bybit', { maxWaitMs: 0 });

    expect(second.acquired).toBe(false);

    first.release();
    const third = await lock.acquire('bybit', { maxWaitMs: 0 });
    expect(third.acquired).toBe(true);
    third.release();
  });
});
