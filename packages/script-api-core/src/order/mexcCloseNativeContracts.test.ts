import type { Market, Order, PlaceOrderOpts, Position } from '@tealstreet/safe-cex/src/types';
import type { HostBindings } from '../adapter/HostBindings';

import { describe, expect, it, vi } from 'vitest';
import { OrderSide, OrderStatus, OrderType, PositionMode, PositionSide } from '@tealstreet/safe-cex/src/types';
import { createCoreAdapter } from '../adapter/createCoreAdapter';
import { downSizeReduceOrder } from './downSizeReduceOrder';
import { roundMarketCloseSize } from './orderPricing';
import { clampReduceOnlyOrder, prepareOrder } from './prepareOrder';

// Public DOGE_USDT contract/detail capture: 100 DOGE/contract, volUnit 1, maxVol 445000.
// https://contract.mexc.com/api/v1/contract/detail?symbol=DOGE_USDT (2026-11-10).
// Native MEXC publishes precision and amount limits in base units (lib.rs::market).
const market: Market = {
  id: 'DOGE_USDT',
  symbol: 'DOGEUSDT',
  base: 'DOGE',
  quote: 'USDT',
  active: true,
  contractSize: 100,
  isPerpetual: true,
  precision: { amount: 100, price: 0.00001 },
  limits: { amount: { min: 100, max: 44_500_000 }, leverage: { min: 1, max: 300 } },
};
const price = 0.1;
const position = (native: number): Position => ({
  symbol: market.symbol,
  side: PositionSide.Long,
  contracts: native * 100,
  entryPrice: price,
  notional: native * 100 * price,
  leverage: 10,
  unrealizedPnl: 0,
  liquidationPrice: 0,
  positionMode: PositionMode.OneWay,
  extra: { contractsNative: native },
});
const close = (amount: number, type = OrderType.Market): PlaceOrderOpts => ({
  symbol: market.symbol,
  side: OrderSide.Sell,
  type,
  amount,
  price,
  reduceOnly: true,
});
const preparedClose = (native: number, selected = market, held: readonly Order[] = []) => {
  const priced = prepareOrder(close(native), { market: selected, exchangeName: 'mexc' });
  const result = clampReduceOnlyOrder(priced, {
    market: selected,
    exchangeName: 'mexc',
    positions: [position(native)],
    restingOrders: held,
  });
  return { priced, result };
};

