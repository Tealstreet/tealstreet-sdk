import type { PlaceOrderOpts } from '../../../safe-cex/src/types';
import type { HostBindings } from './HostBindings';

import { expect, it, vi } from 'vitest';

import { nativeFixture } from '../../../safe-cex/src/__tests__/cutover/nativeFixture';
import { delegatedIntentTime } from '../../../safe-cex/src/delegatedOrderFrames';
import { capturePlacementIntent, placementLegIntent, runWithPlacementIntent } from '../../../safe-cex/src/placementIntentContext';
import { FrameStore } from '../../../safe-cex/src/store/frameStore';
import { OrderSide, OrderType, PositionSide } from '../../../safe-cex/src/types';
import { createCoreAdapter } from './createCoreAdapter';

const symbol = 'BTCUSDT';
function fixture() {
  const store = new FrameStore();
  store.apply(nativeFixture({
    markets: [{ id: symbol, symbol, base: 'BTC', quote: 'USDT', active: true, contractSize: 1, isSpot: false,
      precision: { amount: 0.1, price: 1 }, limits: { amount: { min: 0.1, max: 100 }, leverage: { min: 1, max: 100 } } }],
    positions: [PositionSide.Long, PositionSide.Short].map((side) => ({
      id: symbol + side, symbol, side, contracts: 2, notional: 200, entryPrice: 100, leverage: 1,
      unrealizedPnl: 0, liquidationPrice: null, extra: { contractsNative: 2 },
    })),
  }));
  const observed: (string | undefined)[] = [];
  const orders: PlaceOrderOpts[] = [];
  const placeOrder = vi.fn(async (order: PlaceOrderOpts) => {
    orders.push(order);
    observed.push(capturePlacementIntent()?.id);
    await Promise.resolve();
    return ['native-' + orders.length];
  });
  const host: HostBindings = {
    getExchange: () => ({ name: 'binance', store, placeOrder }), getStore: () => store,
    readActiveAccount: () => 'main', readActiveSymbol: () => symbol,
    readStreamingAccounts: () => ['main'], readAllAccounts: () => [{ name: 'main', exchange: 'binance' }],
    readOrderbook: () => undefined,
  };
  return { adapter: createCoreAdapter(host), observed, orders };
}

it('retains stable child identities across the actual two-leg close await', async () => {
  const f = fixture();
  const id = '019a9b43-1000-7000-8000-000000000001';
  const parent = { id, createdAt: delegatedIntentTime(id) };
  await runWithPlacementIntent(id, () => f.adapter.closePosition({ symbol }, 'main'));
  expect(f.observed).toEqual([
    placementLegIntent(parent, JSON.stringify([symbol, PositionSide.Long])).id,
    placementLegIntent(parent, JSON.stringify([symbol, PositionSide.Short])).id,
  ]);
  expect(capturePlacementIntent()).toBeUndefined();
});

it('keeps unkeyed calls and venue payload/client-ID policy unchanged', async () => {
  const f = fixture();
  await expect(f.adapter.closePosition({ symbol }, 'main')).resolves.toEqual({ orderIds: ['native-1', 'native-2'] });
  expect(f.observed).toEqual([undefined, undefined]);
  expect(f.orders).toEqual([OrderSide.Sell, OrderSide.Buy].map((side) => ({
    symbol, side, type: OrderType.Market, amount: 2, reduceOnly: true,
  })));
});
