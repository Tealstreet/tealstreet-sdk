import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { KrakenExchange } from '../reference/kraken/kraken.exchange';
import { createAPI as createKrakenAPI } from '../reference/kraken/kraken.api';
import { ENDPOINTS } from '../reference/kraken/kraken.types';
import { KrakenSpotExchange } from '../reference/kraken/krakenspot.exchange';
import { ENDPOINTS as SPOT_ENDPOINTS, INTERVAL } from '../reference/kraken/krakenspot.types';
import { KrakenSpotPrivateWebsocket } from '../reference/kraken/krakenspot.ws-private';
import { exchangeMetadata } from '../../exchangeMetadata';
import { DefaultStore } from '../../store/store.base';
import {
  MarginType,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../types';

const FUTURES_MARKET = {
  id: 'BTC/USD:USD',
  symbol: 'PF_XBTUSD',
  prettySymbol: 'XBTUSD',
  base: 'BTC',
  quote: 'USD',
  active: true,
  precision: { amount: 1, price: 0.5 },
  limits: { amount: { min: 1, max: 1_000_000 }, leverage: { min: 1, max: 50 } },
  contractSize: 1,
  isSpot: false,
};

const SPOT_MARKET = {
  id: 'BTC/USD',
  symbol: 'XBT/USD',
  prettySymbol: 'BTCUSD',
  base: 'BTC',
  quote: 'USD',
  active: true,
  precision: { amount: 0.00000001, price: 0.1 },
  limits: { amount: { min: 0.0001, max: 1_000_000 }, leverage: { min: 1, max: 1 } },
  contractSize: 1,
  isSpot: true,
  info: { pairName: 'XXBTZUSD', altname: 'XBTUSD' },
};

const createFuturesExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [FUTURES_MARKET],
    tickers: [
      {
        id: FUTURES_MARKET.id,
        symbol: FUTURES_MARKET.symbol,
        bid: 49_999,
        ask: 50_001,
        last: 50_000,
        mark: 50_000,
        index: 50_000,
        percentage: 0,
        fundingRate: 0,
        fundingTime: 0,
        volume: 0,
        quoteVolume: 0,
        openInterest: 0,
      },
    ],
  });
  const exchange = new KrakenExchange('kraken', { key: 'key', secret: 'c2VjcmV0' }, store);
  return { exchange, store };
};

const createSpotExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [SPOT_MARKET],
    tickers: [
      {
        id: SPOT_MARKET.id,
        symbol: SPOT_MARKET.symbol,
        bid: 49_999,
        ask: 50_001,
        last: 50_000,
        mark: 50_000,
        index: 50_000,
        percentage: 0,
        fundingRate: 0,
        fundingTime: 0,
        volume: 0,
        quoteVolume: 0,
        openInterest: 0,
      },
    ],
  });
  const exchange = new KrakenSpotExchange('krakenspot', { key: 'key', secret: 'c2VjcmV0' }, store);
  return { exchange, store };
};

