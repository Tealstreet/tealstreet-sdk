import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Market, Order, OrderSide, OrderStatus, OrderType, Position, PositionSide, Ticker } from '../types';
import { DefaultStore, defaultStore } from './store.base';
import { stubStore } from './store.stub';

// Helper factories
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
  lastUpdate: Date.now(),
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
    last: 50000,
    bid: 49990,
    ask: 50010,
    high: 51000,
    low: 49000,
    ...overrides,
  }) as Ticker;

describe('DefaultStore', () => {
  let store: DefaultStore;

  beforeEach(() => {
    store = new DefaultStore();
  });

  describe('initial state', () => {
    it('should initialize with default values', () => {
      expect(store.latency).toBe(0);
      expect(store.balance).toEqual({ used: 0, free: 0, total: 0, upnl: 0 });
      expect(store.markets).toEqual([]);
      expect(store.tickers).toEqual([]);
      expect(store.positions).toEqual([]);
      expect(store.orders).toEqual([]);
    });

    it('should initialize with loaded flags set to false', () => {
      expect(store.loaded.balance).toBe(false);
      expect(store.loaded.orders).toBe(false);
      expect(store.loaded.markets).toBe(false);
      expect(store.loaded.tickers).toBe(false);
      expect(store.loaded.positions).toBe(false);
    });

    it('should initialize with socket health flags set to false', () => {
      expect(store.socketHealth?.publicHealthy).toBe(false);
      expect(store.socketHealth?.privateHealthy).toBe(false);
    });
  });

  describe('update', () => {
    it('should update state with partial changes', () => {
      store.update({ latency: 100 });
      expect(store.latency).toBe(100);
    });

    it('should update balance', () => {
      store.update({ balance: { used: 1000, free: 9000, total: 10000, upnl: 500 } });
      expect(store.balance.total).toBe(10000);
      expect(store.balance.upnl).toBe(500);
    });

    it('should rebuild ticker maps when tickers are updated', () => {
      const tickers = [createTicker({ id: 'BTCUSDT', symbol: 'BTCUSDT' })];
      store.update({ tickers });
      expect(store.tickersById['BTCUSDT']).toBeDefined();
      expect(store.tickersBySymbol['BTCUSDT']).toBeDefined();
    });

    it('should rebuild market maps when markets are updated', () => {
      const markets = [createMarket({ id: 'BTCUSDT', symbol: 'BTCUSDT' })];
      store.update({ markets });
      expect(store.marketsById['BTCUSDT']).toBeDefined();
      expect(store.marketsBySymbol['BTCUSDT']).toBeDefined();
    });

    it('should notify listeners on update', () => {
      const listener = vi.fn();
      store.subscribe(listener);
      store.update({ latency: 100 });
      expect(listener).toHaveBeenCalledTimes(1);
    });

    it('adopts worker updateIds without bumping repeated private snapshots', () => {
      store.update(
        {
          orders: [createOrder({ id: 'order-1' })],
          positions: [createPosition({ id: 'position-1' })],
          updateIds: { orders: 4, positions: 5 },
        },
        true,
      );

      expect(store.updateIds).toEqual({ orders: 4, positions: 5 });

      const listener = vi.fn();
      store.subscribe(listener);
      store.update(
        {
          orders: [createOrder({ id: 'order-2' })],
          positions: [createPosition({ id: 'position-2' })],
          updateIds: { orders: 4, positions: 5 },
        },
        true,
      );

      expect(store.orders[0].id).toBe('order-1');
      expect(store.positions[0].id).toBe('position-1');
      expect(store.updateIds).toEqual({ orders: 4, positions: 5 });
      expect(listener).not.toHaveBeenCalled();
    });

    it('keeps derived displayed positions when worker position updateId repeats', () => {
      store.update(
        {
          positions: [createPosition({ id: 'position-1' })],
          displayedPositions: [],
          updateIds: { orders: 0, positions: 1 },
        },
        true,
      );

      const listener = vi.fn();
      store.subscribe(listener);
      store.update(
        {
          positions: [createPosition({ id: 'position-2' })],
          displayedPositions: [createPosition({ id: 'displayed-position-1' }) as any],
          updateIds: { orders: 0, positions: 1 },
        },
        true,
      );

      expect(store.positions[0].id).toBe('position-1');
      expect(store.displayedPositions[0].id).toBe('displayed-position-1');
      expect(listener).toHaveBeenCalledTimes(1);
    });
  });

  describe('reset', () => {
    it('should reset to default state', () => {
      store.update({ latency: 100 });
      store.reset();
      expect(store.latency).toBe(0);
    });

    it('should clear all maps', () => {
      store.update({ tickers: [createTicker()] });
      store.reset();
      expect(Object.keys(store.tickersById).length).toBe(0);
      expect(Object.keys(store.tickersBySymbol).length).toBe(0);
    });
  });

  describe('subscribe/notify', () => {
    it('should call subscriber on changes', () => {
      const listener = vi.fn();
      store.subscribe(listener);
      store.update({ latency: 100 });
      expect(listener).toHaveBeenCalled();
    });

    it('should return unsubscribe function', () => {
      const listener = vi.fn();
      const unsubscribe = store.subscribe(listener);
      unsubscribe();
      store.update({ latency: 100 });
      expect(listener).not.toHaveBeenCalled();
    });

    it('should pass fromWorker flag to listeners', () => {
      const listener = vi.fn();
      store.subscribe(listener);
      store.update({ latency: 100 }, true);
      expect(listener).toHaveBeenCalledWith(expect.anything(), true);
    });
  });

  describe('order operations', () => {
    describe('addOrder', () => {
      it('should add order to store', () => {
        const order = createOrder();
        store.addOrder(order);
        expect(store.orders.length).toBe(1);
        expect(store.orders[0].id).toBe('order-1');
      });

      it('should increment updateIds.orders', () => {
        const initialId = store.updateIds.orders;
        store.addOrder(createOrder());
        expect(store.updateIds.orders).toBe(initialId + 1);
      });
    });

    describe('removeOrder', () => {
      it('should remove order by id', () => {
        store.addOrder(createOrder());
        store.removeOrder({ id: 'order-1' });
        expect(store.orders.length).toBe(0);
      });

      it('should not fail if order not found', () => {
        const initialId = store.updateIds.orders;
        expect(() => store.removeOrder({ id: 'non-existent' })).not.toThrow();
        expect(store.updateIds.orders).toBe(initialId);
      });
    });

    describe('removeOrders', () => {
      it('should remove multiple orders', () => {
        store.addOrder(createOrder({ id: 'order-1' }));
        store.addOrder(createOrder({ id: 'order-2' }));
        store.addOrder(createOrder({ id: 'order-3' }));
        store.removeOrders([{ id: 'order-1' }, { id: 'order-3' }]);
        expect(store.orders.length).toBe(1);
        expect(store.orders[0].id).toBe('order-2');
      });
    });

    describe('updateOrder', () => {
      it('should update existing order', () => {
        store.addOrder(createOrder());
        store.updateOrder({ id: 'order-1' }, { price: 51000 });
        expect(store.orders[0].price).toBe(51000);
      });

      it('should not fail if order not found', () => {
        expect(() => store.updateOrder({ id: 'non-existent' }, { price: 51000 })).not.toThrow();
      });
    });

    describe('updateOrderStatus', () => {
      it('should update order status', () => {
        store.addOrder(createOrder());
        store.updateOrderStatus({ id: 'order-1' }, OrderStatus.Closed);
        expect(store.orders[0].status).toBe(OrderStatus.Closed);
      });
    });

    describe('updateOrdersStatus', () => {
      it('should update status of multiple orders', () => {
        store.addOrder(createOrder({ id: 'order-1' }));
        store.addOrder(createOrder({ id: 'order-2' }));
        store.updateOrdersStatus([{ id: 'order-1' }, { id: 'order-2' }], OrderStatus.Canceled);
        expect(store.orders[0].status).toBe(OrderStatus.Canceled);
        expect(store.orders[1].status).toBe(OrderStatus.Canceled);
      });
    });

    describe('addOrUpdateOrder', () => {
      it('should add new order', () => {
        store.addOrUpdateOrder(createOrder());
        expect(store.orders.length).toBe(1);
      });

      it('should update existing order', () => {
        store.addOrder(createOrder());
        store.addOrUpdateOrder(createOrder({ price: 51000 }));
        expect(store.orders.length).toBe(1);
        expect(store.orders[0].price).toBe(51000);
      });
    });

    describe('addOrUpdateOrders', () => {
      it('should add multiple orders', () => {
        store.addOrUpdateOrders([createOrder({ id: 'order-1' }), createOrder({ id: 'order-2' })]);
        expect(store.orders.length).toBe(2);
      });

      it('should update existing and add new', () => {
        store.addOrder(createOrder({ id: 'order-1', price: 50000 }));
        store.addOrUpdateOrders([createOrder({ id: 'order-1', price: 51000 }), createOrder({ id: 'order-2' })]);
        expect(store.orders.length).toBe(2);
        expect(store.orders.find((o) => o.id === 'order-1')?.price).toBe(51000);
      });
    });

    describe('setOrdersForSymbol', () => {
      it('should replace orders for specific symbol', () => {
        store.addOrder(createOrder({ id: 'btc-1', symbol: 'BTCUSDT' }));
        store.addOrder(createOrder({ id: 'eth-1', symbol: 'ETHUSDT' }));
        store.setOrdersForSymbol('BTCUSDT', [createOrder({ id: 'btc-new', symbol: 'BTCUSDT' })]);
        expect(store.orders.length).toBe(2);
        expect(store.orders.find((o) => o.symbol === 'BTCUSDT')?.id).toBe('btc-new');
        expect(store.orders.find((o) => o.symbol === 'ETHUSDT')?.id).toBe('eth-1');
      });
    });
  });

  // A REST snapshot that read the venue before a final event must not reopen the order.
  describe('markOrdersTerminal', () => {
    let store: DefaultStore;
    beforeEach(() => {
      vi.useFakeTimers({ toFake: ['Date'], now: 1_000_000 });
      store = new DefaultStore();
      store.update({ orders: [createOrder({ id: 'a' }), createOrder({ id: 'b' })] });
    });
    afterEach(() => vi.useRealTimers());

    it('marks the status, and keeps the order closed against a stale snapshot', () => {
      store.markOrdersTerminal([{ id: 'a' }], OrderStatus.Canceled);
      expect(store.orders.find((o) => o.id === 'a')?.status).toBe(OrderStatus.Canceled);

      store.update({ orders: [createOrder({ id: 'a' }), createOrder({ id: 'b' })] });
      expect(store.orders.map((o) => [o.id, o.status])).toEqual([
        ['a', OrderStatus.Canceled],
        ['b', OrderStatus.Open],
      ]);
    });

    it('holds against every upsert and replace path', () => {
      store.markOrdersTerminal([{ id: 'a' }], OrderStatus.Closed);
      const reopened = createOrder({ id: 'a' });

      store.addOrUpdateOrder(reopened);
      store.addOrUpdateOrders([reopened]);
      store.setOrdersForSymbol('BTCUSDT', [reopened]);
      expect(store.orders.find((o) => o.id === 'a')?.status).toBe(OrderStatus.Closed);

      store.addOrUpdateOrdersFull([reopened]);
      store.addOrder(reopened);
      expect(store.orders.filter((o) => o.id === 'a')).toEqual([
        expect.objectContaining({ status: OrderStatus.Closed }),
      ]);
    });

    it('drops a reopening row when the closed order has already left the store', () => {
      store.markOrdersTerminal([{ id: 'a' }], OrderStatus.Closed);
      store.removeOrders([{ id: 'a' }]);

      store.update({ orders: [createOrder({ id: 'a' })] });
      expect(store.orders).toEqual([]);
    });

    it('still accepts the terminal row itself, other orders, and the id again after a minute', () => {
      store.markOrdersTerminal([{ id: 'a' }], OrderStatus.Closed);
      store.addOrUpdateOrders([
        createOrder({ id: 'a', status: OrderStatus.Closed, filled: 1 }),
        createOrder({ id: 'c' }),
      ]);
      expect(store.orders.find((o) => o.id === 'a')?.filled).toBe(1);
      expect(store.orders.some((o) => o.id === 'c')).toBe(true);

      vi.setSystemTime(1_060_000);
      store.update({ orders: [createOrder({ id: 'a' })] });
      expect(store.orders.find((o) => o.id === 'a')?.status).toBe(OrderStatus.Open);
    });

    // A removal or a mapped status is not proof of finality (Hyperliquid, Lighter, Bybit "Triggered").
    it('infers nothing from a removal or a plain status update', () => {
      store.removeOrders([{ id: 'a' }]);
      store.updateOrdersStatus([{ id: 'b' }], OrderStatus.Closed);
      store.update({ orders: [createOrder({ id: 'a' }), createOrder({ id: 'b' })] });
      expect(store.orders.every((o) => o.status === OrderStatus.Open)).toBe(true);
    });

    it('forgets everything on reset', () => {
      store.markOrdersTerminal([{ id: 'a' }], OrderStatus.Closed);
      store.reset();
      store.update({ orders: [createOrder({ id: 'a' })] });
      expect(store.orders.map((o) => o.status)).toEqual([OrderStatus.Open]);
    });
  });

  describe('position operations', () => {
    describe('addOrUpdatePosition', () => {
      it('should add new position', () => {
        store.addOrUpdatePosition(createPosition());
        expect(store.positions.length).toBe(1);
      });

      it('should update existing position', () => {
        store.addOrUpdatePosition(createPosition());
        store.addOrUpdatePosition(createPosition({ contracts: 2 }));
        expect(store.positions.length).toBe(1);
        expect(store.positions[0].contracts).toBe(2);
      });

      it('should increment updateIds.positions', () => {
        const initialId = store.updateIds.positions;
        store.addOrUpdatePosition(createPosition());
        expect(store.updateIds.positions).toBe(initialId + 1);
      });
    });

    describe('updatePosition', () => {
      it('should update existing position', () => {
        store.addOrUpdatePosition(createPosition());
        store.updatePosition({ id: 'position-1' }, { unrealizedPnl: 1000 });
        expect(store.positions[0].unrealizedPnl).toBe(1000);
      });

      it('does not advance the update id when the position is missing', () => {
        const initialId = store.updateIds.positions;
        store.updatePosition({ id: 'non-existent' }, { unrealizedPnl: 1000 });
        expect(store.updateIds.positions).toBe(initialId);
      });
    });

    describe('updatePositions', () => {
      it('should update multiple positions', () => {
        store.addOrUpdatePosition(createPosition({ id: 'pos-1' }));
        store.addOrUpdatePosition(createPosition({ id: 'pos-2' }));
        store.updatePositions([
          [{ id: 'pos-1' }, { unrealizedPnl: 100 }],
          [{ id: 'pos-2' }, { unrealizedPnl: 200 }],
        ]);
        expect(store.positions.find((p) => p.id === 'pos-1')?.unrealizedPnl).toBe(100);
        expect(store.positions.find((p) => p.id === 'pos-2')?.unrealizedPnl).toBe(200);
      });
    });

    describe('addOrUpdatePositions', () => {
      it('should add multiple positions', () => {
        store.addOrUpdatePositions([createPosition({ id: 'pos-1' }), createPosition({ id: 'pos-2' })]);
        expect(store.positions.length).toBe(2);
      });
    });

    describe('createEmptyPositionFrom', () => {
      it('should create empty position with zero values', () => {
        const position = createPosition();
        const emptyPosition = store.createEmptyPositionFrom(position);
        expect(emptyPosition.contracts).toBe(0);
        expect(emptyPosition.entryPrice).toBe(0);
        expect(emptyPosition.notional).toBe(0);
        expect(emptyPosition.unrealizedPnl).toBe(0);
        expect(emptyPosition.liquidationPrice).toBe(0);
      });

      it('should preserve position id and symbol', () => {
        const position = createPosition();
        const emptyPosition = store.createEmptyPositionFrom(position);
        expect(emptyPosition.id).toBe(position.id);
        expect(emptyPosition.symbol).toBe(position.symbol);
      });
    });

    describe('addOrUpdatePositionsFull', () => {
      it('should update provided positions and empty others', () => {
        store.addOrUpdatePosition(createPosition({ id: 'pos-1' }));
        store.addOrUpdatePosition(createPosition({ id: 'pos-2', contracts: 2 }));
        store.addOrUpdatePositionsFull([createPosition({ id: 'pos-1', contracts: 5 })]);

        const pos1 = store.positions.find((p) => p.id === 'pos-1');
        const pos2 = store.positions.find((p) => p.id === 'pos-2');
        expect(pos1?.contracts).toBe(5);
        expect(pos2?.contracts).toBe(0); // emptied because not in the update
      });
    });
  });

  describe('ticker operations', () => {
    it('strips info from full ticker snapshots', () => {
      const tickerWithInfo = createTicker({ info: { source: 'raw' } });
      store.update({ tickers: [tickerWithInfo] });
      expect((store.tickers[0] as any).info).toBeUndefined();
    });

    it('strips info from incremental ticker updates', () => {
      store.update({ tickers: [createTicker()] });
      store.updateTicker({ id: 'BTCUSDT' }, { info: { source: 'raw' } as any, last: 51500 });

      expect((store.tickers[0] as any).info).toBeUndefined();
      expect(store.tickers[0].last).toBe(51500);
    });

    it('ignores info-only ticker deltas', () => {
      store.update({ tickers: [createTicker()] });
      const listener = vi.fn();
      store.subscribe(listener);

      store.applyTickerDeltas([[0, { info: { raw: true } as any }]], true);

      expect(listener).not.toHaveBeenCalled();
      expect((store.tickers[0] as any).info).toBeUndefined();
    });

    it('packs numeric ticker deltas and ignores unsupported fields', () => {
      const deltas: Array<[number, Partial<Ticker>]> = [
        [0, { bid: 100, last: 101, info: { raw: true } as any }],
        [1, { info: { raw: true } as any }],
      ];

      const { packed } = DefaultStore.packTickerDeltas(deltas);
      expect(packed).toEqual([[0, 5, 100, 101]]);
    });

    it('tracks ticker snapshot version only on full ticker replacement', () => {
      expect(store.getTickersVersion()).toBe(0);

      store.update({ tickers: [createTicker()] });
      expect(store.getTickersVersion()).toBe(1);

      store.updateTicker({ id: 'BTCUSDT' }, { last: 51000 });
      expect(store.getTickersVersion()).toBe(1);
    });

    it('patches same-layout worker ticker snapshots in place', () => {
      store.update({ tickers: [createTicker({ last: 50000 })] });
      const tickersRef = store.tickers;
      const tickerRef = store.tickers[0];
      const version = store.getTickersVersion();
      const listener = vi.fn();
      store.subscribe(listener);

      store.update({ tickers: [createTicker({ last: 51000, info: { source: 'raw' } })] }, true);

      expect(store.tickers).toBe(tickersRef);
      expect(store.tickers[0]).toBe(tickerRef);
      expect(store.tickers[0].last).toBe(51000);
      expect((store.tickers[0] as any).info).toBeUndefined();
      expect(store.tickersById['BTCUSDT']).toBe(tickerRef);
      expect(store.tickersBySymbol['BTCUSDT']).toBe(tickerRef);
      expect(store.getTickersVersion()).toBe(version);
      expect(listener).toHaveBeenCalledTimes(1);
      expect(listener).toHaveBeenCalledWith(expect.anything(), true);
    });

    it('does not notify for unchanged same-layout worker ticker snapshots', () => {
      store.update({ tickers: [createTicker({ last: 50000 })] });
      const tickersRef = store.tickers;
      const tickerRef = store.tickers[0];
      const version = store.getTickersVersion();
      const listener = vi.fn();
      store.subscribe(listener);

      store.update({ tickers: [createTicker({ last: 50000, info: { source: 'raw' } })] }, true);

      expect(store.tickers).toBe(tickersRef);
      expect(store.tickers[0]).toBe(tickerRef);
      expect((store.tickers[0] as any).info).toBeUndefined();
      expect(store.getTickersVersion()).toBe(version);
      expect(listener).not.toHaveBeenCalled();
    });

    it('replaces worker ticker snapshots when ticker layout changes', () => {
      store.update({ tickers: [createTicker({ id: 'BTCUSDT', symbol: 'BTCUSDT' })] });
      const tickersRef = store.tickers;

      store.update({ tickers: [createTicker({ id: 'ETHUSDT', symbol: 'ETHUSDT' })] }, true);

      expect(store.tickers).not.toBe(tickersRef);
      expect(store.tickersById['ETHUSDT']).toBeDefined();
      expect(store.tickersBySymbol['ETHUSDT']).toBeDefined();
      expect(store.getTickersVersion()).toBe(2);
    });

    it('does not collect deltas via updateTicker (delta queue is for inbound deltas only)', () => {
      // Delta patching is downstream of the store now (handled by the
      // virtual-worker / relay shim that sits between worker and main).
      // `updateTicker` no longer populates `pendingTickerDeltas`; the queue
      // is kept around for `applyTickerDeltas` test scaffolding only.
      store.update({ tickers: [createTicker()] });

      store.updateTicker({ id: 'BTCUSDT' }, { last: 51000 });
      store.updateTicker({ id: 'BTCUSDT' }, { bid: 50990 });

      expect(store.consumeTickerDeltas()).toEqual([]);
    });

    it('applies ticker deltas and notifies once', () => {
      store.update({ tickers: [createTicker()] });
      const listener = vi.fn();
      store.subscribe(listener);

      store.applyTickerDeltas([[0, { last: 52000, ask: 52010 }]], true);

      expect(store.tickers[0].last).toBe(52000);
      expect(store.tickers[0].ask).toBe(52010);
      expect(listener).toHaveBeenCalledTimes(1);
      expect(listener).toHaveBeenCalledWith(expect.anything(), true);
    });

    it('applies packed ticker deltas and notifies once', () => {
      store.update({ tickers: [createTicker()] });
      const listener = vi.fn();
      store.subscribe(listener);

      store.applyPackedTickerDeltas([[0, 5, 53000, 53100]], true); // bid + last

      expect(store.tickers[0].bid).toBe(53000);
      expect(store.tickers[0].last).toBe(53100);
      expect(listener).toHaveBeenCalledTimes(1);
      expect(listener).toHaveBeenCalledWith(expect.anything(), true);
    });

    it('applies packed ticker deltas inside worker store updates without replacing tickers', () => {
      store.update({ tickers: [createTicker({ last: 50000 })] });
      const tickersRef = store.tickers;
      const tickerRef = store.tickers[0];
      const version = store.getTickersVersion();
      const listener = vi.fn();
      store.subscribe(listener);

      store.update(
        {
          tickers: [createTicker({ last: 99999 })],
          tickerDeltas: [[0, 4, 52000]],
        },
        true,
      );

      expect(store.tickers).toBe(tickersRef);
      expect(store.tickers[0]).toBe(tickerRef);
      expect(store.tickers[0].last).toBe(52000);
      expect(store.getTickersVersion()).toBe(version);
      expect(listener).toHaveBeenCalledTimes(1);
      expect(listener).toHaveBeenCalledWith(expect.anything(), true);
    });

    describe('updateTicker', () => {
      it('should update ticker by id', () => {
        store.update({ tickers: [createTicker()] });
        store.updateTicker({ id: 'BTCUSDT' }, { last: 51000 });
        expect(store.tickers[0].last).toBe(51000);
      });

      it('should update ticker maps', () => {
        store.update({ tickers: [createTicker()] });
        store.updateTicker({ id: 'BTCUSDT' }, { last: 51000 });
        expect(store.tickersById['BTCUSDT'].last).toBe(51000);
        expect(store.tickersBySymbol['BTCUSDT'].last).toBe(51000);
      });

      it('should not fail if ticker not found', () => {
        expect(() => store.updateTicker({ id: 'NON_EXISTENT' }, { last: 51000 })).not.toThrow();
      });
    });
  });

  describe('market operations', () => {
    it('tracks markets version when markets change', () => {
      expect(store.getMarketsVersion()).toBe(0);

      store.update({ markets: [createMarket()] });
      expect(store.getMarketsVersion()).toBe(1);

      store.updateMarket({ id: 'BTCUSDT' }, { active: false });
      expect(store.getMarketsVersion()).toBe(2);

      store.reset();
      expect(store.getMarketsVersion()).toBe(0);
    });

    it('patches same-layout worker market snapshots in place', () => {
      store.update({ markets: [createMarket({ active: true })] });
      const marketsRef = store.markets;
      const marketRef = store.markets[0];
      const version = store.getMarketsVersion();
      const listener = vi.fn();
      store.subscribe(listener);

      store.update({ markets: [createMarket({ active: false })] }, true);

      expect(store.markets).toBe(marketsRef);
      expect(store.markets[0]).toBe(marketRef);
      expect(store.markets[0].active).toBe(false);
      expect(store.marketsById['BTCUSDT']).toBe(marketRef);
      expect(store.marketsBySymbol['BTCUSDT']).toBe(marketRef);
      expect(store.getMarketsVersion()).toBe(version);
      expect(listener).toHaveBeenCalledTimes(1);
      expect(listener).toHaveBeenCalledWith(expect.anything(), true);
    });

    it('replaces worker market snapshots when market layout changes', () => {
      store.update({ markets: [createMarket({ id: 'BTCUSDT', symbol: 'BTCUSDT' })] });
      const marketsRef = store.markets;

      store.update({ markets: [createMarket({ id: 'ETHUSDT', symbol: 'ETHUSDT' })] }, true);

      expect(store.markets).not.toBe(marketsRef);
      expect(store.marketsById['ETHUSDT']).toBeDefined();
      expect(store.marketsBySymbol['ETHUSDT']).toBeDefined();
      expect(store.getMarketsVersion()).toBe(2);
    });

    describe('updateMarket', () => {
      it('should update market by id', () => {
        store.update({ markets: [createMarket()] });
        store.updateMarket({ id: 'BTCUSDT' }, { active: false });
        expect(store.markets[0].active).toBe(false);
      });

      it('should update market maps', () => {
        store.update({ markets: [createMarket()] });
        store.updateMarket({ id: 'BTCUSDT' }, { active: false });
        expect(store.marketsById['BTCUSDT'].active).toBe(false);
        expect(store.marketsBySymbol['BTCUSDT'].active).toBe(false);
      });
    });
  });

  describe('socket health', () => {
    describe('updateSocketHealth', () => {
      it('should update socket health flags', () => {
        store.updateSocketHealth({ publicHealthy: true, privateHealthy: true });
        expect(store.socketHealth?.publicHealthy).toBe(true);
        expect(store.socketHealth?.privateHealthy).toBe(true);
      });

      it('should merge with existing socket health', () => {
        store.updateSocketHealth({ publicHealthy: true });
        store.updateSocketHealth({ privateHealthy: true });
        expect(store.socketHealth?.publicHealthy).toBe(true);
        expect(store.socketHealth?.privateHealthy).toBe(true);
      });
    });
  });

  describe('setSetting', () => {
    it('should update option setting', () => {
      store.setSetting('isHedged', true);
      expect(store.options.isHedged).toBe(true);
    });

    it('should notify on setting change', () => {
      const listener = vi.fn();
      store.subscribe(listener);
      store.setSetting('isHedged', true);
      expect(listener).toHaveBeenCalled();
    });

    it('should not notify if value unchanged', () => {
      store.setSetting('isHedged', false);
      const listener = vi.fn();
      store.subscribe(listener);
      store.setSetting('isHedged', false);
      expect(listener).not.toHaveBeenCalled();
    });
  });

  describe('serialize', () => {
    it('should return serializable copy of state', () => {
      store.update({
        latency: 100,
        tickers: [createTicker()],
        markets: [createMarket()],
      });

      const serialized = store.serialize();
      expect(serialized.latency).toBe(100);
      expect(serialized.tickers.length).toBe(1);
      expect(serialized.markets.length).toBe(1);
    });

    it('should return copies not references', () => {
      store.update({
        tickers: [createTicker()],
        displayedMargin: { longExposure: 10, shortExposure: 20, totalNotional: 30 },
      });
      const serialized = store.serialize();
      expect(serialized.tickers).not.toBe(store.tickers);
      expect(serialized.displayedMargin).toEqual(store.displayedMargin);
      expect(serialized.displayedMargin).not.toBe(store.displayedMargin);
    });
  });
});