describe('MEXC native close quantities through real pricing and downsizing', () => {
  it.each([99, 100, 101, 130, 149, 150, 151])(
    'preserves %s native contracts on both sides of the old 100-base-unit rounding boundary',
    (native) => {
      const { priced, result } = preparedClose(native);
      expect(priced.amount).toBe(native);
      expect(result.amount).toBe(native);
    },
  );

  it('preserves 130 through standalone close pricing', () => {
    expect(roundMarketCloseSize(130, market, 'mexc')).toBe(130);
  });

  it('preserves 130 through standalone downsizing', () => {
    expect(downSizeReduceOrder(market, close(130), [position(130)], [], price, { exchangeName: 'mexc' }).amount).toBe(
      130,
    );
  });

  it.each([OrderType.Market, OrderType.Limit])(
    'sends the full 130-contract reduce through the adapter (%s)',
    async (type) => {
      const placeOrder = vi.fn(async () => ['closed']);
      const store = { markets: [market], positions: [position(130)], orders: [], tickers: [] };
      const host = {
        getExchange: () => ({ name: 'mexc', placeOrder, store }),
        getStore: () => store,
        findMarket: () => market,
      } as unknown as HostBindings;
      await createCoreAdapter(host).placeOrder(close(130, type) as never, 'fixture');
      expect(placeOrder).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ amount: 130, reduceOnly: true }));
      placeOrder.mockClear();
      await createCoreAdapter(host).closePosition({ symbol: market.symbol }, 'fixture');
      expect(placeOrder).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ amount: 130, reduceOnly: true }));
    },
  );

  it.each([444_999, 445_000])('preserves %s contracts at the real maximum boundary', (native) => {
    expect(preparedClose(native).result.amount).toBe(native);
  });

  it.each([445_001, 130.4, NaN])('refuses invalid reduce/close size %s before placement I/O', async (amount) => {
    const placeOrder = vi.fn(async () => ['closed']);
    const store = { markets: [market], positions: [position(130)], orders: [], tickers: [] };
    const host = {
      getExchange: () => ({ name: 'mexc', placeOrder, store }),
      getStore: () => store,
      findMarket: () => market,
    } as unknown as HostBindings;
    const adapter = createCoreAdapter(host);
    await expect(adapter.placeOrder(close(amount) as never, 'fixture')).rejects.toThrow();
    await expect(adapter.closePosition({ symbol: market.symbol, amount }, 'fixture')).rejects.toThrow();
    expect(placeOrder).not.toHaveBeenCalled();
  });

  it('explicitly refuses one contract above the real maximum instead of truncating', () => {
    expect(() => preparedClose(445_001)).toThrow(/maximum.*445000/);
    expect(() => roundMarketCloseSize(445_001, market, 'mexc')).toThrow(/maximum.*445000/);
  });

  it.each([0.9, 1.1, 130.4])(
    'refuses an unrepresentable reduce of %s contracts instead of silently rounding it',
    (native) => {
      expect(() => preparedClose(native)).toThrow(/lot step/);
      expect(() => roundMarketCloseSize(native, market, 'mexc')).toThrow(/lot step/);
    },
  );

  it('does not silently remove the 30 contracts covered by a resting order', () => {
    const resting = {
      id: 'resting',
      symbol: market.symbol,
      side: OrderSide.Sell,
      type: OrderType.Limit,
      status: OrderStatus.Open,
      price,
      amount: 3000,
      filled: 0,
      remaining: 3000,
      reduceOnly: true,
    } as Order;
    expect(() => preparedClose(130, market, [resting])).toThrow(/130.*100/);
  });

  it('preserves an explicit partial reduce and refuses an oversized request instead of shrinking it', () => {
    const clamp = (native: number) =>
      clampReduceOnlyOrder(prepareOrder(close(native), { market, exchangeName: 'mexc' }), {
        market,
        exchangeName: 'mexc',
        positions: [position(130)],
        restingOrders: [],
      });
    expect(clamp(50).amount).toBe(50);
    expect(() => clamp(150)).toThrow(/150.*130/);
  });

  it('retains the fractional BTC contract conversion and other venues native precision', () => {
    const btc: Market = {
      ...market,
      id: 'BTC_USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      contractSize: 0.0001,
      precision: { amount: 0.0001, price: 0.1 },
      limits: { ...market.limits, amount: { min: 0.0001, max: 40 } },
    };
    const btcOrder = prepareOrder({ ...close(130), symbol: btc.symbol }, { market: btc, exchangeName: 'mexc' });
    const btcPosition = { ...position(130), symbol: btc.symbol, contracts: 0.013 };
    expect(
      clampReduceOnlyOrder(btcOrder, {
        market: btc,
        exchangeName: 'mexc',
        positions: [btcPosition],
        restingOrders: [],
      }).amount,
    ).toBe(130);
    expect(roundMarketCloseSize(130, { ...market, precision: { amount: 1, price: 0.00001 } }, 'okx')).toBe(130);
  });

  it.each([undefined, 0, NaN])(
    'refuses unknown or invalid contract size %s without treating it as one or zero',
    (contractSize) => {
      expect(() => preparedClose(130, { ...market, contractSize })).toThrow(/units/);
    },
  );

  it.each([0, NaN])('refuses an unknown lot step %s without inventing a zero-sized order', (amount) => {
    expect(() => preparedClose(130, { ...market, precision: { ...market.precision, amount } })).toThrow(/lot step/);
  });

  it.each([NaN, Infinity])('refuses unknown/nonfinite requested size %s', (amount) => {
    expect(() => preparedClose(amount)).toThrow();
  });

  it('keeps an absent maximum unknown and preserves the requested amount', () => {
    const noMaximum = { ...market, limits: { ...market.limits, amount: { min: 100 } } } as Market;
    expect(preparedClose(130, noMaximum).result.amount).toBe(130);
  });

  it('preserves an intentional zero-sized full-position stop', () => {
    const stop = prepareOrder({ ...close(0, OrderType.StopLoss), stopLoss: price }, { market, exchangeName: 'mexc' });
    expect(stop.amount).toBe(0);
  });
});
