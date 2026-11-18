import { describe, expect, it } from 'vitest';

import {
  type ExchangeName,
  type Market,
  type Order,
  OrderSide,
  OrderStatus,
  OrderType,
  type Position,
  PositionMode,
  PositionSide,
} from '@tealstreet/safe-cex/src/types';
import { ReduceOrderLockedError } from './downSizeReduceOrder';
import { clampReduceOnlyOrder } from './prepareOrder';

const market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  isSpot: false,
  info: {},
  precision: { amount: 0.0001, price: 0.1 },
  limits: { amount: { min: 0.0001, max: 1000 }, leverage: { min: 1, max: 100 } },
} as Market;
const positions = [
  {
    symbol: 'BTCUSDT',
    side: PositionSide.Long,
    contracts: 0.4872,
    positionMode: PositionMode.OneWay,
  } as Position,
];
const plan = (id: string, type: OrderType, amount: number) =>
  ({
    id,
    symbol: 'BTCUSDT',
    type,
    side: OrderSide.Sell,
    price: type === OrderType.TakeProfit ? 60000 : 40000,
    amount,
    filled: 0,
    remaining: amount,
    status: OrderStatus.Open,
    reduceOnly: true,
  }) as Order;
const restingOrders = [plan('tp', OrderType.TakeProfit, 0.1451), plan('sl', OrderType.StopLoss, 0.1451)];
const close = { symbol: 'BTCUSDT', type: OrderType.Market, side: OrderSide.Sell, amount: 0.4872, reduceOnly: true };
const clamp = (exchangeName: ExchangeName, held = restingOrders) =>
  clampReduceOnlyOrder(close, {
    market,
    exchangeName,
    positions,
    restingOrders: held,
    ticker: { mark: 50000, last: 50000 } as never,
  } as never);

describe('the scripts/OEMS reduce-only clamp on a venue whose close orders lock position size', () => {
  it('caps a Bitget close to the part its sized TP/SL plans leave free', () => {
    expect(clamp('bitget' as ExchangeName).amount).toBeCloseTo(0.197, 9);
  });

  it('refuses a Bitget close the plans hold entirely', () => {
    const whole = [plan('tp', OrderType.TakeProfit, 0.4872)];
    expect(() => clamp('bitget' as ExchangeName, whole)).toThrow(ReduceOrderLockedError);
  });

  it('leaves a downsizing venue without locks capping at the position only', () => {
    expect(clamp('hyperliquid' as ExchangeName).amount).toBeCloseTo(0.4872, 9);
  });

  // Its order amounts are native contracts and position.contracts the coin size: not nettable yet.
  it('keeps the lock off for bitgetinverse', () => {
    expect(clamp('bitgetinverse' as ExchangeName).amount).toBeCloseTo(0.4872, 9);
  });
});
