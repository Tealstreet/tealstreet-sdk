import type { ExchangeOptions } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { BybitDemoExchange } from '../reference/bybit/bybitdemo.exchange';
import { BybitInverseExchange } from '../reference/bybit/bybitinverse.exchange';
import { BybitSpotExchange } from '../reference/bybit/bybitspot.exchange';
import { DefaultStore } from '../../store/store.base';
import { Order, OrderSide, OrderStatus, OrderType } from '../../types';
import { virtualClock } from '../../utils/virtualClock';

// Regression: `/v5/order/realtime` lags order creation (worst on api-demo), and
// several paths wholesale-replace `store.orders` from that endpoint —
// private-WS reconnect housekeeping, a startup reinit, the SET_TRADING_STOP
// backstop. A snapshot fetched before the venue indexed a just-placed order
// erased it from the store; the chaser supervisor saw its live order
// "disappear", stopped itself, and cancelled that order on the exchange: the
// "chaser does nothing on Bybit demo" bug.
//
// `preserveRecentlyPlacedOrders` shields an order ONLY between placement and
// its first sighting (REST snapshot / WS frame / our own cancel), bounded by a
// short grace window. A sighted order's later absence (= filled or cancelled)
// is always honored, so the chaser can never re-place on top of a fill.
//
// This file was deleted by SDK-mirror-sync commit 3fd8301610 along with the
// code it guards, which is how the fix silently regressed once already.

const makeOrder = (id: string, overrides: Partial<Order> = {}): Order =>
  ({
    id,
    status: OrderStatus.Open,
    symbol: 'BTCUSDT',
    type: OrderType.Limit,
    side: OrderSide.Buy,
    price: 100,
    amount: 1,
    filled: 0,
    remaining: 1,
    reduceOnly: false,
    lastUpdate: virtualClock.getCurrentTimeMs(),
    ...overrides,
  }) as Order;

const makeExchange = (demo = false, optionOverrides: Partial<ExchangeOptions> = {}) => {
  const store = new DefaultStore();
  const options = { key: 'k', secret: 's', ...optionOverrides } as ExchangeOptions;
  const exchange = demo
    ? new BybitDemoExchange('bybitdemo' as any, options, store)
    : new BybitExchange('bybit' as any, options, store);
  return { exchange: exchange as any, store };
};

const makeVariantExchange = (
  variant: 'bybitinverse' | 'bybitspot',
  optionOverrides: Partial<ExchangeOptions> = {},
) => {
  const store = new DefaultStore();
  const options = { key: 'k', secret: 's', ...optionOverrides } as ExchangeOptions;
  const exchange =
    variant === 'bybitinverse'
      ? new BybitInverseExchange('bybitinverse' as any, options, store)
      : new BybitSpotExchange('bybitspot' as any, options, store);
  return { exchange: exchange as any, store };
};

const variantCases = [
  { variant: 'bybitinverse' as const, label: 'Bybit Inverse' },
  { variant: 'bybitspot' as const, label: 'Bybit Spot' },
];

const seedRecentlyPlaced = (exchange: any, order: Order, ageMs = 0) => {
  exchange.recentlyPlacedOrders.set(order.id, {
    placedAt: virtualClock.getCurrentTimeMs() - ageMs,
    order,
  });
};

