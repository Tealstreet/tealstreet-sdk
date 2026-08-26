import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { TwapInstance } from './TwapInstance';
import { TwapOptions } from './TwapOptions';
import { OrderSide, OrderType, Market } from '../../types';
import { Exchange } from '../../exchanges/base';
import { chaserOrderManagerInstance } from '../chaserOrder/ChaserOrderManager';
import type { ChaserOrderInstance } from '../chaserOrder/ChaserOrderInstance';

const market: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.01 },
  limits: {
    amount: { min: 0.001, max: 1000 },
    leverage: { min: 1, max: 100 },
  },
} as Market;

function makeExchange() {
  const ticker = { symbol: 'BTCUSDT', last: 50000, bid: 49995, ask: 50005 };
  return {
    name: 'mock',
    store: {
      tickers: [ticker],
      tickersBySymbol: { BTCUSDT: ticker },
      orders: [],
      positions: [],
      markets: [market],
      marketsBySymbol: { BTCUSDT: market },
      updateIds: { orders: 0, positions: 0, tickers: 0, balance: 0 },
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
    on: vi.fn(),
    off: vi.fn(),
    placeOrder: vi.fn().mockResolvedValue(['order-mkt']),
    cancelOrders: vi.fn().mockResolvedValue(undefined),
  } as unknown as Exchange;
}

function makeWorker() {
  return { postMessage: vi.fn() } as unknown as Worker;
}

function baseOptions(overrides: Partial<TwapOptions> = {}): TwapOptions {
  return {
    accountName: 'a',
    activeMarket: market,
    size: 1.0,
    duration: 10, // 10 minutes
    ordersCount: 5, // → slice = 2 minutes = 120,000ms
    side: OrderSide.Buy,
    chaserDelay: 1000,
    reduceOnly: false,
    randomnessPercentage: 0,
    useLimitOrders: false,
    useChaserOrders: true,
    ...overrides,
  };
}

function lastChaser(): ChaserOrderInstance | undefined {
  const arr = chaserOrderManagerInstance.instances;
  return arr[arr.length - 1];
}

/** Filter placeOrder calls to just the type=Market ones — the chaser
 *  itself uses type=Limit, which is noise for the fallback assertions. */
function marketCalls(exchange: Exchange) {
  return (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls
    .map((c) => c[0])
    .filter((o: { type: string }) => o.type === OrderType.Market);
}

describe('TwapInstance — twap chase timeout/fail policy', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    // Drain singleton so tests don't bleed
    while (chaserOrderManagerInstance.instances.length > 0) {
      const inst = chaserOrderManagerInstance.instances[0];
      // Forcibly mark complete + remove without async cleanup
      try {
        chaserOrderManagerInstance.removeKeepOrder(inst.id);
      } catch {
        // fall through
      }
      if (chaserOrderManagerInstance.instances.includes(inst)) {
        // Brute-force in case removeKeepOrder didn't catch it
        const idx = chaserOrderManagerInstance.instances.indexOf(inst);
        if (idx >= 0) chaserOrderManagerInstance.instances.splice(idx, 1);
      }
    }
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('does not schedule a timeout when timeoutPct is undefined (legacy useChaserOrders path)', async () => {
    const exchange = makeExchange();
    const inst = new TwapInstance(
      exchange,
      makeWorker(),
      baseOptions({ timeoutPct: undefined }),
      vi.fn(),
    );
    await inst.placeOrder(0.2);
    expect(chaserOrderManagerInstance.instances.length).toBe(1);

    // Advance well past any reasonable slice interval
    vi.advanceTimersByTime(5 * 60 * 1000);
    // Chaser still alive (no timeout was scheduled)
    expect(chaserOrderManagerInstance.instances.length).toBe(1);
    // No market fallback — the chaser places its own limits, but no
    // type=Market orders should have been fired.
    expect(marketCalls(exchange)).toHaveLength(0);
  });

  it('schedules a chaser when timeoutPct is set', async () => {
    const exchange = makeExchange();
    const inst = new TwapInstance(
      exchange,
      makeWorker(),
      baseOptions({ timeoutPct: 50, timeoutAction: 'market' }),
      vi.fn(),
    );
    await inst.placeOrder(0.2);
    expect(chaserOrderManagerInstance.instances.length).toBe(1);
  });

  describe('timeoutAction', () => {
    it("`market`: cancels chaser and fires market for the unfilled remainder", async () => {
      const exchange = makeExchange();
      const inst = new TwapInstance(
        exchange,
        makeWorker(),
        baseOptions({ timeoutPct: 50, timeoutAction: 'market' }),
        vi.fn(),
      );
      await inst.placeOrder(0.2);
      const chaser = lastChaser()!;
      expect(chaser).toBeDefined();
      expect(chaser.isStopped).toBe(false);

      // 50% of 120s = 60s
      vi.advanceTimersByTime(60_000);
      // Chaser should be removed from the manager
      expect(chaserOrderManagerInstance.instances).not.toContain(chaser);
      // A market order should have been fired for the full unfilled size
      const mkts = marketCalls(exchange);
      expect(mkts).toHaveLength(1);
      expect(mkts[0].side).toBe(OrderSide.Buy);
      expect(mkts[0].amount).toBe(0.2);
      expect(mkts[0].reduceOnly).toBe(false);
    });

    it("`next`: cancels chaser without firing a market", async () => {
      const exchange = makeExchange();
      const inst = new TwapInstance(
        exchange,
        makeWorker(),
        baseOptions({ timeoutPct: 50, timeoutAction: 'next' }),
        vi.fn(),
      );
      await inst.placeOrder(0.2);
      const chaser = lastChaser()!;

      vi.advanceTimersByTime(60_000);
      expect(chaserOrderManagerInstance.instances).not.toContain(chaser);
      expect(marketCalls(exchange)).toHaveLength(0);
    });

    it("does nothing when the chaser already filled before timeout", async () => {
      const exchange = makeExchange();
      const inst = new TwapInstance(
        exchange,
        makeWorker(),
        baseOptions({ timeoutPct: 50, timeoutAction: 'market' }),
        vi.fn(),
      );
      await inst.placeOrder(0.2);
      const chaser = lastChaser()!;
      // Simulate the chaser fully filling
      chaser.markComplete('Fully filled');
      chaser.recordCumulativePlacementFill('x', 0.2);

      vi.advanceTimersByTime(60_000);
      expect(marketCalls(exchange)).toHaveLength(0);
    });
  });

  describe('failAction (chaser stopped before timeout with remaining)', () => {
    it("`market`: fires market for the unfilled remainder when timeout finds a stopped+unfilled chaser", async () => {
      const exchange = makeExchange();
      const inst = new TwapInstance(
        exchange,
        makeWorker(),
        baseOptions({
          timeoutPct: 50,
          timeoutAction: 'next', // ensure timeout-action wouldn't otherwise market
          failAction: 'market',
        }),
        vi.fn(),
      );
      await inst.placeOrder(0.2);
      const chaser = lastChaser()!;
      // Simulate chaser failure with remaining size
      chaser.markComplete('Reached maximum number of attempts');
      // remainingSize is still 0.2 because no fills happened

      vi.advanceTimersByTime(60_000);
      const mkts = marketCalls(exchange);
      expect(mkts).toHaveLength(1);
      expect(mkts[0].amount).toBe(0.2);
    });

    it("`next`: skips even when failed", async () => {
      const exchange = makeExchange();
      const inst = new TwapInstance(
        exchange,
        makeWorker(),
        baseOptions({
          timeoutPct: 50,
          timeoutAction: 'market', // would otherwise market
          failAction: 'next',
        }),
        vi.fn(),
      );
      await inst.placeOrder(0.2);
      const chaser = lastChaser()!;
      chaser.markComplete('Reached maximum number of attempts');

      vi.advanceTimersByTime(60_000);
      expect(marketCalls(exchange)).toHaveLength(0);
    });
  });

  describe('terminate cleanup', () => {
    it('cancels pending slice timeouts', async () => {
      const exchange = makeExchange();
      const inst = new TwapInstance(
        exchange,
        makeWorker(),
        baseOptions({ timeoutPct: 50, timeoutAction: 'market' }),
        vi.fn(),
      );
      await inst.placeOrder(0.2);

      await inst.terminate();
      // Even if we advance past the timeout, no market should fire
      // because terminate cleared the slice timer.
      vi.advanceTimersByTime(60_000);
      expect(marketCalls(exchange)).toHaveLength(0);
    });
  });

  describe('reduceOnly threading', () => {
    it('passes reduceOnly through to the fallback market order', async () => {
      const exchange = makeExchange();
      const inst = new TwapInstance(
        exchange,
        makeWorker(),
        baseOptions({
          reduceOnly: true,
          timeoutPct: 50,
          timeoutAction: 'market',
        }),
        vi.fn(),
      );
      await inst.placeOrder(0.2);
      vi.advanceTimersByTime(60_000);
      const mkts = marketCalls(exchange);
      expect(mkts).toHaveLength(1);
      expect(mkts[0].reduceOnly).toBe(true);
    });
  });
});
