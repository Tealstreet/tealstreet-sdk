import type { ExchangeName, ExchangeOptions, Order, StoreData, Writable } from '../../types';

import { describe, expect, it } from 'vitest';

import { OrderSide, OrderStatus, OrderType } from '../../types';
import { TwapOrderEnrichmentPostprocessor } from './TwapOrderEnrichmentPostprocessor';

const createOrder = (id: string, extra?: Order['extra']): Order => ({
  id,
  status: OrderStatus.Open,
  symbol: 'BTCUSDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 50_000,
  amount: 1,
  filled: 0,
  remaining: 1,
  reduceOnly: false,
  lastUpdate: 1,
  extra,
});

describe('TwapOrderEnrichmentPostprocessor', () => {
  it('marks only orders owned by an active direct TWAP limit slice', () => {
    const processor = new TwapOrderEnrichmentPostprocessor(
      'test-account',
      'bybit' as ExchangeName,
      {} as ExchangeOptions,
    );
    const twapOrder = createOrder('twap-order', { remainingNative: 1 });
    const manualOrder = createOrder('manual-order');
    const store = {
      activeTWAPs: [{ id: 'twap-1', currentOrderIds: ['twap-order'] }],
      orders: [twapOrder, manualOrder],
    } as unknown as Writable<StoreData>;

    processor.process(store);

    expect((twapOrder.extra as any)?.isFromTwap).toBe(true);
    expect((twapOrder.extra as any)?.twapId).toBe('twap-1');
    expect(twapOrder.extra?.remainingNative).toBe(1);
    expect((manualOrder.extra as any)?.isFromTwap).toBeUndefined();
  });
});
