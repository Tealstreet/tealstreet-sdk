import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  getHyperliquidRestRequestWeight,
  RateLimiter,
} from '../reference/hyperliquid/hyperliquid.rateLimiter';
import { ENDPOINTS, INFO_TYPES } from '../../exchanges/hyperliquid/hyperliquid.types';

describe('Hyperliquid REST weighted rate limiter', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('classifies /info request weights by Hyperliquid type', () => {
    expect(getHyperliquidRestRequestWeight({ url: ENDPOINTS.INFO, data: { type: INFO_TYPES.USER_ROLE } })).toBe(60);
    expect(
      getHyperliquidRestRequestWeight({ url: ENDPOINTS.INFO, data: { type: INFO_TYPES.PERPS_CLEARINGHOUSE_STATE } }),
    ).toBe(2);
    expect(
      getHyperliquidRestRequestWeight({ url: ENDPOINTS.INFO, data: { type: INFO_TYPES.PERPS_META_AND_ASSET_CTXS } }),
    ).toBe(20);
    expect(
      getHyperliquidRestRequestWeight({
        url: ENDPOINTS.INFO,
        data: { type: INFO_TYPES.CANDLE_SNAPSHOT, limit: 120 },
      }),
    ).toBe(22);
    expect(
      getHyperliquidRestRequestWeight({
        url: ENDPOINTS.INFO,
        data: JSON.stringify({ type: INFO_TYPES.HISTORICAL_ORDERS, limit: 41 }),
      }),
    ).toBe(22);
  });

  it('classifies /exchange batch weights from action size', () => {
    expect(
      getHyperliquidRestRequestWeight({
        url: ENDPOINTS.EXCHANGE,
        data: { action: { type: 'updateLeverage' } },
      }),
    ).toBe(1);
    expect(
      getHyperliquidRestRequestWeight({
        url: ENDPOINTS.EXCHANGE,
        data: { action: { type: 'order', orders: Array.from({ length: 79 }) } },
      }),
    ).toBe(2);
  });

  it('paces queued requests smoothly instead of allowing a startup burst', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);

    const limiter = new RateLimiter(2, 1_000);
    const first = vi.fn();
    const second = vi.fn();

    await limiter.waitForToken(1).then(first);
    const queued = limiter.waitForToken(1).then(second);

    await Promise.resolve();
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(500);
    await queued;

    expect(second).toHaveBeenCalledTimes(1);
  });
});

/**
 * These two numbers are what make the HIP-3 startup budget arithmetic work, and
 * nothing pinned them: `metaAndAssetCtxs` at weight 20 against a 50ms-per-unit
 * serial pacer means EACH HIP-3 dex reserves ~1s of startup. That is why a flat 8s
 * budget stopped fitting once mainnet passed ~6 dexes and every HIP-3 market
 * disappeared. If either number moves, `HIP3_PER_DEX_LOAD_BUDGET_MS` must move too.
 */
describe('the per-dex cost behind the HIP-3 startup budget', () => {
  it('charges a HIP-3 market metadata call 20 weight', () => {
    expect(
      getHyperliquidRestRequestWeight({
        url: ENDPOINTS.INFO,
        data: { type: INFO_TYPES.PERPS_META_AND_ASSET_CTXS, dex: 'xyz' },
      }),
    ).toBe(20);
  });

  it('paces those calls serially at ~1s each, so ten dexes span ~9s of waiting', async () => {
    vi.useFakeTimers();
    const limiter = new RateLimiter();

    const done: number[] = [];
    const calls = Array.from({ length: 10 }, (_, i) => limiter.waitForToken(20).then(() => done.push(i)));

    // The first token is granted immediately, so the Nth is released at (N-1) seconds:
    // eight seconds in, the tenth dex still has not been allowed to start.
    await vi.advanceTimersByTimeAsync(8_000);
    expect(done.length).toBe(9);

    await vi.advanceTimersByTimeAsync(1_000);
    await Promise.all(calls);
    expect(done.length).toBe(10);
  });
});
