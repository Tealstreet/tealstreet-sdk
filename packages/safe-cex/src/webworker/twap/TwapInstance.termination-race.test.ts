import type { Exchange } from '../../exchanges/base';
import type { TwapOptions } from './TwapOptions';

import { afterEach, expect, it, vi } from 'vitest';

import { OrderSide } from '../../types';
import { TwapInstance } from './TwapInstance';
import { twapManager } from './TwapManager';
import { TwapStatus } from './TwapStatus';

afterEach(() => {
  twapManager.runningTwaps.clear();
  vi.useRealTimers();
});

it('does not resurrect a TWAP when an in-flight placement settles after termination', async () => {
  vi.useFakeTimers();
  let finish!: () => void;
  const placement = new Promise<void>((resolve) => {
    finish = resolve;
  });
  const placeOrder = vi.fn(() => placement);
  const exchange = { placeOrder } as unknown as Exchange;
  const options = {
    accountName: 'same-name',
    activeMarket: { symbol: 'BTCUSDT' },
    size: 2,
    duration: 1,
    ordersCount: 2,
    side: OrderSide.Buy,
    randomnessPercentage: 0,
    useLimitOrders: false,
    useChaserOrders: false,
  } as TwapOptions;
  const instance = new TwapInstance(exchange, { postMessage: vi.fn() } as unknown as Worker, options, vi.fn());
  twapManager.add(instance);
  const started = instance.start();
  await instance.terminate();
  finish();
  await started;
  await vi.advanceTimersByTimeAsync(120_000);
  expect(placeOrder).toHaveBeenCalledTimes(1);
  expect(instance.state.status).toBe(TwapStatus.Completed);
  expect(twapManager.runningTwaps.has(instance.id)).toBe(false);
  expect(vi.getTimerCount()).toBe(0);
});
