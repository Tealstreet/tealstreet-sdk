import type { Exchange } from '../../exchanges/base';
import type { Market, Order, StoreData, Ticker } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionSide } from '../../types';
import { ChaserOrderInstance } from './ChaserOrderInstance';
import { ChaserOrderManager } from './ChaserOrderManager';

type Handler = (...args: any[]) => void;

const createMarket = (): Market => ({
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: {
    amount: 0.001,
    price: 1,
  },
  limits: {
    amount: {
      min: 0.001,
      max: 100,
    },
    leverage: {
      min: 1,
      max: 100,
    },
  },
});

const createTicker = (): Ticker => ({
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  bid: 100,
  ask: 101,
  last: 100,
  mark: 100,
  index: 100,
  percentage: 0,
  openInterest: 0,
  fundingRate: 0,
  fundingTime: 0,
  volume: 0,
  quoteVolume: 0,
});

const createOrder = (id: string, price: number): Order => ({
  id,
  status: OrderStatus.Open,
  symbol: 'BTCUSDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price,
  amount: 1,
  filled: 0,
  remaining: 1,
  reduceOnly: false,
  timeInForce: OrderTimeInForce.PostOnly,
  lastUpdate: Date.now(),
});

const createMockExchange = () => {
  const market = createMarket();
  const ticker = createTicker();
  const order = createOrder('order-1', 100);
  const handlers: Record<string, Set<Handler>> = {};
  const store = {
    orders: [order],
    tickers: [ticker],
    markets: [market],
    positions: [],
    updateIds: {
      orders: 0,
      positions: 0,
    },
    tickersBySymbol: {
      BTCUSDT: ticker,
    },
    marketsBySymbol: {
      BTCUSDT: market,
    },
  } as unknown as StoreData & {
    tickersBySymbol: Record<string, Ticker>;
    marketsBySymbol: Record<string, Market>;
  };

  const exchange = {
    name: 'bybit',
    store,
    emitter: {
      emit: vi.fn(),
    },
    on: vi.fn((event: string, handler: Handler) => {
      handlers[event] ??= new Set();
      handlers[event].add(handler);
    }),
    off: vi.fn((event: string, handler?: Handler) => {
      if (!handler) {
        handlers[event]?.clear();
        return;
      }
      handlers[event]?.delete(handler);
    }),
    placeOrder: vi.fn().mockResolvedValueOnce(['order-1']).mockResolvedValueOnce(['order-2']),
    cancelOrders: vi.fn().mockResolvedValue(undefined),
  } as unknown as Exchange;

  return {
    exchange,
    order,
    store,
    emit: async (event: string, data: unknown) => {
      await Promise.all(Array.from(handlers[event] || []).map((handler) => handler(data)));
    },
  };
};

