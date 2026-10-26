import type {
  ActiveChaser,
  DisplayedPosition,
  ExchangeName,
  ExchangeOptions,
  Market,
  Order,
  Position,
  Ticker,
} from '../types';

import { describe, expect, it, vi } from 'vitest';

import { FrameApplier } from '../store/frameApplier';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderStatus, OrderType, PositionSide } from '../types';
import { chaserOrderManagerInstance } from './chaserOrder/ChaserOrderManager';
import { ExchangeWorker } from './ExchangeWorker';

const createMarket = (overrides?: Partial<Market>): Market =>
  ({
    id: 'BTCUSDT',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { amount: 0.001, price: 0.01 },
    limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 100 } },
    isInverse: false,
    ...overrides,
  }) as Market;

const createTicker = (overrides?: Partial<Ticker>): Ticker =>
  ({
    id: 'BTCUSDT',
    symbol: 'BTCUSDT',
    bid: 49990,
    ask: 50010,
    last: 50000,
    mark: 50000,
    index: 50000,
    percentage: 1,
    openInterest: 100,
    fundingRate: 0.0001,
    fundingTime: 1_700_000_000_000,
    volume: 1000,
    quoteVolume: 50_000_000,
    ...overrides,
  }) as Ticker;

const createOrder = (overrides?: Partial<Order>): Order => ({
  id: 'order-1',
  symbol: 'BTCUSDT',
  side: OrderSide.Buy,
  type: OrderType.Limit,
  status: OrderStatus.Open,
  price: 50000,
  amount: 1,
  filled: 0,
  remaining: 1,
  reduceOnly: false,
  lastUpdate: 1_700_000_000_000,
  ...overrides,
});

const createPosition = (overrides?: Partial<Position>): Position =>
  ({
    id: 'position-1',
    symbol: 'BTCUSDT',
    side: PositionSide.Long,
    contracts: 1,
    entryPrice: 50000,
    notional: 50000,
    unrealizedPnl: 0,
    leverage: 10,
    liquidationPrice: 45000,
    ...overrides,
  }) as Position;

const createDisplayedPosition = (overrides?: Partial<DisplayedPosition>): DisplayedPosition =>
  ({
    ...createPosition(),
    account: 'test-account',
    exchangeName: 'bybit',
    ...overrides,
  }) as DisplayedPosition;

const createActiveChaser = (overrides?: Partial<ActiveChaser>): ActiveChaser => ({
  id: 'chaser-1',
  symbol: 'BTCUSDT',
  size: 1,
  side: OrderSide.Buy,
  maxAttempts: 3,
  attempts: 1,
  isStopped: false,
  stopReason: '',
  currentOrderIds: ['order-1'],
  ...overrides,
});

const createWorker = () => {
  const messages: any[] = [];
  const ctx = {
    postMessage: vi.fn((message) => {
      messages.push(message);
    }),
  } as unknown as Worker;
  const worker = new ExchangeWorker(
    ctx,
    'test-account',
    'bybit' as ExchangeName,
    {} as ExchangeOptions,
    {} as any,
    {} as any,
    'test-session',
  );

  return { worker, messages };
};

const createHostedWorker = () => {
  const messages: any[] = [];
  const ctx = {
    postMessage: vi.fn((message) => {
      messages.push(message);
    }),
  } as unknown as Worker;
  const worker = new ExchangeWorker(
    ctx,
    'test-account',
    'bybit' as ExchangeName,
    { hostedMode: true } as ExchangeOptions,
    {} as any,
    {} as any,
    'test-session',
  );

  return { worker, messages };
};

const emitStoreUpdate = (worker: ExchangeWorker) => {
  (worker as any)._storeUpdate(worker.store.serialize());
};

