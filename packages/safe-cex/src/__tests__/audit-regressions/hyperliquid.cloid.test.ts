import { describe, expect, it, vi } from 'vitest';

import { replacementClientOrderIdOf } from '../../exchanges/clientOrderId';
import { mintHyperliquidCloid, placementRecordFromWire } from '../../exchanges/hyperliquid/hyperliquid.cloid';
import { getCrossCache, PLACEMENT_RECORD_TTL_MS } from '../reference/hyperliquid/hyperliquid.cross-cache';
import { HyperliquidExchange } from '../reference/hyperliquid/hyperliquid.exchange';
import { HyperliquidSpotExchange } from '../reference/hyperliquid/hyperliquidspot.exchange';
import { DefaultStore } from '../../store/store.base';
import {
  MarginType,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../types';

// Every Hyperliquid order carries a client order ID (cloid, wire field `c`):
// `0x` + 32 hex chars, on any order including triggers and TP/SL groupings.
// `orderUpdates` frames and `frontendOpenOrders` rows echo it back (checked
// live 2026-10-11), which is what lets the WS handler identify our orders
// exactly instead of inferring type and trigger price.
const CLOID = /^0x[0-9a-f]{32}$/;
// Built at runtime: a 0x+64-hex literal trips gitleaks' private-key rule on the public mirror.
const SECRET = `0x${'11'.repeat(32)}`;
const WALLET = '0x0000000000000000000000000000000000000001';

const okResponse = (statuses: unknown[]) => ({
  data: { status: 'ok', response: { type: 'order', data: { statuses } } },
});

const createExchange = (options: Record<string, unknown> = {}) => {
  const store = new DefaultStore();
  const market = {
    id: 'BTC',
    symbol: 'BTC-PERP',
    base: 'BTC',
    quote: 'USDC',
    active: true,
    precision: { amount: 0.00001, price: 1 },
    limits: { amount: { min: 0.00001, max: 1000 } },
    contractSize: 1,
    isSpot: false,
    info: { hyperliquidId: 0 },
  } as any;
  store.update({
    markets: [market],
    tickers: [{ id: 'BTC', symbol: 'BTC-PERP', bid: 78490, ask: 78510, last: 78500, mark: 78500, index: 78502 } as any],
    positions: [
      {
        id: 'BTC-PERP:long',
        symbol: 'BTC-PERP',
        side: PositionSide.Long,
        contracts: 0.5,
        entryPrice: 78500,
        notional: 39250,
        leverage: 1,
        unrealizedPnl: 0,
        liquidationPrice: 0,
        marginType: MarginType.Cross,
        positionMode: PositionMode.OneWay,
      } as any,
    ],
  });
  const exchange = new HyperliquidExchange(
    'hyperliquid' as any,
    { key: WALLET, secret: SECRET, ...options } as any,
    store,
  );
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  return { exchange, store };
};

const wiresOf = (post: ReturnType<typeof vi.fn>, call = 0) => post.mock.calls[call][1].action.orders as any[];

describe('mintHyperliquidCloid', () => {
  it('mints 0x + 32 lowercase hex chars, uniquely', () => {
    const minted = new Set<string>();
    for (let i = 0; i < 10_000; i++) {
      const cloid = mintHyperliquidCloid();
      expect(cloid).toMatch(CLOID);
      minted.add(cloid);
    }
    expect(minted.size).toBe(10_000);
  });
});

describe('hyperliquid placement wires carry a cloid on every wire', () => {
  it('market order', async () => {
    const { exchange } = createExchange();
    const post = vi.fn().mockResolvedValue(okResponse([{ resting: { oid: 1 } }]));
    exchange.xhr = { post } as any;

    await exchange.placeOrder({ symbol: 'BTC-PERP', type: OrderType.Market, side: OrderSide.Buy, amount: 0.1 });

    const [wire] = wiresOf(post);
    expect(wire.c).toMatch(CLOID);
    // msgpack key order is part of the signed hash: `c` must follow `t`.
    expect(Object.keys(wire)).toEqual(['a', 'b', 'p', 's', 'r', 't', 'c']);
  });

  it('limit entry with attached SL and TP: three wires, three distinct cloids, caller ID on the entry', async () => {
    const { exchange } = createExchange();
    const post = vi
      .fn()
      .mockResolvedValue(okResponse([{ resting: { oid: 1 } }, 'waitingForTrigger', 'waitingForTrigger']));
    exchange.xhr = { post } as any;
    const callerId = '0x0123456789abcdef0123456789abcdef';

    const placed = await exchange.placeOrderWithClientOrderId({
      symbol: 'BTC-PERP',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 0.1,
      price: 78000,
      stopLoss: 77000,
      takeProfit: 80000,
      clientOrderId: callerId,
    });

    expect(placed).toEqual({ orderIds: ['1'], clientOrderId: callerId });
    expect(post.mock.calls[0][1].action.grouping).toBe('normalTpsl');
    const wires = wiresOf(post);
    expect(wires).toHaveLength(3);
    expect(wires[0].c).toBe(callerId);
    for (const wire of wires) expect(wire.c).toMatch(CLOID);
    expect(new Set(wires.map((w) => w.c)).size).toBe(3);
  });

  it('standalone partial stop (na) and positional stop (positionTpsl)', async () => {
    const { exchange } = createExchange();
    const post = vi
      .fn()
      .mockResolvedValueOnce(okResponse([{ resting: { oid: 2 } }]))
      .mockResolvedValueOnce(okResponse(['waitingForTrigger']));
    exchange.xhr = { post } as any;

    await exchange.placeOrder({
      symbol: 'BTC-PERP',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 0.2,
      price: 77000,
      reduceOnly: true,
    });
    await exchange.placeOrder({
      symbol: 'BTC-PERP',
      type: OrderType.TakeProfit,
      side: OrderSide.Sell,
      amount: 0,
      price: 80000,
      reduceOnly: true,
    });

    expect(post.mock.calls[0][1].action.grouping).toBe('na');
    expect(post.mock.calls[1][1].action.grouping).toBe('positionTpsl');
    expect(wiresOf(post, 0)[0].c).toMatch(CLOID);
    expect(wiresOf(post, 1)[0].c).toMatch(CLOID);
  });

  it('records every wire before the REST call goes out', async () => {
    const { exchange } = createExchange();
    const seenAtSend: unknown[] = [];
    const post = vi.fn().mockImplementation(async (_url: string, payload: any) => {
      for (const wire of payload.action.orders) seenAtSend.push(getCrossCache().getPlacementRecord(wire.c));
      return okResponse([{ resting: { oid: 3 } }, 'waitingForTrigger']);
    });
    exchange.xhr = { post } as any;

    await exchange.placeOrder({
      symbol: 'BTC-PERP',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 0.1,
      price: 78000,
      timeInForce: OrderTimeInForce.PostOnly,
      stopLoss: 77000,
    });

    expect(seenAtSend).toEqual([
      expect.objectContaining({
        type: OrderType.Limit,
        price: 78000,
        side: OrderSide.Buy,
        amount: 0.1,
        reduceOnly: false,
        timeInForce: OrderTimeInForce.PostOnly,
        symbol: 'BTC-PERP',
      }),
      expect.objectContaining({
        type: OrderType.StopLoss,
        price: 77000,
        side: OrderSide.Sell,
        amount: 0.1,
        reduceOnly: true,
        timeInForce: undefined,
      }),
    ]);
  });
});

describe('hyperliquid updateOrder (cancel+replace)', () => {
  it('returns the replacement cloid when the venue answers waitingForTrigger without an oid', async () => {
    const { exchange } = createExchange();
    const post = vi
      .fn()
      .mockResolvedValueOnce({ data: { status: 'ok', response: { type: 'cancel', data: { statuses: ['success'] } } } })
      .mockResolvedValueOnce(okResponse(['waitingForTrigger']));
    exchange.xhr = { post } as any;

    const result = await exchange.updateOrder({
      order: {
        id: '42',
        symbol: 'BTC-PERP',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        price: 77000,
        amount: 0,
        filled: 0,
        remaining: 0,
        reduceOnly: true,
        status: OrderStatus.Open,
        lastUpdate: 0,
        clientOrderId: '0xffffffffffffffffffffffffffffffff',
      },
      update: { price: 76000 },
    });

    const replacementWire = wiresOf(post, 1)[0];
    expect(result).toHaveLength(0);
    expect(result[0]).toBeUndefined();
    expect(replacementClientOrderIdOf(result)).toBe(replacementWire.c);
    // A fresh cloid, never the cancelled order's.
    expect(replacementWire.c).not.toBe('0xffffffffffffffffffffffffffffffff');
    expect(getCrossCache().getPlacementRecord(replacementWire.c)).toMatchObject({
      type: OrderType.StopLoss,
      price: 76000,
    });
  });
});

describe('hyperliquid REST orders carry the cloid', () => {
  it('fetchOrders sets clientOrderId from frontendOpenOrders rows, and leaves it off rows without one', async () => {
    const { exchange, store } = createExchange({ applicationId: WALLET });
    const row = {
      coin: 'BTC',
      side: 'A',
      limitPx: '73150',
      sz: '0.0',
      oid: 7,
      timestamp: 1,
      isTrigger: true,
      triggerPx: '77000',
      reduceOnly: true,
      orderType: 'Stop Market',
      origSz: '0.0',
      tif: null,
      cloid: '0x0123456789abcdef0123456789abcdef',
    };
    const post = vi.fn().mockResolvedValue({ data: [row, { ...row, oid: 8, cloid: null }] });
    exchange.xhr = { post } as any;

    await exchange.fetchOrders();

    expect(store.orders.find((o) => o.id === '7')?.clientOrderId).toBe('0x0123456789abcdef0123456789abcdef');
    expect(store.orders.find((o) => o.id === '8')).not.toHaveProperty('clientOrderId');
  });
});

describe('hyperliquid findOrderByClientOrderId', () => {
  const statusRow = {
    coin: 'BTC',
    side: 'B',
    limitPx: '78000.0',
    sz: '0.0',
    oid: 9,
    timestamp: 1,
    isTrigger: false,
    triggerPx: '0.0',
    reduceOnly: false,
    orderType: 'Limit',
    origSz: '0.1',
    tif: 'Gtc',
    cloid: '0x0123456789abcdef0123456789abcdef',
  };

  it('finds a filled order through orderStatus without writing the store', async () => {
    const { exchange, store } = createExchange({ applicationId: WALLET });
    const post = vi.fn().mockResolvedValue({
      data: { status: 'order', order: { order: statusRow, status: 'filled', statusTimestamp: 2 } },
    });
    exchange.infoXhr = { post } as any;

    const lookup = await exchange.findOrderByClientOrderId('BTC-PERP', statusRow.cloid);

    expect(post.mock.calls[0][1]).toEqual({ type: 'orderStatus', user: WALLET, oid: statusRow.cloid });
    expect(lookup).toMatchObject({
      found: true,
      order: { id: '9', status: OrderStatus.Closed, clientOrderId: statusRow.cloid },
    });
    expect(store.orders).toHaveLength(0);
  });

  it('reports an unknown cloid as a non-definitive miss', async () => {
    const { exchange } = createExchange({ applicationId: WALLET });
    exchange.infoXhr = { post: vi.fn().mockResolvedValue({ data: { status: 'unknownOid' } }) } as any;

    await expect(exchange.findOrderByClientOrderId('BTC-PERP', statusRow.cloid)).resolves.toEqual({
      found: false,
      definitive: false,
    });
  });
});

describe('placementRecordFromWire', () => {
  it('reads a market order (Gtc limit at the slippage cap) as a Limit at its wire price', () => {
    expect(
      placementRecordFromWire(
        { a: 0, b: true, p: '82425', s: '0.1', r: false, t: { limit: { tif: 'Gtc' } }, c: '0x' },
        'BTC-PERP',
        5,
      ),
    ).toEqual({
      type: OrderType.Limit,
      price: 82425,
      side: OrderSide.Buy,
      amount: 0.1,
      reduceOnly: false,
      timeInForce: OrderTimeInForce.GoodTillCancel,
      symbol: 'BTC-PERP',
      placedAt: 5,
    });
  });
});

describe('hyperliquidspot placement wires carry a cloid', () => {
  it('limit entry with an attached stop', async () => {
    const store = new DefaultStore();
    store.update({
      markets: [
        {
          id: '@1',
          symbol: 'HFUN/USDC',
          base: 'HFUN',
          quote: 'USDC',
          active: true,
          isSpot: true,
          precision: { amount: 0.01, price: 0.0001 },
          limits: { amount: { min: 0.01, max: 1000000 } },
          info: { hyperliquidId: 10001 },
        } as any,
      ],
    });
    const exchange = new HyperliquidSpotExchange(
      'hyperliquidspot' as any,
      { key: WALLET, secret: SECRET } as any,
      store,
    );
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
    const post = vi.fn().mockResolvedValue(okResponse([{ resting: { oid: 1 } }, 'waitingForTrigger']));
    exchange.xhr = { post } as any;
    (exchange as any).requestSpotBalanceRefresh = vi.fn();

    await exchange.placeOrder({
      symbol: 'HFUN/USDC',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 10,
      stopLoss: 9,
    });

    const wires = wiresOf(post);
    expect(wires).toHaveLength(2);
    for (const wire of wires) expect(wire.c).toMatch(CLOID);
    expect(wires[0].c).not.toBe(wires[1].c);
  });
});

describe('cross-cache placement records', () => {
  it('expires a record after its TTL', () => {
    const cache = getCrossCache();
    const cloid = mintHyperliquidCloid();
    const now = Date.now();
    cache.addPlacementRecord(cloid, {
      type: OrderType.StopLoss,
      price: 1,
      side: OrderSide.Sell,
      amount: 1,
      reduceOnly: true,
      symbol: 'BTC-PERP',
      placedAt: now,
    });
    expect(cache.getPlacementRecord(cloid)).toMatchObject({ price: 1 });

    const clock = vi.spyOn(Date, 'now').mockReturnValue(now + PLACEMENT_RECORD_TTL_MS + 1);
    expect(cache.getPlacementRecord(cloid)).toBeUndefined();
    clock.mockRestore();

    cache.removePlacementRecord(cloid);
    expect(cache.getPlacementRecord(cloid)).toBeUndefined();
  });

  it('removes exactly the pre-placement entry placed with a cloid', () => {
    const cache = getCrossCache();
    const [first, second] = [mintHyperliquidCloid(), mintHyperliquidCloid()];
    cache.addPrePlacement('ETH', OrderSide.Buy, OrderType.StopLoss, 3000, 1, first);
    cache.addPrePlacement('ETH', OrderSide.Buy, OrderType.StopLoss, 3100, 1, second);

    cache.removePrePlacementByCloid('ETH', OrderSide.Buy, first);

    // With one candidate left, the strict matcher answers — and it is the second one.
    expect(cache.matchPrePlacementTrigger('ETH', OrderSide.Buy, 1)).toEqual({ type: OrderType.StopLoss, price: 3100 });
  });
});