describe('serialize() shape parity with stubStore', () => {
  /**
   * `apps/web`'s IndexedDB warm-boot cache validates a cached snapshot with
   * `shapesCompatible(stubStore, storeData)`, which requires EVERY stub key to
   * be present in the serialized store. A field added to `stubStore` but not to
   * `serialize()` therefore invalidates every snapshot and silently cold-boots
   * every account — with no error anywhere, because a cache miss looks exactly
   * like a cold start.
   *
   * `requestStats` shipped that way once. This asserts the general invariant so
   * the next field cannot.
   */
  it('serializes every top-level key present in stubStore', () => {
    /**
     * Shallow on purpose. The AUTHORITATIVE check is
     * `apps/web/src/utils/storage/exchangeStoreCache.test.ts`, which runs the
     * real `validateCachedStoreShape` against a real `serialize()` — including
     * its dictionary carve-out (a stub object keyed only `'stub'` is a
     * `Record<string, T>` and its keys are not required).
     *
     * Reimplementing that rule here would drift from it silently, and safe-cex
     * cannot import from `apps/web` — it ships in the OSS mirror. So this file
     * asserts only the part that needs no knowledge of the consumer.
     */
    const serialized = new DefaultStore().serialize() as Record<string, unknown>;
    const missing = Object.keys(stubStore).filter((key) => !(key in serialized));

    expect(missing).toEqual([]);
  });
});

