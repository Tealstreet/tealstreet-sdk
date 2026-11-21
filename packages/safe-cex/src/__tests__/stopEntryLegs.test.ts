import type { PlaceOrderOpts } from '../types';

import { describe, expect, it, vi } from 'vitest';

import { FrameStore } from '../store/frameStore';
import { OrderSide, OrderType } from '../types';
import { BinanceExchange } from './reference/binance/binance.exchange';
import { BinancePMExchange } from './reference/binance/binancepm.exchange';
import { OKXExchange } from './reference/okx/okx.exchange';
import { stopEntryLegsRefusal, stopEntryWithAttachedLegs } from './reference/stopEntryLegs';

// AGT-115: the TS reference refuses the same shapes as the Rust engine helper.
describe('stopEntryWithAttachedLegs', () => {
  const order = (type: OrderType, legs: Partial<PlaceOrderOpts>) =>
    ({ symbol: 'BTCUSDT', type, side: OrderSide.Buy, amount: 1, ...legs }) as PlaceOrderOpts;

  it('flags only a non-plain entry with a non-zero leg', () => {
    for (const type of [OrderType.Conditional, OrderType.StopLoss, OrderType.TakeProfit, OrderType.TrailingStopLoss]) {
      expect(stopEntryWithAttachedLegs(order(type, { stopLoss: 1 }))).toBe(true);
      expect(stopEntryWithAttachedLegs(order(type, { stopLoss: 0, takeProfit: 2 }))).toBe(true);
      expect(stopEntryWithAttachedLegs(order(type, {}))).toBe(false);
      expect(stopEntryWithAttachedLegs(order(type, { stopLoss: 0, takeProfit: 0 }))).toBe(false);
    }
    for (const type of [OrderType.Limit, OrderType.Market]) {
      expect(stopEntryWithAttachedLegs(order(type, { stopLoss: 1, takeProfit: 2 }))).toBe(false);
    }
  });

  it('names the venue and says nothing was sent', () => {
    expect(stopEntryLegsRefusal('OKX')).toBe(
      "Attaching a stop-loss or take-profit to a stop entry isn't supported on OKX, so nothing was sent. Place the stop-loss after the entry fills.",
    );
  });
});

// Both the single-order and batch entry points refuse before any position-mode read or
// placement, and a plain limit in the same batch is not sent either.
describe('reference placement refuses a stop entry with attached legs before any write', () => {
  const options = {
    key: 'test-key',
    secret: 'test-secret',
    passphrase: 'test-passphrase',
    tuning: { consoleLogging: false },
  };
  const plain = {
    symbol: 'BTCUSDT',
    type: OrderType.Limit,
    side: OrderSide.Buy,
    amount: 1,
    price: 90,
  } as PlaceOrderOpts;
  const stop = (type: OrderType) =>
    ({
      symbol: 'BTCUSDT',
      type,
      side: OrderSide.Buy,
      amount: 1,
      price: 110,
      triggerPrice: 110,
      stopLoss: 80,
    }) as PlaceOrderOpts;
  const subjects = [
    {
      name: 'OKX',
      make: () => new OKXExchange('okx', options as any, new FrameStore()),
      writes: ['getOrFetchPositionMode', 'placeOrderBatch', 'placeAlgoOrderBatch'],
      types: [OrderType.Conditional, OrderType.StopLoss, OrderType.TakeProfit, OrderType.TrailingStopLoss],
    },
    {
      name: 'Binance',
      make: () => new BinanceExchange('binance', options as any, new FrameStore() as any),
      writes: ['placeRegularOrderBatch', 'placeAlgoOrders'],
      types: [OrderType.Conditional, OrderType.TrailingStopLoss],
    },
    {
      name: 'Binance',
      make: () => new BinancePMExchange('binancepm', options as any, new FrameStore()),
      writes: ['placeOrderBatch'],
      types: [OrderType.Conditional, OrderType.TrailingStopLoss],
    },
  ];

  for (const subject of subjects) {
    for (const type of subject.types) {
      it(`${subject.name} ${subject.make().constructor.name} ${type}`, async () => {
        const exchange = subject.make() as any;
        const writes = Object.fromEntries(subject.writes.map((name) => [name, vi.fn().mockResolvedValue([])]));
        Object.assign(exchange, writes, {
          emitAndLogError: vi.fn(),
          cancelExistingClosePositionOrders: vi.fn().mockResolvedValue(undefined),
        });
        await expect(exchange.placeOrder(stop(type))).rejects.toThrow(stopEntryLegsRefusal(subject.name));
        await expect(exchange.placeOrders([stop(type), plain])).rejects.toThrow(stopEntryLegsRefusal(subject.name));
        for (const write of Object.values(writes)) expect(write).not.toHaveBeenCalled();
      });
    }
  }
});
