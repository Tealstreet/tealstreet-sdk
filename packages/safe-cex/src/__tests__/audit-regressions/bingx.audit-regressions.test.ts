import { describe, expect, it, vi } from 'vitest';

import { BingxExchange } from '../reference/bingx/bingx.exchange';
import { ENDPOINTS } from '../reference/bingx/bingx.types';
import { BingxPublicWebsocket } from '../reference/bingx/bingx.ws-public';
import { DefaultStore } from '../../store/store.base';
import { LogSeverity, MarginType, Order, OrderSide, OrderStatus, OrderType, PositionMode, PositionSide } from '../../types';
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

  it('dispatches public frames only by exact dataType while preserving control frames', async () => {
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
    const send = vi.spyOn(websocket, 'send').mockImplementation(() => undefined);
    const handlePongEvent = vi.spyOn(websocket, 'handlePongEvent').mockImplementation(() => undefined);

    await websocket.onMessage({
      data: JSON.stringify({ dataType: 'market.kline.BTC-USDT.1min.trade.utc+8', data: { value: 1 } }),
    } as MessageEvent);
    await websocket.onMessage({
      data: JSON.stringify({ dataType: 'market.kline.ETH-USDT.1min.trade.utc+8', data: { value: 2 } }),
    } as MessageEvent);
    await websocket.onMessage({ data: 'Ping' } as MessageEvent);
    await websocket.onMessage({ data: JSON.stringify({ ping: 'server-ping' }) } as MessageEvent);
    await websocket.onMessage({ data: 'Pong' } as MessageEvent);
    await websocket.onMessage({ data: JSON.stringify({ pong: 'server-pong' }) } as MessageEvent);

    expect(matched).toHaveBeenCalledOnce();
    expect(unrelated).not.toHaveBeenCalled();
    expect(recordStats).toHaveBeenCalledOnce();
    expect(recordStats).toHaveBeenCalledWith('market.kline.BTC-USDT.1min.trade.utc+8');
    expect(send).toHaveBeenCalledTimes(2);
    expect(send).toHaveBeenNthCalledWith(1, 'Pong');
    expect(send).toHaveBeenNthCalledWith(2, 'Pong');
    expect(handlePongEvent).toHaveBeenCalledTimes(2);
  });

  it('derives REST ticker touch prices from last price instead of BingX askPrice', () => {
    const exchange = createExchange();
    exchange.store.update({
      markets: [
        {
          id: 'PUMP-BTC',
          symbol: 'PUMPBTC',
          base: 'PUMP',
          quote: 'BTC',
          active: true,
          precision: { amount: 1, price: 0.0001 },
          limits: { amount: { min: 1, max: Infinity }, leverage: { min: 1, max: 100 } },
        },
      ],
    });

    const [ticker] = exchange.formatTickers({
      dailys: [
        {
          symbol: 'PUMP-BTC',
          bidPrice: '0.2403',
          askPrice: '0.7702',
          lastPrice: '0.2408',
          priceChangePercent: '-0.67',
          volume: '4360000',
          quoteVolume: '1049.888',
        },
      ],
      prices: [
        {
          symbol: 'PUMP-BTC',
          markPrice: '0.2408',
          indexPrice: '0.2407',
          lastFundingRate: '0.01',
          nextFundingTime: '1800000000000',
        },
      ],
    });

    expect(ticker).toMatchObject({
      symbol: 'PUMPBTC',
      last: 0.2408,
      bid: 0.2407,
      ask: 0.2409,
    });
  });

  it('does not keep stale startup ask prices after live public ticker updates', async () => {
    const exchange = createExchange();
    exchange.store.update({
      markets: [
        {
          id: 'OG-USDT',
          symbol: 'OGUSDT',
          base: 'OG',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.0001 },
          limits: { amount: { min: 1, max: Infinity }, leverage: { min: 1, max: 100 } },
        },
      ],
      tickers: [
        {
          id: 'OG-USDT',
          symbol: 'OGUSDT',
          bid: 0.2073,
          ask: 0.2769,
          last: 0.2077,
          mark: 0.2077,
          index: 0.2077,
          percentage: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
          openInterest: 0,
        },
      ],
    });

    await exchange.publicWebsocket.onMessage({
      data: JSON.stringify({
        dataType: 'market.contracts',
        data: {
          contracts: [
            {
              symbol: 'OG-USDT',
              tradePrice: '0.2077',
              fairPrice: '0.2076',
              indexPrice: '0.2075',
              changePercentage: '-0.67',
              fundingRate: '0.01',
              volumeF: '1000',
              valueF: '207.7',
            },
          ],
        },
      }),
    } as MessageEvent);

    expect(exchange.store.tickersBySymbol.OGUSDT).toMatchObject({
      last: 0.2077,
      bid: 0.2076,
      ask: 0.2078,
    });

    await exchange.publicWebsocket.onMessage({
      data: JSON.stringify({
        dataType: 'market.contracts',
        data: {
          contracts: [
            {
              symbol: 'OG-USDT',
              tradePrice: '0.208',
              bidPrice: '0.1000',
              askPrice: '0.7702',
              fairPrice: '0.208',
              indexPrice: '0.208',
              changePercentage: '-0.5',
              fundingRate: '0.01',
              volumeF: '1001',
              valueF: '208',
            },
          ],
        },
      }),
    } as MessageEvent);

    expect(exchange.store.tickersBySymbol.OGUSDT).toMatchObject({
      last: 0.208,
      bid: 0.2079,
      ask: 0.2081,
    });
  });

  it('returns no candles instead of emitting when the BingX kline response has no array payload', async () => {
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
    });
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({ data: { code: 0, msg: '', data: undefined } }),
    } as any;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    const log = vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    await expect(exchange.fetchOHLCV({ symbol: 'BTCUSDT', interval: '1m', limit: 10 })).resolves.toEqual([]);

    expect(emitAndLogError).not.toHaveBeenCalled();
    expect(log).toHaveBeenCalledWith('BingX kline response did not include candles for BTCUSDT', LogSeverity.Warning);
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

  it('keeps live WORKING limit orders from private WS visible as open orders', () => {
    const exchange = createExchange();

    exchange.privateWebsocket.handleOrderEvents([
      {
        e: 'ORDER_TRADE_UPDATE',
        E: 1_786_326_437_891,
        o: {
          s: 'ARB-USDT',
          c: 'tso7inqbxfbxoo23j612zor',
          i: '2086630434376200192',
          S: 'BUY',
          o: 'LIMIT',
          q: '26.00000000',
          p: '0.07725000',
          sp: '0.00000000',
          ap: '0.00000000',
          x: 'TRADE',
          X: 'WORKING',
          N: 'USDT',
          n: '0.00000000',
          T: 0,
          wt: '',
          ps: 'LONG',
          rp: '0.00000000',
          z: '0.00000000',
          sg: 'false',
          ti: 0,
          ro: false,
        },
      } as any,
    ]);

    expect(exchange.store.orders).toEqual([
      expect.objectContaining({
        id: '2086630434376200192',
        status: OrderStatus.Open,
        symbol: 'ARBUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        price: 0.07725,
        amount: 26,
        remaining: 26,
        reduceOnly: false,
      }),
    ]);
  });

  it('maps BingX open-order rows with PENDING status and quantity fields', async () => {
    const exchange = createExchange();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          code: 0,
          msg: '',
          data: {
            orders: [
              {
                orderId: '2086630434376200192',
                orderID: '2086630434376200192',
                symbol: 'ARB-USDT',
                positionSide: 'LONG',
                side: 'BUY',
                type: 'LIMIT',
                price: 0.07725,
                quantity: 26,
                timeInForce: 'PostOnly',
                reduceOnly: false,
                status: 'PENDING',
                executedQty: '0.0',
              },
            ],
          },
        },
      }),
    } as any;

    await expect(exchange.fetchOrders()).resolves.toEqual([
      expect.objectContaining({
        id: '2086630434376200192',
        status: OrderStatus.Open,
        symbol: 'ARBUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        price: 0.07725,
        amount: 26,
        remaining: 26,
        reduceOnly: false,
      }),
    ]);
  });

  it('does not reclassify raw stop-loss rows when mark price is invalid', async () => {
    const exchange = createExchange();
    exchange.store.update({
      tickers: [
        {
          id: 'ARB-USDT',
          symbol: 'ARBUSDT',
          bid: 0,
          ask: 0,
          last: 0,
          mark: 0,
          index: 0,
          percentage: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
          openInterest: 0,
        },
      ],
    });
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          code: 0,
          msg: '',
          data: {
            orders: [
              {
                orderId: '2086648321078292480',
                symbol: 'ARB-USDT',
                positionSide: 'LONG',
                side: 'SELL',
                type: 'STOP_MARKET',
                price: '0.00000',
                stopPrice: '0.07698',
                origQty: '27.0',
                executedQty: '0.0',
                status: 'NEW',
                workingType: 'MARK_PRICE',
                reduceOnly: true,
              },
            ],
          },
        },
      }),
    } as any;

    await expect(exchange.fetchOrders()).resolves.toEqual([
      expect.objectContaining({
        id: '2086648321078292480',
        type: OrderType.StopLoss,
        price: 0.07698,
      }),
    ]);
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

  it('assigns distinct bounded clientOrderId values to every ordinary MARKET and LIMIT lot only', async () => {
    const exchange = createExchange();
    const createClientOrderId = vi.spyOn(exchange as any, 'createClientOrderId');
    createClientOrderId
      .mockReturnValueOnce('tsmarketlot1')
      .mockReturnValueOnce('tsmarketlot2')
      .mockReturnValueOnce('tslimitlot1')
      .mockReturnValueOnce('tslimitlot2');
    exchange.store.update({
      markets: [
        {
          id: 'BTC-USDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 1 }, leverage: { min: 1, max: 100 } },
        },
      ],
    });
    (exchange as any)._accountPositionMode = PositionMode.Hedged;

    const marketPayloads = await (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 2,
    });
    const limitPayloads = await (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 2,
      price: 50_000,
    });
    const payloads = [...marketPayloads, ...limitPayloads];
    const clientOrderIds = payloads.map((payload) => payload.clientOrderId);

    expect(payloads).toHaveLength(4);
    expect(clientOrderIds).toEqual(['tsmarketlot1', 'tsmarketlot2', 'tslimitlot1', 'tslimitlot2']);
    expect(createClientOrderId).toHaveBeenCalledTimes(4);
    expect(payloads.every((payload) => !('clientOrderID' in payload))).toBe(true);

    const generatedId = (createExchange() as any).createClientOrderId();
    expect(generatedId).toMatch(/^ts[a-z0-9]*$/);
    expect(generatedId.length).toBeGreaterThanOrEqual(2);
    expect(generatedId.length).toBeLessThanOrEqual(23);
  });

  it('keeps clientOrderId off standalone, attached, and trailing trigger payloads', async () => {
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
    (exchange as any)._accountPositionMode = PositionMode.Hedged;

    const [parent, attachedStop, attachedTakeProfit] = await (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 50_000,
      stopLoss: 49_000,
      takeProfit: 51_000,
    });
    const [standaloneStop] = await (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      side: OrderSide.Buy,
      amount: 1,
      price: 51_000,
    });
    const [trailingStop] = await (exchange as any).formatCreateTrailingStopLossOrder({
      symbol: 'BTCUSDT',
      type: OrderType.TrailingStopLoss,
      side: OrderSide.Buy,
      amount: 1,
      price: 51_000,
    });

    expect(parent.clientOrderId).toMatch(/^ts[a-z0-9]+$/);
    for (const triggerPayload of [attachedStop, attachedTakeProfit, standaloneStop, trailingStop]) {
      expect(triggerPayload).not.toHaveProperty('clientOrderId');
      expect(triggerPayload).not.toHaveProperty('clientOrderID');
    }
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