describe('ExchangeWorker store payloads', () => {
  // Two delta layers must never stack, and hosted mode is where they would.
  //
  // The far side there is a VirtualWorkerContext that throttles payloads and
  // does its own per-item partial diff of `tickers`. Packing deltas here as
  // well is unsafe because the layers have different properties: a full array
  // is idempotent (drop one, the next carries current values) while a delta is
  // not — `_packTickerDeltasSinceBaseline` advances the baseline WHILE packing,
  // so anything dropped downstream is never sent again. That froze tickers in
  // production on 2026-09-14 and priced BBO hotkeys off a dead book while every
  // WS topic still reported fresh.
  it('never packs ticker deltas in hosted mode', () => {
    const { worker, messages } = createHostedWorker();
    worker.store.update({ tickers: [createTicker()] });

    (worker as any)._storeUpdate(worker.store.serialize(), { forceFullStore: true });

    // The same sequence that produces `tickerDeltas` outside hosted mode — see
    // 'sends packed ticker deltas after a full ticker baseline'.
    worker.store.updateTicker({ id: 'BTCUSDT' }, { last: 51000 });
    emitStoreUpdate(worker);

    const payload = messages.at(-1)?.data;
    expect(payload).not.toHaveProperty('tickerDeltas');

    // The full array keeps flowing instead, which is what makes a dropped
    // payload harmless: the next one carries current values.
    expect(payload.tickers).toHaveLength(1);
    expect(payload.tickers[0].last).toBe(51000);
  });

  it('cleans up exchange-owned chasers before disposing the exchange', () => {
    const { worker } = createWorker();
    const exchange = { dispose: vi.fn() } as any;
    (worker as any)._exchange = exchange;
    const cleanupSpy = vi.spyOn(chaserOrderManagerInstance, 'cleanupExchange');

    worker.dispose();

    expect(cleanupSpy).toHaveBeenCalledWith(exchange);
    expect(cleanupSpy.mock.invocationCallOrder[0]).toBeLessThan(exchange.dispose.mock.invocationCallOrder[0]);
    cleanupSpy.mockRestore();
  });

  it('clears handler timers and runtime pipelines on dispose', () => {
    const { worker } = createWorker();
    const handler = { dispose: vi.fn() };
    const cancel = vi.spyOn((worker as any)._throttledStoreUpdate, 'cancel');
    (worker as any)._exchange = { dispose: vi.fn() };
    (worker as any)._handlers = [handler];
    (worker as any)._postprocessors = [{ process: vi.fn() }];
    (worker as any)._exchangeDependentPostprocessors = [{ process: vi.fn() }];

    worker.dispose();

    expect(cancel).toHaveBeenCalled();
    expect(handler.dispose).toHaveBeenCalled();
    expect((worker as any)._handlers).toEqual([]);
    expect((worker as any)._postprocessors).toEqual([]);
    expect((worker as any)._exchangeDependentPostprocessors).toEqual([]);
  });

  it('cancels trailing store work and clears both processor pipelines on deactivate', () => {
    const { worker, messages } = createWorker();
    const handler = { dispose: vi.fn() };
    const cancel = vi.spyOn((worker as any)._throttledStoreUpdate, 'cancel');
    (worker as any)._exchange = { dispose: vi.fn() };
    (worker as any)._handlers = [handler];
    (worker as any)._postprocessors = [{ process: vi.fn() }];
    (worker as any)._exchangeDependentPostprocessors = [{ process: vi.fn() }];

    worker.handlePayload({ op: 'deactivate' } as any);

    expect(cancel).toHaveBeenCalled();
    expect(handler.dispose).toHaveBeenCalled();
    expect((worker as any)._handlers).toEqual([]);
    expect((worker as any)._postprocessors).toEqual([]);
    expect((worker as any)._exchangeDependentPostprocessors).toEqual([]);
    expect(messages.at(-1)).toEqual({ op: 'deactivate-status', data: true });
  });

  it('omits unchanged markets from repeated live store payloads', () => {
    const { worker, messages } = createWorker();
    worker.store.update({ markets: [createMarket()] });

    (worker as any)._storeUpdate(worker.store.serialize(), { forceFullStore: true });
    expect(messages.at(-1)?.data.markets).toHaveLength(1);

    emitStoreUpdate(worker);
    expect(messages.at(-1)?.data).not.toHaveProperty('markets');
  });

  it('sends markets again after the market version changes', () => {
    const { worker, messages } = createWorker();
    worker.store.update({ markets: [createMarket({ active: true })] });

    (worker as any)._storeUpdate(worker.store.serialize(), { forceFullStore: true });
    emitStoreUpdate(worker);

    worker.store.updateMarket({ id: 'BTCUSDT' }, { active: false });
    emitStoreUpdate(worker);

    expect(messages.at(-1)?.data.markets).toHaveLength(1);
    expect(messages.at(-1)?.data.markets[0].active).toBe(false);

    emitStoreUpdate(worker);
    expect(messages.at(-1)?.data).not.toHaveProperty('markets');
  });

  it('keeps explicit snapshots as full baselines', () => {
    const { worker, messages } = createWorker();
    worker.store.update({
      markets: [createMarket()],
      tickers: [createTicker()],
      orders: [createOrder()],
      positions: [createPosition()],
    });

    (worker as any)._storeUpdate(worker.store.serialize(), { forceFullStore: true });
    emitStoreUpdate(worker);
    expect(messages.at(-1)?.data).not.toHaveProperty('markets');
    expect(messages.at(-1)?.data).not.toHaveProperty('tickers');
    expect(messages.at(-1)?.data).not.toHaveProperty('orders');
    expect(messages.at(-1)?.data).not.toHaveProperty('positions');

    worker.sendSnapshot();
    expect(messages.at(-1)?.data.markets).toHaveLength(1);
    expect(messages.at(-1)?.data.tickers).toHaveLength(1);
    expect(messages.at(-1)?.data.orders).toHaveLength(1);
    expect(messages.at(-1)?.data.positions).toHaveLength(1);
    expect(messages.at(-1)?.data).not.toHaveProperty('tickerDeltas');
  });

  it('omits unchanged orders and positions from repeated live store payloads', () => {
    const { worker, messages } = createWorker();
    worker.store.update({ orders: [createOrder()], positions: [createPosition()] });

    emitStoreUpdate(worker);
    expect(messages.at(-1)?.data.orders).toHaveLength(1);
    expect(messages.at(-1)?.data.positions).toHaveLength(1);

    emitStoreUpdate(worker);
    expect(messages.at(-1)?.data).not.toHaveProperty('orders');
    expect(messages.at(-1)?.data).not.toHaveProperty('positions');
  });

  it('resends orders and positions after their updateIds change', () => {
    const { worker, messages } = createWorker();
    worker.store.update({ orders: [createOrder()], positions: [createPosition()] });

    emitStoreUpdate(worker);
    emitStoreUpdate(worker);
    expect(messages.at(-1)?.data).not.toHaveProperty('orders');
    expect(messages.at(-1)?.data).not.toHaveProperty('positions');

    worker.store.updateOrder({ id: 'order-1' }, { price: 51000 });
    worker.store.updatePosition({ id: 'position-1' }, { contracts: 2 });
    emitStoreUpdate(worker);

    expect(messages.at(-1)?.data.orders).toHaveLength(1);
    expect(messages.at(-1)?.data.orders[0].price).toBe(51000);
    expect(messages.at(-1)?.data.positions).toHaveLength(1);
    expect(messages.at(-1)?.data.positions[0].contracts).toBe(2);
  });

  it('keeps orders and positions when updateIds are missing', () => {
    const { worker, messages } = createWorker();
    worker.store.update({ orders: [createOrder()], positions: [createPosition()] });

    const emitStoreUpdateWithoutUpdateIds = () => {
      const data = worker.store.serialize() as any;
      delete data.updateIds;
      (worker as any)._storeUpdate(data);
    };

    emitStoreUpdateWithoutUpdateIds();
    expect(messages.at(-1)?.data.orders).toHaveLength(1);
    expect(messages.at(-1)?.data.positions).toHaveLength(1);

    emitStoreUpdateWithoutUpdateIds();
    expect(messages.at(-1)?.data.orders).toHaveLength(1);
    expect(messages.at(-1)?.data.positions).toHaveLength(1);
  });

  it('keeps orders while active chasers can update order enrichment', () => {
    const { worker, messages } = createWorker();
    worker.store.update({
      orders: [createOrder()],
      activeChasers: [createActiveChaser({ attempts: 1 })],
    });

    emitStoreUpdate(worker);
    expect(messages.at(-1)?.data.orders).toHaveLength(1);

    worker.store.update({ activeChasers: [createActiveChaser({ attempts: 2 })] });
    emitStoreUpdate(worker);

    expect(messages.at(-1)?.data.orders).toHaveLength(1);

    worker.store.update({ activeChasers: [] });
    emitStoreUpdate(worker);
    expect(messages.at(-1)?.data.orders).toHaveLength(1);

    emitStoreUpdate(worker);
    expect(messages.at(-1)?.data).not.toHaveProperty('orders');
  });

  it('keeps displayed positions even when raw positions are unchanged', () => {
    const { worker, messages } = createWorker();
    worker.store.update({
      positions: [createPosition()],
      displayedPositions: [createDisplayedPosition({ oneMinUpnl: 1 })],
    });

    emitStoreUpdate(worker);
    worker.store.update({ displayedPositions: [createDisplayedPosition({ oneMinUpnl: 2 })] });
    emitStoreUpdate(worker);

    expect(messages.at(-1)?.data).not.toHaveProperty('positions');
    expect(messages.at(-1)?.data.displayedPositions).toHaveLength(1);
    expect(messages.at(-1)?.data.displayedPositions[0].oneMinUpnl).toBe(2);
  });

  it('sends packed ticker deltas after a full ticker baseline', () => {
    const { worker, messages } = createWorker();
    worker.store.update({ tickers: [createTicker()] });

    (worker as any)._storeUpdate(worker.store.serialize(), { forceFullStore: true });
    expect(messages.at(-1)?.data.tickers).toHaveLength(1);
    expect(messages.at(-1)?.data).not.toHaveProperty('tickerDeltas');

    worker.store.updateTicker({ id: 'BTCUSDT' }, { last: 51000 });
    emitStoreUpdate(worker);

    expect(messages.at(-1)?.data).not.toHaveProperty('tickers');
    expect(messages.at(-1)?.data.tickerDeltas).toEqual([[0, 4, 51000]]);
  });

  it('sends full tickers when the ticker version changes', () => {
    const { worker, messages } = createWorker();
    worker.store.update({ tickers: [createTicker()] });

    (worker as any)._storeUpdate(worker.store.serialize(), { forceFullStore: true });
    worker.store.update({
      tickers: [createTicker(), createTicker({ id: 'ETHUSDT', symbol: 'ETHUSDT', bid: 3990, ask: 4010, last: 4000 })],
    });
    emitStoreUpdate(worker);

    expect(messages.at(-1)?.data.tickers).toHaveLength(2);
    expect(messages.at(-1)?.data).not.toHaveProperty('tickerDeltas');
  });

  // Regression guard for the worker→main bridge shared-reference race.
  //
  // The real bridge path is: `store.subscribe(cb)` fires `cb(this.state)` with
  // the LIVE store state object. The throttled callback then calls
  // `_storeUpdate(state)` which calls `_prepareStorePayload(state)` →
  // `_sanitizeForPostMessage(state)`. The sanitize step does `{ ...state }`
  // which creates a new top-level object but keeps the SAME REFERENCE for
  // `state.updateIds`. Between the omit decision and the structured-clone
  // serialization at postMessage time, an inbound WS frame (e.g. Canceled)
  // can fire `addOrUpdateOrders` synchronously and bump
  // `state.updateIds.orders++`. The bump bleeds into the still-pending payload
  // via the shared object reference. Receiver sees `updateIds.orders=N+1`
  // with `orders` omitted, advances its own counter, and the controller's
  // `pruneUnchangedWorkerStorePayload` drops orders from the next (real)
  // payload because counters match. Cancel never lands on the main thread.
  //
  // The fix decouples `updateIds`, `loaded`, and `socketHealth` at sanitize
  // time. See packages/safe-cex/src/webworker/CLAUDE.md →
  // "Worker → main thread bridge: structured-clone race".
  //
  // The test simulates the race by:
  //   1. Pushing the LIVE store state (not serialize()) into _storeUpdate,
  //      same as store.subscribe does.
  //   2. Reading the payload off the captured postMessage call.
  //   3. Mutating the live store's updateIds (simulates the inbound WS frame
  //      between sanitize and structured-clone-delivery).
  //   4. Asserting the captured payload's updateIds didn't move.
  const emitLiveStoreUpdate = (worker: ExchangeWorker) => {
    // store.subscribe fires with this.state directly — bypass serialize() so
    // the shared-reference race is exercised the way the real bridge sees it.
    (worker as any)._storeUpdate((worker.store as any).state);
  };

  it('decouples updateIds/loaded/socketHealth from the live store after sanitize', () => {
    const { worker, messages } = createWorker();
    worker.store.update({ orders: [createOrder()], positions: [createPosition()] });

    emitLiveStoreUpdate(worker);
    const payload = messages.at(-1)?.data;
    expect(payload?.orders).toHaveLength(1);

    const sentOrdersUpdateId = payload.updateIds?.orders;
    const sentPositionsUpdateId = payload.updateIds?.positions;

    // Simulate the post-decision mutation: an inbound WS frame mutates the
    // live store between the sanitize call and the structured-clone deliver.
    worker.store.addOrUpdateOrders([createOrder({ id: 'order-2', price: 51000 })]);
    worker.store.addOrUpdatePositions([createPosition({ id: 'position-2', symbol: 'ETHUSDT' })]);

    // The already-delivered payload's updateIds MUST NOT have changed.
    expect(payload.updateIds.orders).toBe(sentOrdersUpdateId);
    expect(payload.updateIds.positions).toBe(sentPositionsUpdateId);
    // The live store's counters should have advanced.
    expect(worker.store.updateIds.orders).toBeGreaterThan(sentOrdersUpdateId);
    expect(worker.store.updateIds.positions).toBeGreaterThan(sentPositionsUpdateId);
  });

  // Regression guard for the omit-decision invariant. When the worker decides
  // shouldSendOrders=false (omit), the payload's updateIds.orders MUST equal
  // the updateIds.orders the decision was based on — not a later live-store
  // value. Same race shape as above, but explicitly exercises the omit path.
  it('keeps updateIds frozen on omit-orders payloads even when the live store advances', () => {
    const { worker, messages } = createWorker();
    worker.store.update({ orders: [createOrder()], positions: [createPosition()] });

    emitLiveStoreUpdate(worker); // first emit — sends orders
    emitLiveStoreUpdate(worker); // second emit — omits orders (no change since last send)

    const omitPayload = messages.at(-1)?.data;
    expect(omitPayload).not.toHaveProperty('orders');
    const omitOrdersUpdateId = omitPayload.updateIds?.orders;

    // Simulate the post-decision mutation.
    worker.store.addOrUpdateOrders([createOrder({ id: 'order-2' })]);

    // The omit payload's updateIds.orders MUST stay frozen at the omit-decision
    // value. If it bleeds to the new live value, the receiver advances its
    // counter without applying any orders array and the bug recurs.
    expect(omitPayload.updateIds.orders).toBe(omitOrdersUpdateId);
    expect(worker.store.updateIds.orders).toBeGreaterThan(omitOrdersUpdateId);
  });
});

