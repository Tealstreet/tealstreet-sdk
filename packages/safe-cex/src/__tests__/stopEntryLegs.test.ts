import type { PlaceOrderOpts } from '../types';

import { describe, expect, it, vi } from 'vitest';

import { FrameStore } from '../store/frameStore';
import { OrderSide, OrderType, PositionSide } from '../types';
import { AsterDEXExchange } from './reference/asterdex/asterdex.exchange';
import { BinanceExchange } from './reference/binance/binance.exchange';
import { BinancePMExchange } from './reference/binance/binancepm.exchange';
import { BitgetExchange } from './reference/bitget/bitget.exchange';
import { BitunixExchange } from './reference/bitunix/bitunix.exchange';
import { BybitExchange } from './reference/bybit/bybit.exchange';
import { BybitInverseExchange } from './reference/bybit/bybitinverse.exchange';
import { HyrotraderExchange } from './reference/bybit/hyrotrader.exchange';
import { HyperliquidExchange } from './reference/hyperliquid/hyperliquid.exchange';
import { HyperliquidSpotExchange } from './reference/hyperliquid/hyperliquidspot.exchange';
import { LighterExchange } from './reference/lighter/lighter.exchange';
import { MexcExchange } from './reference/mexc/mexc.exchange';
import { OKXExchange } from './reference/okx/okx.exchange';
import { PhemexExchange } from './reference/phemex/phemex.exchange';
import { stopEntryLegsRefusal, stopEntryWithAttachedLegs } from './reference/stopEntryLegs';
import { WeexExchange } from './reference/weex/weex.exchange';
import { WOOXExchange } from './reference/woo/woo.exchange';

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

