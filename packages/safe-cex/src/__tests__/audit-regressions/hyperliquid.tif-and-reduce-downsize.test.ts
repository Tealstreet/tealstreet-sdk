import { describe, expect, it, vi } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { HyperliquidExchange } from '../reference/hyperliquid/hyperliquid.exchange';
import { parseHyperliquidOrderTimeInForce } from '../../exchanges/hyperliquid/hyperliquid.order-inference';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderTimeInForce, OrderType, PositionMode, PositionSide } from '../../types';

const createExchange = () =>
  new HyperliquidExchange('hyperliquid' as any, { key: 'test', secret: 'test' } as any, new DefaultStore());

// e2e run 31317468325 sent {"s":"336","r":true} against a 168-contract long and
// Hyperliquid rested it at 336 — it does not cap reduce-only size server-side.
describe('hyperliquid reduce-only downsizing', () => {
  it('opts into client-side downsizing', () => {
    expect(exchangeMetadata.hyperliquid.autoDownSizeReduceOrders).toBe(true);
  });

  // Spot has no positions to reduce, so the flag would be meaningless there.
  it('leaves the spot venue on the default', () => {
    expect(exchangeMetadata.hyperliquidspot.autoDownSizeReduceOrders).toBe(false);
  });
});

describe('hyperliquid core-perp trigger order safety', () => {
  const createExchangeWithBtcMarket = () => {
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
      tickers: [
        {
          id: 'BTC',
          symbol: 'BTC-PERP',
          bid: 78490,
          ask: 78510,
          last: 78500,
          mark: 78500,
          index: 78502,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        } as any,
      ],
      positions: [
        {
          id: 'BTC-PERP:long',
          symbol: 'BTC-PERP',
          side: PositionSide.Long,
          contracts: 0.00014,
          entryPrice: 78500,
          notional: 10.99,
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
      {
        key: '0x0000000000000000000000000000000000000001',
        secret: '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
      } as any,
      store,
    );
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
    return { exchange, store };
  };

  it('routes explicit full-size reduce-only stops through positional TP/SL sizing', async () => {
    const { exchange } = createExchangeWithBtcMarket();
    const post = vi.fn().mockResolvedValue({
      data: {
        status: 'ok',
        response: {
          type: 'order',
          data: { statuses: [{ resting: { oid: 12345 } }] },
        },
      },
    });
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTC-PERP',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 0.00014,
        price: 77000,
        reduceOnly: true,
      }),
    ).resolves.toEqual(['12345']);

    const payload = post.mock.calls[0][1];
    expect(payload.action.grouping).toBe('positionTpsl');
    expect(payload.action.orders[0]).toMatchObject({
      a: 0,
      b: false,
      s: '0',
      r: true,
      t: { trigger: { isMarket: true, tpsl: 'sl', triggerPx: '77000' } },
    });
  });

  it('dedupes duplicate frontendOpenOrders rows before mutating the store', async () => {
    const store = new DefaultStore();
    const market = {
      id: 'ARB',
      symbol: 'ARB-PERP',
      base: 'ARB',
      quote: 'USDC',
      active: true,
      precision: { amount: 0.1, price: 0.00001 },
      limits: { amount: { min: 0.1, max: 1000000 } },
      contractSize: 1,
      isSpot: false,
      info: { hyperliquidId: 11 },
    } as any;

    store.update({ markets: [market] });

    const exchange = new HyperliquidExchange(
      'hyperliquid' as any,
      {
        key: '0x0000000000000000000000000000000000000001',
        secret: '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
        applicationId: '0x0000000000000000000000000000000000000001',
      } as any,
      store,
    );
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
    (exchange as any).hip3DexNames = ['mkts'];

    const openOrder = {
      coin: 'ARB',
      side: 'A',
      limitPx: '0.08762',
      sz: '0.0',
      oid: 528182185290,
      timestamp: 1787819295129,
      triggerCondition: 'Price below 0.08717',
      isTrigger: true,
      triggerPx: '0.08717',
      children: [],
      isPositionTpsl: true,
      reduceOnly: true,
      orderType: 'Stop Market',
      origSz: '0.0',
      tif: null,
      cloid: null,
    };
    const post = vi.fn().mockResolvedValueOnce({ data: [openOrder] }).mockResolvedValueOnce({ data: [openOrder] });
    exchange.xhr = { post } as any;

    await expect(exchange.fetchOrders()).resolves.toHaveLength(1);
    expect(store.orders).toHaveLength(1);
    expect(store.orders[0]).toMatchObject({
      id: '528182185290',
      symbol: 'ARB-PERP',
      type: OrderType.StopLoss,
      price: 0.08717,
    });
  });

  it('keeps one-lot-smaller reduce-only stops as partial standalone trigger orders', async () => {
    const { exchange } = createExchangeWithBtcMarket();
    const post = vi.fn().mockResolvedValue({
      data: {
        status: 'ok',
        response: {
          type: 'order',
          data: { statuses: [{ resting: { oid: 12345 } }] },
        },
      },
    });
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTC-PERP',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 0.00013,
        price: 77000,
        reduceOnly: true,
      }),
    ).resolves.toEqual(['12345']);

    const payload = post.mock.calls[0][1];
    expect(payload.action.grouping).toBe('na');
    expect(payload.action.orders[0]).toMatchObject({
      a: 0,
      b: false,
      s: '0.00013',
      r: true,
      t: { trigger: { isMarket: true, tpsl: 'sl', triggerPx: '77000' } },
    });
  });

  it('refreshes orders when positional TP/SL is accepted without an order id', async () => {
    const { exchange } = createExchangeWithBtcMarket();
    const post = vi.fn().mockResolvedValue({
      data: {
        status: 'ok',
        response: {
          type: 'order',
          data: { statuses: ['waitingForTrigger'] },
        },
      },
    });
    exchange.xhr = { post } as any;
    exchange.scheduleOrderRefresh = vi.fn();

    await expect(
      exchange.placeOrder({
        symbol: 'BTC-PERP',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 0.00014,
        price: 77000,
        reduceOnly: true,
      }),
    ).resolves.toEqual([]);

    expect(exchange.scheduleOrderRefresh).toHaveBeenCalledTimes(1);
  });

  it('rejects trigger orders before POST when ticker price data is missing', async () => {
    const { exchange, store } = createExchangeWithBtcMarket();
    store.update({
      tickers: [
        {
          id: 'BTC',
          symbol: 'BTC-PERP',
          bid: 0,
          ask: 0,
          last: 0,
          mark: 0,
          index: 0,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        } as any,
      ],
    });
    const post = vi.fn();
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTC-PERP',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 0.00014,
        price: 77000,
        reduceOnly: true,
      }),
    ).rejects.toThrow('missing valid ticker price data');

    expect(post).not.toHaveBeenCalled();
  });
});