describe('recordRequestOutcome', () => {
  it('increments the named outcome and leaves the others alone', () => {
    const store = new DefaultStore();

    store.recordRequestOutcome('ok');
    store.recordRequestOutcome('ok');
    store.recordRequestOutcome('rateLimit');

    expect(store.requestStats).toEqual({ ok: 2, rateLimit: 1, auth: 0, network: 0, other: 0 });
  });

  /**
   * It runs on EVERY request. Notifying here would manufacture a store delta
   * per request — the main-thread churn that made per-request logging too
   * expensive to keep. The throttled store ship carries the field instead.
   */
  it('does NOT notify subscribers', () => {
    const store = new DefaultStore();
    const listener = vi.fn();
    store.subscribe(listener);
    listener.mockClear();

    store.recordRequestOutcome('other');

    expect(listener).not.toHaveBeenCalled();
  });
});

describe('defaultStore', () => {
  it('should have all required properties', () => {
    expect(defaultStore).toHaveProperty('latency');
    expect(defaultStore).toHaveProperty('balance');
    expect(defaultStore).toHaveProperty('markets');
    expect(defaultStore).toHaveProperty('tickers');
    expect(defaultStore).toHaveProperty('positions');
    expect(defaultStore).toHaveProperty('orders');
    expect(defaultStore).toHaveProperty('loaded');
    expect(defaultStore).toHaveProperty('options');
    expect(defaultStore).toHaveProperty('updateIds');
    expect(defaultStore).toHaveProperty('socketHealth');
  });
});

describe('wire midpoint continuity', () => {
  it('updates and clears midpoint on stable worker ticker replacements', () => {
    const store = new DefaultStore();
    store.update({ tickers: [createTicker({ midPrice: 100 })] });
    const retained = store.tickers[0];
    store.update({ tickers: [createTicker({ midPrice: 101 })] }, true);
    expect(store.tickers[0]).toBe(retained);
    expect(retained.midPrice).toBe(101);
    store.update({ tickers: [createTicker()] }, true);
    expect('midPrice' in retained).toBe(false);
  });
});