describe('Bybit recently-placed order snapshot preservation', () => {
  it('re-adds a just-placed order that the REST snapshot omitted', () => {
    const { exchange } = makeExchange();
    const placed = makeOrder('new-1');
    seedRecentlyPlaced(exchange, placed);

    const result = exchange.preserveRecentlyPlacedOrders([makeOrder('other')]);

    expect(result.map((o: Order) => o.id)).toEqual(['other', 'new-1']);
    // Still unsighted: the next omitting snapshot must preserve it again.
    expect(exchange.recentlyPlacedOrders.has('new-1')).toBe(true);
  });

  it('prefers the live store row over the constructed placement row', () => {
    const { exchange, store } = makeExchange();
    seedRecentlyPlaced(exchange, makeOrder('new-1', { price: 100 }));
    const storeRow = makeOrder('new-1', { price: 101, filled: 0.5 });
    store.addOrUpdateOrders([storeRow]);

    const result = exchange.preserveRecentlyPlacedOrders([]);

    expect(result).toHaveLength(1);
    expect(result[0].price).toBe(101);
    expect(result[0].filled).toBe(0.5);
  });

  it('stops protecting an order once a snapshot has sighted it; later absence is honored', () => {
    const { exchange } = makeExchange();
    seedRecentlyPlaced(exchange, makeOrder('new-1'));

    const sighting = exchange.preserveRecentlyPlacedOrders([makeOrder('new-1')]);
    expect(sighting.map((o: Order) => o.id)).toEqual(['new-1']);
    expect(exchange.recentlyPlacedOrders.has('new-1')).toBe(false);

    // Order filled/cancelled since: the empty snapshot must stay empty, or the
    // chaser would treat a filled order as resting and place on top of it.
    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
  });

  it('drops protection after the grace window expires', () => {
    const { exchange } = makeExchange();
    seedRecentlyPlaced(exchange, makeOrder('stale'), 10_000);

    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
    expect(exchange.recentlyPlacedOrders.has('stale')).toBe(false);
  });

  it('markOrderSeen (WS frame / explicit cancel) removes protection immediately', () => {
    const { exchange } = makeExchange();
    seedRecentlyPlaced(exchange, makeOrder('new-1'));

    exchange.markOrderSeen('new-1');

    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
  });

  it('does not locally inject normal live Bybit orders while private WS owns the row', () => {
    const { exchange, store } = makeExchange();

    exchange.recordRecentlyPlacedOrder(makeOrder('live-1'));

    expect(store.orders).toEqual([]);
    expect(exchange.preserveRecentlyPlacedOrders([]).map((order: Order) => order.id)).toEqual(['live-1']);
  });

  it('locally injects Bybit Brazil resting orders while direct browser WS may be degraded', () => {
    const { exchange, store } = makeExchange(false, { bybitSiteId: 'BRA_BTL' });

    exchange.recordRecentlyPlacedOrder(makeOrder('brazil-1'));

    expect(store.orders.map((order) => order.id)).toEqual(['brazil-1']);
  });

  it('records Bybit Brazil batch-created resting orders for local display', async () => {
    const { exchange, store } = makeExchange(false, { bybitSiteId: 'BRA_BTL' });
    exchange.formatOrderForBatch = async (order: Order) => ({
      symbol: order.symbol,
      qty: '2',
      price: '100',
    });
    exchange.xhr = {
      post: async () => ({
        data: {
          retCode: 0,
          retMsg: 'OK',
          result: { list: [{ orderId: 'batch-1' }] },
          retExtInfo: { list: [] },
        },
      }),
    };

    await expect(
      exchange.processBatch('linear', [
        {
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 2,
          price: 100,
        },
      ]),
    ).resolves.toEqual(['batch-1']);

    expect(store.orders).toMatchObject([{ id: 'batch-1', amount: 2, price: 100 }]);
  });

  it('cancelOrders counts as a sighting so a cancelled order cannot resurrect', async () => {
    const { exchange } = makeExchange();
    const placed = makeOrder('new-1');
    seedRecentlyPlaced(exchange, placed);
    // Stub the transport: batch cancel resolves with an OK envelope.
    exchange.unlimitedXHR = {
      post: async () => ({ data: { retCode: 0, retMsg: 'OK', result: {}, retExtInfo: { list: [] } } }),
    };

    await exchange.cancelOrders([placed]);

    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
  });

  it('locally retires Bybit Brazil rows after a successful cancel', async () => {
    const { exchange, store } = makeExchange(false, { bybitSiteId: 'BRA_BTL' });
    const placed = makeOrder('brazil-1');
    store.addOrUpdateOrders([placed]);
    exchange.unlimitedXHR = {
      post: async () => ({ data: { retCode: 0, retMsg: 'OK', result: {}, retExtInfo: { list: [] } } }),
    };

    await exchange.cancelOrders([placed]);

    expect(store.orders).toEqual([]);
  });

  it('locally updates Bybit Brazil rows after a successful REST amend', async () => {
    const { exchange, store } = makeExchange(false, { bybitSiteId: 'BRA_BTL' });
    const placed = makeOrder('brazil-1', { price: 100 });
    store.addOrUpdateOrders([placed]);
    exchange.xhr = {
      post: async () => ({ data: { retCode: 0, retMsg: 'OK', result: { orderId: 'brazil-1' } } }),
    };

    await expect(exchange.updateOrder({ order: placed, update: { price: 101 } })).resolves.toEqual(['brazil-1']);

    expect(store.orders).toMatchObject([{ id: 'brazil-1', price: 101 }]);
  });

  it('keeps the demo recently-placed shield row current after a successful REST amend', async () => {
    const { exchange, store } = makeExchange(true);
    const placed = makeOrder('demo-1', { price: 100, amount: 1, remaining: 1 });
    exchange.recordRecentlyPlacedOrder(placed);
    exchange.xhr = {
      post: async () => ({ data: { retCode: 0, retMsg: 'OK', result: { orderId: 'demo-1' } } }),
    };

    await expect(exchange.updateOrder({ order: placed, update: { price: 101, amount: 2 } })).resolves.toEqual([
      'demo-1',
    ]);

    expect(store.orders).toMatchObject([{ id: 'demo-1', price: 101, amount: 2, remaining: 2 }]);
    expect(exchange.recentlyPlacedOrders.get('demo-1')?.order).toMatchObject({
      id: 'demo-1',
      price: 101,
      amount: 2,
      remaining: 2,
    });
  });

  // The chaser cancels at the touch, so an order can fill during the cancel
  // round trip. Throwing here made the chaser stop and abandon the chase.
  it('treats already-gone cancel rows as success', async () => {
    const { exchange } = makeExchange();
    const placed = makeOrder('new-1');
    seedRecentlyPlaced(exchange, placed);
    exchange.unlimitedXHR = {
      post: async () => ({
        data: {
          retCode: 0,
          retMsg: 'OK',
          result: {},
          retExtInfo: { list: [{ code: 110001, msg: 'order not exists or too late to cancel' }] },
        },
      }),
    };

    await expect(exchange.cancelOrders([placed])).resolves.toBeUndefined();

    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
  });

  it('still surfaces genuine cancel failures', async () => {
    const { exchange } = makeExchange();
    exchange.unlimitedXHR = {
      post: async () => ({
        data: {
          retCode: 0,
          retMsg: 'OK',
          result: {},
          retExtInfo: { list: [{ code: 10001, msg: 'insufficient permissions' }] },
        },
      }),
    };

    await expect(exchange.cancelOrders([makeOrder('new-1')])).rejects.toThrow('insufficient permissions');
  });

  // The chaser's only fill signal when the Bybit Demo private WS never
  // delivers order frames. Without it the manager cannot distinguish a filled
  // order from a vanished one, so a degraded-WS chase never completes.
  it('fetches fills for chaser order reconciliation by order id', async () => {
    const { exchange, store } = makeExchange();
    store.addOrUpdateOrders([makeOrder('filled-1')]);
    exchange.xhr = {
      get: async () => ({
        data: {
          retCode: 0,
          retMsg: 'OK',
          result: {
            list: [
              { orderId: 'filled-1', execQty: '0.4' },
              { orderId: 'filled-1', execQty: '0.6' },
              { orderId: 'other', execQty: '5' },
            ],
          },
        },
      }),
    };

    await expect(exchange.fetchFillsForOrderIds(['filled-1'])).resolves.toEqual(new Map([['filled-1', 1]]));
  });

  // The chaser cancels, reconciles fills, then replaces. Demo retirement removes
  // the row in step one, so a store-only symbol lookup found nothing in step two
  // and every id was silently skipped — the empty map read as "no fills" and the
  // replacement overfilled the target.
  it('still reconciles fills for an order whose row was just retired', async () => {
    const { exchange, store } = makeExchange(true);
    store.addOrUpdateOrders([makeOrder('chased-1')]);
    exchange.unlimitedXHR = {
      post: async () => ({ data: { retCode: 0, retMsg: 'OK', result: {}, retExtInfo: { list: [] } } }),
    };
    exchange.xhr = {
      get: async () => ({
        data: { retCode: 0, retMsg: 'OK', result: { list: [{ orderId: 'chased-1', execQty: '0.25' }] } },
      }),
    };

    await exchange.cancelOrders([makeOrder('chased-1')]);
    expect(store.orders).toEqual([]);

    await expect(exchange.fetchFillsForOrderIds(['chased-1'])).resolves.toEqual(new Map([['chased-1', 0.25]]));
  });

  it('throws rather than under-reporting when a symbol cannot be resolved', async () => {
    const { exchange } = makeExchange(true);

    await expect(exchange.fetchFillsForOrderIds(['never-seen'])).rejects.toThrow(/cannot resolve a symbol/i);
  });

  it('pages execution history to exhaustion before reporting fills', async () => {
    const { exchange, store } = makeExchange(true);
    store.addOrUpdateOrders([makeOrder('big-1')]);
    const pages = [
      { list: [{ orderId: 'big-1', execQty: '0.5' }], nextPageCursor: 'page-2' },
      { list: [{ orderId: 'big-1', execQty: '0.25' }], nextPageCursor: '' },
    ];
    let call = 0;
    exchange.xhr = {
      get: async () => ({ data: { retCode: 0, retMsg: 'OK', result: pages[call++] } }),
    };

    await expect(exchange.fetchFillsForOrderIds(['big-1'])).resolves.toEqual(new Map([['big-1', 0.75]]));
    expect(call).toBe(2);
  });

  it('fails closed when execution history will not stop paging', async () => {
    const { exchange, store } = makeExchange(true);
    store.addOrUpdateOrders([makeOrder('loop-1')]);
    let cursor = 0;
    exchange.xhr = {
      get: async () => ({
        data: { retCode: 0, retMsg: 'OK', result: { list: [], nextPageCursor: `cursor-${cursor++}` } },
      }),
    };

    await expect(exchange.fetchFillsForOrderIds(['loop-1'])).rejects.toThrow(/exceeded 20 pages/i);
  });

  it('fails closed when Bybit fill reconciliation errors', async () => {
    const { exchange, store } = makeExchange();
    store.addOrUpdateOrders([makeOrder('filled-1')]);
    exchange.xhr = {
      get: async () => ({ data: { retCode: 10001, retMsg: 'bad request', result: { list: [] } } }),
    };

    await expect(exchange.fetchFillsForOrderIds(['filled-1'])).rejects.toThrow('bad request');
  });

  // Demo drops private `order` frames and the periodic tick refetches positions
  // and balance but NOT orders, so `handleOrderTopic` — the only path that
  // removes an order row — never runs. A cancelled row injected at placement
  // then sticks forever: a phantom resting order whose cancel button appears
  // dead, because the UI's optimistic removal is undone by the next broadcast.
  // Each chaser reprice cancels and replaces, so the debris accumulates.
  it('retires cancelled rows from the demo store, since no WS frame will', async () => {
    const { exchange, store } = makeExchange(true);
    store.addOrUpdateOrders([makeOrder('stale-1'), makeOrder('keep-1')]);
    exchange.unlimitedXHR = {
      post: async () => ({ data: { retCode: 0, retMsg: 'OK', result: {}, retExtInfo: { list: [] } } }),
    };

    await exchange.cancelOrders([makeOrder('stale-1')]);

    expect(store.orders.map((o: Order) => o.id)).toEqual(['keep-1']);
  });

  it('retires the row when Bybit reports the order was already gone', async () => {
    const { exchange, store } = makeExchange(true);
    store.addOrUpdateOrders([makeOrder('stale-1')]);
    exchange.unlimitedXHR = {
      post: async () => ({
        data: {
          retCode: 0,
          retMsg: 'OK',
          result: {},
          retExtInfo: { list: [{ code: 110001, msg: 'order not exists or too late to cancel' }] },
        },
      }),
    };

    await exchange.cancelOrders([makeOrder('stale-1')]);

    expect(store.orders).toEqual([]);
  });

  it('leaves the row alone when a cancel genuinely fails', async () => {
    const { exchange, store } = makeExchange(true);
    store.addOrUpdateOrders([makeOrder('live-1')]);
    exchange.unlimitedXHR = {
      post: async () => ({
        data: {
          retCode: 0,
          retMsg: 'OK',
          result: {},
          retExtInfo: { list: [{ code: 10001, msg: 'insufficient permissions' }] },
        },
      }),
    };

    await expect(exchange.cancelOrders([makeOrder('live-1')])).rejects.toThrow('insufficient permissions');

    expect(store.orders.map((o: Order) => o.id)).toEqual(['live-1']);
  });

  // Live Bybit has working `order` frames; removing locally there would race
  // the WS-driven reconciliation that owns the row.
  it('does not retire rows locally on live bybit', async () => {
    const { exchange, store } = makeExchange(false);
    store.addOrUpdateOrders([makeOrder('live-1')]);
    exchange.unlimitedXHR = {
      post: async () => ({ data: { retCode: 0, retMsg: 'OK', result: {}, retExtInfo: { list: [] } } }),
    };

    await exchange.cancelOrders([makeOrder('live-1')]);

    expect(store.orders.map((o: Order) => o.id)).toEqual(['live-1']);
  });

  it('cancelSymbolOrders retires that symbol only, and marks the ids seen', async () => {
    const { exchange, store } = makeExchange(true);
    store.addOrUpdateOrders([makeOrder('eth-1', { symbol: 'ETHUSDT' }), makeOrder('btc-1')]);
    seedRecentlyPlaced(exchange, makeOrder('eth-1', { symbol: 'ETHUSDT' }));
    exchange.unlimitedXHR = {
      post: async () => ({ data: { retCode: 0, retMsg: 'OK', result: {} } }),
    };

    await exchange.cancelSymbolOrders('ETHUSDT');

    expect(store.orders.map((o: Order) => o.id)).toEqual(['btc-1']);
    // The shield must not resurrect it on the next snapshot.
    expect(exchange.preserveRecentlyPlacedOrders([]).map((o: Order) => o.id)).toEqual([]);
  });

  it('recordRecentlyPlacedOrder adds the row to the store on demo only', () => {
    const demo = makeExchange(true);
    demo.exchange.recordRecentlyPlacedOrder(makeOrder('demo-1'));
    expect(demo.store.orders.map((order) => order.id)).toEqual(['demo-1']);

    const live = makeExchange(false);
    live.exchange.recordRecentlyPlacedOrder(makeOrder('live-1'));
    expect(live.store.orders).toEqual([]);
    // Live still gets grace-window preservation against degraded-mode wipes.
    expect(live.exchange.preserveRecentlyPlacedOrders([]).map((o: Order) => o.id)).toEqual(['live-1']);
  });

  it('does not inject or preserve plain market orders on demo', () => {
    const { exchange, store } = makeExchange(true);

    exchange.recordRecentlyPlacedOrder(makeOrder('market-1', { type: OrderType.Market }));

    expect(store.orders).toEqual([]);
    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
  });

  it('retires filled demo reduce-only limit closes instead of leaving the local placeholder open', async () => {
    vi.useFakeTimers();
    try {
      const { exchange, store } = makeExchange(true);
      const placed = makeOrder('close-1', { side: OrderSide.Sell, reduceOnly: true });
      exchange.xhr = {
        get: async (_endpoint: string, config: any) => ({
          data: {
            retCode: 0,
            retMsg: 'OK',
            result: {
              list: [{ orderId: config.params.orderId, execQty: '1' }],
              nextPageCursor: '',
            },
          },
        }),
      };

      exchange.recordRecentlyPlacedOrder(placed);
      expect(store.orders.map((order: Order) => order.id)).toEqual(['close-1']);

      await vi.advanceTimersByTimeAsync(1_000);

      expect(store.orders).toEqual([]);
      expect(exchange.recentlyPlacedOrders.has('close-1')).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it('still injects conditional market triggers because they are resting orders', () => {
    const { exchange, store } = makeExchange(true);

    exchange.recordRecentlyPlacedOrder(makeOrder('conditional-1', { type: OrderType.Conditional }));

    expect(store.orders.map((order: Order) => order.id)).toEqual(['conditional-1']);
    expect(exchange.preserveRecentlyPlacedOrders([]).map((order: Order) => order.id)).toEqual(['conditional-1']);
  });

  variantCases.forEach(({ variant, label }) => {
    describe(`${label} Brazil recently-placed lifecycle`, () => {
      it('preserves an unsighted recently placed order across omitted REST snapshots', () => {
        const { exchange } = makeVariantExchange(variant);
        seedRecentlyPlaced(exchange, makeOrder('new-1'));

        expect(exchange.preserveRecentlyPlacedOrders([]).map((order: Order) => order.id)).toEqual(['new-1']);

        exchange.markOrderSeen('new-1');
        expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
      });

      it('locally injects Brazil resting orders but leaves normal live accounts WS-owned', () => {
        const brazil = makeVariantExchange(variant, { bybitSiteId: 'BRA_BTL' });
        brazil.exchange.recordRecentlyPlacedOrder(makeOrder('brazil-1'));
        expect(brazil.store.orders.map((order: Order) => order.id)).toEqual(['brazil-1']);

        const live = makeVariantExchange(variant);
        live.exchange.recordRecentlyPlacedOrder(makeOrder('live-1'));
        expect(live.store.orders).toEqual([]);
        expect(live.exchange.preserveRecentlyPlacedOrders([]).map((order: Order) => order.id)).toEqual(['live-1']);
      });

      it('locally updates Brazil rows after successful REST amend', async () => {
        const { exchange, store } = makeVariantExchange(variant, { bybitSiteId: 'BRA_BTL' });
        const placed = makeOrder('brazil-1', { price: 100 });
        store.addOrUpdateOrders([placed]);
        exchange.xhr = {
          post: async () => ({ data: { retCode: 0, retMsg: 'OK', result: { orderId: 'brazil-1' } } }),
        };

        await expect(exchange.updateOrder({ order: placed, update: { price: 101 } })).resolves.toEqual(['brazil-1']);

        expect(store.orders).toMatchObject([{ id: 'brazil-1', price: 101 }]);
      });

      it('locally retires Brazil rows after successful or already-gone REST cancel', async () => {
        const { exchange, store } = makeVariantExchange(variant, { bybitSiteId: 'BRA_BTL' });
        const placed = makeOrder('brazil-1');
        store.addOrUpdateOrders([placed]);
        seedRecentlyPlaced(exchange, placed);
        exchange.unlimitedXHR = {
          post: async () => ({
            data: {
              retCode: 0,
              retMsg: 'OK',
              result: {},
              retExtInfo: { list: [{ code: 110001, msg: 'order not exists or too late to cancel' }] },
            },
          }),
        };

        await expect(exchange.cancelOrders([placed])).resolves.toBeUndefined();

        expect(store.orders).toEqual([]);
        expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
      });
    });
  });
});