// The WS `orderUpdates` frame carries no `tif` field for any market, so this
// parser only ever resolves on the REST path.
describe('parseHyperliquidOrderTimeInForce', () => {
  it('maps the wire values REST sends', () => {
    expect(parseHyperliquidOrderTimeInForce('Gtc')).toBe(OrderTimeInForce.GoodTillCancel);
    expect(parseHyperliquidOrderTimeInForce('Alo')).toBe(OrderTimeInForce.PostOnly);
  });

  it('returns undefined for an omitted tif, which is what the WS path always hits', () => {
    expect(parseHyperliquidOrderTimeInForce(undefined)).toBeUndefined();
    expect(parseHyperliquidOrderTimeInForce(null)).toBeUndefined();
  });
});

// Core perps get no optimistic store row (that is HIP-3 only) and tick() only
// re-fetches orders when HIP-3 dexes exist, so this cache is the sole source of
// time-in-force for them once the WS frame lands.
describe('hyperliquid placement-time timeInForce cache', () => {
  it('returns undefined for an order it never saw', () => {
    expect(createExchange().getPlacedLimitOrderTimeInForce('999')).toBeUndefined();
  });

  it('recalls what placement recorded', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('1', OrderTimeInForce.PostOnly);
    expect(exchange.getPlacedLimitOrderTimeInForce('1')).toBe(OrderTimeInForce.PostOnly);
  });

  it('drops the entry when the order reaches a terminal status', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('1', OrderTimeInForce.GoodTillCancel);
    exchange.forgetLimitOrderTimeInForce('1');
    expect(exchange.getPlacedLimitOrderTimeInForce('1')).toBeUndefined();
  });

  it('evicts oldest-first past the bound instead of growing without limit', () => {
    const exchange = createExchange();
    for (let i = 0; i < 600; i++) {
      exchange.rememberLimitOrderTimeInForce(`${i}`, OrderTimeInForce.GoodTillCancel);
    }
    expect(exchange.getPlacedLimitOrderTimeInForce('0')).toBeUndefined();
    expect(exchange.getPlacedLimitOrderTimeInForce('599')).toBe(OrderTimeInForce.GoodTillCancel);
  });

  it('re-recording an id refreshes its position rather than duplicating it', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('keep', OrderTimeInForce.PostOnly);
    for (let i = 0; i < 499; i++) {
      exchange.rememberLimitOrderTimeInForce(`${i}`, OrderTimeInForce.GoodTillCancel);
    }
    exchange.rememberLimitOrderTimeInForce('keep', OrderTimeInForce.PostOnly);
    exchange.rememberLimitOrderTimeInForce('overflow', OrderTimeInForce.GoodTillCancel);
    expect(exchange.getPlacedLimitOrderTimeInForce('keep')).toBe(OrderTimeInForce.PostOnly);
  });

  it('clears on dispose', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('1', OrderTimeInForce.GoodTillCancel);
    exchange.dispose();
    expect(exchange.getPlacedLimitOrderTimeInForce('1')).toBeUndefined();
  });
});

