import type { Market } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { BlofinExchange } from '../../exchanges/blofin/blofin.exchange';
import { BASE_URL, BASE_WS_URL, ENDPOINTS } from '../../exchanges/blofin/blofin.types';
import {
  BLOFIN_MAX_SUBSCRIBE_FRAME_BYTES,
  buildBlofinSubscribeFrames,
} from '../../exchanges/blofin/blofin.ws-public';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';

const MARKET: Market = {
  id: 'BTC-USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 1, price: 0.1 },
  limits: { amount: { min: 1, max: 1_000 }, leverage: { min: 1, max: 100 } },
  contractSize: 0.001,
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET], loaded: { ...store.loaded, markets: true } });
  return new BlofinExchange('blofin', { key: 'key', secret: 'secret', passphrase: 'pass' }, store);
};

describe('BloFin remaining audit regressions', () => {
  it('keeps dormant testnet routing on the documented demo-trading hosts', () => {
    expect(BASE_URL).toEqual({
      livenet: 'https://openapi.blofin.com',
      testnet: 'https://demo-trading-openapi.blofin.com',
    });
    expect(BASE_WS_URL.public.testnet).toBe('wss://demo-trading-openapi.blofin.com/ws/public');
    expect(BASE_WS_URL.private.testnet).toBe('wss://demo-trading-openapi.blofin.com/ws/private');
  });

  it('chunks all-market subscriptions below BloFin\'s documented 4096-byte limit', () => {
    const topics = Array.from({ length: 500 }, (_, index) => ({
      channel: index % 2 === 0 ? 'tickers' : 'funding-rate',
      instId: `MARKET-${String(index).padStart(4, '0')}-USDT`,
    }));

    const frames = buildBlofinSubscribeFrames(topics);

    expect(frames.length).toBeGreaterThan(1);
    expect(
      frames.every((frame) => new TextEncoder().encode(frame).byteLength <= BLOFIN_MAX_SUBSCRIBE_FRAME_BYTES),
    ).toBe(true);
    expect(frames.flatMap((frame) => JSON.parse(frame).args)).toEqual(topics);
  });

  it('drops a discontinuous book delta and resubscribes for a fresh snapshot', () => {
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    const sent: string[] = [];
    websocket.ws = {
      readyState: WebSocket.OPEN,
      send: (frame: string) => sent.push(frame),
    } as unknown as WebSocket;
    const callback = vi.fn();
    websocket.listenOrderBook(MARKET.symbol, callback);
    const parser = websocket.messageHandlers[`books.${MARKET.id}`][0];

    parser({
      action: 'snapshot',
      data: { bids: [['50000', '2']], asks: [['50001', '3']], prevSeqId: '0', seqId: '10' },
    });
    parser({
      action: 'update',
      data: { bids: [['50000', '1']], asks: [], prevSeqId: '9', seqId: '11' },
    });

    expect(callback).toHaveBeenCalledTimes(1);
    expect(sent.slice(-2).map((frame) => JSON.parse(frame))).toEqual([
      { op: 'unsubscribe', args: [{ channel: 'books', instId: MARKET.id }] },
      { op: 'subscribe', args: [{ channel: 'books', instId: MARKET.id }] },
    ]);
  });

  it('fetches and maps externally placed opening triggers as read-only conditionals', async () => {
    const exchange = createExchange();
    const trigger = {
      algoId: 'algo-2101',
      instId: MARKET.id,
      marginMode: 'cross',
      positionSide: 'net',
      side: 'sell',
      orderType: 'trigger',
      size: '2',
      state: 'live',
      triggerPrice: '49000',
      triggerPriceType: 'last',
      reduceOnly: 'false',
    };
    let triggerFetches = 0;
    const get = vi.fn().mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.UNFILLED_TRIGGER_ORDERS) {
        triggerFetches += 1;
        return Promise.resolve({ data: { code: '0', msg: 'success', data: triggerFetches === 1 ? [trigger] : [] } });
      }
      return Promise.resolve({ data: { code: '0', msg: 'success', data: [] } });
    });
    exchange.xhr = { get } as any;

    const orders = await exchange.fetchOrders();

    expect(orders).toEqual([
      expect.objectContaining({
        id: 'algo-2101',
        symbol: MARKET.symbol,
        type: OrderType.Conditional,
        side: OrderSide.Sell,
        status: OrderStatus.Open,
        price: 49_000,
        amount: 0.002,
        remaining: 0.002,
        reduceOnly: false,
      }),
    ]);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.UNFILLED_TRIGGER_ORDERS, {
      params: { after: undefined, limit: 100, orderType: 'trigger' },
    });
    expect(get).toHaveBeenCalledWith(ENDPOINTS.UNFILLED_TRIGGER_ORDERS, {
      params: { after: 'algo-2101', limit: 100, orderType: 'trigger' },
    });
  });

  it('fails closed instead of sending trigger orders to normal or TPSL write endpoints', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    exchange.xhr = { post } as any;
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    const triggerOrder = exchange.mapOrders([
      {
        algoId: 'algo-2101',
        instId: MARKET.id,
        side: 'buy',
        orderType: 'trigger',
        size: '1',
        state: 'live',
        triggerPrice: '51000',
      },
    ])[0];

    await expect(exchange.cancelOrders([triggerOrder])).rejects.toThrow('read-only');
    await expect(exchange.updateOrder({ order: triggerOrder, update: { price: 52_000 } })).rejects.toThrow(
      'read-only',
    );
    await expect(
      exchange.placeOrder({
        symbol: MARKET.symbol,
        side: OrderSide.Buy,
        type: OrderType.Conditional,
        amount: 0.001,
        triggerPrice: 51_000,
      }),
    ).rejects.toThrow('not implemented');
    expect(post).not.toHaveBeenCalled();
  });

  it('fails a rejected margin-mode change before refetching or mutating position state', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { code: '51000', msg: 'margin mode rejected' } });
    exchange.xhr = { post } as any;
    const refetchMarginType = vi.spyOn(exchange, 'getOrFetchMarginType');
    const fetchPositions = vi.spyOn(exchange, 'fetchPositions');
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(exchange.changeMarginType(true, 'BTCUSDT')).rejects.toThrow('margin mode rejected');

    expect(post).toHaveBeenCalledWith(ENDPOINTS.SET_MARGIN_MODE, { marginMode: 'isolated' });
    expect(refetchMarginType).not.toHaveBeenCalled();
    expect(fetchPositions).not.toHaveBeenCalled();
  });

  it.each(['filled', 'canceled'])('prunes fill-delta memory when an order becomes %s', (state) => {
    const exchange = createExchange();
    const websocket = exchange.privateWebsocket;
    const mappedOrder = {
      id: 'order-1',
      status: OrderStatus.Open,
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 50_000,
      amount: 2,
      filled: 1,
      remaining: 1,
      reduceOnly: false,
      lastUpdate: 1,
      info: {},
    };
    vi.spyOn(exchange, 'mapOrders').mockReturnValue([mappedOrder]);
    const fillMemory = (websocket as any).lastFilledSizeByOrderId as Map<string, number>;
    fillMemory.set('order-1', 1);

    websocket.handleOrderTopic({
      data: [{ orderId: 'order-1', state, filledSize: '2', averagePrice: '50000' }],
    });

    expect(fillMemory.has('order-1')).toBe(false);
    websocket.dispose();
  });
});