describe('ChaserOrderManager', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it('rate-limits ticker-only price checks without waiting for the configured delay', async () => {
    const { emit, exchange, order, store } = createMockExchange();
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });

    manager.add(instance);

    await vi.advanceTimersByTimeAsync(1000);

    expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
    expect(exchange.placeOrder).toHaveBeenLastCalledWith(
      expect.objectContaining({
        price: 100,
      }),
    );

    await emit('update', store);

    (store.tickersBySymbol.BTCUSDT as any).bid = 101;
    (store.tickersBySymbol.BTCUSDT as any).ask = 102;

    await emit('update', store);

    expect(exchange.cancelOrders).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(50);

    expect(exchange.cancelOrders).toHaveBeenCalledWith([order]);
    expect(exchange.placeOrder).toHaveBeenCalledTimes(2);
    expect(exchange.placeOrder).toHaveBeenLastCalledWith(
      expect.objectContaining({
        price: 101,
      }),
    );

    instance.markComplete();
    await vi.advanceTimersByTimeAsync(1000);
  });

  it('still completes immediately on order status updates while a ticker check is queued', async () => {
    const { emit, exchange, order, store } = createMockExchange();
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });

    manager.add(instance);

    await vi.advanceTimersByTimeAsync(1000);

    await emit('update', store);

    (store.tickersBySymbol.BTCUSDT as any).bid = 101;
    (store.tickersBySymbol.BTCUSDT as any).ask = 102;
    await emit('update', store);

    expect(exchange.cancelOrders).not.toHaveBeenCalled();

    (order as any).status = OrderStatus.Closed;
    (order as any).filled = 1;
    store.updateIds.orders += 1;
    await emit('update', store);

    expect(instance.isStopped).toBe(true);
    expect(instance.state.stopReason).toBe('Completed');
    expect(manager.instances).not.toContain(instance);
  });

  it('does not miss an order fill that arrives while a reprice is pending', async () => {
    const { emit, exchange, order, store } = createMockExchange();
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });
    let resolveCancel!: () => void;
    const cancelPromise = new Promise<void>((resolve) => {
      resolveCancel = resolve;
    });
    (exchange.cancelOrders as any).mockReturnValueOnce(cancelPromise);

    manager.add(instance);

    await vi.advanceTimersByTimeAsync(1000);

    (store.tickersBySymbol.BTCUSDT as any).bid = 101;
    (store.tickersBySymbol.BTCUSDT as any).ask = 102;
    const priceUpdate = emit('update', store);
    await Promise.resolve();

    expect(exchange.cancelOrders).toHaveBeenCalledWith([order]);

    (order as any).status = OrderStatus.Closed;
    (order as any).filled = 1;
    store.updateIds.orders += 1;
    await emit('update', store);

    expect(instance.isStopped).toBe(true);

    resolveCancel();
    await priceUpdate;

    expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
  });

  it('rate-limits the follow-up check queued during an in-flight order update', async () => {
    const { emit, exchange, store } = createMockExchange();
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });
    let resolveCancel!: () => void;
    (exchange.cancelOrders as any).mockReturnValueOnce(
      new Promise<void>((resolve) => {
        resolveCancel = resolve;
      }),
    );

    manager.add(instance);
    await vi.advanceTimersByTimeAsync(1000);
    await emit('update', store);

    (store.tickersBySymbol.BTCUSDT as any).bid = 101;
    (store.tickersBySymbol.BTCUSDT as any).ask = 102;
    store.updateIds.orders += 1;
    const inFlightUpdate = emit('update', store);
    await Promise.resolve();
    expect(exchange.cancelOrders).toHaveBeenCalledTimes(1);

    // A second order edge arrives while cancel is still pending. It must not
    // recurse immediately when the first pass finishes.
    store.updateIds.orders += 1;
    await emit('update', store);
    resolveCancel();
    await vi.advanceTimersByTimeAsync(1000);
    await inFlightUpdate;

    const replacement = createOrder('order-2', 101);
    (store as any).orders = [replacement];
    (exchange.placeOrder as any).mockResolvedValueOnce(['order-3']);
    (store.tickersBySymbol.BTCUSDT as any).bid = 102;
    (store.tickersBySymbol.BTCUSDT as any).ask = 103;

    await vi.advanceTimersByTimeAsync(49);
    expect(exchange.cancelOrders).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(1);
    expect(exchange.cancelOrders).toHaveBeenCalledTimes(2);
    expect(exchange.cancelOrders).toHaveBeenLastCalledWith([replacement]);
  });

  it('never replaces after cancellation when fill reconciliation fails', async () => {
    // This regression intentionally lives with the chaser, not GRVT: any
    // fill-based exchange must fail closed once cancellation may have won.
    const { emit, exchange, order, store } = createMockExchange();
    (exchange as any).name = 'grvt';
    (exchange as any).fetchFillsForOrderIds = vi.fn().mockRejectedValue(new Error('cursor integrity failure'));
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });

    manager.add(instance);
    await vi.advanceTimersByTimeAsync(1000);
    await emit('update', store);

    (store.tickersBySymbol.BTCUSDT as any).bid = 101;
    (store.tickersBySymbol.BTCUSDT as any).ask = 102;
    await emit('update', store);
    await vi.advanceTimersByTimeAsync(50);

    expect(exchange.cancelOrders).toHaveBeenCalledWith([order]);
    expect((exchange as any).fetchFillsForOrderIds).toHaveBeenCalledWith(['order-1']);
    expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
    expect(instance.state.stopReason).toBe('Failed to reconcile fills after cancellation');
    expect(exchange.emitter.emit).toHaveBeenCalledWith('error', expect.stringMatching(/stopped before replacement/i));
    expect(manager.instances).not.toContain(instance);
  });

  it('contains startup failures and removes the failed instance', async () => {
    const { exchange, store } = createMockExchange();
    (store as any).markets = [];
    (store as any).tickers = [];
    store.marketsBySymbol = {};
    store.tickersBySymbol = {};
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });

    manager.add(instance);
    await vi.advanceTimersByTimeAsync(0);

    expect(instance.isStopped).toBe(true);
    expect(manager.instances).not.toContain(instance);
    expect(exchange.emitter.emit).toHaveBeenCalledWith(
      'error',
      expect.stringMatching(/supervisor failed during startup/i),
    );
  });

  it('contains store-update failures instead of leaking an unhandled rejection', async () => {
    const { emit, exchange, store } = createMockExchange();
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });

    manager.add(instance);
    await vi.advanceTimersByTimeAsync(1000);
    await emit('update', store);

    (store.tickersBySymbol.BTCUSDT as any).bid = 101;
    (store.tickersBySymbol.BTCUSDT as any).ask = 102;
    store.updateIds.orders += 1;
    (exchange.cancelOrders as any).mockRejectedValueOnce(new Error('permission denied'));

    await expect(emit('update', store)).resolves.toBeUndefined();

    expect(instance.state.stopReason).toBe('Failed to cancel orders');
    expect(manager.instances).not.toContain(instance);
    expect(exchange.emitter.emit).toHaveBeenCalledWith(
      'error',
      expect.stringMatching(/supervisor failed during store update/i),
    );
  });

  it('stops reduce-only chasers when the reducing position is gone on a store update', async () => {
    const { emit, exchange, store } = createMockExchange();
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: true,
      delay: 250,
    });

    (store as any).positions = [
      {
        id: 'BTCUSDT-short',
        symbol: 'BTCUSDT',
        side: PositionSide.Short,
        entryPrice: 100,
        notional: 100,
        leverage: 1,
        unrealizedPnl: 0,
        contracts: 1,
        liquidationPrice: 0,
      },
    ];

    manager.add(instance);

    await vi.advanceTimersByTimeAsync(1000);

    (store as any).positions = [];

    await emit('update', store);

    expect(instance.isStopped).toBe(true);
    expect(instance.state.stopReason).toBe('Completed');
    expect(manager.instances).not.toContain(instance);
  });

  it('requires two order updates before treating a missing tracked order as final', async () => {
    const { emit, exchange, order, store } = createMockExchange();
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });

    manager.add(instance);
    await vi.advanceTimersByTimeAsync(1000);
    await emit('update', store);

    (store as any).orders = [];
    store.updateIds.orders += 1;
    await emit('update', store);

    expect(instance.isStopped).toBe(false);
    expect(manager.instances).toContain(instance);
    expect(exchange.cancelOrders).not.toHaveBeenCalled();

    // A correcting snapshot resets the debounce.
    (store as any).orders = [order];
    store.updateIds.orders += 1;
    await emit('update', store);
    (store as any).orders = [];
    store.updateIds.orders += 1;
    await emit('update', store);
    expect(instance.isStopped).toBe(false);

    store.updateIds.orders += 1;
    await emit('update', store);

    expect(instance.state.stopReason).toBe('Tracked orders disappeared from store');
    expect(manager.instances).not.toContain(instance);
    expect(exchange.cancelOrders).toHaveBeenCalledWith([
      expect.objectContaining({ id: 'order-1', symbol: 'BTCUSDT' }),
    ]);
  });

  it('cancels orders placed during the user-cancel window so STOP fully clears the book', async () => {
    // Race scenario: user clicks STOP while `placeOrder` is mid-flight.
    // Pre-fix, `tryPlaceOrders` would still hand the new order ID back
    // and the instance would be spliced, leaving an orphan resting
    // order — that's the "chase comes back" symptom users reported.
    const { exchange, store } = createMockExchange();
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });

    // Make placeOrder block so we can call remove() while it's in flight.
    let resolvePlace!: (ids: string[]) => void;
    const placePromise = new Promise<string[]>((resolve) => {
      resolvePlace = resolve;
    });
    (exchange.placeOrder as any).mockReset();
    (exchange.placeOrder as any).mockReturnValueOnce(placePromise);

    manager.add(instance);

    // Let processInstance reach the placeOrder await
    await Promise.resolve();
    await Promise.resolve();
    expect(exchange.placeOrder).toHaveBeenCalledTimes(1);

    // User clicks STOP while placeOrder is still pending
    manager.remove(instance.id);
    expect(instance.isStopped).toBe(true);

    // placeOrder resolves with the in-flight order ID
    resolvePlace(['order-late']);
    await vi.advanceTimersByTimeAsync(0);

    // The late order must be cancelled by `tryPlaceOrders` itself
    // (cleanupInstance can't see it because currentOrderIds was still
    // empty when STOP fired).
    expect(exchange.cancelOrders).toHaveBeenCalledWith([
      expect.objectContaining({ id: 'order-late', symbol: 'BTCUSDT' }),
    ]);
    // Instance must be removed from the manager so the UI stops
    // listing the chaser.
    expect(manager.instances).not.toContain(instance);
    // No residual state — `currentOrderIds` must be cleared so a
    // late WS update can't resurrect the order.
    expect(instance.currentOrderIds).toEqual([]);
    void store;
  });

  it('detaches `update` listeners synchronously on remove() so an in-flight tick cannot place a new order', async () => {
    const { emit, exchange, store } = createMockExchange();
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });

    manager.add(instance);
    await vi.advanceTimersByTimeAsync(1000);
    expect(exchange.placeOrder).toHaveBeenCalledTimes(1);

    // Surrogate for "next ticker tick": call remove() then emit. The
    // emitted update must NOT trigger another placeOrder, because
    // cleanupInstance must have already unsubscribed the handler.
    manager.remove(instance.id);
    const placeCallsBefore = (exchange.placeOrder as any).mock.calls.length;

    (store.tickersBySymbol.BTCUSDT as any).bid = 105;
    (store.tickersBySymbol.BTCUSDT as any).ask = 106;
    await emit('update', store);
    await vi.advanceTimersByTimeAsync(100);

    expect((exchange.placeOrder as any).mock.calls.length).toBe(placeCallsBefore);
  });

  it('tears down and warns for active chasers when their exchange is disposed', async () => {
    const { exchange } = createMockExchange();
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });

    manager.add(instance);
    await vi.advanceTimersByTimeAsync(1000);

    manager.cleanupExchange(exchange);

    expect(instance.isStopped).toBe(true);
    expect(manager.instances).not.toContain(instance);
    expect(exchange.off).toHaveBeenCalledWith('update', expect.any(Function));
    expect(exchange.cancelOrders).toHaveBeenCalledWith([
      expect.objectContaining({ id: 'order-1', symbol: 'BTCUSDT' }),
    ]);
    expect(exchange.emitter.emit).toHaveBeenCalledWith(
      'warning',
      expect.stringMatching(/stopped because the exchange connection was disposed/i),
    );
  });
});