it('forwards midpoint changes and clearing through the object worker delta bridge', () => {
  const { worker, messages } = createWorker();
  const main = new DefaultStore();
  worker.store.update({ tickers: [createTicker({ midPrice: 100 })] });
  emitStoreUpdate(worker);
  main.update(messages.at(-1).data, true);
  expect(main.tickers[0].midPrice).toBe(100);
  worker.store.updateTicker({ id: 'BTCUSDT' }, { midPrice: 101 });
  emitStoreUpdate(worker);
  expect(messages.at(-1).data.tickerDeltas).toEqual([[0, 1 << 11, 101]]);
  main.update(messages.at(-1).data, true);
  expect(main.tickers[0].midPrice).toBe(101);
  worker.store.updateTicker({ id: 'BTCUSDT' }, { midPrice: undefined });
  emitStoreUpdate(worker);
  expect(messages.at(-1).data.tickerDeltas).toEqual([[0, 1 << 11, NaN]]);
  main.update(messages.at(-1).data, true);
  expect('midPrice' in main.tickers[0]).toBe(false);
});


it('forwards query-only leverage in frame mode, including late-client snapshots and authoritative clearing', () => {
  const { worker, messages } = createWorker();
  (worker as any)._forwardsFrames = true;
  const applier = new FrameApplier(worker.store);
  const main = new DefaultStore();
  const send = () => (worker as any)._storeUpdate({
    ...worker.store.serialize(), fetchedLeverage: worker.store.fetchedLeverage,
  });
  const rails = { buyLeverage: 7, sellLeverage: 9 };
  applier.applyLeverage('BTCUSDT', rails);
  send();
  main.update(messages.at(-1).data, true);
  expect(main.fetchedLeverage.BTCUSDT).toEqual(rails);
  expect(messages.at(-1).data).not.toHaveProperty('orders');
  expect(messages.at(-1).data).not.toHaveProperty('tickers');
  send();
  expect(messages.at(-1).data).not.toHaveProperty('fetchedLeverage');
  worker.sendSnapshot();
  expect(messages.at(-1).data.fetchedLeverage.BTCUSDT).toEqual(rails);
  worker.store.update({ fetchedLeverage: {} });
  send();
  main.update(messages.at(-1).data, true);
  expect(main.fetchedLeverage).toEqual({});
});
