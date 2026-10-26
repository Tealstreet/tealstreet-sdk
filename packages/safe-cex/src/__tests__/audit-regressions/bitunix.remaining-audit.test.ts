import type { ExchangeOptions, Market, Position } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { BitunixExchange } from '../reference/bitunix/bitunix.exchange';
import { ENDPOINTS } from '../reference/bitunix/bitunix.types';
import { BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION } from '../reference/bitunix/bitunix.ws-public';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderStopTriggerType, OrderType, PositionSide } from '../../types';

const options: ExchangeOptions = {
  key: 'test-key',
  secret: 'test-secret',
  testnet: false,
  tuning: { consoleLogging: false },
};

type SubscriptionFrame = {
  at: number;
  payload: { args: Array<{ ch: string; symbol: string }> };
};

const market: Market = {
  id: 'BTC/USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 125 } },
};

const createExchange = () => new BitunixExchange('bitunix', options, new DefaultStore());

describe('Bitunix remaining July audit regressions', () => {
  it('keeps only open API-tradable USDT markets and marks them active', async () => {
    const exchange = createExchange();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          data: [
            {
              symbol: 'BTCUSDT',
              base: 'BTC',
              quote: 'USDT',
              basePrecision: 4,
              quotePrecision: 1,
              minTradeVolume: '0.0001',
              maxLimitOrderVolume: '100',
              minLeverage: 1,
              maxLeverage: 125,
              symbolStatus: 'OPEN',
              isApiSupported: true,
            },
            {
              symbol: 'XAUTUSDT',
              base: 'XAUT',
              quote: 'USDT',
              basePrecision: 3,
              quotePrecision: 2,
              symbolStatus: 'OPEN',
              isApiSupported: false,
            },
            {
              symbol: 'ZECUSDT',
              base: 'ZEC',
              quote: 'USDT',
              basePrecision: 3,
              quotePrecision: 2,
              symbolStatus: 'OPEN',
              isApiSupported: 'false',
            },
            {
              symbol: 'OLDUSDT',
              base: 'OLD',
              quote: 'USDT',
              symbolStatus: 'STOP',
              isApiSupported: true,
            },
          ],
        },
      }),
    } as any;

    await expect(exchange.fetchMarkets()).resolves.toEqual([
      expect.objectContaining({ symbol: 'BTCUSDT', active: true }),
    ]);
  });

  it('derives REST percentage from the documented open and last fields', async () => {
    const exchange = createExchange();
    exchange.store.update({ markets: [market] });
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          data: [
            {
              symbol: 'BTCUSDT',
              open: '50000',
              last: '51000',
              markPrice: '50950',
              baseVol: '12.5',
              quoteVol: '631250',
            },
          ],
        },
      }),
    } as any;

    await expect(exchange.fetchTickers()).resolves.toEqual([
      expect.objectContaining({ symbol: 'BTCUSDT', last: 51000, percentage: 2 }),
    ]);
  });

  it.each([
    { hedged: true, positionMode: 'HEDGE' },
    { hedged: false, positionMode: 'ONE_WAY' },
  ])('sends the documented $positionMode position-mode payload', async ({ hedged, positionMode }) => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { code: 0 } });
    const setSetting = vi.spyOn(exchange.store, 'setSetting');
    exchange.xhr = { post } as any;

    await exchange.changePositionMode(hedged);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.HEDGE_MODE, { positionMode });
    expect(setSetting).toHaveBeenCalledWith('isHedged', hedged);
  });

  it('uses only the documented history-trades endpoint for executions', async () => {
    const exchange = createExchange();
    exchange.store.update({ markets: [market] });
    const get = vi.fn().mockResolvedValue({ data: { data: { tradeList: [] } } });
    exchange.xhr = { get } as any;

    await expect(exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' })).resolves.toEqual([]);

    expect(get).toHaveBeenCalledOnce();
    expect(get.mock.calls[0][0]).toBe(ENDPOINTS.HISTORY_TRADES);
  });

  it('splits a combined pending TP/SL row into distinct display orders with the raw Bitunix id retained', async () => {
    const exchange = createExchange();
    exchange.store.update({
      markets: [market],
      positions: [
        {
          id: 'BTCUSDT:long',
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          contracts: 0.01,
          entryPrice: 50_000,
          notional: 500,
          leverage: 10,
          unrealizedPnl: 0,
          realizedPnl: 0,
          liquidationPrice: 45_000,
          info: { positionId: 'position-1' },
        } as Position,
      ],
    });
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          data: [
            {
              id: 'tpsl-1',
              positionId: 'position-1',
              symbol: 'BTCUSDT',
              tpPrice: '52000',
              tpStopType: 'LAST_PRICE',
              tpQty: '0.01',
              slPrice: '49000',
              slStopType: 'MARK_PRICE',
              slQty: '0.01',
            },
          ],
        },
      }),
    } as any;

    const orders = await exchange.fetchTpslOrders();

    expect(orders).toHaveLength(2);
    expect(orders.map((order) => order.id).sort()).toEqual([
      'tpsl-1__position-1__sl',
      'tpsl-1__position-1__tp',
    ]);
    expect(new Set(orders.map((order) => order.id)).size).toBe(2);
    expect(orders).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'tpsl-1__position-1__tp',
          type: OrderType.TakeProfit,
          status: OrderStatus.Open,
          price: 52_000,
          info: expect.objectContaining({ id: 'tpsl-1', orderId: 'tpsl-1' }),
        }),
        expect.objectContaining({
          id: 'tpsl-1__position-1__sl',
          type: OrderType.StopLoss,
          status: OrderStatus.Open,
          price: 49_000,
          info: expect.objectContaining({ id: 'tpsl-1', orderId: 'tpsl-1' }),
        }),
      ]),
    );
  });

  it('refreshes a missing TP/SL positionId before emitting a preflight error', async () => {
    const exchange = createExchange();
    const stalePosition = {
      id: 'BTCUSDT:long',
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      contracts: 1,
      entryPrice: 50_000,
      notional: 50_000,
      leverage: 10,
      unrealizedPnl: 0,
      realizedPnl: 0,
      liquidationPrice: 45_000,
      info: {},
    } as Position;
    const freshPosition = {
      ...stalePosition,
      info: { positionId: 'position-1' },
    };
    exchange.store.update({ markets: [market], positions: [stalePosition] });
    const fetchPositions = vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([freshPosition]);
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');
    const post = vi.fn().mockResolvedValue({
      data: { code: 0, data: { orderId: 'tpsl-1' }, msg: 'Success' },
    });
    exchange.unlimitedXHR = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 0.5,
        price: 49_000,
      }),
    ).resolves.toEqual(['tpsl-1']);

    expect(fetchPositions).toHaveBeenCalledOnce();
    expect(emitAndLogError).not.toHaveBeenCalled();
    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.TP_SL_ORDER,
      expect.objectContaining({
        symbol: 'BTCUSDT',
        positionId: 'position-1',
        slPrice: '49000',
        slQty: '0.5',
      }),
    );
  });

  it('refreshes current TP/SL rows before full-position placement so adding TP preserves existing SL', async () => {
    const exchange = createExchange();
    const position = {
      id: 'BTCUSDT:long',
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      contracts: 1,
      entryPrice: 50_000,
      notional: 50_000,
      leverage: 10,
      unrealizedPnl: 0,
      realizedPnl: 0,
      liquidationPrice: 45_000,
      info: { positionId: 'position-1' },
    } as Position;
    const existingStopLoss = {
      id: 'position-tpsl-1',
      status: OrderStatus.Open,
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      price: 49_000,
      amount: 0,
      filled: 0,
      remaining: 0,
      reduceOnly: true,
      lastUpdate: 1,
      info: {
        id: 'position-tpsl-1',
        orderId: 'position-tpsl-1',
        type: 'POSITION_TPSL',
        positionId: 'position-1',
        slPrice: '49000',
        slStopType: 'MARK_PRICE',
      },
      slTriggerType: OrderStopTriggerType.MarkPrice,
    };
    exchange.store.update({ markets: [market], positions: [position], orders: [] });
    const fetchOrders = vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([existingStopLoss as any]);
    const post = vi.fn().mockResolvedValue({
      data: { code: 0, data: { orderId: 'position-tpsl-1' }, msg: 'Success' },
    });
    exchange.unlimitedXHR = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.TakeProfit,
        side: OrderSide.Sell,
        amount: 0,
        price: 52_000,
      }),
    ).resolves.toEqual(['position-tpsl-1']);

    expect(fetchOrders).toHaveBeenCalledOnce();
    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.POSITION_TP_SL_ORDER,
      expect.objectContaining({
        symbol: 'BTCUSDT',
        positionId: 'position-1',
        tpPrice: '52000',
        tpStopType: 'LAST_PRICE',
        slPrice: '49000',
        slStopType: 'MARK_PRICE',
      }),
    );
  });

  it('shards public subscriptions at 300 topics and paces each connection without duplicate frames', () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const exchange = createExchange();
    const markets = Array.from({ length: 601 }, (_, index) => ({
      ...market,
      id: `COIN${index}/USDT`,
      symbol: `COIN${index}USDT`,
      base: `COIN${index}`,
    }));
    exchange.store.update({ markets });

    const websockets = (exchange as any).configurePublicWebsocketShards(markets);
    expect(websockets).toHaveLength(6);

    const sentByConnection: SubscriptionFrame[][] = websockets.map((websocket: any) => {
      const sent: SubscriptionFrame[] = [];
      websocket.ws = {
        readyState: WebSocket.OPEN,
        send: (payload: string) => sent.push({ at: Date.now(), payload: JSON.parse(payload) }),
        close: vi.fn(),
      } as any;
      vi.spyOn(websocket, 'ping').mockImplementation(() => {});
      websocket.onOpen();
      return sent;
    });

    vi.advanceTimersByTime(4_000);

    expect(
      sentByConnection.flatMap((frames) => frames.flatMap((frame) => frame.payload.args)),
    ).toHaveLength(601 * 2);
    for (const frames of sentByConnection) {
      const args = frames.flatMap((frame) => frame.payload.args);
      expect(args.length).toBeLessThanOrEqual(BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION);
      expect(new Set(args.map((arg: any) => `${arg.ch}:${arg.symbol}`)).size).toBe(args.length);
      for (let index = 1; index < frames.length; index++) {
        expect(frames[index].at - frames[index - 1].at).toBeGreaterThanOrEqual(250);
      }
    }

    exchange.dispose();
    vi.useRealTimers();
  });

  it('preserves the sorted depthop snapshot order while calculating cumulative totals', () => {
    const exchange = createExchange();
    exchange.store.update({ markets: [market] });
    const websocket = exchange.futuresWebsocket;
    websocket.ws = { readyState: WebSocket.OPEN, send: vi.fn() } as any;
    const callback = vi.fn();

    const stop = websocket.listenOrderBook('BTCUSDT', callback);
    const handler = websocket.messageHandlers['market_btcusdt_depthop_0.1'][0];
    handler({
      data: {
        bids: [
          ['64834.8', '3.5908'],
          ['64834.7', '4.2607'],
          ['64834.6', '2.4312'],
        ],
        asks: [
          ['64834.9', '4.1825'],
          ['64835.0', '1.2894'],
          ['64835.1', '2.0932'],
        ],
      },
    });

    const orderBook = callback.mock.calls[0][0];
    expect(orderBook.bids.map((level: any) => level.price)).toEqual([64834.8, 64834.7, 64834.6]);
    expect(orderBook.asks.map((level: any) => level.price)).toEqual([64834.9, 64835, 64835.1]);
    expect(orderBook.bids[2].total).toBeCloseTo(10.2827);
    expect(orderBook.asks[2].total).toBeCloseTo(7.5651);

    stop();
  });

  it.each([
    {
      name: 'missing position',
      configure: (_exchange: BitunixExchange) => {},
      opts: { symbol: 'BTCUSDT', type: OrderType.StopLoss, side: OrderSide.Sell, amount: 0, price: 49000 },
      error: 'No open long position found for BTCUSDT',
    },
    {
      name: 'missing position id',
      configure: (exchange: BitunixExchange) => {
        exchange.store.update({
          positions: [
            {
              symbol: 'BTCUSDT',
              side: PositionSide.Long,
              contracts: 1,
              info: {},
            } as Position,
          ],
        });
      },
      opts: { symbol: 'BTCUSDT', type: OrderType.StopLoss, side: OrderSide.Sell, amount: 0, price: 49000 },
      error: 'Missing positionId for BTCUSDT TP/SL order',
    },
    {
      name: 'missing full-position price',
      configure: (exchange: BitunixExchange) => {
        exchange.store.update({
          positions: [
            {
              symbol: 'BTCUSDT',
              side: PositionSide.Long,
              contracts: 1,
              info: { positionId: 'position-1' },
            } as Position,
          ],
        });
      },
      opts: { symbol: 'BTCUSDT', type: OrderType.StopLoss, side: OrderSide.Sell, amount: 0 },
      error: 'Price is required for Position TP/SL orders',
    },
    {
      name: 'missing partial price',
      configure: (exchange: BitunixExchange) => {
        exchange.store.update({
          positions: [
            {
              symbol: 'BTCUSDT',
              side: PositionSide.Long,
              contracts: 1,
              info: { positionId: 'position-1' },
            } as Position,
          ],
        });
      },
      opts: {
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 0.5,
        slTriggerType: OrderStopTriggerType.MarkPrice,
      },
      error: 'Price is required for TP/SL orders',
    },
  ])('emits the $name TP/SL preflight error before throwing', ({ configure, opts, error }) => {
    const exchange = createExchange();
    exchange.store.update({ markets: [market] });
    configure(exchange);
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');

    expect(() => (exchange as any).formatTPSLOrder(opts)).toThrow(error);
    expect(emitAndLogError).toHaveBeenCalledWith(error);
  });
});
