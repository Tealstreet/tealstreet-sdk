import { describe, expect, it, vi } from 'vitest';

import { BingxExchange } from '../../exchanges/bingx/bingx.exchange';
import { ENDPOINTS } from '../../exchanges/bingx/bingx.types';
import { BingxPublicWebsocket } from '../../exchanges/bingx/bingx.ws-public';
import { DefaultStore } from '../../store/store.base';
import { MarginType, Order, OrderSide, OrderStatus, OrderType, PositionMode, PositionSide } from '../../types';
import { getExchangeMetadata } from '../../exchangeMetadata';

const createExchange = () => new BingxExchange('bingx', { key: 'key', secret: 'secret' }, new DefaultStore());

const order = (id: string): Order => ({
  id,
  symbol: 'BTCUSDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 50_000,
  amount: 1,
  filled: 0,
  remaining: 1,
  status: OrderStatus.Open,
  reduceOnly: false,
  lastUpdate: 0,
});

describe('audit regression: BingX', () => {
  it('uses the documented v3 balance array for account identity, validation, and balances', async () => {
    const exchange = createExchange();
    const balance = {
      userId: '116***295',
      asset: 'USDT',
      balance: '194.8212',
      equity: '196.7431',
      unrealizedProfit: '1.9219',
      realisedProfit: '-109.2504',
      availableMargin: '193.7609',
      usedMargin: '1.0602',
      freezedMargin: '0.0000',
    };
    const get = vi.fn().mockResolvedValue({ data: { code: 0, msg: '', data: [balance] } });
    exchange.xhr = { get } as any;

    await expect(exchange.getAccount()).resolves.toEqual({ userId: '116***295' });
    await expect(exchange.validateAccount()).resolves.toEqual({
      error: '',
      balance: { total: 194.8212, free: 193.7609, used: 1.0602, upnl: 1.9219 },
    });
    await expect(exchange.fetchBalance()).resolves.toEqual({
      total: 194.8212,
      free: 193.7609,
      used: 1.0602,
      upnl: 1.9219,
      info: balance,
    });
    expect(get).toHaveBeenCalledTimes(3);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.BALANCE);
    expect(ENDPOINTS.BALANCE).toBe('/openApi/swap/v3/user/balance');
  });

  it('dispatches a public frame only to its exact topic and records only that topic', async () => {
    const parent = {
      isDisposed: false,
      options: { testnet: false },
      store: { tickersById: {} },
      emitter: { emit: vi.fn() },
      log: vi.fn(),
    } as any;
    const websocket = new BingxPublicWebsocket(parent);
    const matched = vi.fn();
    const unrelated = vi.fn();
    websocket.messageHandlers = {
      'market.kline.BTC-USDT.1min.trade.utc+8': [matched],
      'market.depth.ETH-USDT.step0.level100': [unrelated],
    };
    const recordStats = vi.spyOn(websocket as any, 'recordMessageHandlerStats');

    await websocket.onMessage({
      data: JSON.stringify({ dataType: 'market.kline.BTC-USDT.1min.trade.utc+8', data: { value: 1 } }),
    } as MessageEvent);

    expect(matched).toHaveBeenCalledOnce();
    expect(unrelated).not.toHaveBeenCalled();
    expect(recordStats).toHaveBeenCalledOnce();
    expect(recordStats).toHaveBeenCalledWith('market.kline.BTC-USDT.1min.trade.utc+8');
  });

  it('continues individual cancels and marks only confirmed or already-absent orders canceled', async () => {
    const exchange = createExchange();
    const deleteOrder = vi
      .fn()
      .mockResolvedValueOnce({ data: { code: 0, msg: '' } })
      .mockResolvedValueOnce({ data: { code: 80016, msg: 'order not exist' } })
      .mockResolvedValueOnce({ data: { code: 100001, msg: 'cancel rejected' } })
      .mockRejectedValueOnce(new Error('network down'));
    exchange.xhr = { delete: deleteOrder } as any;
    const updateStatuses = vi.spyOn(exchange.store, 'updateOrdersStatus');
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);

    await expect(exchange.cancelOrders(['ok', 'gone', 'rejected', 'network'].map(order))).resolves.toBeUndefined();

    expect(deleteOrder).toHaveBeenCalledTimes(4);
    expect(updateStatuses).toHaveBeenCalledOnce();
    expect(updateStatuses).toHaveBeenCalledWith([{ id: 'ok' }, { id: 'gone' }], OrderStatus.Canceled);
  });

  it('drops cumulative fill dedupe state as soon as an order reaches FILLED', () => {
    const exchange = createExchange();
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    const websocket = exchange.privateWebsocket;
    const emitted = vi.spyOn(exchange.emitter, 'emit');
    (websocket as any).emittedFillQtyByOrderId.set('123', 0.5);

    websocket.handleOrderEvents([
      {
        e: 'ORDER_TRADE_UPDATE',
        E: 1,
        o: {
          s: 'BTC-USDT',
          i: '123',
          S: 'SELL',
          o: 'LIMIT',
          X: 'FILLED',
          q: '1',
          z: '1',
          l: '0.5',
          p: '50000',
          ap: '50000',
          R: true,
        },
      } as any,
    ]);

    expect((websocket as any).emittedFillQtyByOrderId.has('123')).toBe(false);
    expect(emitted).toHaveBeenCalledWith(
      'fill',
      expect.objectContaining({ id: '123', symbol: 'BTCUSDT', amount: 1 }),
    );
  });

  it('formats documented trailing stops from the internal symbol and exposes the capability', async () => {
    const exchange = createExchange();
    exchange.store.update({
      markets: [
        {
          id: 'BTC-USDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
        },
      ],
      tickers: [
        {
          id: 'BTC-USDT',
          symbol: 'BTCUSDT',
          bid: 49_999,
          ask: 50_001,
          last: 50_000,
          mark: 50_000,
          index: 50_000,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        },
      ],
      positions: [
        {
          id: 'BTCUSDT-short',
          symbol: 'BTCUSDT',
          side: PositionSide.Short,
          entryPrice: 50_000,
          notional: 50_000,
          leverage: 10,
          marginType: MarginType.Cross,
          positionMode: PositionMode.Hedged,
          unrealizedPnl: 0,
          contracts: 1,
          liquidationPrice: 60_000,
        },
      ],
    });

    const payloads = await (exchange as any).formatCreateTrailingStopLossOrder({
      symbol: 'BTCUSDT',
      type: OrderType.TrailingStopLoss,
      side: OrderSide.Buy,
      amount: 1,
      price: 51_000,
    });

    expect(payloads).toEqual([
      {
        symbol: 'BTC-USDT',
        positionSide: 'SHORT',
        side: 'BUY',
        type: 'TRAILING_STOP_MARKET',
        quantity: '1',
        priceRate: '0.02',
        priceProtect: 'true',
      },
    ]);
    expect(getExchangeMetadata('bingx').supportsTrailingStopLoss).toBe(true);
  });

  it('keeps quantity stops partial and sends closePosition only for true full-position stops', async () => {
    const exchange = createExchange();
    exchange.store.update({
      markets: [
        {
          id: 'BTC-USDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
        },
      ],
      positions: [
        {
          id: 'BTCUSDT-long',
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          entryPrice: 50_000,
          notional: 50_000,
          leverage: 10,
          marginType: MarginType.Cross,
          positionMode: PositionMode.Hedged,
          unrealizedPnl: 0,
          contracts: 1,
          liquidationPrice: 40_000,
        },
      ],
    });

    const [partial] = await (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 0.25,
      price: 49_000,
      reduceOnly: true,
    });
    const [full] = await (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 0,
      price: 49_000,
      reduceOnly: true,
      close: true,
    });

    expect(partial).toMatchObject({ symbol: 'BTC-USDT', quantity: 0.25 });
    expect(partial).not.toHaveProperty('closePosition');
    expect(full).toMatchObject({ symbol: 'BTC-USDT', closePosition: 'true' });
    expect(full).not.toHaveProperty('quantity');
    expect(full).not.toHaveProperty('reduceOnly');
    expect(getExchangeMetadata('bingx').supportsPartialStops).toBe(true);
    expect(getExchangeMetadata('bingx').supportsInfinityStops).toBe(true);
  });

  it('retries failed listen-key refreshes and recovers expired streams with a fresh socket', async () => {
    vi.useFakeTimers();
    try {
      const exchange = createExchange();
      const post = vi
        .fn()
        .mockResolvedValueOnce({ data: { listenKey: 'first-key' } })
        .mockResolvedValueOnce({ data: { listenKey: 'second-key' } });
      const put = vi.fn().mockRejectedValueOnce(new Error('temporary tunnel failure')).mockResolvedValue({ data: {} });
      exchange.xhr = { post, put } as any;
      vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

      const firstSocket = { addEventListener: vi.fn(), removeEventListener: vi.fn(), close: vi.fn() };
      const secondSocket = { addEventListener: vi.fn(), removeEventListener: vi.fn(), close: vi.fn() };
      const createWebsocket = vi
        .spyOn(exchange.privateWebsocket as any, 'createWebsocket')
        .mockReturnValueOnce(firstSocket)
        .mockReturnValueOnce(secondSocket);

      await exchange.privateWebsocket.connectAndSubscribe();
      exchange.privateWebsocket.onOpen();
      await vi.advanceTimersByTimeAsync(30 * 60 * 1000);

      expect(put).toHaveBeenCalledOnce();
      await vi.advanceTimersByTimeAsync(30 * 1000);
      expect(put).toHaveBeenCalledTimes(2);

      await exchange.privateWebsocket.onMessage({
        data: JSON.stringify({ e: 'listenKeyExpired', listenKey: 'first-key' }),
      } as MessageEvent);

      expect(firstSocket.close).toHaveBeenCalledOnce();
      expect(firstSocket.removeEventListener).toHaveBeenCalledWith('close', (exchange.privateWebsocket as any).onClose);
      expect(post).toHaveBeenCalledTimes(2);
      expect(createWebsocket).toHaveBeenNthCalledWith(
        2,
        'wss://open-api-swap.bingx.com/swap-market?listenKey=second-key',
      );

      exchange.privateWebsocket.onOpen();
      exchange.privateWebsocket.dispose();
      expect(secondSocket.close).toHaveBeenCalledOnce();
      expect((exchange.privateWebsocket as any).listenKeyRefreshTimer).toBeUndefined();
    } finally {
      vi.useRealTimers();
    }
  });

});
