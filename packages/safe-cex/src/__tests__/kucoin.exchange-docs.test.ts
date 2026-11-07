import type { ExchangeOptions } from '../types';

import createHmac from 'create-hmac';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createAPI } from './reference/kucoin/kucoin.api';
import { KucoinExchange } from './reference/kucoin/kucoin.exchange';
import { BROKER_ID, ENDPOINTS } from './reference/kucoin/kucoin.types';
import { createKucoinSpotAPI } from './reference/kucoin/kucoinspot.api';
import { KUCOIN_SPOT_ENDPOINTS } from './reference/kucoin/kucoinspot.types';
import { KucoinPublicWebsocket } from './reference/kucoin/kucoin.ws-public';
import { DefaultStore } from '../store/store.base';
import { SIGNING_CAPTURE_FLAG } from '../utils/signingCapture';
import {
  MarginType,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../types';

const OPEN_STATE = typeof WebSocket !== 'undefined' ? WebSocket.OPEN : 1;

const createMockWs = () =>
  ({
    readyState: OPEN_STATE,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    close: vi.fn(),
    send: vi.fn(),
  }) as any;

const makeExchange = () => {
  const store = new DefaultStore();
  const exchange = new KucoinExchange(
    'kucoin' as any,
    {
      key: 'k',
      secret: 's',
      passphrase: 'p',
    } as ExchangeOptions,
    store,
  );
  return { exchange, store };
};

const getHeader = (headers: any, name: string) => headers?.get?.(name) ?? headers?.[name];

const kucoinBrokerExtra = {
  kucoinBroker: {
    partner: 'goodbroker',
    brokerKey: 'broker-secret',
    brokerName: 'goodbrokerND',
  },
};

describe('KucoinExchange current futures docs compatibility', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('does not attach KuCoin API credentials to public-data-only futures REST requests', async () => {
    const { exchange } = makeExchange();
    const adapter = vi.fn().mockResolvedValue({
      data: { code: '200000', data: [] },
      status: 200,
      statusText: 'OK',
      headers: {},
      config: {},
    });
    const api = createAPI({ publicDataOnly: true } as ExchangeOptions, exchange);
    api.defaults.adapter = adapter;

    await api.get(ENDPOINTS.MARKETS, { params: { symbol: 'XBTUSDTM' } });

    const headers = adapter.mock.calls[0][0].headers;
    expect(getHeader(headers, 'KC-API-KEY')).toBeUndefined();
    expect(getHeader(headers, 'KC-API-SIGN')).toBeUndefined();
    expect(getHeader(headers, 'KC-API-TIMESTAMP')).toBeUndefined();
    expect(getHeader(headers, 'KC-API-PASSPHRASE')).toBeUndefined();
    expect(getHeader(headers, 'KC-API-KEY-VERSION')).toBeUndefined();
  });

  it('signs private futures REST requests with the unencoded query string', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-05-02T12:00:00Z'));

    const { exchange } = makeExchange();
    const adapter = vi.fn().mockResolvedValue({
      data: { code: '200000', data: {} },
      status: 200,
      statusText: 'OK',
      headers: {},
      config: {},
    });
    const api = createAPI({ key: 'k', secret: 's', passphrase: 'p' } as ExchangeOptions, exchange);
    api.defaults.adapter = adapter;

    await api.delete('/api/v1/sub/api-key', {
      params: { apiKey: '67b3', subName: 'test', passphrase: 'abc!@#11' },
    });

    const timestamp = Date.parse('2026-05-02T12:00:00Z');
    const expectedSignature = createHmac('sha256', 's')
      .update(`${timestamp}DELETE/api/v1/sub/api-key?apiKey=67b3&subName=test&passphrase=abc!@#11`)
      .digest('base64');

    expect(getHeader(adapter.mock.calls[0][0].headers, 'KC-API-SIGN')).toBe(expectedSignature);
  });

  it('adds official signed broker headers to futures private REST requests when configured', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-05-02T12:00:00Z'));

    const { exchange } = makeExchange();
    const adapter = vi.fn().mockResolvedValue({
      data: { code: '200000', data: {} },
      status: 200,
      statusText: 'OK',
      headers: {},
      config: {},
    });
    const api = createAPI(
      { key: 'k', secret: 's', passphrase: 'p', extra: kucoinBrokerExtra } as ExchangeOptions,
      exchange,
    );
    api.defaults.adapter = adapter;

    await api.post(ENDPOINTS.PLACE_ORDER, { symbol: 'XBTUSDTM', side: 'buy', type: 'limit', size: '1' });

    const timestamp = `${Date.parse('2026-05-02T12:00:00Z')}`;
    const expectedPartnerSign = createHmac('sha256', 'broker-secret')
      .update(`${timestamp}goodbrokerk`)
      .digest('base64');
    const headers = adapter.mock.calls[0][0].headers;

    expect(getHeader(headers, 'X-CHANNEL-API-CODE')).toBe(BROKER_ID);
    expect(getHeader(headers, 'KC-API-PARTNER')).toBe('goodbroker');
    expect(getHeader(headers, 'KC-API-PARTNER-SIGN')).toBe(expectedPartnerSign);
    expect(getHeader(headers, 'KC-BROKER-NAME')).toBe('goodbrokerND');
    expect(getHeader(headers, 'KC-API-PARTNER-VERIFY')).toBe('true');
  });

  it('reuses an existing KuCoin timestamp so remote broker and user signatures match', async () => {
    const { exchange } = makeExchange();
    const adapter = vi.fn().mockResolvedValue({
      data: { code: '200000', data: {} },
      status: 200,
      statusText: 'OK',
      headers: {},
      config: {},
    });
    const api = createAPI(
      { key: 'k', secret: 's', passphrase: 'p', extra: kucoinBrokerExtra } as ExchangeOptions,
      exchange,
    );
    api.defaults.adapter = adapter;

    await api.post(
      ENDPOINTS.PLACE_ORDER,
      { symbol: 'XBTUSDTM', side: 'buy', type: 'limit', size: '1' },
      // The device's signing pass: the server's timestamp is kept for the broker signature.
      { headers: { 'KC-API-TIMESTAMP': '123456789' }, [SIGNING_CAPTURE_FLAG]: true } as any,
    );

    const headers = adapter.mock.calls[0][0].headers;
    const expectedApiSign = createHmac('sha256', 's')
      .update('123456789POST/api/v1/orders{"symbol":"XBTUSDTM","side":"buy","type":"limit","size":"1"}')
      .digest('base64');
    const expectedPartnerSign = createHmac('sha256', 'broker-secret')
      .update('123456789goodbrokerk')
      .digest('base64');

    expect(getHeader(headers, 'KC-API-TIMESTAMP')).toBe('123456789');
    expect(getHeader(headers, 'KC-API-SIGN')).toBe(expectedApiSign);
    expect(getHeader(headers, 'KC-API-PARTNER-SIGN')).toBe(expectedPartnerSign);
  });

  it('includes signed broker headers in the futures remote-signer request envelope', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-05-02T12:00:00Z'));

    const { exchange } = makeExchange();
    const sign = vi.fn().mockRejectedValue(new Error('stop after capture'));
    const api = createAPI(
      {
        key: 'k',
        secret: '',
        passphrase: 'p',
        remoteSigner: {
          sign,
          isAvailable: () => true,
          onAvailabilityChange: () => () => undefined,
          accountName: 'account-1',
        },
        extra: kucoinBrokerExtra,
      } as ExchangeOptions,
      exchange,
    );

    await expect(api.post(ENDPOINTS.PLACE_ORDER, { symbol: 'XBTUSDTM' })).rejects.toThrow('stop after capture');

    const timestamp = `${Date.parse('2026-05-02T12:00:00Z')}`;
    const headers = sign.mock.calls[0][0].config.headers;

    expect(headers['KC-API-TIMESTAMP']).toBe(timestamp);
    expect(headers['KC-API-PARTNER']).toBe('goodbroker');
    expect(headers['KC-BROKER-NAME']).toBe('goodbrokerND');
    expect(headers['KC-API-PARTNER-VERIFY']).toBe('true');
  });

  it('adds official signed broker headers to spot private REST requests when configured', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-05-02T12:00:00Z'));

    const { exchange } = makeExchange();
    const adapter = vi.fn().mockResolvedValue({
      data: { code: '200000', data: {} },
      status: 200,
      statusText: 'OK',
      headers: {},
      config: {},
    });
    const api = createKucoinSpotAPI(
      { key: 'k', secret: 's', passphrase: 'p', extra: kucoinBrokerExtra } as ExchangeOptions,
      exchange,
    );
    api.defaults.adapter = adapter;

    await api.post(KUCOIN_SPOT_ENDPOINTS.PLACE_ORDER, { symbol: 'BTC-USDT', side: 'buy', type: 'limit', size: '1' });

    const timestamp = `${Date.parse('2026-05-02T12:00:00Z')}`;
    const expectedPartnerSign = createHmac('sha256', 'broker-secret')
      .update(`${timestamp}goodbrokerk`)
      .digest('base64');
    const headers = adapter.mock.calls[0][0].headers;

    expect(getHeader(headers, 'KC-API-PARTNER')).toBe('goodbroker');
    expect(getHeader(headers, 'KC-API-PARTNER-SIGN')).toBe(expectedPartnerSign);
    expect(getHeader(headers, 'KC-BROKER-NAME')).toBe('goodbrokerND');
    expect(getHeader(headers, 'KC-API-PARTNER-VERIFY')).toBe('true');
  });

  it('maps active perpetual USDT contracts from current /contracts/active fields', async () => {
    const { exchange } = makeExchange();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          code: '200000',
          data: [
            {
              symbol: 'XBTUSDTM',
              baseCurrency: 'XBT',
              quoteCurrency: 'USDT',
              settleCurrency: 'USDT',
              status: 'Open',
              expireDate: null,
              lotSize: 1,
              tickSize: 0.1,
              maxOrderQty: 1000000,
              maxLeverage: 125,
              multiplier: 0.001,
              isInverse: false,
              isQuanto: true,
            },
            {
              symbol: 'ETHUSDTM',
              baseCurrency: 'ETH',
              quoteCurrency: 'USDT',
              settleCurrency: 'USDT',
              status: 'Open',
              expireDate: 1798761600000,
              lotSize: 1,
              tickSize: 0.01,
              maxOrderQty: 1000000,
              maxLeverage: 100,
              multiplier: 0.01,
              isInverse: false,
            },
          ],
        },
      }),
    } as any;

    const markets = await exchange.fetchMarkets();

    expect(markets).toHaveLength(1);
    expect(markets[0]).toMatchObject({
      id: 'XBTUSDTM',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 1, price: 0.1 },
      limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
      contractSize: 0.001,
      isExpiring: false,
      isInverse: false,
    });
    // KuCoin's misleading `isQuanto: true` on multiplier-linear contracts must be
    // forced off so the app doesn't run BitMEX-style quanto sizing for it.
    expect((markets[0].info as any).isQuanto).toBe(false);
  });

  it('maps /allTickers array rows through marketsById', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          code: '200000',
          data: [
            {
              symbol: 'XBTUSDTM',
              price: '67153',
              bestBidPrice: '67152.9',
              bestAskPrice: '67153.1',
              size: 21,
            },
          ],
        },
      }),
    } as any;

    const tickers = await exchange.fetchTickers();

    expect(tickers).toEqual([
      expect.objectContaining({
        id: 'XBTUSDTM',
        symbol: 'BTCUSDT',
        bid: 67152.9,
        ask: 67153.1,
        last: 67153,
        mark: 67153,
        index: 67153,
        volume: 21,
      }),
    ]);
  });

  it('uses millisecond kline query bounds and parses millisecond candle timestamps', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-05-02T12:00:00Z'));

    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });
    const get = vi.fn().mockResolvedValue({
      data: {
        data: [[1757659080000, 115328.4, 115328.4, 115284.2, 115284.2, 602, 69424.9528]],
      },
    });
    exchange.xhr = { get } as any;

    const candles = await exchange.fetchOHLCV({ symbol: 'BTCUSDT', interval: '1h', limit: 2 });

    expect(get).toHaveBeenCalledWith(ENDPOINTS.KLINE, {
      params: {
        symbol: 'XBTUSDTM',
        granularity: '60',
        from: Date.parse('2026-05-02T10:00:00Z'),
        to: Date.parse('2026-05-02T12:00:00Z'),
      },
    });
    expect(candles[0]).toMatchObject({
      timestamp: 1757659080,
      open: 115328.4,
      close: 115284.2,
      volume: 602,
    });

    vi.useRealTimers();
  });

  it('maps current position rows with uppercase margin mode and explicit position side', async () => {
    const { exchange } = makeExchange();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          code: '200000',
          data: [
            {
              symbol: 'XBTUSDTM',
              currentQty: 1,
              markPrice: 120009.57,
              avgEntryPrice: 120016.4,
              leverage: 6,
              unrealisedPnl: -0.00683,
              liquidationPrice: 100475.86,
              marginMode: 'ISOLATED',
              positionSide: 'SHORT',
            },
          ],
        },
      }),
    } as any;

    const positions = await exchange.fetchOpenPositions();

    expect(positions).toEqual([
      expect.objectContaining({
        symbol: 'BTCUSDT',
        side: PositionSide.Short,
        positionMode: PositionMode.Hedged,
        marginType: MarginType.Isolated,
      }),
    ]);
  });

  it('fetches missing-position metadata from current margin, position, and leverage endpoints', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });
    const get = vi.fn((endpoint: string) => {
      if (endpoint === ENDPOINTS.GET_MARGIN_MODE) {
        return Promise.resolve({ data: { code: '200000', data: { marginMode: 'CROSS' } } });
      }
      if (endpoint === ENDPOINTS.GET_POSITION_MODE) {
        return Promise.resolve({ data: { code: '200000', data: { positionMode: 1 } } });
      }
      if (endpoint === ENDPOINTS.POSITION_LEVERAGE) {
        return Promise.resolve({ data: { code: '200000', data: { leverage: '3' } } });
      }
      throw new Error(`Unexpected endpoint ${endpoint}`);
    });
    exchange.xhr = { get } as any;

    const metadata = await exchange.fetchPositionMetadata(['BTCUSDT']);

    expect(metadata).toEqual([
      {
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        leverage: 3,
        positionMode: PositionMode.Hedged,
        marginType: MarginType.Cross,
      },
      {
        symbol: 'BTCUSDT',
        side: PositionSide.Short,
        leverage: 3,
        positionMode: PositionMode.Hedged,
        marginType: MarginType.Cross,
      },
    ]);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.GET_MARGIN_MODE, { params: { symbol: 'XBTUSDTM' } });
    expect(get).toHaveBeenCalledWith(ENDPOINTS.GET_POSITION_MODE);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.POSITION_LEVERAGE, { params: { symbol: 'XBTUSDTM' } });
  });

  it('uses current REST endpoints and payloads for cross leverage, margin mode, and position mode', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 125 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });
    const post = vi.fn().mockResolvedValue({ data: { code: '200000', data: true } });
    exchange.xhr = { post } as any;
    vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);

    await exchange.setLeverage('BTCUSDT', 7);
    await exchange.changeMarginType(true, 'BTCUSDT');
    await exchange.changePositionMode(true);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.SET_LEVERAGE, { symbol: 'XBTUSDTM', leverage: '7' });
    expect(post).toHaveBeenCalledWith(ENDPOINTS.SET_MARGIN_MODE, { symbol: 'XBTUSDTM', marginMode: 'ISOLATED' });
    expect(post).toHaveBeenCalledWith(ENDPOINTS.SET_POSITION_MODE, { positionMode: '1' });
  });

  it('does not call the cross leverage endpoint for isolated margin', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 125 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });
    const post = vi.fn();
    exchange.xhr = { post } as any;
    vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Isolated);

    await expect(exchange.setLeverage('BTCUSDT', 7)).rejects.toThrow(/cross-margin leverage/);
    expect(post).not.toHaveBeenCalled();
  });

  it('cancels normal and stop orders from current cancel-all endpoints', async () => {
    const { exchange } = makeExchange();
    const deleteRequest = vi.fn().mockResolvedValue({ data: { code: '200000', data: {} } });
    exchange.unlimitedXHR = { delete: deleteRequest } as any;

    await exchange.cancelAllOrders();

    expect(deleteRequest).toHaveBeenCalledWith(ENDPOINTS.CANCEL_ALL_ORDERS);
    expect(deleteRequest).toHaveBeenCalledWith(ENDPOINTS.CANCEL_ALL_STOP_ORDERS);
  });

  it('uses native KuCoin contract ids for order create and cancel requests', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });
    const deleteRequest = vi.fn().mockResolvedValue({ data: { code: '200000', data: {} } });
    exchange.unlimitedXHR = { delete: deleteRequest } as any;

    const [payload] = await exchange.formatCreateOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      price: 67000,
      amount: 3,
      timeInForce: OrderTimeInForce.GoodTillCancel,
    });

    await exchange.cancelNormalOrders([
      {
        id: 'order-id',
        symbol: 'BTCUSDT',
      } as any,
    ]);
    await exchange.cancelSymbolOrders('BTCUSDT');

    expect(payload.symbol).toBe('XBTUSDTM');
    expect(deleteRequest).toHaveBeenCalledWith(`${ENDPOINTS.CANCEL_ORDERS}/order-id`, {
      params: { symbol: 'XBTUSDTM' },
    });
    expect(deleteRequest).toHaveBeenCalledWith(ENDPOINTS.CANCEL_SYMBOL_ORDERS, {
      params: { symbol: 'XBTUSDTM' },
    });
  });

  it('cancels a standalone TP/SL via /api/v1/orders (where it is placed), not /api/v1/stopOrders', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });
    const deleteRequest = vi.fn().mockResolvedValue({ data: { code: '200000', data: {} } });
    exchange.unlimitedXHR = { delete: deleteRequest } as any;

    // placeAlgoOrders posts stop-flagged TP/SL to /api/v1/orders, so the cancel
    // must hit /api/v1/orders/{id}. Routing to /api/v1/stopOrders/{id} 404s and
    // orphans the order (confirmed live).
    await exchange.cancelOrders([{ id: 'sl-1', symbol: 'BTCUSDT', type: OrderType.StopLoss } as any]);

    expect(deleteRequest).toHaveBeenCalledWith(`${ENDPOINTS.CANCEL_ORDERS}/sl-1`, {
      params: { symbol: 'XBTUSDTM' },
    });
    expect(deleteRequest).not.toHaveBeenCalledWith(ENDPOINTS.CANCEL_ALGO_ORDER.replace('{orderId}', 'sl-1'));
  });

  it('places a full-position (infinity) TP/SL via closeOrder:true with no size, never size:"0"', () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          contractSize: 0.001,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });

    // amount:0 is the app's "Full Position / infinity TP/SL" (the chart bracket
    // sends 0 when supportsInfinityStops is on). KuCoin's /api/v1/orders rejects
    // size:"0", so a full-position stop must ride the closeOrder flag with no size.
    const payload = exchange.formatAlgoOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      amount: 0,
      price: 42000,
      reduceOnly: true,
    } as any);

    expect(payload.closeOrder).toBe(true);
    expect(payload.size).toBeUndefined();
    expect(payload.stop).toBe('down'); // sell stop-loss triggers on the way down
  });

  it('places a partial TP/SL with size in native contracts and no closeOrder flag', () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          contractSize: 0.001,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });

    const payload = exchange.formatAlgoOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.TakeProfit,
      amount: 3,
      price: 90000,
      reduceOnly: true,
    } as any);

    expect(payload.size).toBe('3'); // integer native contracts
    expect(payload.closeOrder).toBeUndefined();
    expect(payload.stop).toBe('up'); // sell take-profit triggers on the way up
  });

  it('surfaces a non-200000 KuCoin rejection code instead of swallowing it (silent place failure)', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          contractSize: 0.001,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });

    // KuCoin returns HTTP 200 with a rejection code in the body. The old code only
    // read body.data.orderId, so a rejection produced no id, no throw, no toast —
    // exactly how full-position stops used to vanish.
    const post = vi.fn().mockResolvedValue({
      data: { code: '400100', msg: 'The order size cannot be 0', data: null },
    });
    exchange.unlimitedXHR = { post } as any;
    const errors: any[] = [];
    exchange.emitter.on('error', (e: any) => errors.push(e));

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Sell,
        type: OrderType.StopLoss,
        amount: 1,
        price: 42000,
        reduceOnly: true,
      } as any),
    ).rejects.toThrow(/order size cannot be 0|400100/);
    expect(errors.length).toBeGreaterThan(0);
  });

  const kucoinMarket = {
    id: 'XBTUSDTM',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    contractSize: 0.001,
    precision: { amount: 1, price: 0.1 },
    limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
  };

  it('preserves existing TP/SL orders when the stop-order book fetch fails (does not wipe the chart)', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [kucoinMarket],
      // The chart full-replaces store.orders with fetchOrders(); seed it with a
      // WS-delivered SL alongside a limit, as it would be on a live chart.
      orders: [
        {
          id: 'limit-1',
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          price: 60000,
          amount: 0.001,
          remaining: 0.001,
          status: OrderStatus.Open,
        } as any,
        {
          id: 'sl-1',
          symbol: 'BTCUSDT',
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 50000,
          amount: 0.001,
          remaining: 0.001,
          status: OrderStatus.Open,
        } as any,
      ],
    });

    // Normal book (/api/v1/orders) succeeds; stop book (/api/v1/stopOrders) FAILS.
    const get = vi.fn((endpoint: string) => {
      if (endpoint === ENDPOINTS.ORDERS) {
        return Promise.resolve({
          data: {
            code: '200000',
            data: {
              items: [
                {
                  id: 'limit-1',
                  symbol: 'XBTUSDTM',
                  type: 'limit',
                  side: 'buy',
                  price: '60000',
                  size: 1,
                  filledSize: '0',
                  stop: '',
                  status: 'open',
                },
              ],
            },
          },
        });
      }
      if (endpoint === ENDPOINTS.ALGO_ORDERS) {
        return Promise.resolve({ data: { code: '400100', msg: 'rate limited' } });
      }
      throw new Error(`unexpected endpoint ${endpoint}`);
    });
    exchange.xhr = { get } as any;

    const ids = (await exchange.fetchOrders()).map((o) => o.id);
    // Limit refreshed from the working book; the SL is PRESERVED despite the
    // stop-book failure (previously it was wiped → vanished from the chart).
    expect(ids).toContain('limit-1');
    expect(ids).toContain('sl-1');
  });

  it('preserves existing limit orders when the normal-order book fetch fails', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [kucoinMarket],
      orders: [
        {
          id: 'limit-1',
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          price: 60000,
          amount: 0.001,
          remaining: 0.001,
          status: OrderStatus.Open,
        } as any,
      ],
    });

    // Normal book FAILS; stop book succeeds (returns a fresh SL).
    const get = vi.fn((endpoint: string) => {
      if (endpoint === ENDPOINTS.ORDERS) {
        return Promise.resolve({ data: { code: '400100', msg: 'rate limited' } });
      }
      if (endpoint === ENDPOINTS.ALGO_ORDERS) {
        return Promise.resolve({
          data: {
            code: '200000',
            data: {
              items: [
                {
                  id: 'sl-1',
                  symbol: 'XBTUSDTM',
                  type: 'stop',
                  side: 'sell',
                  stop: 'down',
                  stopPrice: '50000',
                  size: 1,
                  filledSize: '0',
                  status: 'open',
                },
              ],
            },
          },
        });
      }
      throw new Error(`unexpected endpoint ${endpoint}`);
    });
    exchange.xhr = { get } as any;

    const ids = (await exchange.fetchOrders()).map((o) => o.id);
    expect(ids).toContain('limit-1'); // preserved despite normal-book failure
    expect(ids).toContain('sl-1'); // refreshed from the working stop book
  });

  it('sends post-only via the postOnly boolean, not an invalid timeInForce', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });

    const [payload] = await exchange.formatCreateOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      price: 67000,
      amount: 1,
      timeInForce: OrderTimeInForce.PostOnly,
    });

    // KuCoin Futures rejects timeInForce='POST_ONLY' ("Time in Force type
    // invalid"); post-only must ride the separate boolean with TIF left GTC.
    expect(payload.timeInForce).toBe('GTC');
    expect(payload.postOnly).toBe(true);
  });

  it('does not misclassify a normal limit order (stop: "") as a stop, preserving its price', () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          contractSize: 0.001,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });

    // KuCoin returns stop: "" for non-stop orders — must stay a Limit with its
    // real price, not get rewritten to stopPrice (0).
    const [order] = exchange.mapOrder({
      id: 'lim-1',
      symbol: 'XBTUSDTM',
      type: 'limit',
      side: 'buy',
      price: '67000',
      size: 1,
      filledSize: '0',
      stop: '',
      stopPrice: '0',
      status: 'open',
    });

    expect(order.type).toBe(OrderType.Limit);
    expect(order.price).toBe(67000);
    expect(order.amount).toBe(0.001); // 1 contract × 0.001 contractSize (base units)
  });

  it('routes attached take-profit and stop-loss orders through current st-orders fields', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
      positions: [
        {
          id: 'BTCUSDT:long',
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          entryPrice: 65000,
          notional: 65000,
          leverage: 5,
          marginType: MarginType.Isolated,
          positionMode: PositionMode.Hedged,
          unrealizedPnl: 0,
          contracts: 1,
          liquidationPrice: 0,
        },
      ],
    });
    const post = vi.fn().mockResolvedValue({ data: { code: '200000', data: { orderId: 'order-id' } } });
    exchange.unlimitedXHR = { post } as any;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      price: 67000,
      amount: 1,
      takeProfit: 70000,
      stopLoss: 64000,
      tpTriggerType: OrderStopTriggerType.MarkPrice,
    });

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ALGO_ORDER,
      expect.objectContaining({
        symbol: 'XBTUSDTM',
        side: 'buy',
        type: 'limit',
        leverage: 5,
        marginMode: 'ISOLATED',
        positionSide: 'LONG',
        triggerStopUpPrice: '70000',
        triggerStopDownPrice: '64000',
        stopPriceType: 'MP',
      }),
    );
  });

  it('places standalone futures stop orders through the normal order endpoint stop fields', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
      positions: [
        {
          id: 'BTCUSDT:long',
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          entryPrice: 65000,
          notional: 65000,
          leverage: 5,
          marginType: MarginType.Cross,
          positionMode: PositionMode.Hedged,
          unrealizedPnl: 0,
          contracts: 1,
          liquidationPrice: 0,
        },
      ],
    });
    const post = vi.fn().mockResolvedValue({ data: { code: '200000', data: { orderId: 'stop-id' } } });
    exchange.unlimitedXHR = { post } as any;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      price: 64000,
      amount: 1,
      slTriggerType: OrderStopTriggerType.IndexPrice,
    });

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ORDER,
      expect.objectContaining({
        symbol: 'XBTUSDTM',
        side: 'sell',
        type: 'market',
        stop: 'down',
        stopPrice: '64000',
        stopPriceType: 'IP',
        reduceOnly: true,
        marginMode: 'CROSS',
        positionSide: 'LONG',
      }),
    );
  });

  it('rejects trailing stop placement because it is absent from current futures REST docs', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Sell,
        type: OrderType.TrailingStopLoss,
        price: 64000,
        amount: 1,
      }),
    ).rejects.toThrow(/trailing stop placement/);
  });

  it('maps native KuCoin order symbols and canceled statuses into internal orders', () => {
    const { exchange } = makeExchange();

    const [order] = exchange.mapOrder({
      id: 'order-id',
      symbol: 'XBTUSDTM',
      status: 'cancelled',
      side: 'buy',
      type: 'limit',
      price: '67000',
      size: '1',
      filledSize: '0',
    });

    expect(order).toMatchObject({
      id: 'order-id',
      symbol: 'BTCUSDT',
      status: OrderStatus.Canceled,
    });
  });

  it('snapshot socket bulk-subscribes its shard; detail socket is on-demand only', () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });

    // Snapshot socket: bulk last/24h%/funding/volume for its interleaved shard.
    const snapshot = new KucoinPublicWebsocket(exchange, { role: 'snapshot', shardIndex: 0, shardCount: 1 });
    const snapSocket = createMockWs();
    snapshot.ws = snapSocket;
    snapshot.subscribe();
    const snapPayloads = snapSocket.send.mock.calls.map((c: any[]) => JSON.parse(String(c[0] || '{}')));
    expect(snapPayloads).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ type: 'subscribe', topic: '/contractMarket/snapshot:XBTUSDTM' }),
      ]),
    );

    // Detail socket: subscribes NOTHING in bulk (no snapshot fan-out), only
    // candles + the viewed symbol's bid/ask + mark/index on demand.
    const detail = exchange.publicWebsocket;
    const detailSocket = createMockWs();
    detail.ws = detailSocket;
    detail.subscribe();
    const unsubCandle = detail.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, vi.fn());
    unsubCandle();
    detail.subscribeActiveTicker('XBTUSDTM'); // what listenOrderBook does for the viewed symbol

    const detailPayloads = detailSocket.send.mock.calls.map((c: any[]) => JSON.parse(String(c[0] || '{}')));
    expect(detailPayloads).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ type: 'subscribe', topic: '/contractMarket/limitCandle:XBTUSDTM_1min' }),
        expect.objectContaining({ type: 'unsubscribe', topic: '/contractMarket/limitCandle:XBTUSDTM_1min' }),
        expect.objectContaining({ type: 'subscribe', topic: '/contractMarket/tickerV2:XBTUSDTM' }),
        expect.objectContaining({ type: 'subscribe', topic: '/contract/instrument:XBTUSDTM' }),
      ]),
    );
    // The detail socket must NOT bulk-subscribe snapshot for the whole market list.
    expect(detailPayloads.some((p: any) => String(p.topic).startsWith('/contractMarket/snapshot'))).toBe(false);
  });

  it('parses current futures websocket ticker and candle payloads into internal symbols', () => {
    const { exchange, store } = makeExchange();
    const ticker = {
      id: 'XBTUSDTM',
      symbol: 'BTCUSDT',
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
    };
    store.update({ tickers: [ticker] });
    const ws = exchange.publicWebsocket;
    ws.ws = createMockWs();
    const updateTicker = vi.spyOn(store, 'updateTicker');
    const candleCallback = vi.fn();

    ws.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, candleCallback);
    ws.handleTickerV2({
      data: {
        symbol: 'XBTUSDTM',
        bestBidPrice: '86454.5',
        bestAskPrice: '86454.6',
      },
    });
    ws.onMessage({
      data: JSON.stringify({
        topic: '/contractMarket/limitCandle:XBTUSDTM_1min',
        type: 'message',
        data: {
          symbol: 'XBTUSDTM',
          candles: ['1731898200', '90638.6', '90638.7', '90639.1', '90638.2', '21.0', '21'],
        },
      }),
    } as MessageEvent);

    expect(updateTicker).toHaveBeenCalledWith(
      expect.objectContaining({ symbol: 'BTCUSDT' }),
      expect.objectContaining({ bid: 86454.5, ask: 86454.6 }),
    );
    expect(candleCallback).toHaveBeenCalledWith({
      timestamp: 1731898200,
      open: 90638.6,
      close: 90638.7,
      high: 90639.1,
      low: 90638.2,
      volume: 21,
    });
  });

  it('updates last/24h%/funding/volume from the snapshot channel WITHOUT zeroing bid/ask', () => {
    const { exchange, store } = makeExchange();
    const ticker = {
      id: 'XBTUSDTM',
      symbol: 'BTCUSDT',
      bid: 86454.5,
      ask: 86454.6,
      last: 0,
      mark: 0,
      index: 0,
      percentage: 0,
      openInterest: 0,
      fundingRate: 0,
      fundingTime: 0,
      volume: 0,
      quoteVolume: 0,
    };
    store.update({ tickers: [ticker] });
    const ws = exchange.publicWebsocket;
    ws.ws = createMockWs();
    const updateTicker = vi.spyOn(store, 'updateTicker');

    // snapshot.24h carries last/funding/24h%/volume but NOT bid/ask — the
    // update must omit bid/ask so the tickerV2 values survive.
    ws.handleSnapshot({
      data: {
        symbol: 'XBTUSDTM',
        lastPrice: 72175.4,
        fundingRate: 0.000107,
        priceChgPct: -0.0238,
        volume: 2581.453,
        turnover: 188909573.1256,
      },
    });

    const update = updateTicker.mock.calls[0][1];
    expect(update.last).toBe(72175.4);
    expect(update.fundingRate).toBe(0.000107);
    expect(update.percentage).toBeCloseTo(-2.38, 6); // priceChgPct(-0.0238) * 100
    expect(update.volume).toBe(2581.453);
    expect(update.quoteVolume).toBe(188909573.1256);
    expect(update).not.toHaveProperty('bid');
    expect(update).not.toHaveProperty('ask');
  });

  it('updates mark/index from the instrument channel, resolving symbol from the topic', () => {
    const { exchange, store } = makeExchange();
    const ticker = {
      id: 'XBTUSDTM',
      symbol: 'BTCUSDT',
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
    };
    store.update({ tickers: [ticker] });
    const ws = exchange.publicWebsocket;
    ws.ws = createMockWs();
    const updateTicker = vi.spyOn(store, 'updateTicker');

    // /contract/instrument data has no symbol field — it comes from the topic.
    ws.handleInstrument({
      topic: '/contract/instrument:XBTUSDTM',
      subject: 'mark.index.price',
      data: { markPrice: 72176.3, indexPrice: 72172.81, granularity: 1000 },
    });

    expect(updateTicker).toHaveBeenCalledWith(expect.objectContaining({ symbol: 'BTCUSDT' }), {
      mark: 72176.3,
      index: 72172.81,
    });
  });

  it('seeds futures websocket orderbook from REST snapshot and applies incremental changes', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });
    const get = vi.fn().mockResolvedValue({
      data: {
        code: '200000',
        data: {
          sequence: 16,
          bids: [['100', '1']],
          asks: [['101', '2']],
        },
      },
    });
    exchange.xhr = { get } as any;
    const ws = exchange.publicWebsocket;
    const mockSocket = createMockWs();
    ws.ws = mockSocket;
    const callback = vi.fn();

    ws.listenOrderBook('BTCUSDT', callback);
    await vi.waitFor(() => expect(callback).toHaveBeenCalledTimes(1));
    ws.onMessage({
      data: JSON.stringify({
        topic: '/contractMarket/level2:XBTUSDTM',
        type: 'message',
        subject: 'level2',
        sn: 17,
        data: {
          sequence: 17,
          change: '100,buy,3',
        },
      }),
    } as MessageEvent);

    expect(get).toHaveBeenCalledWith(ENDPOINTS.ORDERBOOK, { params: { symbol: 'XBTUSDTM' } });
    expect(JSON.parse(String(mockSocket.send.mock.calls[0][0]))).toMatchObject({
      type: 'subscribe',
      topic: '/contractMarket/level2:XBTUSDTM',
    });
    expect(callback).toHaveBeenCalledTimes(2);
    expect(callback.mock.calls.at(-1)?.[0]).toMatchObject({
      bids: [expect.objectContaining({ price: 100, amount: 3 })],
      asks: [expect.objectContaining({ price: 101, amount: 2 })],
    });
  });

  it('refetches futures websocket orderbook snapshots instead of applying sequence gaps', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });
    const get = vi
      .fn()
      .mockResolvedValueOnce({
        data: {
          code: '200000',
          data: {
            sequence: 16,
            bids: [['100', '1']],
            asks: [['101', '2']],
          },
        },
      })
      .mockResolvedValueOnce({
        data: {
          code: '200000',
          data: {
            sequence: 18,
            bids: [['100', '4']],
            asks: [['101', '2']],
          },
        },
      });
    exchange.xhr = { get } as any;
    const ws = exchange.publicWebsocket;
    ws.ws = createMockWs();
    const callback = vi.fn();

    ws.listenOrderBook('BTCUSDT', callback);
    await vi.waitFor(() => expect(callback).toHaveBeenCalledTimes(1));
    ws.onMessage({
      data: JSON.stringify({
        topic: '/contractMarket/level2:XBTUSDTM',
        type: 'message',
        subject: 'level2',
        sn: 18,
        data: {
          sequence: 18,
          change: '100,buy,3',
        },
      }),
    } as MessageEvent);

    await vi.waitFor(() => expect(get).toHaveBeenCalledTimes(2));
    await vi.waitFor(() => expect(callback).toHaveBeenCalledTimes(2));
    expect(callback.mock.calls.at(-1)?.[0]).toMatchObject({
      bids: [expect.objectContaining({ price: 100, amount: 4 })],
    });
  });

  it('keeps refetching futures websocket orderbooks when a replacement snapshot is still behind a buffered gap', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      markets: [
        {
          id: 'XBTUSDTM',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });
    const get = vi
      .fn()
      .mockResolvedValueOnce({
        data: {
          code: '200000',
          data: {
            sequence: 16,
            bids: [['100', '1']],
            asks: [['101', '2']],
          },
        },
      })
      .mockResolvedValueOnce({
        data: {
          code: '200000',
          data: {
            sequence: 16,
            bids: [['100', '2']],
            asks: [['101', '2']],
          },
        },
      })
      .mockResolvedValueOnce({
        data: {
          code: '200000',
          data: {
            sequence: 18,
            bids: [['100', '4']],
            asks: [['101', '2']],
          },
        },
      });
    exchange.xhr = { get } as any;
    const ws = exchange.publicWebsocket;
    ws.ws = createMockWs();
    const callback = vi.fn();

    ws.listenOrderBook('BTCUSDT', callback);
    await vi.waitFor(() => expect(callback).toHaveBeenCalledTimes(1));
    ws.onMessage({
      data: JSON.stringify({
        topic: '/contractMarket/level2:XBTUSDTM',
        type: 'message',
        subject: 'level2',
        sn: 18,
        data: {
          sequence: 18,
          change: '100,buy,3',
        },
      }),
    } as MessageEvent);

    await vi.waitFor(() => expect(get).toHaveBeenCalledTimes(3));
    await vi.waitFor(() => expect(callback).toHaveBeenCalledTimes(2));
    expect(callback.mock.calls.at(-1)?.[0]).toMatchObject({
      bids: [expect.objectContaining({ price: 100, amount: 4 })],
    });
  });

  it('subscribes current futures private websocket channels without an extra login frame', () => {
    const { exchange } = makeExchange();
    const ws = exchange.privateWebsocket;
    const mockSocket = createMockWs();
    ws.ws = mockSocket;
    vi.spyOn(exchange, 'fetchAndSetPrivateSnapshot').mockResolvedValue(undefined);

    ws.onOpen();

    const payloads = mockSocket.send.mock.calls.map((call: any[]) => JSON.parse(String(call[0] || '{}')));
    expect(payloads.map((payload: Record<string, any>) => payload.topic)).toEqual(
      expect.arrayContaining([
        '/contractMarket/tradeOrders',
        '/contractMarket/advancedOrders',
        '/contract/positionAll',
        '/contractAccount/wallet',
        '/contract/marginMode',
        '/contract/crossLeverage',
      ]),
    );
    expect(payloads).not.toEqual(expect.arrayContaining([expect.objectContaining({ type: 'login' })]));
  });

  it('debounces private snapshot refreshes for current account websocket topics', async () => {
    vi.useFakeTimers();
    const { exchange } = makeExchange();
    const ws = exchange.privateWebsocket;
    const refresh = vi.spyOn(exchange, 'fetchAndSetPrivateSnapshot').mockResolvedValue(undefined);

    ws.onMessage({
      data: JSON.stringify({
        topic: '/contract/positionAll',
        type: 'message',
        data: { symbol: 'XBTUSDTM' },
      }),
    } as MessageEvent);
    ws.onMessage({
      data: JSON.stringify({
        topic: '/contractAccount/wallet',
        type: 'message',
        data: { currency: 'USDT' },
      }),
    } as MessageEvent);
    ws.onMessage({
      data: JSON.stringify({
        topic: '/contract/marginMode',
        type: 'message',
        data: { XBTUSDTM: 'CROSS' },
      }),
    } as MessageEvent);
    ws.onMessage({
      data: JSON.stringify({
        topic: '/contract/crossLeverage',
        type: 'message',
        data: { XBTUSDTM: { leverage: '51' } },
      }),
    } as MessageEvent);

    expect(refresh).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(499);
    expect(refresh).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it('handles current futures private order event types', () => {
    const { exchange, store } = makeExchange();
    const ws = exchange.privateWebsocket;
    const updateStatus = vi.spyOn(store, 'updateOrderStatus');
    const addOrUpdateOrder = vi.spyOn(store, 'addOrUpdateOrder');
    const emit = vi.spyOn(exchange.emitter, 'emit');

    ws.handleOrderTopic({
      data: {
        symbol: 'XBTUSDTM',
        orderId: 'cancel-id',
        type: 'canceled',
        status: 'done',
        side: 'buy',
        orderType: 'limit',
        price: '90000',
        size: '1',
        filledSize: '0',
        remainSize: '0',
      },
    });
    ws.handleOrderTopic({
      data: {
        symbol: 'XBTUSDTM',
        orderId: 'match-id',
        type: 'match',
        status: 'match',
        side: 'buy',
        orderType: 'limit',
        price: '91670',
        matchPrice: '91670',
        size: '1',
        filledSize: '1',
        matchSize: '1',
        remainSize: '0',
      },
    });

    // Terminal `filled` frame: cumulative filledSize, NO matchSize → must NOT
    // emit a second fill (the match frame already did), and closes the order.
    ws.handleOrderTopic({
      data: {
        symbol: 'XBTUSDTM',
        orderId: 'match-id',
        type: 'filled',
        status: 'done',
        side: 'buy',
        orderType: 'limit',
        price: '91670',
        size: '1',
        filledSize: '1',
        remainSize: '0',
      },
    });

    expect(updateStatus).toHaveBeenCalledWith(expect.objectContaining({ id: 'cancel-id' }), OrderStatus.Canceled);
    expect(emit).toHaveBeenCalledWith(
      'fill',
      expect.objectContaining({ id: 'match-id', symbol: 'BTCUSDT', price: 91670, amount: 1 }),
    );
    expect(addOrUpdateOrder).toHaveBeenCalledWith(expect.objectContaining({ id: 'match-id', symbol: 'BTCUSDT' }));
    // exactly one fill across the match + filled pair (no double-fire)
    expect((emit as any).mock.calls.filter((c: any[]) => c[0] === 'fill')).toHaveLength(1);
    expect(updateStatus).toHaveBeenCalledWith(expect.objectContaining({ id: 'match-id' }), OrderStatus.Closed);
  });

  it('maps current futures stop order websocket payloads', () => {
    const { exchange } = makeExchange();

    const [order] = exchange.mapOrder({
      symbol: 'XBTUSDTM',
      orderId: 'stop-id',
      type: 'open',
      orderType: 'stop',
      stop: 'down',
      stopPrice: '1000',
      orderPrice: '900',
      status: 'open',
      side: 'buy',
      size: '1',
      filledSize: '0',
    });

    expect(order).toMatchObject({
      id: 'stop-id',
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      price: 1000,
    });
  });
});