describe('Kraken remaining July audit regressions', () => {
  it('places a futures trailing stop with a native quote-currency deviation', async () => {
    const { exchange } = createFuturesExchange();
    const post = vi.fn().mockResolvedValue({
      data: { result: 'success', sendStatus: { status: 'placed', order_id: 'trailing-id' } },
    });
    exchange.unlimitedXHR = { post } as unknown as Axios;

    await expect(
      exchange.placeOrder({
        symbol: 'PF_XBTUSD',
        type: OrderType.TrailingStopLoss,
        side: OrderSide.Sell,
        price: 48_000,
        amount: 2,
        slTriggerType: OrderStopTriggerType.MarkPrice,
      }),
    ).resolves.toEqual(['trailing-id']);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.ORDER,
      expect.objectContaining({
        orderType: 'trailing_stop',
        size: 2,
        side: 'sell',
        trailingStopMaxDeviation: 2_000,
        trailingStopDeviationUnit: 'QUOTE_CURRENCY',
        triggerSignal: 'mark',
        reduceOnly: true,
      }),
    );
    expect(exchangeMetadata.kraken.supportsTrailingStopLoss).toBe(true);
  });

  it('rejects a zero-distance futures trailing stop before REST placement', async () => {
    const { exchange } = createFuturesExchange();
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    const post = vi.fn();
    exchange.unlimitedXHR = { post } as unknown as Axios;

    await expect(
      exchange.placeOrder({
        symbol: 'PF_XBTUSD',
        type: OrderType.TrailingStopLoss,
        side: OrderSide.Sell,
        price: 50_000,
        amount: 2,
      }),
    ).rejects.toThrow('must differ from the last price');

    expect(post).not.toHaveBeenCalled();
  });

  it('rejects futures trailing-stop edits before canceling or writing', async () => {
    const { exchange } = createFuturesExchange();
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    const cancelOrders = vi.spyOn(exchange, 'cancelOrders').mockResolvedValue();
    const post = vi.fn();
    exchange.unlimitedXHR = { post } as unknown as Axios;

    await expect(
      exchange.updateOrder({
        order: {
          id: 'trailing-id',
          status: OrderStatus.Open,
          symbol: 'PF_XBTUSD',
          type: OrderType.TrailingStopLoss,
          side: OrderSide.Sell,
          price: 48_000,
          amount: 2,
          filled: 0,
          remaining: 2,
          reduceOnly: true,
          lastUpdate: 1,
          info: { trailingStopMaxDeviation: 2_000 },
        },
        update: { price: 47_000 },
      }),
    ).rejects.toThrow('recreating it would reset its trailing state');

    expect(cancelOrders).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });

  it('surfaces rejected futures limit edits from nested editStatus', async () => {
    const { exchange } = createFuturesExchange();
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    const post = vi.fn().mockResolvedValue({
      data: { result: 'success', editStatus: { status: 'invalidPrice', orderId: 'limit-id' } },
    });
    exchange.unlimitedXHR = { post } as unknown as Axios;

    await expect(
      exchange.updateOrder({
        order: {
          id: 'limit-id',
          status: OrderStatus.Open,
          symbol: 'PF_XBTUSD',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          price: 50_000,
          amount: 2,
          filled: 0,
          remaining: 2,
          reduceOnly: false,
          lastUpdate: 1,
          info: {},
        },
        update: { price: 50_000.1 },
      }),
    ).rejects.toThrow('Order update rejected: invalidPrice');
  });

  it('surfaces rejected futures cancellations from nested cancel statuses', async () => {
    const { exchange } = createFuturesExchange();
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    const post = vi.fn().mockResolvedValue({
      data: { result: 'success', cancelStatus: { status: 'notFound', order_id: 'limit-id' } },
    });
    exchange.unlimitedXHR = { post } as unknown as Axios;

    await expect(
      exchange.cancelOrders([
        {
          id: 'limit-id',
          status: OrderStatus.Open,
          symbol: 'PF_XBTUSD',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          price: 50_000,
          amount: 2,
          filled: 0,
          remaining: 2,
          reduceOnly: false,
          lastUpdate: 1,
          info: {},
        },
      ]),
    ).rejects.toThrow('Order cancellation rejected: notFound');
  });

  it('places a Spot trailing stop with Kraken\'s required relative price', async () => {
    const { exchange } = createSpotExchange();
    const post = vi.fn().mockResolvedValue({ data: { error: [], result: { txid: ['spot-trailing-id'] } } });
    exchange.unlimitedXHR = { post } as unknown as Axios;

    await expect(
      exchange.placeOrder({
        symbol: 'XBT/USD',
        type: OrderType.TrailingStopLoss,
        side: OrderSide.Sell,
        price: 48_000,
        amount: 0.1,
      }),
    ).resolves.toEqual(['spot-trailing-id']);

    expect(post).toHaveBeenCalledWith(
      SPOT_ENDPOINTS.ADD_ORDER,
      expect.objectContaining({ ordertype: 'trailing-stop', price: '+2000', type: 'sell', volume: '0.1' }),
    );
    expect(exchangeMetadata.krakenspot.supportsTrailingStopLoss).toBe(true);
  });

  it('maps the documented unrealizedPnl field and preserves WS-only position values', async () => {
    const { exchange, store } = createFuturesExchange();
    store.update({
      positions: [
        {
          id: 'PF_XBTUSD:long',
          symbol: 'PF_XBTUSD',
          side: PositionSide.Long,
          entryPrice: 48_000,
          notional: 123_456,
          leverage: 10,
          unrealizedPnl: 200,
          contracts: 2,
          liquidationPrice: 42_000,
          marginType: MarginType.Cross,
          positionMode: PositionMode.OneWay,
        },
      ],
    });
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          result: 'success',
          openPositions: [
            {
              side: 'long',
              symbol: 'PF_XBTUSD',
              price: 48_000,
              size: 2,
              unrealizedPnl: 321.5,
              pnl: -999,
              pv: -999,
              liqPrice: -999,
            },
          ],
        },
      }),
    } as unknown as Axios;

    const positions = await exchange.fetchPositions();

    expect(positions[0]).toMatchObject({
      id: 'PF_XBTUSD:long',
      unrealizedPnl: 321.5,
      notional: 123_456,
      liquidationPrice: 42_000,
      contracts: 2,
    });
    expect((exchange.xhr.get as any).mock.calls[0][0]).toBe(ENDPOINTS.POSITIONS);
  });

  it('preserves a futures protective order trigger signal across cancel-and-replace', async () => {
    const { exchange } = createFuturesExchange();
    const cancelOrders = vi.spyOn(exchange, 'cancelOrders').mockResolvedValue();
    const placeOrder = vi.spyOn(exchange, 'placeOrder').mockResolvedValue(['replacement-id']);
    const order = {
      id: 'stop-id',
      status: OrderStatus.Open,
      symbol: 'PF_XBTUSD',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      price: 45_000,
      amount: 2,
      filled: 0,
      remaining: 2,
      reduceOnly: true,
      lastUpdate: 1,
      info: { triggerSignal: 'mark' },
    };

    await expect(exchange.updateOrder({ order, update: { price: 44_000 } })).resolves.toEqual(['replacement-id']);

    expect(cancelOrders).toHaveBeenCalledWith([order]);
    expect(placeOrder).toHaveBeenCalledWith(
      expect.objectContaining({
        type: OrderType.StopLoss,
        price: 44_000,
        slTriggerType: OrderStopTriggerType.MarkPrice,
      }),
    );
  });

  it('round-trips Kraken spot trigger provenance through the internal index trigger type', async () => {
    const { exchange } = createFuturesExchange();
    const cancelOrders = vi.spyOn(exchange, 'cancelOrders').mockResolvedValue();
    const placeOrder = vi.spyOn(exchange, 'placeOrder').mockResolvedValue(['replacement-id']);
    const order = {
      id: 'spot-trigger-stop-id',
      status: OrderStatus.Open,
      symbol: 'PF_XBTUSD',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      price: 45_000,
      amount: 2,
      filled: 0,
      remaining: 2,
      reduceOnly: true,
      lastUpdate: 1,
      info: { triggerSignal: 'spot' },
    };

    await expect(exchange.updateOrder({ order, update: { price: 44_000 } })).resolves.toEqual(['replacement-id']);

    expect(cancelOrders).toHaveBeenCalledWith([order]);
    expect(placeOrder).toHaveBeenCalledWith(
      expect.objectContaining({
        type: OrderType.StopLoss,
        price: 44_000,
        slTriggerType: OrderStopTriggerType.IndexPrice,
      }),
    );
  });

  it('sends the exact documented spot trigger signal on futures protective orders', async () => {
    const { exchange } = createFuturesExchange();
    const post = vi.fn().mockResolvedValue({
      data: { result: 'success', sendStatus: { status: 'placed', order_id: 'spot-trigger-id' } },
    });
    exchange.unlimitedXHR = { post } as unknown as Axios;

    await expect(
      exchange.placeOrder({
        symbol: 'PF_XBTUSD',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        price: 44_000,
        amount: 2,
        slTriggerType: OrderStopTriggerType.IndexPrice,
      }),
    ).resolves.toEqual(['spot-trigger-id']);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.ORDER,
      expect.objectContaining({
        orderType: 'stp',
        triggerSignal: 'spot',
      }),
    );
  });

  it.each([undefined, 'unknown'])('rejects %s trigger provenance before canceling a futures stop', async (signal) => {
    const { exchange } = createFuturesExchange();
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    const cancelOrders = vi.spyOn(exchange, 'cancelOrders').mockResolvedValue();
    const placeOrder = vi.spyOn(exchange, 'placeOrder').mockResolvedValue(['replacement-id']);

    await expect(
      exchange.updateOrder({
        order: {
          id: 'ambiguous-trigger-stop-id',
          status: OrderStatus.Open,
          symbol: 'PF_XBTUSD',
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 45_000,
          amount: 2,
          filled: 0,
          remaining: 2,
          reduceOnly: true,
          lastUpdate: 1,
          info: signal === undefined ? {} : { triggerSignal: signal },
        },
        update: { price: 44_000 },
      }),
    ).rejects.toThrow('triggerSignal provenance is missing or unsupported');

    expect(cancelOrders).not.toHaveBeenCalled();
    expect(placeOrder).not.toHaveBeenCalled();
  });

  it('surfaces that a protective order was canceled when replacement fails', async () => {
    const { exchange } = createFuturesExchange();
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    vi.spyOn(exchange, 'cancelOrders').mockResolvedValue();
    vi.spyOn(exchange, 'placeOrder').mockRejectedValue(new Error('margin unavailable'));

    await expect(
      exchange.updateOrder({
        order: {
          id: 'take-profit-id',
          status: OrderStatus.Open,
          symbol: 'PF_XBTUSD',
          type: OrderType.TakeProfit,
          side: OrderSide.Sell,
          price: 55_000,
          amount: 2,
          filled: 0,
          remaining: 2,
          reduceOnly: true,
          lastUpdate: 1,
          info: { triggerSignal: 'spot' },
        },
        update: { price: 56_000 },
      }),
    ).rejects.toThrow('canceled the original take_profit_market order but failed to place its replacement');
  });

  it('never sends Kraken Spot the rejected 720-minute OHLC interval', async () => {
    const { exchange } = createSpotExchange();
    const get = vi.fn().mockResolvedValue({
      data: { result: { XXBTZUSD: [[1_784_505_600, '1', '2', '0.5', '1.5', '1.2', '10', 3]], last: 1 } },
    });
    exchange.xhr = { get } as unknown as Axios;

    await exchange.fetchOHLCV({ symbol: 'XBT/USD', interval: '12h' });

    expect(INTERVAL['12h']).toBe(240);
    expect(get).toHaveBeenCalledWith(SPOT_ENDPOINTS.OHLC, {
      params: { pair: 'XXBTZUSD', interval: 240 },
    });
  });

  it('maps XBT open orders through the AssetPairs altname', async () => {
    const { exchange } = createSpotExchange();
    exchange.xhr = {
      post: vi.fn().mockResolvedValue({
        data: {
          result: {
            open: {
              'order-id': {
                descr: { pair: 'XBTUSD', ordertype: 'limit', type: 'buy', price: '50000' },
                vol: '0.1',
                vol_exec: '0.02',
                opentm: 1_784_505_600,
              },
            },
          },
        },
      }),
    } as unknown as Axios;

    await expect(exchange.fetchOrders()).resolves.toEqual([
      expect.objectContaining({ id: 'order-id', symbol: 'XBT/USD', amount: 0.1, filled: 0.02, remaining: 0.08 }),
    ]);
  });

  it('fetches a fresh private WebSocket token on every socket open', async () => {
    const { exchange } = createSpotExchange();
    const fetchWsToken = vi.spyOn(exchange, 'fetchWsToken').mockResolvedValueOnce('fresh-one').mockResolvedValueOnce('fresh-two');
    const websocket = new KrakenSpotPrivateWebsocket(exchange);
    const send = vi.fn();
    websocket.ws = { readyState: WebSocket.OPEN, send } as unknown as WebSocket;
    websocket.setToken('stale-token');

    await websocket.onOpen();
    await websocket.onOpen();

    expect(fetchWsToken).toHaveBeenCalledTimes(2);
    const subscriptions = send.mock.calls.map(([payload]) => JSON.parse(payload));
    expect(subscriptions.map((payload) => payload.params.token)).toEqual([
      'fresh-one',
      'fresh-one',
      'fresh-two',
      'fresh-two',
    ]);
    expect(subscriptions.some((payload) => payload.params.token === 'stale-token')).toBe(false);
  });

  it('preserves Kraken response context when normalizing API errors', async () => {
    const { exchange } = createFuturesExchange();
    const semanticResponse = {
      data: { result: 'error', error: 'authenticationError' },
      status: 200,
      statusText: 'OK',
      headers: {},
      config: {},
    };
    const semanticApi = createKrakenAPI({ key: 'key', secret: 'c2VjcmV0' }, exchange);
    semanticApi.defaults.adapter = vi.fn().mockResolvedValue(semanticResponse);

    await expect(semanticApi.get(ENDPOINTS.ACCOUNT)).rejects.toMatchObject({
      message: 'Authentication failed. Please check your API key and secret.',
      response: semanticResponse,
    });

    const httpResponse = { status: 403, data: { error: 'permissionDenied' } };
    const transportError = new Error('request failed') as Error & { response?: unknown };
    transportError.response = httpResponse;
    const httpApi = createKrakenAPI({ key: 'key', secret: 'c2VjcmV0' }, exchange);
    httpApi.defaults.adapter = vi.fn().mockRejectedValue(transportError);

    await expect(httpApi.get(ENDPOINTS.ACCOUNT)).rejects.toMatchObject({
      message: expect.stringContaining('Access forbidden (403)'),
      response: httpResponse,
      cause: transportError,
    });
  });
});
