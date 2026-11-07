import type { Market } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { getExchangeMetadata } from '../../exchangeMetadata';
import { BlofinExchange } from '../reference/blofin/blofin.exchange';
import { BASE_URL, BASE_WS_URL, ENDPOINTS } from '../../exchanges/blofin/blofin.types';
import { BLOFIN_MAX_SUBSCRIBE_FRAME_BYTES, buildBlofinSubscribeFrames } from '../reference/blofin/blofin.ws-public';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderStatus, OrderStopTriggerType, OrderType, PositionMode } from '../../types';

const MARKET: Market = {
  id: 'BTC-USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 1, price: 0.1 },
  limits: { amount: { min: 1, max: 1_000 }, leverage: { min: 1, max: 100 } },
  contractSize: 0.001,
  info: { lotSize: '1', minSize: '1', contractValue: '0.001' },
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

  it("chunks all-market subscriptions below BloFin's documented 4096-byte limit", () => {
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

  it('keeps books sorted when a top-five snapshot moves through old levels', () => {
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    websocket.ws = { readyState: WebSocket.OPEN, send: vi.fn() } as unknown as WebSocket;
    const callback = vi.fn();
    const dispose = websocket.listenOrderBook(MARKET.symbol, callback);
    const full = websocket.messageHandlers[`books.${MARKET.id}`][0];
    const top = websocket.messageHandlers[`books5.${MARKET.id}`][0];
    full({
      action: 'snapshot',
      data: {
        bids: [
          ['99', '1'],
          ['98', '2'],
          ['97', '3'],
          ['96', '4'],
        ],
        asks: [
          ['101', '1'],
          ['102', '2'],
          ['103', '3'],
          ['104', '4'],
        ],
        seqId: '1',
      },
    });
    top({
      data: {
        bids: [
          ['98', '5'],
          ['97', '6'],
        ],
        asks: [
          ['102', '5'],
          ['103', '6'],
        ],
      },
    });
    const book = callback.mock.calls.at(-1)![0];
    expect(book.bids.map(({ price, amount }: { price: number; amount: number }) => [price, amount])).toEqual([
      [98, 5],
      [97, 6],
    ]);
    full({
      action: 'update',
      data: {
        bids: [['97', '0']],
        asks: [['103', '8']],
        prevSeqId: '1',
        seqId: '2',
      },
    });
    expect(book.bids.map(({ price }: { price: number }) => price)).toEqual([98]);
    expect(book.asks.map(({ price }: { price: number }) => price)).toEqual([102, 103]);
    dispose();
  });

  it('fetches and maps externally placed opening triggers as conditionals', async () => {
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

  it('retries a failed trigger-order snapshot and preserves the existing store after exhaustion', async () => {
    vi.useFakeTimers();
    const exchange = createExchange();
    const existingOrder = exchange.mapOrders([
      {
        algoId: 'existing-trigger',
        instId: MARKET.id,
        side: 'sell',
        orderType: 'trigger',
        size: '1',
        state: 'live',
        triggerPrice: '49000',
      },
    ])[0];
    exchange.store.update({ orders: [existingOrder] });
    const get = vi.fn().mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.UNFILLED_TRIGGER_ORDERS) {
        return Promise.reject(new Error('trigger snapshot unavailable'));
      }
      return Promise.resolve({ data: { code: '0', msg: 'success', data: [] } });
    });
    exchange.xhr = { get } as any;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    try {
      const pending = exchange.fetchOrders();
      await vi.runAllTimersAsync();

      await expect(pending).resolves.toEqual([existingOrder]);
      expect(get).toHaveBeenCalledTimes(9);
      expect(get.mock.calls.filter(([endpoint]) => endpoint === ENDPOINTS.UNFILLED_TRIGGER_ORDERS)).toHaveLength(3);
      expect(emitAndLogError).toHaveBeenCalledOnce();
      expect(emitAndLogError).toHaveBeenCalledWith('trigger snapshot unavailable');
    } finally {
      vi.useRealTimers();
    }
  });

  it('discards a partial trigger-order snapshot when a later page fails', async () => {
    vi.useFakeTimers();
    const exchange = createExchange();
    const existingOrder = exchange.mapOrders([
      {
        algoId: 'existing-trigger',
        instId: MARKET.id,
        side: 'sell',
        orderType: 'trigger',
        size: '1',
        state: 'live',
        triggerPrice: '49000',
      },
    ])[0];
    exchange.store.update({ orders: [existingOrder] });
    const partialTrigger = {
      algoId: 'partial-trigger',
      instId: MARKET.id,
      side: 'buy',
      orderType: 'trigger',
      size: '1',
      state: 'live',
      triggerPrice: '51000',
    };
    let triggerFetches = 0;
    const get = vi.fn().mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.UNFILLED_TRIGGER_ORDERS) {
        triggerFetches += 1;
        if (triggerFetches % 2 === 1) {
          return Promise.resolve({ data: { code: '0', msg: 'success', data: [partialTrigger] } });
        }
        return Promise.reject(new Error('trigger page unavailable'));
      }
      return Promise.resolve({ data: { code: '0', msg: 'success', data: [] } });
    });
    exchange.xhr = { get } as any;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    try {
      const pending = exchange.fetchOrders();
      await vi.runAllTimersAsync();

      await expect(pending).resolves.toEqual([existingOrder]);
      expect(triggerFetches).toBe(6);
      expect(emitAndLogError).toHaveBeenCalledOnce();
      expect(emitAndLogError).toHaveBeenCalledWith('trigger page unavailable');
    } finally {
      vi.useRealTimers();
    }
  });

  it('places opening triggers on the dedicated algo endpoint and advertises the capability', async () => {
    const exchange = createExchange();
    vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
    vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
    const post = vi.fn().mockResolvedValue({
      data: { code: '0', msg: 'success', data: { algoId: 'algo-new', clientOrderId: 'client-new', code: '0' } },
    });
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: MARKET.symbol,
        side: OrderSide.Buy,
        type: OrderType.Conditional,
        amount: 2,
        price: 50_500,
        triggerPrice: 51_000,
        stopLoss: 49_000,
        takeProfit: 55_000,
        slTriggerType: OrderStopTriggerType.MarkPrice,
        tpTriggerType: OrderStopTriggerType.IndexPrice,
      }),
    ).resolves.toEqual(['algo-new']);

    expect(getExchangeMetadata('blofin').supportsOpeningTriggerOrders).toBe(true);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.PLACE_TRIGGER_ORDER, {
      instId: MARKET.id,
      marginMode: 'cross',
      positionSide: 'net',
      side: 'buy',
      size: '2',
      clientOrderId: expect.any(String),
      orderPrice: '50500',
      orderType: 'trigger',
      triggerPrice: '51000',
      triggerPriceType: 'last',
      attachAlgoOrders: [
        {
          tpTriggerPrice: '55000',
          tpOrderPrice: '-1',
          tpTriggerPriceType: 'index',
          slTriggerPrice: '49000',
          slOrderPrice: '-1',
          slTriggerPriceType: 'mark',
        },
      ],
    });
  });

  it('cancel-replaces a reconciled trigger while preserving execution and attachment provenance', async () => {
    vi.useFakeTimers();
    const exchange = createExchange();
    const triggerOrder = exchange.mapOrders([
      {
        algoId: 'algo-2101',
        instId: MARKET.id,
        marginMode: 'isolated',
        positionSide: 'long',
        side: 'buy',
        orderType: 'trigger',
        size: '2',
        state: 'live',
        triggerPrice: '51000',
        triggerPriceType: 'mark',
        orderPrice: '50500',
        reduceOnly: 'false',
        attachAlgoOrders: [
          {
            tpTriggerPrice: '55000',
            tpOrderPrice: '-1',
            tpTriggerPriceType: 'index',
            slTriggerPrice: '49000',
            slOrderPrice: '-1',
            slTriggerPriceType: 'mark',
          },
        ],
      },
    ])[0];
    const post = vi.fn().mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.CANCEL_TRIGGER_ORDER) {
        return Promise.resolve({ data: { code: '0', data: { algoId: 'algo-2101', code: '0' } } });
      }
      if (endpoint === ENDPOINTS.PLACE_TRIGGER_ORDER) {
        return Promise.resolve({ data: { code: '0', data: { algoId: 'algo-replacement', code: '0' } } });
      }
      throw new Error(`unexpected endpoint ${endpoint}`);
    });
    exchange.xhr = { post } as any;

    try {
      const pending = exchange.updateOrder({ order: triggerOrder, update: { price: 52_000 } });
      await vi.runAllTimersAsync();
      await expect(pending).resolves.toEqual(['algo-replacement']);
    } finally {
      vi.useRealTimers();
    }

    expect(post.mock.calls[0]).toEqual([ENDPOINTS.CANCEL_TRIGGER_ORDER, { instId: MARKET.id, algoId: 'algo-2101' }]);
    expect(post.mock.calls[1][0]).toBe(ENDPOINTS.PLACE_TRIGGER_ORDER);
    expect(post.mock.calls[1][1]).toEqual(
      expect.objectContaining({
        instId: MARKET.id,
        marginMode: 'isolated',
        positionSide: 'long',
        side: 'buy',
        size: '2',
        orderPrice: '50500',
        orderType: 'trigger',
        triggerPrice: '52000',
        triggerPriceType: 'mark',
        attachAlgoOrders: [
          {
            tpTriggerPrice: '55000',
            tpOrderPrice: '-1',
            tpTriggerPriceType: 'index',
            slTriggerPrice: '49000',
            slOrderPrice: '-1',
            slTriggerPriceType: 'mark',
          },
        ],
      }),
    );
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
});

it('keeps native remaining quantity monotonic on stale cumulative order frames', () => {
  const exchange = createExchange();
  const event = (filledSize: string, state = 'partially_filled') => ({
    data: [
      {
        instId: MARKET.id,
        orderId: '42',
        orderCategory: 'normal',
        orderType: 'limit',
        side: 'buy',
        positionSide: 'net',
        marginMode: 'cross',
        size: '10',
        price: '60000',
        averagePrice: '60000',
        filledSize,
        state,
      },
    ],
  });
  try {
    exchange.privateWebsocket.handleOrderTopic(event('4'));
    exchange.privateWebsocket.handleOrderTopic(event('2'));
    expect(exchange.store.orders[0]).toMatchObject({ filled: 0.004, remaining: 0.006 });
    exchange.privateWebsocket.handleOrderTopic(event('10', 'filled'));
    exchange.privateWebsocket.handleOrderTopic(event('4'));
    expect(exchange.store.orders[0].status).toBe(OrderStatus.Closed);
  } finally {
    exchange.dispose();
  }
});