// Mirrors the resolution order in hyperliquid.ws-private.ts `handleOrderUpdates`.
const resolveTimeInForce = (wsTif: unknown, orderId: string, exchange: HyperliquidExchange, stored?: StoredOrder) =>
  parseHyperliquidOrderTimeInForce(wsTif) ?? stored?.timeInForce ?? exchange.getPlacedLimitOrderTimeInForce(orderId);

type StoredOrder = { timeInForce?: OrderTimeInForce };

describe('hyperliquid websocket timeInForce preservation', () => {
  it('keeps the stored value when the frame omits tif', () => {
    const exchange = createExchange();
    expect(resolveTimeInForce(undefined, '1', exchange, { timeInForce: OrderTimeInForce.GoodTillCancel })).toBe(
      OrderTimeInForce.GoodTillCancel,
    );
  });

  it('falls back to the placement record when nothing is stored', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('1', OrderTimeInForce.PostOnly);
    expect(resolveTimeInForce(undefined, '1', exchange, undefined)).toBe(OrderTimeInForce.PostOnly);
  });

  it('prefers the frame over both fallbacks when tif is present', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('1', OrderTimeInForce.GoodTillCancel);
    expect(resolveTimeInForce('Alo', '1', exchange, { timeInForce: OrderTimeInForce.GoodTillCancel })).toBe(
      OrderTimeInForce.PostOnly,
    );
  });

  it('no longer resolves to undefined for a plain core-perp GTC order', () => {
    const exchange = createExchange();
    exchange.rememberLimitOrderTimeInForce('1', OrderTimeInForce.GoodTillCancel);
    expect(resolveTimeInForce(undefined, '1', exchange, undefined)).not.toBeUndefined();
  });
});
