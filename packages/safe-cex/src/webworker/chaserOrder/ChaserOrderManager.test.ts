import type { Exchange } from '../../exchanges/base';
import type { Market, Order, StoreData, Ticker } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Frame, SECTION } from '@tealstreet/safe-cex-wasm/wire';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionSide } from '../../types';
import { ChaserOrderInstance } from './ChaserOrderInstance';
import { ChaserOrderManager } from './ChaserOrderManager';

type Handler = (...args: any[]) => void;

// Encode revision-zero rows independently so the real binary reader and fill
// bridge participate in the chaser regression without requiring a WASM build.
const createFillFrame = (fills: Array<{ orderId: string; market: number; amount: number }>): Frame => {
  const align8 = (value: number) => Math.ceil(value / 8) * 8;
  const strings = ['BTCUSDT', 'ETHUSDT', 'BTC', 'ETH', 'USDT', ...fills.map((fill) => fill.orderId)];
  const sections: Array<{ kind: number; bytes: Uint8Array }> = [];
  const table = (kind: number, words: number[], numbers: number[]) => {
    const bytes = new Uint8Array(align8(words.length * 4) + numbers.length * 8);
    const view = new DataView(bytes.buffer);
    words.forEach((word, i) => view.setUint32(i * 4, word, true));
    numbers.forEach((number, i) => view.setFloat64(align8(words.length * 4) + i * 8, number, true));
    sections.push({ kind, bytes });
  };
  table(
    SECTION.MARKETS,
    [2, 0, 0, 2, 4, 1, 1, 3, 4, 1, 1],
    [0.001, 1, 0.001, 100, 1, 100, NaN, 1, NaN, NaN].flatMap((value) => [value, value]),
  );
  table(
    SECTION.FILLS,
    [fills.length, ...fills.map((_, i) => 5 + i), ...fills.map((fill) => fill.market), ...fills.map(() => 0)],
    fills.flatMap((fill) => [100, fill.amount]),
  );
  const encoded = strings.map((value) => new TextEncoder().encode(value));
  const stringBytes = new Uint8Array(
    align8(4 + (strings.length + 1) * 4 + encoded.reduce((size, value) => size + value.length, 0)),
  );
  const stringView = new DataView(stringBytes.buffer);
  stringView.setUint32(0, strings.length, true);
  let end = 0;
  encoded.forEach((value, i) => {
    stringView.setUint32(4 + i * 4, end, true);
    stringBytes.set(value, 4 + (strings.length + 1) * 4 + end);
    end += value.length;
  });
  stringView.setUint32(4 + strings.length * 4, end, true);
  sections.push({ kind: SECTION.STRINGS, bytes: stringBytes });
  const buffer = new ArrayBuffer(16 + sections.reduce((size, section) => size + 8 + section.bytes.length, 0));
  const view = new DataView(buffer);
  view.setUint32(0, 0x46584353, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, sections.length, true);
  let offset = 16;
  for (const section of sections) {
    view.setUint16(offset, section.kind, true);
    view.setUint32(offset + 4, section.bytes.length, true);
    new Uint8Array(buffer, offset + 8, section.bytes.length).set(section.bytes);
    offset += 8 + section.bytes.length;
  }
  return new Frame(buffer);
};

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
    socketHealth: {
      publicHealthy: true,
      privateHealthy: true,
      publicInitialized: true,
      privateInitialized: true,
    },
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

  it('tracks Rust FILLS frames through the GRVT chaser while the order status remains open', async () => {
    const { emit, exchange, order } = createMockExchange();
    (exchange as any).name = 'grvt';
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });
    const applier = new FrameApplier(new DefaultStore());
    const deliver = async (fills: Parameters<typeof createFillFrame>[0]) => {
      const frame = createFillFrame(fills);
      applier.apply(frame);
      const events = applier.fills(frame);
      expect(events.map((fill) => fill.orderId)).toEqual(fills.map((fill) => fill.orderId));
      for (const fill of events) await emit('fill', fill);
    };

    manager.add(instance);
    await vi.advanceTimersByTimeAsync(1000);
    expect(exchange.on).toHaveBeenCalledWith('fill', expect.any(Function));

    await deliver([
      { orderId: 'unrelated-order', market: 0, amount: 1 },
      { orderId: 'order-1', market: 1, amount: 1 },
    ]);
    expect(instance.remainingSize).toBe(1);
    expect(instance.isStopped).toBe(false);

    await deliver([{ orderId: 'order-1', market: 0, amount: 0.4 }]);
    expect(instance.remainingSize).toBe(0.6);
    expect(instance.isStopped).toBe(false);
    expect(manager.instances).toContain(instance);
    expect(order.status).toBe(OrderStatus.Open);
    expect(order.filled).toBe(0);

    await deliver([{ orderId: 'order-1', market: 0, amount: 0.6 }]);
    await vi.advanceTimersByTimeAsync(0);
    expect(instance.remainingSize).toBe(0);
    expect(instance.isStopped).toBe(true);
    expect(manager.instances).not.toContain(instance);
    expect(order.status).toBe(OrderStatus.Open);
    expect(order.filled).toBe(0);
    expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
    expect(exchange.emitter.emit).toHaveBeenCalledWith('chaser:complete', expect.objectContaining({ filled: 1 }));
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

    expect(instance.state.attempts).toBe(1);
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

  it('tracks scaled partial fills in the same placement units used for replacement', async () => {
    const { emit, exchange, order: initialOrder, store } = createMockExchange();
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      size: 15,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });
    const scaledOrder: Order = {
      ...initialOrder,
      amount: 15000,
      filled: 2500,
      remaining: 12500,
      extra: {
        filledInPlacementUnits: 2.5,
      },
    };
    store.orders.splice(0, store.orders.length, scaledOrder);

    manager.add(instance);
    await vi.advanceTimersByTimeAsync(1000);
    await emit('update', store);

    expect(instance.isStopped).toBe(false);
    expect(instance.remainingSize).toBe(12.5);
    expect(manager.instances).toContain(instance);

    await vi.advanceTimersByTimeAsync(1500);
    Object.assign(store.tickersBySymbol.BTCUSDT, { bid: 101, ask: 102 });
    const repriceUpdate = emit('update', store);
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(1000);
    await repriceUpdate;

    expect(exchange.cancelOrders).toHaveBeenCalledWith([scaledOrder]);
    expect(exchange.placeOrder).toHaveBeenLastCalledWith(
      expect.objectContaining({
        amount: 12.5,
      }),
    );
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

  // A dead private socket is an expected mode on Bybit Demo in geo-restricted
  // regions. Refusing the chase there deleted the feature for those users; the
  // REST fill/position reconciliation path exists to carry it instead.
  it('runs a bybitdemo chaser over REST reconciliation when private websocket reconnects are paused', async () => {
    const { exchange, store } = createMockExchange();
    (exchange as any).name = 'bybitdemo';
    (store as any).socketHealth = {
      publicHealthy: true,
      privateHealthy: false,
      publicInitialized: true,
      privateInitialized: true,
      privateReconnectPaused: true,
      privateCloseCode: 1006,
    };
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

    expect(exchange.placeOrder).toHaveBeenCalled();
    expect(instance.isStopped).toBe(false);
    expect(manager.instances).toContain(instance);
    expect(exchange.emitter.emit).toHaveBeenCalledWith(
      'warning',
      expect.stringMatching(
        /reconciling over REST.*reconnects are paused after repeated failures.*last close code 1006/i,
      ),
    );
    expect(exchange.emitter.emit).toHaveBeenCalledWith(
      'warning',
      expect.stringMatching(/VPN or Tealstreet IP routing/i),
    );
  });

  it('warns but still chases when a bybitdemo private websocket never initializes', async () => {
    const { exchange, store } = createMockExchange();
    (exchange as any).name = 'bybitdemo';
    (store as any).socketHealth = {
      publicHealthy: true,
      privateHealthy: false,
      publicInitialized: true,
      privateInitialized: false,
    };
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
    await vi.advanceTimersByTimeAsync(4000);

    expect(exchange.placeOrder).toHaveBeenCalled();
    expect(instance.isStopped).toBe(false);
    expect(exchange.emitter.emit).toHaveBeenCalledWith(
      'warning',
      expect.stringMatching(/reconciling over REST.*never delivered a message/i),
    );
  });

  // A socket that is down but still retrying recovers on its own within
  // seconds. Refusing here made every routine `1006` blip a failed chase; the
  // Bybit snapshot shield is what keeps the resting order safe meanwhile.
  it('still starts a bybitdemo chaser when the private websocket is unhealthy but retrying', async () => {
    const { exchange, store } = createMockExchange();
    (exchange as any).name = 'bybitdemo';
    (store as any).socketHealth = {
      publicHealthy: true,
      privateHealthy: false,
      publicInitialized: true,
      privateInitialized: true,
      privateCloseCode: 1006,
    };
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

    expect(exchange.placeOrder).toHaveBeenCalled();
    expect(instance.isStopped).toBe(false);
    expect(manager.instances).toContain(instance);
  });

  it('starts a bybitdemo chaser once a cold private websocket initializes within the grace window', async () => {
    const { exchange, store } = createMockExchange();
    (exchange as any).name = 'bybitdemo';
    (store as any).socketHealth = {
      publicHealthy: true,
      privateHealthy: false,
      publicInitialized: true,
      privateInitialized: false,
    };
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
    expect(exchange.placeOrder).not.toHaveBeenCalled();

    (store as any).socketHealth = {
      ...store.socketHealth!,
      privateHealthy: true,
      privateInitialized: true,
    };
    await vi.advanceTimersByTimeAsync(1500);

    expect(exchange.placeOrder).toHaveBeenCalled();
    expect(instance.isStopped).toBe(false);
  });

  // The runtime health check used to run on every store update, so a single
  // transient close killed a live chase and cancelled its resting order.
  it('does not stop an active bybitdemo chaser when the private websocket blips', async () => {
    const { emit, exchange, store } = createMockExchange();
    (exchange as any).name = 'bybitdemo';
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

    (store as any).socketHealth = {
      ...store.socketHealth!,
      privateHealthy: false,
      privateReconnectPaused: true,
      privateCloseCode: 1006,
    };
    await emit('update', store);

    expect(instance.isStopped).toBe(false);
    expect(manager.instances).toContain(instance);
    expect(exchange.cancelOrders).not.toHaveBeenCalled();
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

  it('stops non-reduce chasers when the target position size is reached', async () => {
    const { emit, exchange, store } = createMockExchange();
    (exchange as any).name = 'bybitdemo';
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

    (store as any).positions = [
      {
        id: 'BTCUSDT-long',
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        entryPrice: 100,
        notional: 100,
        leverage: 1,
        unrealizedPnl: 0,
        contracts: 1,
        liquidationPrice: 0,
      },
    ];
    store.updateIds.positions += 1;

    await emit('update', store);

    expect(instance.isStopped).toBe(true);
    expect(instance.state.stopReason).toBe('Completed');
    expect(manager.instances).not.toContain(instance);
    expect(exchange.cancelOrders).toHaveBeenCalledWith([expect.objectContaining({ id: 'order-1', symbol: 'BTCUSDT' })]);
  });

  it('uses the starting same-side position as the non-reduce chaser target baseline', async () => {
    const { emit, exchange, store } = createMockExchange();
    (exchange as any).name = 'bybitdemo';
    (store as any).positions = [
      {
        id: 'BTCUSDT-long',
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        entryPrice: 100,
        notional: 200,
        leverage: 1,
        unrealizedPnl: 0,
        contracts: 2,
        liquidationPrice: 0,
      },
    ];
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

    (store.positions[0] as any).contracts = 2.5;
    store.updateIds.positions += 1;
    await emit('update', store);
    expect(instance.isStopped).toBe(false);

    (store.positions[0] as any).contracts = 3;
    store.updateIds.positions += 1;
    await emit('update', store);

    expect(instance.isStopped).toBe(true);
    expect(instance.state.stopReason).toBe('Completed');
  });

  it('does not use position-target completion outside Bybit demo', async () => {
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

    (store as any).positions = [
      {
        id: 'BTCUSDT-long',
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        entryPrice: 100,
        notional: 100,
        leverage: 1,
        unrealizedPnl: 0,
        contracts: 1,
        liquidationPrice: 0,
      },
    ];
    store.updateIds.positions += 1;

    await emit('update', store);

    expect(instance.isStopped).toBe(false);
    expect(exchange.cancelOrders).not.toHaveBeenCalled();
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
    expect(exchange.cancelOrders).toHaveBeenCalledWith([expect.objectContaining({ id: 'order-1', symbol: 'BTCUSDT' })]);
    // This path completing silently is how the Bybit demo bug went unreported
    // for two weeks. It must always tell the user why the chase ended.
    expect(exchange.emitter.emit).toHaveBeenCalledWith('warning', expect.stringMatching(/left the order book/i));
  });

  it('warns the user when a chaser cannot place its first order', async () => {
    const { exchange } = createMockExchange();
    (exchange.placeOrder as any).mockReset();
    (exchange.placeOrder as any).mockResolvedValue([]);
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
    await vi.advanceTimersByTimeAsync(5000);

    expect(instance.isStopped).toBe(true);
    expect(manager.instances).not.toContain(instance);
    expect(exchange.emitter.emit).toHaveBeenCalledWith('error', expect.stringMatching(/failed to place orders/i));
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
    expect(exchange.cancelOrders).toHaveBeenCalledWith([expect.objectContaining({ id: 'order-1', symbol: 'BTCUSDT' })]);
    expect(exchange.emitter.emit).toHaveBeenCalledWith(
      'warning',
      expect.stringMatching(/stopped because the exchange connection was disposed/i),
    );
  });

  describe('chaser:complete', () => {
    const completeEvents = (exchange: Exchange) =>
      (exchange.emitter.emit as any).mock.calls.filter(([event]: [string]) => event === 'chaser:complete');

    const newInstance = (exchange: Exchange) =>
      new ChaserOrderInstance(exchange, {
        symbol: 'BTCUSDT',
        size: 1,
        side: OrderSide.Buy,
        maxAttempts: 10,
        reduceOnly: false,
        delay: 5000,
        clientId: 'CHS-test',
      });

    it('emits once with the full size when the chase fills', async () => {
      const { emit, exchange, order, store } = createMockExchange();
      const manager = new ChaserOrderManager();
      const instance = newInstance(exchange);

      manager.add(instance);
      await vi.advanceTimersByTimeAsync(1000);
      await emit('update', store);

      (order as any).status = OrderStatus.Closed;
      (order as any).filled = 1;
      store.updateIds.orders += 1;
      await emit('update', store);
      await vi.advanceTimersByTimeAsync(0);
      manager.remove(instance.id);
      await vi.advanceTimersByTimeAsync(0);

      expect(completeEvents(exchange)).toEqual([
        [
          'chaser:complete',
          expect.objectContaining({
            id: instance.id,
            clientId: 'CHS-test',
            side: OrderSide.Buy,
            reduceOnly: false,
            size: 1,
            filled: 1,
            stopReason: 'Completed',
          }),
        ],
      ]);
    });

    it('counts a partial fill the store shows on the cancelled order when the user stops', async () => {
      const { exchange, order } = createMockExchange();
      const manager = new ChaserOrderManager();
      const instance = newInstance(exchange);

      manager.add(instance);
      await vi.advanceTimersByTimeAsync(1000);
      // The fill landed but its update never reached the (now detached) supervisor.
      (order as any).filled = 0.4;

      manager.remove(instance.id);
      await vi.advanceTimersByTimeAsync(0);

      expect(completeEvents(exchange)).toEqual([
        ['chaser:complete', expect.objectContaining({ filled: 0.4, stopReason: 'User cancelled' })],
      ]);
    });

    it('emits filled 0 when nothing filled', async () => {
      const { exchange } = createMockExchange();
      const manager = new ChaserOrderManager();
      const instance = newInstance(exchange);

      manager.add(instance);
      await vi.advanceTimersByTimeAsync(1000);
      manager.removeKeepOrder(instance.id);

      expect(completeEvents(exchange)).toEqual([['chaser:complete', expect.objectContaining({ filled: 0 })]]);
    });

    it('sizes a bybitdemo fill from the position when order frames never arrived', async () => {
      const { emit, exchange, store } = createMockExchange();
      (exchange as any).name = 'bybitdemo';
      const manager = new ChaserOrderManager();
      const instance = newInstance(exchange);

      manager.add(instance);
      await vi.advanceTimersByTimeAsync(1000);

      (store as any).positions = [
        { id: 'BTCUSDT-long', symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
      ];
      store.updateIds.positions += 1;
      await emit('update', store);
      await vi.advanceTimersByTimeAsync(0);

      expect(completeEvents(exchange)).toEqual([
        ['chaser:complete', expect.objectContaining({ filled: 1, stopReason: 'Completed' })],
      ]);
    });

    it('a concurrent second cleanup does not report before the first cancel lands', async () => {
      const { exchange, order } = createMockExchange();
      const manager = new ChaserOrderManager();
      const instance = newInstance(exchange);

      manager.add(instance);
      await vi.advanceTimersByTimeAsync(1000);

      let resolveCancel!: () => void;
      (exchange.cancelOrders as any).mockReturnValueOnce(new Promise<void>((resolve) => (resolveCancel = resolve)));
      instance.markComplete('User cancelled');
      const first = manager.cleanupInstance(instance);
      await manager.cleanupInstance(instance);
      expect(completeEvents(exchange)).toEqual([]);

      (order as any).filled = 0.4;
      resolveCancel();
      await first;

      expect(completeEvents(exchange)).toEqual([['chaser:complete', expect.objectContaining({ filled: 0.4 })]]);
    });

    it('reads REST fills for an order whose row the venue already dropped', async () => {
      const { exchange, store } = createMockExchange();
      (exchange as any).name = 'grvt';
      (exchange as any).fetchFillsForOrderIds = vi.fn().mockResolvedValue(new Map([['order-1', 0.7]]));
      const manager = new ChaserOrderManager();
      const instance = newInstance(exchange);

      manager.add(instance);
      await vi.advanceTimersByTimeAsync(1000);
      (store as any).orders = [];

      manager.remove(instance.id);
      await vi.advanceTimersByTimeAsync(0);

      expect((exchange as any).fetchFillsForOrderIds).toHaveBeenCalledWith(['order-1']);
      expect(completeEvents(exchange)).toEqual([['chaser:complete', expect.objectContaining({ filled: 0.7 })]]);
    });

    it('skips REST reconciliation for a chase nobody is waiting on', async () => {
      const { exchange } = createMockExchange();
      (exchange as any).name = 'grvt';
      (exchange as any).fetchFillsForOrderIds = vi.fn().mockResolvedValue(new Map());
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
      manager.remove(instance.id);
      await vi.advanceTimersByTimeAsync(0);

      expect((exchange as any).fetchFillsForOrderIds).not.toHaveBeenCalled();
      expect(completeEvents(exchange)).toHaveLength(1);
    });

    it('trusts REST over the bybitdemo position delta, which counts other trades', async () => {
      const { exchange, store } = createMockExchange();
      (exchange as any).name = 'bybitdemo';
      (exchange as any).fetchFillsForOrderIds = vi.fn().mockResolvedValue(new Map([['order-1', 0]]));
      const manager = new ChaserOrderManager();
      const instance = newInstance(exchange);

      manager.add(instance);
      await vi.advanceTimersByTimeAsync(1000);
      // A separate market buy grew the long; this chase filled nothing.
      (store as any).positions = [
        { id: 'BTCUSDT-long', symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 0.5 },
      ];

      manager.remove(instance.id);
      await vi.advanceTimersByTimeAsync(0);

      expect(completeEvents(exchange)).toEqual([['chaser:complete', expect.objectContaining({ filled: 0 })]]);
    });

    it('counts a closed order the supervisor never processed when the user stops', async () => {
      const { exchange, order } = createMockExchange();
      const manager = new ChaserOrderManager();
      const instance = newInstance(exchange);

      manager.add(instance);
      await vi.advanceTimersByTimeAsync(1000);
      (order as any).status = OrderStatus.Closed;
      (order as any).filled = 1;

      manager.remove(instance.id);
      await vi.advanceTimersByTimeAsync(0);

      expect(completeEvents(exchange)).toEqual([['chaser:complete', expect.objectContaining({ filled: 1 })]]);
    });
  });
});

// A placement whose response is lost may have landed; placing again doubles it.
describe('ChaserOrderManager lost placement', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  const startChase = (exchange: Exchange) => {
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
    return instance;
  };

  const withLostResponse = () => {
    const mock = createMockExchange();
    const exchange = mock.exchange as any;
    exchange.mintClientOrderId = vi.fn(() => 'chase-cid-1');
    exchange.placeOrder = vi.fn().mockRejectedValueOnce(new Error('timeout of 20000ms exceeded'));
    exchange.findOrderByClientOrderId = vi.fn();
    return { ...mock, exchange };
  };

  it('adopts the order a lost response placed instead of placing it again', async () => {
    const { exchange } = withLostResponse();
    exchange.findOrderByClientOrderId
      .mockResolvedValueOnce({ found: false, definitive: false })
      .mockResolvedValueOnce({ found: true, order: createOrder('order-1', 100) });

    const instance = startChase(exchange);
    await vi.advanceTimersByTimeAsync(5_000);

    expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
    expect(exchange.placeOrder).toHaveBeenCalledWith(expect.objectContaining({ clientOrderId: 'chase-cid-1' }));
    expect(instance.currentOrderIds).toEqual(['order-1']);
    expect(instance.isStopped).toBe(false);
  });

  it('stops rather than placing blind when the order cannot be confirmed', async () => {
    const { exchange } = withLostResponse();
    exchange.findOrderByClientOrderId.mockResolvedValue({ found: false, definitive: false });

    const instance = startChase(exchange);
    await vi.advanceTimersByTimeAsync(20_000);

    expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
    expect(instance.isStopped).toBe(true);
    expect(exchange.emitter.emit).toHaveBeenCalledWith('error', expect.stringContaining('check open orders'));
    const completions = (exchange.emitter.emit as any).mock.calls.filter(([event]: [string]) => event === 'chaser:complete');
    expect(completions).toHaveLength(1);
    expect(completions[0][1]).toMatchObject({ stopReason: 'Could not confirm order placement' });
  });

  it('counts a fill the lost order already made, then places only the rest', async () => {
    const { exchange } = withLostResponse();
    exchange.placeOrder.mockResolvedValueOnce(['order-2']);
    exchange.findOrderByClientOrderId.mockResolvedValueOnce({
      found: true,
      order: { ...createOrder('order-x', 100), status: OrderStatus.Canceled, filled: 0.4 },
    });

    startChase(exchange);
    await vi.advanceTimersByTimeAsync(3_000);

    expect(exchange.placeOrder).toHaveBeenCalledTimes(2);
    expect(exchange.placeOrder).toHaveBeenLastCalledWith(expect.objectContaining({ amount: 0.6 }));
  });

  it('a signing timeout sent nothing, so it retries instead of looking up', async () => {
    const { exchange } = withLostResponse();
    exchange.placeOrder = vi
      .fn()
      .mockRejectedValueOnce(new Error('Signing request abc timed out after 15000ms'))
      .mockResolvedValueOnce(['order-1']);

    startChase(exchange);
    await vi.advanceTimersByTimeAsync(1_000);

    expect(exchange.findOrderByClientOrderId).not.toHaveBeenCalled();
    expect(exchange.placeOrder).toHaveBeenCalledTimes(2);
  });

  it('retries at once when the venue refused the order', async () => {
    const { exchange } = withLostResponse();
    exchange.placeOrder = vi
      .fn()
      .mockRejectedValueOnce(Object.assign(new Error('Order would immediately match'), { response: { status: 400 } }))
      .mockResolvedValueOnce(['order-1']);

    startChase(exchange);
    await vi.advanceTimersByTimeAsync(1_000);

    expect(exchange.findOrderByClientOrderId).not.toHaveBeenCalled();
    expect(exchange.placeOrder).toHaveBeenCalledTimes(2);
  });
});
