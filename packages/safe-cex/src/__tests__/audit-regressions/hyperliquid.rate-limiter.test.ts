import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  getHyperliquidRestRequestWeight,
  RateLimiter,
} from '../../exchanges/hyperliquid/hyperliquid.rateLimiter';
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