// AGT-115 follow-up: the same refusal on every venue that dropped attached legs. The
// single path and the batch path each refuse before any write, and in a batch the
// plain limit beside the bracket is not sent either.
describe('more venues refuse an entry whose attached legs they would drop', () => {
  const STOP = OrderType.StopLoss;
  const TP = OrderType.TakeProfit;
  const TRAIL = OrderType.TrailingStopLoss;
  const COND = OrderType.Conditional;
  const options = {
    key: 'test-key',
    secret: '0x' + '11'.repeat(32),
    passphrase: 'test-passphrase',
    applicationId: '0x' + '22'.repeat(20),
    apiKeyIndex: '2',
    tuning: { consoleLogging: false },
  };
  const order = (type: OrderType, extra: Partial<PlaceOrderOpts> = {}) =>
    ({
      symbol: 'BTCUSDT',
      type,
      side: OrderSide.Sell,
      amount: 1,
      price: 95,
      triggerPrice: 95,
      ...extra,
    }) as PlaceOrderOpts;
  const plain = {
    symbol: 'BTCUSDT',
    type: OrderType.Limit,
    side: OrderSide.Buy,
    amount: 1,
    price: 90,
  } as PlaceOrderOpts;
  const legs: Partial<PlaceOrderOpts>[] = [{ stopLoss: 120 }, { takeProfit: 60 }, { stopLoss: 0, takeProfit: 60 }];
  type Subject = {
    venue: string;
    make: () => object;
    single: string[];
    batch: string[];
    types: OrderType[];
    extra?: Partial<PlaceOrderOpts>;
  };
  const store = () => new FrameStore() as any;
  const subjects: Subject[] = [
    {
      venue: 'AsterDEX',
      make: () => new AsterDEXExchange('asterdex', options as any, store()),
      single: ['assertAlgoOrderCapacity'],
      batch: ['placeOrder'],
      types: [STOP, TP, COND, TRAIL],
    },
    {
      venue: 'Bitget',
      make: () => new BitgetExchange('bitget', options as any, store()),
      single: [
        'ensureAccountTypeDetected',
        'placeOrderBatch',
        'placeAlgoOrders',
        'placeUtaOrders',
        'placeUtaStrategyOrders',
      ],
      batch: [
        'ensureAccountTypeDetected',
        'placeOrderBatch',
        'placeAlgoOrders',
        'placeUtaOrders',
        'placeUtaStrategyOrders',
      ],
      types: [STOP, TP, TRAIL],
    },
    {
      venue: 'Bitunix',
      make: () => new BitunixExchange('bitunix', options as any, store()),
      single: ['hydratePositionIdBeforeFormatting', 'placeOrderBatch'],
      batch: ['hydratePositionIdBeforeFormatting', 'placeOrderBatch'],
      types: [STOP, TP],
    },
    {
      venue: 'Bybit',
      make: () => new BybitExchange('bybit', options as any, store()),
      single: ['placeBatchOrders', 'placeStopLossOrTakeProfit', 'placeTrailingStopLoss'],
      batch: ['placeBatchOrders', 'placeOrder'],
      types: [TRAIL],
    },
    {
      venue: 'Bybit',
      make: () => new BybitInverseExchange('bybitinverse', options as any, store()),
      single: ['placeBatchOrders', 'placeBatchOrdersForCategory'],
      batch: ['placeBatchOrders', 'placeBatchOrdersForCategory', 'placeOrder'],
      types: [TRAIL],
      extra: { symbol: 'BTCUSD' },
    },
    {
      venue: 'HyroTrader',
      make: () => {
        const exchange = new HyrotraderExchange('hyrotrader' as any, options as any, store()) as any;
        exchange.assertPrivateReady = vi.fn();
        // HyroTrader trades USDT perps only; its capability check reads the market first.
        Object.defineProperty(exchange.store, 'marketsBySymbol', { value: { BTCUSDT: { quote: 'USDT' } } });
        return exchange;
      },
      single: ['placeBatchOrders', 'placeStopLossOrTakeProfit', 'placeTrailingStopLoss'],
      batch: ['placeBatchOrders', 'placeOrder'],
      types: [TRAIL],
    },
    {
      venue: 'Hyperliquid',
      make: () => new HyperliquidExchange('hyperliquid', options as any, store()),
      single: ['getOrderBasePrice'],
      batch: ['placeOrder'],
      types: [STOP, TP],
    },
    {
      venue: 'Hyperliquid',
      make: () => new HyperliquidSpotExchange('hyperliquidspot', options as any, store()),
      single: [],
      batch: ['placeOrder'],
      types: [STOP, TP],
    },
    {
      venue: 'Lighter',
      make: () => new LighterExchange('lighter', options as any, store()),
      single: ['ensureSignerAdapter'],
      batch: ['placeOrdersSequentially', 'shouldUseBatchedCreateOrders', 'placeOrder'],
      types: [STOP, TP, COND, TRAIL],
    },
    {
      venue: 'MEXC',
      make: () => new MexcExchange('mexc', options as any, store()),
      single: ['placePlanOrder', 'placePositionStopOrder'],
      batch: ['placeOrder', 'placeOrdersSequentially', 'placeRegularOrdersBatch', 'ensureSymbolConfig'],
      types: [STOP, TP],
    },
    {
      venue: 'Phemex',
      make: () => new PhemexExchange('phemex', options as any, store()),
      single: ['formatCreateOrder'],
      batch: ['placeOrder'],
      types: [TRAIL],
    },
    {
      venue: 'WEEX',
      make: () => new WeexExchange('weex', options as any, store()),
      single: ['formatOrderPayload'],
      batch: ['placeOrder', 'placeRegularOrderBatch'],
      types: [STOP, TP],
    },
    {
      venue: 'WOO',
      make: () => new WOOXExchange('woo', options as any, store()),
      single: ['placeAlgoOrder'],
      batch: ['placeOrder'],
      types: [STOP, TP, TRAIL],
    },
  ];
  const stubbed = (subject: Subject, names: string[]) => {
    const exchange = subject.make() as any;
    const writes = Object.fromEntries(names.map((name) => [name, vi.fn().mockResolvedValue([])]));
    Object.assign(exchange, writes, { emitAndLogError: vi.fn() });
    return { exchange, writes };
  };

  for (const subject of subjects) {
    const refusal = stopEntryLegsRefusal(subject.venue);
    for (const type of subject.types) {
      it(`${subject.venue} ${subject.make().constructor.name} ${type}`, async () => {
        for (const leg of legs) {
          const bracket = order(type, { ...subject.extra, ...leg });
          const single = stubbed(subject, subject.single);
          await expect(single.exchange.placeOrder(bracket)).rejects.toThrow(refusal);
          const batch = stubbed(subject, subject.batch);
          await expect(batch.exchange.placeOrders([{ ...plain, ...subject.extra }, bracket])).rejects.toThrow(refusal);
          for (const write of [...Object.values(single.writes), ...Object.values(batch.writes)]) {
            expect(write).not.toHaveBeenCalled();
          }
        }
      });
    }
  }

  // Bybit's stop and take-profit go to trading-stop, dropping legs, only when they close a
  // covering position; otherwise order create carries the legs (live, 2026-11-21).
  it('Bybit refuses a stop with legs only when it closes a covering position', async () => {
    const long = [{ symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 }];
    for (const subject of subjects.filter((s) => s.venue === 'Bybit' && !s.extra)) {
      for (const type of [STOP, TP]) {
        for (const leg of legs) {
          const bracket = order(type, leg);
          const covered = stubbed(subject, subject.batch);
          Object.defineProperty(covered.exchange.store, 'positions', { value: long });
          await expect(covered.exchange.placeOrders([plain, bracket])).rejects.toThrow(stopEntryLegsRefusal('Bybit'));
          for (const write of Object.values(covered.writes)) expect(write).not.toHaveBeenCalled();
          const open = stubbed(subject, subject.batch);
          await open.exchange.placeOrders([bracket]);
          expect(Object.values(open.writes).some((write) => write.mock.calls.length > 0)).toBe(true);
        }
      }
    }
  });

  // Past the check, the first write-shaping call throws a marker: reaching it proves the
  // shape was not refused, without any network.
  it('still sends the shapes whose legs the venue carries', async () => {
    const carried: [string, OrderType, string][] = [
      ['Phemex', STOP, 'formatCreateOrder'],
      ['WEEX', COND, 'formatOrderPayload'],
    ];
    for (const [venue, type, shaping] of carried) {
      const exchange = subjects.find((s) => s.venue === venue)!.make() as any;
      exchange[shaping] = vi.fn(() => {
        throw new Error('reached the write path');
      });
      await expect(exchange.placeOrder(order(type, { stopLoss: 120, takeProfit: 60 }))).rejects.toThrow(
        'reached the write path',
      );
    }
  });

  it('Lighter refuses legs on a reduce-only limit or market entry', async () => {
    const lighter = subjects.find((s) => s.venue === 'Lighter')!;
    for (const type of [OrderType.Limit, OrderType.Market]) {
      const bracket = order(type, { reduceOnly: true, stopLoss: 120 });
      const { exchange, writes } = stubbed(lighter, [
        ...lighter.single,
        ...lighter.batch.filter((n) => n !== 'placeOrder'),
      ]);
      await expect(exchange.placeOrder(bracket)).rejects.toThrow("isn't supported on Lighter, so nothing was sent");
      await expect(exchange.placeOrders([plain, bracket])).rejects.toThrow('reduce-only order');
      for (const write of Object.values(writes)) expect(write).not.toHaveBeenCalled();
    }
  });
});
