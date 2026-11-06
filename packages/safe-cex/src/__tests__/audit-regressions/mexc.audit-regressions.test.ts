import { afterEach, describe, expect, it, vi } from 'vitest';

import { getExchangeMetadata } from '../../exchangeMetadata';
import { signMexcRequest, signMexcWsLogin, serializeMexcParams } from '../reference/mexc/mexc.api';
import { MexcExchange } from '../reference/mexc/mexc.exchange';
import { ENDPOINTS } from '../reference/mexc/mexc.types';
import { virtualClock } from '../../utils/virtualClock';
import { setWsDiagnosticsEnabled } from '../../utils/wsDiagnostics';
import {
  isTradableMexcMarket,
  nativeVolumeToAmount,
  normalizeBalance,
  normalizeExecution,
  normalizeKlineResponse,
  normalizeMarket,
  normalizeOrder,
  normalizePosition,
  normalizeTicker,
  toMexcSymbol,
} from '../reference/mexc/mexc.utils';
import { DefaultStore } from '../../store/store.base';
import {
  MarginType,
  Order,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../types';

const createExchange = () => new MexcExchange('mexc', { key: 'key', secret: 'secret' }, new DefaultStore());

const market = normalizeMarket({
  symbol: 'BTC_USDT',
  baseCoin: 'BTC',
  quoteCoin: 'USDT',
  settleCoin: 'USDT',
  state: 0,
  apiAllowed: true,
  contractSize: 0.0001,
  priceScale: 1,
  priceUnit: 0.1,
  volUnit: 1,
  minVol: 1,
  maxVol: 400000,
  minLeverage: 1,
  maxLeverage: 500,
  makerFeeRate: 0,
  takerFeeRate: 0.0002,
});

describe('MEXC futures audit regressions', () => {
  afterEach(() => setWsDiagnosticsEnabled(false));

  it('advertises partial TP/SL support for explicit-volume plan orders', () => {
    expect(getExchangeMetadata('mexc').supportsPartialStops).toBe(true);
  });

  it('advertises infinity TP/SL support through position stop orders', () => {
    expect(getExchangeMetadata('mexc').supportsInfinityStops).toBe(true);
  });

  it('signs GET query strings and POST raw JSON bodies exactly', () => {
    expect(serializeMexcParams({ timestamp: 1700000000000, symbol: 'BTC_USDT' })).toBe(
      'symbol=BTC_USDT&timestamp=1700000000000',
    );
    expect(
      signMexcRequest('testSecret', 'testKey', '1700000000000', 'symbol=BTC_USDT&timestamp=1700000000000'),
    ).toBe('9ca21bd13c47b3866ba4ea2cf168cfad0b69195592c36bc3b6f84b4fa0ee882d');
    expect(
      signMexcRequest(
        'testSecret',
        'testKey',
        '1700000000000',
        '{"symbol":"BTC_USDT","price":"50000","vol":1,"side":1,"type":1}',
      ),
    ).toBe('0e2a71dd46ac50921b19a19b70b5b73a5194c9e0ab8c250c7d70f1de07986d04');
    expect(signMexcWsLogin('testSecret', 'testKey', '1700000000000')).toBe(
      'a896f028b429ff01a8b37a5d60f94bd72a97e0a3fae1f49a3870a8f8ae945904',
    );
  });

  it('emits public WebSocket diagnostics for request drawer subscriptions', () => {
    setWsDiagnosticsEnabled(true);
    const exchange = createExchange();
    const send = vi.fn();
    const sendLogs: string[] = [];
    exchange.store.update({ markets: [market] });
    exchange.publicWebsocket.ws = { readyState: 1, send } as any;
    exchange.on('grafana-pws-sub', (message: string) => sendLogs.push(message));

    const dispose = exchange.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, vi.fn());

    expect(send).toHaveBeenCalledWith(JSON.stringify({ method: 'sub.kline', param: { symbol: 'BTC_USDT', interval: 'Min1' } }));
    expect(sendLogs).toEqual([
      '[mexc public] {"method":"sub.kline","param":{"symbol":"BTC_USDT","interval":"Min1"}}',
    ]);
    dispose();
  });

  it('emits public WebSocket receive diagnostics without flooding ticker or depth pushes', () => {
    setWsDiagnosticsEnabled(true);
    const exchange = createExchange();
    const recordStats = vi.spyOn(exchange.publicWebsocket as any, 'recordMessageHandlerStats');
    const receiveLogs: string[] = [];
    exchange.on('grafana-pws', (message: string) => receiveLogs.push(message));

    exchange.publicWebsocket.onMessage({ data: '{"channel":"rs.sub.kline","success":true}' } as MessageEvent);
    exchange.publicWebsocket.onMessage({
      data: '{"channel":"push.ticker","symbol":"BTC_USDT","data":{"lastPrice":"70000"}}',
    } as MessageEvent);
    exchange.publicWebsocket.onMessage({
      data: '{"channel":"push.depth.full","symbol":"BTC_USDT","data":{"asks":[],"bids":[]}}',
    } as MessageEvent);

    expect(receiveLogs).toEqual(['[mexc public] {"channel":"rs.sub.kline","success":true}']);
    expect(recordStats).not.toHaveBeenCalledWith('rs.sub.kline');
    expect(recordStats).toHaveBeenCalledWith('push.ticker');
    expect(recordStats).toHaveBeenCalledWith('push.depth.full');
  });

  it('does not track one-shot WebSocket control responses as stale-topic liveness feeds', () => {
    const exchange = createExchange();
    const recordPublicStats = vi.spyOn(exchange.publicWebsocket as any, 'recordMessageHandlerStats');
    const recordPrivateStats = vi.spyOn(exchange.privateWebsocket as any, 'recordMessageHandlerStats');

    exchange.publicWebsocket.onMessage({ data: '{"channel":"rs.sub.ticker","success":true}' } as MessageEvent);
    exchange.privateWebsocket.onMessage({ data: '{"channel":"rs.personal.filter","success":true}' } as MessageEvent);

    expect(recordPublicStats).not.toHaveBeenCalled();
    expect(recordPrivateStats).not.toHaveBeenCalled();
    expect(exchange.publicWebsocket.messageHandlersStats['rs.sub.ticker']).toBeUndefined();
    expect(exchange.privateWebsocket.messageHandlersStats['rs.personal.filter']).toBeUndefined();
  });

  it('redacts private WebSocket auth diagnostics before forwarding to the request drawer', async () => {
    setWsDiagnosticsEnabled(true);
    const exchange = createExchange();
    const send = vi.fn();
    const sendLogs: string[] = [];
    exchange.privateWebsocket.ws = { readyState: 1, send } as any;
    exchange.on('grafana-pws-sub', (message: string) => sendLogs.push(message));

    await (exchange.privateWebsocket as any).sendLogin();

    const sentLogin = JSON.parse(send.mock.calls[0][0]);
    expect(sentLogin).toMatchObject({
      method: 'login',
      param: {
        apiKey: 'key',
        reqTime: expect.any(String),
        signature: expect.any(String),
      },
    });
    expect(sendLogs).toHaveLength(1);
    expect(sendLogs[0]).toContain('"apiKey":"[redacted]"');
    expect(sendLogs[0]).toContain('"signature":"[redacted]"');
    expect(sendLogs[0]).not.toContain('"apiKey":"key"');
  });

  it('normalizes market precision and contract-size sizing', () => {
    expect(market).toMatchObject({
      id: 'BTC_USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.0001, price: 0.1 },
      limits: { amount: { min: 0.0001, max: 40 }, leverage: { min: 1, max: 500 } },
      contractSize: 0.0001,
    });
    expect(toMexcSymbol('1000PEPEUSDT')).toBe('1000PEPE_USDT');
    expect(nativeVolumeToAmount(123, market)).toBeCloseTo(0.0123);
  });

  it('includes USDC-settled markets for ticker and margin support', () => {
    const usdcMarket = normalizeMarket({
      symbol: 'BTC_USDC',
      baseCoin: 'BTC',
      quoteCoin: 'USDC',
      settleCoin: 'USDC',
      state: 0,
      apiAllowed: true,
      contractSize: 0.0001,
      priceScale: 1,
      priceUnit: 0.1,
      volUnit: 1,
      minVol: 1,
      maxVol: 400000,
      minLeverage: 1,
      maxLeverage: 500,
    });

    expect(isTradableMexcMarket(usdcMarket.info as Record<string, any>)).toBe(true);
    expect(usdcMarket).toMatchObject({
      id: 'BTC_USDC',
      symbol: 'BTCUSDC',
      quote: 'USDC',
    });
    expect(toMexcSymbol('BTCUSDC')).toBe('BTC_USDC');
  });

  it('normalizes tickers with venue volume converted from native contracts and OI as quote notional', () => {
    const ticker = normalizeTicker(
      {
        symbol: 'BTC_USDT',
        lastPrice: 77182.2,
        bid1: 77182.1,
        ask1: 77182.3,
        fairPrice: 77181.8,
        indexPrice: 77166.4,
        fundingRate: 0.0001,
        holdVol: 652452606,
        volume24: 613590677,
        amount24: 4744606582.44991,
        riseFallRate: 0.0024,
      },
      market,
    );

    expect(ticker.symbol).toBe('BTCUSDT');
    expect(ticker.percentage).toBeCloseTo(0.24);
    expect(ticker.openInterest).toBeCloseTo(65245.2606 * 77181.8);
    expect(ticker.volume).toBeCloseTo(61359.0677);
  });

  it('maps order side, type, status, TIF, and close intent', () => {
    const order = normalizeOrder(
      {
        orderId: '987654321012345678',
        symbol: 'BTC_USDT',
        side: 2,
        type: 2,
        state: 1,
        price: 50000,
        vol: 10,
        dealVol: 3,
        createTime: 1700000000000,
      },
      market,
    );

    expect(order).toMatchObject({
      id: '987654321012345678',
      status: OrderStatus.Open,
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      price: 50000,
      reduceOnly: true,
      timeInForce: OrderTimeInForce.PostOnly,
    });
    expect(order?.amount).toBeCloseTo(0.001);
    expect(order?.filled).toBeCloseTo(0.0003);
    expect(order?.remaining).toBeCloseTo(0.0007);
    expect(order?.extra?.remainingNative).toBe(7);
    expect(order?.extra?.filledInPlacementUnits).toBe(3);
  });

  it('preserves zero-valued position fields', () => {
    const position = normalizePosition(
      {
        symbol: 'BTC_USDT',
        positionType: 1,
        positionMode: 1,
        openType: 2,
        holdVol: 0,
        holdAvgPrice: 0,
        leverage: 0,
        unrealized: 0,
        liquidatePrice: 0,
        updateTime: 0,
      },
      market,
      PositionMode.Hedged,
    );

    expect(position).toMatchObject({
      id: 'BTCUSDT:long',
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      entryPrice: 0,
      notional: 0,
      leverage: 0,
      marginType: MarginType.Cross,
      positionMode: PositionMode.Hedged,
      unrealizedPnl: 0,
      contracts: 0,
      liquidationPrice: 0,
    });
  });

  it('normalizes balance rows from documented MEXC asset fields', () => {
    const balance = normalizeBalance([
      {
        currency: 'USDT',
        availableBalance: '100.5',
        cashBalance: '125',
        equity: '130',
        frozenBalance: '5',
        unrealized: '-2.25',
      },
    ]);

    expect(balance).toMatchObject({
      free: 100.5,
      used: 29.5,
      total: 130,
      upnl: -2.25,
    });
    expect(balance.currencies?.USDT).toMatchObject({ free: 100.5, used: 29.5, total: 130, upnl: -2.25 });
  });

  it('uses funded USDC as the primary balance when USDT is empty', () => {
    const balance = normalizeBalance([
      {
        currency: 'USDT',
        availableBalance: '0',
        cashBalance: '0',
        equity: '0',
      },
      {
        currency: 'USDC',
        availableBalance: '20',
        cashBalance: '30',
        equity: '32',
        frozenBalance: '4',
      },
    ]);

    expect(balance).toMatchObject({ free: 20, used: 12, total: 32 });
    expect(balance.currencies?.USDC).toMatchObject({ free: 20, used: 12, total: 32 });
  });

  it('normalizes kline arrays and private fill payloads', () => {
    expect(
      normalizeKlineResponse({
        data: {
          time: [1700000000],
          open: [50000],
          high: [50100],
          low: [49900],
          close: [50050],
          vol: [1234],
        },
      }),
    ).toEqual([{ timestamp: 1700000000, open: 50000, high: 50100, low: 49900, close: 50050, volume: 1234 }]);

    const fill = normalizeExecution(
      {
        tradeId: 'fill-1',
        orderId: 'order-1',
        symbol: 'BTC_USDT',
        side: 4,
        price: 50050,
        vol: 2,
        fee: '-0.01',
        profit: '3.5',
        createTime: 1700000000000,
      },
      market,
    );

    expect(fill).toMatchObject({
      id: 'fill-1',
      timestamp: 1700000000,
      symbol: 'BTCUSDT',
      orderId: 'order-1',
      price: 50050,
      qty: 0.0002,
      side: OrderSide.Sell,
      commission: 0.01,
      positionSide: PositionSide.Long,
      realizedPnl: 3.5,
    });
  });

  it('requests historical klines with second-based bounds', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: {
        success: true,
        code: 0,
        data: {
          time: [1700000000],
          open: [50000],
          high: [50100],
          low: [49900],
          close: [50050],
          vol: [1234],
        },
      },
    });
    exchange.xhr = { get } as any;

    await expect(
      exchange.fetchOHLCV({
        symbol: 'BTCUSDT',
        interval: '4h',
        from: 1783281600000,
        to: 1787601600000,
      }),
    ).resolves.toEqual([{ timestamp: 1700000000, open: 50000, high: 50100, low: 49900, close: 50050, volume: 1234 }]);

    expect(get).toHaveBeenCalledWith(`${ENDPOINTS.KLINE}/BTC_USDT`, {
      params: {
        interval: 'Hour4',
        start: 1783281600,
        end: 1787601600,
      },
      timeout: 5 * 60 * 1000,
    });
  });

  it('preserves side-specific leverage across flat-position metadata refreshes', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { success: true, code: 0 } });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { post, get: vi.fn().mockRejectedValue(new Error('private leverage unavailable')) } as any;

    await exchange.setLeverage('BTCUSDT', 50, PositionSide.Long);
    await exchange.updateSymbolMetadata('BTCUSDT');

    await expect(exchange.fetchLeverageForSymbol('BTCUSDT')).resolves.toMatchObject({
      buyLeverage: 50,
      sellLeverage: 20,
      marginType: MarginType.Cross,
      positionMode: PositionMode.Hedged,
    });
    expect(exchange.store.fetchedLeverage.BTCUSDT).toMatchObject({
      buyLeverage: 50,
      sellLeverage: 20,
    });
    expect(post).toHaveBeenCalledWith(ENDPOINTS.CHANGE_LEVERAGE, {
      symbol: 'BTC_USDT',
      positionType: 1,
      leverage: 50,
      openType: 2,
    });
  });

  it('keeps side-specific leverage on fake flat positions during tick refreshes', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { success: true, code: 0 } });
    const get = vi.fn().mockResolvedValue({ data: { success: true, code: 0, data: [] } });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { post, get } as any;

    await exchange.setLeverage('BTCUSDT', 50, PositionSide.Long);
    const positions = await exchange.fetchPositions(['BTCUSDT']);

    expect(positions.find((position) => position.side === PositionSide.Long)?.leverage).toBe(50);
    expect(positions.find((position) => position.side === PositionSide.Short)?.leverage).toBe(20);
  });

  it('uses positionId for open-position leverage changes', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { success: true, code: 0 } });
    const openLong = normalizePosition(
      {
        positionId: 1394650,
        symbol: 'BTC_USDT',
        positionType: 1,
        positionMode: 1,
        openType: 2,
        holdVol: 10000,
        holdAvgPrice: 50000,
        leverage: 20,
      },
      market,
      PositionMode.Hedged,
    );
    exchange.store.update({ markets: [market], positions: openLong ? [openLong] : [] });
    exchange.xhr = { post } as any;

    await exchange.setLeverage('BTCUSDT', 50, PositionSide.Long);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.CHANGE_LEVERAGE, {
      positionId: '1394650',
      leverage: 50,
    });
  });

  it('does not update leverage cache when MEXC rejects the mutation envelope', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({
      data: { success: false, code: 2024, message: 'leverage over the maximum' },
    });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { post } as any;

    await expect(exchange.setLeverage('BTCUSDT', 50, PositionSide.Long)).rejects.toThrow(
      'MEXC leverage change failed: leverage over the maximum',
    );
    expect(exchange.store.fetchedLeverage.BTCUSDT).toBeUndefined();
  });

  it('uses private leverage max view to avoid overstating account-specific caps', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: {
        success: true,
        code: 0,
        data: [
          {
            symbol: 'BTC_USDT',
            positionType: 1,
            openType: 2,
            leverage: 50,
            maxLeverageView: 50,
          },
          {
            symbol: 'BTC_USDT',
            positionType: 2,
            openType: 2,
            leverage: 32,
            maxLeverageView: 50,
          },
        ],
      },
    });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { get } as any;

    await expect(exchange.fetchLeverageForSymbol('BTCUSDT')).resolves.toMatchObject({
      buyLeverage: 50,
      sellLeverage: 32,
      marginType: MarginType.Cross,
    });
    expect(exchange.store.marketsBySymbol.BTCUSDT.limits.leverage.max).toBe(50);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.POSITION_LEVERAGE, {
      params: { symbol: 'BTC_USDT' },
    });
  });

  it('hydrates flat-symbol leverage during active-symbol metadata updates', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: {
        success: true,
        code: 0,
        data: [
          {
            symbol: 'BTC_USDT',
            positionType: 1,
            openType: 2,
            leverage: 50,
          },
          {
            symbol: 'BTC_USDT',
            positionType: 2,
            openType: 2,
            leverage: 32,
          },
        ],
      },
    });
    exchange.store.update({ markets: [market], positions: [] });
    exchange.xhr = { get } as any;

    await exchange.updateSymbolMetadata('BTCUSDT');

    expect(exchange.store.fetchedLeverage.BTCUSDT).toMatchObject({
      buyLeverage: 50,
      sellLeverage: 32,
      marginType: MarginType.Cross,
      positionMode: PositionMode.Hedged,
    });
    expect(get).toHaveBeenCalledWith(ENDPOINTS.POSITION_LEVERAGE, {
      params: { symbol: 'BTC_USDT' },
    });
  });

  it('keeps flat-position metadata in sync after position mode changes', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { success: true, code: 0 } });
    exchange.store.update({ markets: [market] });
    exchange.xhr = {
      post,
      get: vi.fn().mockResolvedValue({ data: { success: true, code: 0, data: [] } }),
    } as any;

    await exchange.setLeverage('BTCUSDT', 50, PositionSide.Long);
    await exchange.changePositionMode(false);
    await exchange.updateSymbolMetadata('BTCUSDT');

    await expect(exchange.fetchLeverageForSymbol('BTCUSDT')).resolves.toMatchObject({
      buyLeverage: 50,
      sellLeverage: 20,
      marginType: MarginType.Cross,
      positionMode: PositionMode.OneWay,
    });
    expect(post).toHaveBeenCalledWith(ENDPOINTS.CHANGE_POSITION_MODE, {
      positionMode: 2,
    });
  });

  it('changes margin type via the MEXC leverage endpoint', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { success: true, code: 0 } });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { post } as any;

    await exchange.setLeverage('BTCUSDT', 50);
    post.mockClear();

    await exchange.changeMarginType(true, 'BTCUSDT');

    expect(post).toHaveBeenCalledTimes(2);
    expect(post).toHaveBeenNthCalledWith(1, ENDPOINTS.CHANGE_LEVERAGE, {
      symbol: 'BTC_USDT',
      positionType: 1,
      leverage: 50,
      openType: 1,
    });
    expect(post).toHaveBeenNthCalledWith(2, ENDPOINTS.CHANGE_LEVERAGE, {
      symbol: 'BTC_USDT',
      positionType: 2,
      leverage: 50,
      openType: 1,
    });
    await expect(exchange.fetchLeverageForSymbol('BTCUSDT')).resolves.toMatchObject({
      marginType: MarginType.Isolated,
      buyLeverage: 50,
      sellLeverage: 50,
    });
  });

  it('uses positionId when changing margin type for an open position', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { success: true, code: 0 } });
    const openLong = normalizePosition(
      {
        positionId: 1394650,
        symbol: 'BTC_USDT',
        positionType: 1,
        positionMode: 1,
        openType: 2,
        holdVol: 10000,
        holdAvgPrice: 50000,
        leverage: 50,
      },
      market,
      PositionMode.Hedged,
    );
    exchange.store.update({ markets: [market], positions: openLong ? [openLong] : [] });
    exchange.xhr = { post } as any;

    await exchange.changeMarginType(true, 'BTCUSDT');

    expect(post).toHaveBeenNthCalledWith(1, ENDPOINTS.CHANGE_LEVERAGE, {
      positionId: '1394650',
      leverage: 50,
      openType: 1,
    });
    expect(post).toHaveBeenNthCalledWith(2, ENDPOINTS.CHANGE_LEVERAGE, {
      symbol: 'BTC_USDT',
      positionType: 2,
      leverage: 20,
      openType: 1,
    });
  });

  it('places regular orders with native contract volume', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { success: true, code: 0, data: 'order-1' } });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 10,
        price: 50000,
      }),
    ).resolves.toEqual(['order-1']);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.ORDER_SUBMIT,
      expect.objectContaining({
        symbol: 'BTC_USDT',
        price: '50000',
        vol: 10,
        side: 1,
        type: 1,
        openType: 2,
        leverage: 20,
        externalOid: expect.stringMatching(/^ts_/),
      }),
    );
  });

  it('hydrates symbol margin and leverage before placing a cold regular order', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: {
        success: true,
        code: 0,
        data: [
          {
            symbol: 'BTC_USDT',
            positionType: 1,
            openType: 1,
            leverage: 50,
          },
          {
            symbol: 'BTC_USDT',
            positionType: 2,
            openType: 1,
            leverage: 32,
          },
        ],
      },
    });
    const post = vi.fn().mockResolvedValue({ data: { success: true, code: 0, data: 'order-1' } });
    exchange.store.update({ markets: [market], positions: [] });
    exchange.xhr = { get, post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 10,
        price: 50000,
      }),
    ).resolves.toEqual(['order-1']);

    expect(get).toHaveBeenCalledWith(ENDPOINTS.POSITION_LEVERAGE, {
      params: { symbol: 'BTC_USDT' },
    });
    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.ORDER_SUBMIT,
      expect.objectContaining({
        openType: 1,
        leverage: 50,
      }),
    );
  });

  it('paces concurrent regular order placement to avoid venue spam throttles', async () => {
    vi.useFakeTimers();
    try {
      const exchange = createExchange();
      const post = vi
        .fn()
        .mockResolvedValueOnce({ data: { success: true, code: 0, data: 'order-1' } })
        .mockResolvedValueOnce({ data: { success: true, code: 0, data: 'order-2' } });
      exchange.store.update({ markets: [market] });
      exchange.xhr = { post } as any;

      const result = Promise.all([
        exchange.placeOrder({
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 10,
          price: 50000,
        }),
        exchange.placeOrder({
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 10,
          price: 49900,
        }),
      ]).then((ids) => ids.flat());

      await vi.advanceTimersByTimeAsync(0);
      expect(post).toHaveBeenCalledTimes(1);
      await vi.advanceTimersByTimeAsync(449);
      expect(post).toHaveBeenCalledTimes(1);
      await vi.advanceTimersByTimeAsync(1);
      await expect(result).resolves.toEqual(['order-1', 'order-2']);
      expect(post).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it('places regular scaled orders through the native MEXC batch endpoint', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({
      data: {
        success: true,
        code: 0,
        data: [
          { orderId: 'batch-1', errorCode: 0 },
          { orderId: 'batch-2', errorCode: 0 },
        ],
      },
    });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrders([
        {
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Sell,
          amount: 10,
          price: 51000,
          timeInForce: OrderTimeInForce.PostOnly,
        },
        {
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Sell,
          amount: 10,
          price: 51100,
          timeInForce: OrderTimeInForce.PostOnly,
        },
      ]),
    ).resolves.toEqual(['batch-1', 'batch-2']);

    expect(post).toHaveBeenCalledTimes(1);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.ORDER_SUBMIT_BATCH, [
      expect.objectContaining({
        symbol: 'BTC_USDT',
        price: '51000',
        vol: 10,
        side: 3,
        type: 2,
        openType: 2,
        leverage: 20,
        stpMode: 0,
      }),
      expect.objectContaining({
        symbol: 'BTC_USDT',
        price: '51100',
        vol: 10,
        side: 3,
        type: 2,
        openType: 2,
        leverage: 20,
        stpMode: 0,
      }),
    ]);
  });

  it('refreshes regular orders without fetching plan or stop orders after private WS updates', async () => {
    const exchange = createExchange();
    const planOrder = {
      id: 'plan-1',
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      status: OrderStatus.Open,
      amount: 0.001,
      remaining: 0.001,
      price: 49000,
      info: { __mexcPlanOrder: true },
    } as Order;
    const get = vi.fn(async (url: string) => {
      if (url !== ENDPOINTS.OPEN_ORDERS) {
        throw new Error(`unexpected URL ${url}`);
      }
      return {
        data: {
          success: true,
          code: 0,
          data: [
            {
              orderId: 'regular-1',
              symbol: 'BTC_USDT',
              side: 1,
              orderType: 1,
              state: 2,
              price: 50000,
              vol: 10,
              dealVol: 0,
              createTime: 1787670700000,
            },
          ],
        },
      };
    });
    exchange.store.update({ markets: [market], orders: [planOrder] });
    exchange.xhr = { get } as any;

    await expect(exchange.fetchAndSetRegularOrders()).resolves.toMatchObject([{ id: 'regular-1' }]);

    expect(get).toHaveBeenCalledTimes(1);
    expect(exchange.store.orders.map((order) => order.id)).toEqual(['plan-1', 'regular-1']);
  });

  it('reports already-placed IDs when native batch placement returns per-row failures', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({
      data: {
        success: true,
        code: 0,
        data: [
          { orderId: 'order-1', errorCode: 0 },
          { orderId: 'order-2', errorCode: 0 },
          { orderId: null, errorCode: 510, errorMsg: 'Requests are too frequent, please try again later' },
        ],
      },
    });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrders([
        {
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Sell,
          amount: 10,
          price: 51000,
        },
        {
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Sell,
          amount: 10,
          price: 51100,
        },
        {
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Sell,
          amount: 10,
          price: 51200,
        },
      ]),
    ).rejects.toMatchObject({
      message: 'MEXC batch order placement failed: 1: Requests are too frequent, please try again later',
      partialOrderIds: ['order-1', 'order-2'],
      partialOrderCount: 2,
      totalOrderCount: 3,
      failedOrderIndex: 2,
    });
  });

  it('falls back to paced sequential placement when native batch is unavailable', async () => {
    vi.useFakeTimers();
    try {
      const exchange = createExchange();
      const post = vi
        .fn()
        .mockResolvedValueOnce({
          data: {
            success: false,
            code: 511,
            message: 'You do not have access to the interface',
          },
        })
        .mockResolvedValueOnce({ data: { success: true, code: 0, data: 'fallback-1' } })
        .mockResolvedValueOnce({ data: { success: true, code: 0, data: 'fallback-2' } })
        .mockResolvedValueOnce({ data: { success: true, code: 0, data: 'fallback-3' } });
      exchange.store.update({ markets: [market] });
      exchange.xhr = { post } as any;

      const firstResult = exchange.placeOrders([
        {
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 10,
          price: 50000,
        },
        {
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 10,
          price: 49900,
        },
      ]);

      await vi.advanceTimersByTimeAsync(0);
      expect(post).toHaveBeenCalledTimes(2);
      expect(post).toHaveBeenNthCalledWith(1, ENDPOINTS.ORDER_SUBMIT_BATCH, expect.any(Array));
      expect(post).toHaveBeenNthCalledWith(2, ENDPOINTS.ORDER_SUBMIT, expect.objectContaining({ price: '50000' }));
      await vi.advanceTimersByTimeAsync(1499);
      expect(post).toHaveBeenCalledTimes(2);
      await vi.advanceTimersByTimeAsync(1);
      await expect(firstResult).resolves.toEqual(['fallback-1', 'fallback-2']);
      expect(post).toHaveBeenNthCalledWith(3, ENDPOINTS.ORDER_SUBMIT, expect.objectContaining({ price: '49900' }));

      const secondResult = exchange.placeOrders([
        {
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 10,
          price: 49800,
        },
      ]);
      await vi.advanceTimersByTimeAsync(450);
      await expect(secondResult).resolves.toEqual(['fallback-3']);

      expect(post).toHaveBeenCalledTimes(4);
      expect(post).toHaveBeenNthCalledWith(4, ENDPOINTS.ORDER_SUBMIT, expect.objectContaining({ price: '49800' }));
    } finally {
      vi.useRealTimers();
    }
  });

  it('places standalone reduce-only market TP/SL plan orders', async () => {
    const exchange = createExchange();
    const post = vi
      .fn()
      .mockResolvedValueOnce({ data: { success: true, code: 0, data: 'plan-sl' } })
      .mockResolvedValueOnce({ data: { success: true, code: 0, data: 'plan-tp' } });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 10000,
        price: 49000,
      }),
    ).resolves.toEqual(['plan-sl']);
    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.TakeProfit,
        side: OrderSide.Sell,
        amount: 10000,
        price: 51000,
      }),
    ).resolves.toEqual(['plan-tp']);

    expect(exchange.store.orders).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'plan-sl',
          symbol: 'BTCUSDT',
          type: OrderType.StopLoss,
          status: OrderStatus.Open,
          amount: 1,
          remaining: 1,
          price: 49000,
          info: expect.objectContaining({ __mexcPlanOrder: true, vol: 10000 }),
        }),
        expect.objectContaining({
          id: 'plan-tp',
          symbol: 'BTCUSDT',
          type: OrderType.TakeProfit,
          status: OrderStatus.Open,
          amount: 1,
          remaining: 1,
          price: 51000,
          info: expect.objectContaining({ __mexcPlanOrder: true, vol: 10000 }),
        }),
      ]),
    );
    expect(post).toHaveBeenNthCalledWith(1, ENDPOINTS.PLAN_ORDER_PLACE, {
      symbol: 'BTC_USDT',
      vol: 10000,
      leverage: 20,
      side: 4,
      openType: 2,
      triggerPrice: '49000',
      triggerType: 2,
      executeCycle: 2,
      orderType: 5,
      trend: 1,
      positionMode: 1,
      reduceOnly: true,
    });
    expect(post).toHaveBeenNthCalledWith(2, ENDPOINTS.PLAN_ORDER_PLACE, {
      symbol: 'BTC_USDT',
      vol: 10000,
      leverage: 20,
      side: 4,
      openType: 2,
      triggerPrice: '51000',
      triggerType: 1,
      executeCycle: 2,
      orderType: 5,
      trend: 1,
      positionMode: 1,
      reduceOnly: true,
    });
  });

  it('places full-position TP/SL through the MEXC position stop endpoint', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { success: true, code: 0, data: 'position-stop-1' } });
    exchange.store.update({
      markets: [market],
      positions: [
        {
          id: 'BTCUSDT:long',
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          entryPrice: 50000,
          notional: 1,
          leverage: 20,
          marginType: MarginType.Cross,
          positionMode: PositionMode.Hedged,
          unrealizedPnl: 0,
          contracts: 1,
          liquidationPrice: 0,
          info: {
            positionId: 'native-position-1',
            holdVol: 10000,
          },
          extra: {
            contractsNative: 10000,
          },
        },
      ],
    });
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 0,
        price: 49000,
        slTriggerType: OrderStopTriggerType.MarkPrice,
      }),
    ).resolves.toEqual(['position-stop-1']);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.STOP_ORDER_PLACE, {
      positionId: 'native-position-1',
      vol: 10000,
      lossTrend: 2,
      profitTrend: 1,
      stopLossPrice: '49000',
      profitLossVolType: 'SAME',
      volType: 2,
      stopLossType: 0,
    });
  });

  it('keeps the periodic tick on regular open orders while preserving stop-like rows', async () => {
    const exchange = createExchange();
    const existingStopOrder = {
      id: 'plan-1',
      status: OrderStatus.Open,
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      price: 49000,
      amount: 1,
      filled: 0,
      remaining: 1,
      reduceOnly: true,
      timeInForce: OrderTimeInForce.GoodTillCancel,
      info: { __mexcPlanOrder: true },
    } as Order;
    const get = vi.fn(async (url: string) => {
      if (url === ENDPOINTS.OPEN_ORDERS) {
        return {
          data: {
            success: true,
            code: 0,
            data: [
              {
                orderId: 'regular-1',
                symbol: 'BTC_USDT',
                side: 1,
                type: 1,
                state: 2,
                price: 50000,
                vol: 10000,
                dealVol: 0,
                updateTime: 1700000000000,
              },
            ],
          },
        };
      }
      throw new Error(`unexpected URL ${url}`);
    });
    exchange.store.update({ markets: [market], orders: [existingStopOrder] });
    exchange.xhr = { get } as any;
    vi.spyOn(exchange, 'fetchBalanceAndPositions').mockResolvedValue({ balance: normalizeBalance([]), positions: [] });
    const fetchAndSetRegularOrders = exchange.fetchAndSetRegularOrders;
    vi.spyOn(exchange, 'fetchAndSetRegularOrders').mockImplementation(async (symbols?: string[]) => {
      const orders = await fetchAndSetRegularOrders(symbols);
      exchange.isDisposed = true;
      return orders;
    });

    await exchange.tick();

    expect(get).toHaveBeenCalledTimes(1);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.OPEN_ORDERS, { params: { page_num: 1, page_size: 100 } });
    expect(exchange.store.orders.map((order) => order.id).sort()).toEqual(['plan-1', 'regular-1']);
  });

  it('keeps open plan orders visible after regular open orders are empty', async () => {
    const exchange = createExchange();
    const get = vi.fn(async (url: string) => {
      if (url === ENDPOINTS.OPEN_ORDERS) {
        return { data: { success: true, code: 0, data: [] } };
      }
      if (url === ENDPOINTS.PLAN_ORDER_LIST) {
        return {
          data: {
            success: true,
            code: 0,
            data: {
              orders: [
                {
                  id: '846983655908788224',
                  symbol: 'BTC_USDT',
                  leverage: 20,
                  side: 4,
                  triggerPrice: 74000,
                  price: 0,
                  vol: 10000,
                  openType: 2,
                  triggerType: 2,
                  state: 1,
                  executeCycle: 2,
                  trend: 1,
                  orderType: 5,
                  positionMode: 1,
                  reduceOnly: true,
                  updateTime: 1700000000000,
                },
              ],
            },
          },
        };
      }
      if (url === ENDPOINTS.STOP_ORDER_LIST) {
        return { data: { success: true, code: 0, data: [] } };
      }
      throw new Error(`unexpected URL ${url}`);
    });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { get } as any;

    await expect(exchange.fetchOrders()).resolves.toEqual([
      expect.objectContaining({
        id: '846983655908788224',
        status: OrderStatus.Open,
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        price: 74000,
        amount: 1,
        remaining: 1,
        reduceOnly: true,
        extra: { remainingNative: 10000 },
      }),
    ]);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.PLAN_ORDER_LIST, {
      params: expect.objectContaining({
        states: '1',
        page_num: 1,
        page_size: 100,
      }),
    });
  });

  it('keeps existing regular close limits during symbol-scoped TP/SL refreshes', async () => {
    const exchange = createExchange();
    const regularCloseOrder = {
      id: 'regular-tp-limit-1',
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Sell,
      status: OrderStatus.Open,
      price: 51000,
      amount: 0.75,
      filled: 0,
      remaining: 0.75,
      reduceOnly: true,
      timeInForce: OrderTimeInForce.GoodTillCancel,
    } as Order;
    const stalePlanOrder = {
      id: 'stale-plan-sl',
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      status: OrderStatus.Open,
      price: 48000,
      amount: 0.75,
      filled: 0,
      remaining: 0.75,
      reduceOnly: true,
      timeInForce: OrderTimeInForce.GoodTillCancel,
      info: { __mexcPlanOrder: true },
    } as Order;
    const get = vi.fn(async (url: string) => {
      if (url === `${ENDPOINTS.OPEN_ORDERS}/BTC_USDT`) {
        return { data: { success: true, code: 0, data: [] } };
      }
      if (url === ENDPOINTS.PLAN_ORDER_LIST) {
        return {
          data: {
            success: true,
            code: 0,
            data: [
              {
                id: 'plan-sl-1',
                symbol: 'BTC_USDT',
                side: 4,
                triggerPrice: 49000,
                vol: 7500,
                openType: 2,
                triggerType: 2,
                state: 1,
                executeCycle: 2,
                trend: 1,
                orderType: 5,
                positionMode: 1,
                reduceOnly: true,
                updateTime: 1700000000000,
              },
            ],
          },
        };
      }
      if (url === ENDPOINTS.STOP_ORDER_LIST) {
        return { data: { success: true, code: 0, data: [] } };
      }
      throw new Error(`unexpected URL ${url}`);
    });
    exchange.store.update({ markets: [market], orders: [regularCloseOrder, stalePlanOrder] });
    exchange.xhr = { get } as any;

    await exchange.fetchAndSetOrders(['BTCUSDT']);

    expect(exchange.store.orders.map((order) => order.id).sort()).toEqual(['plan-sl-1', 'regular-tp-limit-1']);
  });

  it('keeps full-position TP/SL stop orders visible as infinity stops', async () => {
    const exchange = createExchange();
    const get = vi.fn(async (url: string) => {
      if (url === ENDPOINTS.OPEN_ORDERS || url === ENDPOINTS.PLAN_ORDER_LIST) {
        return { data: { success: true, code: 0, data: [] } };
      }
      if (url === ENDPOINTS.STOP_ORDER_LIST) {
        return {
          data: {
            success: true,
            code: 0,
            data: {
              orders: [
                {
                  id: 'stop-position-1',
                  symbol: 'BTC_USDT',
                  positionId: 'native-position-1',
                  positionType: 1,
                  stopLossPrice: 49000,
                  takeProfitPrice: 0,
                  triggerSide: 2,
                  state: 1,
                  vol: 10000,
                  volType: 2,
                  updateTime: 1700000000000,
                },
              ],
            },
          },
        };
      }
      throw new Error(`unexpected URL ${url}`);
    });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { get } as any;

    await expect(exchange.fetchOrders()).resolves.toEqual([
      expect.objectContaining({
        id: 'stop-position-1',
        status: OrderStatus.Open,
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        price: 49000,
        amount: 0,
        remaining: 0,
        reduceOnly: true,
        extra: { remainingNative: 0 },
      }),
    ]);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.STOP_ORDER_LIST, {
      params: expect.objectContaining({
        is_finished: 0,
        page_num: 1,
        page_size: 100,
      }),
    });
  });

  it('cancels stop orders through the plan-order cancel endpoint even before REST marks them', async () => {
    const exchange = createExchange();
    const post = vi.fn(async () => ({ data: { success: true, code: 0 } }));
    exchange.xhr = { post } as any;

    await exchange.cancelOrders([
      {
        id: '846983655908788224',
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        status: OrderStatus.Open,
        amount: 1,
        remaining: 1,
        price: 74000,
      } as Order,
    ]);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.PLAN_ORDER_CANCEL, [
      { symbol: 'BTC_USDT', orderId: '846983655908788224' },
    ]);
  });

  it('cancels full-position TP/SL through the stop-order cancel endpoint', async () => {
    const exchange = createExchange();
    const post = vi.fn(async () => ({ data: { success: true, code: 0 } }));
    exchange.xhr = { post } as any;

    await exchange.cancelOrders([
      {
        id: 'stop-position-1',
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        status: OrderStatus.Open,
        amount: 0,
        remaining: 0,
        price: 49000,
      } as Order,
    ]);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.STOP_ORDER_CANCEL, [{ stopPlanOrderId: 'stop-position-1' }]);
  });

  it('does not let lagging stop-order snapshots re-open a just-cancelled TP/SL row', async () => {
    const exchange = createExchange();
    const stopOrder = {
      id: 'stop-position-1',
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      status: OrderStatus.Open,
      amount: 0,
      remaining: 0,
      price: 49000,
      info: { __mexcStopOrder: true },
    } as Order;
    const post = vi.fn(async () => ({ data: { success: true, code: 0 } }));
    const get = vi.fn(async (url: string) => {
      if (url === ENDPOINTS.OPEN_ORDERS || url === ENDPOINTS.PLAN_ORDER_LIST) {
        return { data: { success: true, code: 0, data: [] } };
      }
      if (url === ENDPOINTS.STOP_ORDER_LIST) {
        return {
          data: {
            success: true,
            code: 0,
            data: [
              {
                stopPlanOrderId: 'stop-position-1',
                symbol: 'BTC_USDT',
                positionType: 1,
                triggerSide: 2,
                volType: 2,
                stopLossPrice: 49000,
                state: 1,
                createTime: 1700000000000,
              },
            ],
          },
        };
      }
      throw new Error(`unexpected URL ${url}`);
    });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { get, post } as any;

    await exchange.cancelOrders([stopOrder]);

    await expect(exchange.fetchOrders()).resolves.toEqual([]);
  });

  it('does not preserve already-cancelled stop-like rows during regular-order refreshes', async () => {
    const exchange = createExchange();
    const get = vi.fn(async (url: string) => {
      if (url === ENDPOINTS.OPEN_ORDERS) {
        return {
          data: {
            success: true,
            code: 0,
            data: [
              {
                orderId: 'regular-1',
                symbol: 'BTC_USDT',
                side: 1,
                type: 1,
                state: 2,
                price: 50000,
                vol: 10000,
                dealVol: 0,
                updateTime: 1700000000000,
              },
            ],
          },
        };
      }
      throw new Error(`unexpected URL ${url}`);
    });
    exchange.store.update({
      markets: [market],
      orders: [
        {
          id: 'plan-1',
          status: OrderStatus.Canceled,
          symbol: 'BTCUSDT',
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 49000,
          amount: 1,
          filled: 0,
          remaining: 1,
          reduceOnly: true,
          timeInForce: OrderTimeInForce.GoodTillCancel,
          info: { __mexcPlanOrder: true },
        } as Order,
      ],
    });
    exchange.xhr = { get } as any;

    await exchange.fetchAndSetRegularOrders();

    expect(exchange.store.orders.map((order) => order.id)).toEqual(['regular-1']);
  });

  it('cancels regular orders with the raw order-id array required by the live API', async () => {
    const exchange = createExchange();
    const post = vi.fn(async () => ({
      data: {
        success: true,
        code: 0,
        data: [{ orderId: 'regular-1', errorCode: 0, errorMsg: 'success' }],
      },
    }));
    exchange.xhr = { post } as any;

    await exchange.cancelOrders([
      {
        id: 'regular-1',
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        status: OrderStatus.Open,
        amount: 1,
        remaining: 1,
        price: 74000,
      } as Order,
    ]);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.ORDER_CANCEL, ['regular-1']);
  });

  it('rejects regular cancel rows that MEXC reports as per-order failures', async () => {
    const exchange = createExchange();
    const post = vi.fn(async () => ({
      data: {
        success: true,
        code: 0,
        data: [{ orderId: 'regular-1', errorCode: 2041, errorMsg: 'order state cannot be cancelled' }],
      },
    }));
    exchange.xhr = { post } as any;

    await expect(
      exchange.cancelOrders([
        {
          id: 'regular-1',
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          status: OrderStatus.Open,
          amount: 1,
          remaining: 1,
          price: 74000,
        } as Order,
      ]),
    ).rejects.toThrow('MEXC order cancellation failed: regular-1: order state cannot be cancelled');
  });

  it('replaces normalized MEXC orders with native contract volume on price moves', async () => {
    const exchange = createExchange();
    const post = vi
      .fn()
      .mockResolvedValueOnce({ data: { success: true, code: 0 } })
      .mockResolvedValueOnce({ data: { success: true, code: 0, data: 'replacement-1' } });
    exchange.store.update({ markets: [market] });
    const get = vi.fn().mockResolvedValue({ data: { success: true, code: 0, data: {
      orderId: 'regular-1', symbol: 'BTC_USDT', side: 1, orderType: 1, vol: 8004, dealVol: 0, price: 80000, state: 4,
    } } });
    exchange.xhr = { post, get } as any;

    await expect(
      exchange.updateOrder({
        order: {
          id: 'regular-1',
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          status: OrderStatus.Open,
          amount: 0.8004,
          filled: 0,
          remaining: 0.8004,
          price: 80000,
          reduceOnly: false,
          lastUpdate: 0,
          extra: { remainingNative: 8004 },
        },
        update: { price: 80500 },
      }),
    ).resolves.toEqual(['replacement-1']);

    expect(post).toHaveBeenNthCalledWith(1, ENDPOINTS.ORDER_CANCEL, ['regular-1']);
    expect(post).toHaveBeenNthCalledWith(
      2,
      ENDPOINTS.ORDER_SUBMIT,
      expect.objectContaining({
        symbol: 'BTC_USDT',
        price: '80500',
        vol: 8004,
      }),
    );
  });

  it('preserves recently WS-written open regular orders across a tick REST refresh that lags the venue', async () => {
    const exchange = createExchange();
    const now = 1_700_000_000_000;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);

    const get = vi.fn(async (url: string) => {
      if (url === ENDPOINTS.OPEN_ORDERS) {
        return {
          data: {
            success: true,
            code: 0,
            data: [
              {
                orderId: 'regular-old',
                symbol: 'BTC_USDT',
                side: 1,
                type: 1,
                state: 2,
                price: 50000,
                vol: 1,
                dealVol: 0,
                updateTime: now - 60_000,
              },
            ],
          },
        };
      }
      throw new Error(`unexpected URL ${url}`);
    });
    exchange.xhr = { get } as any;

    // In-store "fresh" regular order that the WS delivered but MEXC's
    // open_orders REST snapshot has not reflected yet.
    exchange.store.update({
      markets: [market],
      orders: [
        {
          id: 'regular-fresh',
          status: OrderStatus.Open,
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          price: 55000,
          amount: 0.0001,
          filled: 0,
          remaining: 0.0001,
          reduceOnly: false,
          timeInForce: OrderTimeInForce.GoodTillCancel,
          lastUpdate: now - 500,
          info: {},
        } as Order,
      ],
    });

    await exchange.fetchAndSetRegularOrders();

    expect(exchange.store.orders.map((order) => order.id).sort()).toEqual(['regular-fresh', 'regular-old']);
  });

  it('drops WS-written open regular orders whose age exceeds the preservation window', async () => {
    const exchange = createExchange();
    const now = 1_700_000_000_000;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);

    const get = vi.fn(async () => ({
      data: { success: true, code: 0, data: [] },
    }));
    exchange.xhr = { get } as any;
    exchange.store.update({
      markets: [market],
      orders: [
        {
          id: 'regular-stale',
          status: OrderStatus.Open,
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          price: 55000,
          amount: 0.0001,
          filled: 0,
          remaining: 0.0001,
          reduceOnly: false,
          timeInForce: OrderTimeInForce.GoodTillCancel,
          // Older than the 15s preservation window; without an anchoring
          // REST row, this is treated as a WS-missed cancellation.
          lastUpdate: now - 30_000,
          info: {},
        } as Order,
      ],
    });

    await exchange.fetchAndSetRegularOrders();

    expect(exchange.store.orders).toEqual([]);
  });

  it('does not preserve non-Open regular rows past the tick', async () => {
    const exchange = createExchange();
    const now = 1_700_000_000_000;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);

    const get = vi.fn(async () => ({
      data: { success: true, code: 0, data: [] },
    }));
    exchange.xhr = { get } as any;
    exchange.store.update({
      markets: [market],
      orders: [
        {
          id: 'regular-canceled',
          status: OrderStatus.Canceled,
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          price: 55000,
          amount: 0.0001,
          filled: 0,
          remaining: 0.0001,
          reduceOnly: false,
          timeInForce: OrderTimeInForce.GoodTillCancel,
          lastUpdate: now - 500,
          info: {},
        } as Order,
      ],
    });

    await exchange.fetchAndSetRegularOrders();

    expect(exchange.store.orders).toEqual([]);
  });

  it('seeds an optimistic open regular order after a successful REST placement', async () => {
    const exchange = createExchange();
    const now = 1_700_000_000_000;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);

    const post = vi.fn(async () => ({
      data: { success: true, code: 0, data: { orderId: 'regular-new' } },
    }));
    exchange.store.update({ markets: [market] });
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 10,
        price: 60000,
        reduceOnly: false,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      }),
    ).resolves.toEqual(['regular-new']);

    expect(exchange.store.orders).toHaveLength(1);
    expect(exchange.store.orders[0]).toMatchObject({
      id: 'regular-new',
      status: OrderStatus.Open,
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 60000,
      reduceOnly: false,
      timeInForce: OrderTimeInForce.GoodTillCancel,
      lastUpdate: now,
    });
    expect((exchange.store.orders[0].info as Record<string, any>).__mexcOptimisticRegular).toBe(true);
    expect(exchange.store.orders[0].extra?.remainingNative).toBe(10);
  });

  it('does not seed an optimistic row for market orders', async () => {
    const exchange = createExchange();
    const post = vi.fn(async () => ({
      data: { success: true, code: 0, data: { orderId: 'regular-market' } },
    }));
    exchange.store.update({ markets: [market] });
    exchange.xhr = { post } as any;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 10,
      price: 0,
      reduceOnly: false,
    });

    expect(exchange.store.orders).toEqual([]);
  });

  it('marks a MEXC state=4 errorCode=10 rejection as canceled and does not resurrect it via a lagging HTTP 200', async () => {
    // Replays the sequence captured in a live ARB_USDC trace: WS state=2
    // (Open) then WS state=4 with errorCode=10 (Canceled by venue) both
    // arrive BEFORE the placement HTTP 200 returns. Without the shield,
    // `addOptimisticRegularOrder` would flip the row back to Open.
    const exchange = createExchange();
    const now = 1_700_000_000_000;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);

    const errors: string[] = [];
    exchange.on('error', (message: string) => errors.push(message));

    const post = vi.fn(async () => ({
      data: { success: true, code: 0, data: 'rejected-1' },
    }));
    const market = normalizeMarket({
      symbol: 'ARB_USDC',
      baseCoin: 'ARB',
      quoteCoin: 'USDC',
      settleCoin: 'USDC',
      state: 0,
      apiAllowed: true,
      contractSize: 10,
      priceScale: 5,
      priceUnit: 0.00001,
      volUnit: 1,
      minVol: 1,
      maxVol: 400000,
      minLeverage: 1,
      maxLeverage: 500,
      makerFeeRate: 0,
      takerFeeRate: 0.0002,
    });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { post } as any;

    const acceptedFrame = {
      channel: 'push.personal.order',
      data: {
        orderId: 'rejected-1',
        symbol: 'ARB_USDC',
        side: 4,
        type: 2,
        state: 2,
        errorCode: 0,
        price: 0.09794,
        vol: 20,
        remainVol: 20,
        dealVol: 0,
        updateTime: now,
      },
    };
    const rejectedFrame = {
      channel: 'push.personal.order',
      data: {
        orderId: 'rejected-1',
        symbol: 'ARB_USDC',
        side: 4,
        type: 2,
        state: 4,
        errorCode: 10,
        price: 0.09794,
        vol: 20,
        remainVol: 20,
        dealVol: 0,
        updateTime: now + 1,
      },
    };

    exchange.privateWebsocket.onMessage({ data: JSON.stringify(acceptedFrame) } as MessageEvent);
    exchange.privateWebsocket.onMessage({ data: JSON.stringify(rejectedFrame) } as MessageEvent);

    await exchange.placeOrder({
      symbol: 'ARBUSDC',
      type: OrderType.Limit,
      side: OrderSide.Sell,
      amount: 20,
      price: 0.09794,
      reduceOnly: true,
      timeInForce: OrderTimeInForce.PostOnly,
    });

    const row = exchange.store.orders.find((order) => order.id === 'rejected-1');
    expect(row?.status).toBe(OrderStatus.Canceled);
    expect((row?.info as Record<string, any>).__mexcOptimisticRegular).toBeUndefined();
    expect(errors.some((message) => message.includes('rejected-1') && message.includes('errorCode 10'))).toBe(true);
  });

  it('does not preserve a MEXC-rejected regular row across the periodic tick REST replace', async () => {
    const exchange = createExchange();
    const now = 1_700_000_000_000;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);

    const get = vi.fn(async () => ({ data: { success: true, code: 0, data: [] } }));
    exchange.xhr = { get } as any;
    exchange.store.update({
      markets: [market],
      orders: [
        {
          id: 'rejected-1',
          status: OrderStatus.Open,
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Sell,
          price: 60000,
          amount: 0.0001,
          filled: 0,
          remaining: 0.0001,
          reduceOnly: true,
          timeInForce: OrderTimeInForce.PostOnly,
          lastUpdate: now - 500,
          info: {},
        } as Order,
      ],
    });

    exchange.on('error', () => {}); // silence traced emitAndLogError

    exchange.noteRejectedRegularOrder('rejected-1', 10);
    // After noting, the local row must already be Canceled — the tick's
    // wholesale REST replace then drops it entirely because no REST anchor
    // exists and preservation skips non-Open rows.
    expect(exchange.store.orders.find((order) => order.id === 'rejected-1')?.status).toBe(OrderStatus.Canceled);

    await exchange.fetchAndSetRegularOrders();

    const openIds = exchange.store.orders
      .filter((order) => order.status === OrderStatus.Open)
      .map((order) => order.id);
    expect(openIds).not.toContain('rejected-1');
  });

  it('does not seed an optimistic regular row when the WS has already reported the id as canceled', async () => {
    const exchange = createExchange();
    const post = vi.fn(async () => ({
      data: { success: true, code: 0, data: 'already-canceled-1' },
    }));
    exchange.store.update({
      markets: [market],
      orders: [
        {
          id: 'already-canceled-1',
          status: OrderStatus.Canceled,
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          price: 60000,
          amount: 0.0001,
          filled: 0,
          remaining: 0.0001,
          reduceOnly: false,
          timeInForce: OrderTimeInForce.GoodTillCancel,
          lastUpdate: 0,
          info: {},
        } as Order,
      ],
    });
    exchange.xhr = { post } as any;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 10,
      price: 60000,
      reduceOnly: false,
      timeInForce: OrderTimeInForce.GoodTillCancel,
    });

    expect(exchange.store.orders.find((order) => order.id === 'already-canceled-1')?.status).toBe(OrderStatus.Canceled);
  });

  it('subscribes to plan / stop / stop-plan private WS filters so TP/SL frames arrive', () => {
    const exchange = createExchange();
    const send = vi.fn();
    exchange.privateWebsocket.ws = { readyState: 1, send } as any;

    // `personal.filter` is MEXC's *narrowing* subscription — omitting any of
    // these keys silently drops the corresponding TP/SL pushes and
    // regresses plan/stop sync back to REST-poll-only.
    (exchange.privateWebsocket as any).subscribePrivateFeeds();

    const payload = JSON.parse(send.mock.calls[0][0]);
    expect(payload.method).toBe('personal.filter');
    const filters = (payload.param.filters as Array<{ filter: string }>).map((row) => row.filter);
    expect(filters).toEqual(
      expect.arrayContaining(['order', 'order.deal', 'position', 'asset', 'plan.order', 'stop.order', 'stop.planorder']),
    );
  });

  it('upserts a plan-order row from push.personal.plan.order and releases its placement shield', () => {
    const exchange = createExchange();
    exchange.store.update({ markets: [market] });

    // Seed a placement-shield entry the way `addOptimisticStopOrder` would
    // right after the placement HTTP 200.
    (exchange as any).notePlacedStopLikeOrder('plan-ws-1');
    expect((exchange as any).recentlyPlacedStopLikeOrderIds.has('plan-ws-1')).toBe(true);

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        channel: 'push.personal.plan.order',
        data: {
          id: 'plan-ws-1',
          symbol: 'BTC_USDT',
          side: 4,
          triggerPrice: 49000,
          price: 0,
          vol: 10000,
          state: 1,
          triggerType: 2,
          executeCycle: 2,
          trend: 1,
          orderType: 5,
          reduceOnly: true,
          updateTime: 1_700_000_000_000,
        },
      }),
    } as MessageEvent);

    const row = exchange.store.orders.find((order) => order.id === 'plan-ws-1');
    expect(row).toMatchObject({
      id: 'plan-ws-1',
      symbol: 'BTCUSDT',
      status: OrderStatus.Open,
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      price: 49000,
    });
    expect((row?.info as Record<string, any>).__mexcPlanOrder).toBe(true);
    // WS sighting releases the placement shield so future REST snapshots are authoritative.
    expect((exchange as any).recentlyPlacedStopLikeOrderIds.has('plan-ws-1')).toBe(false);
  });

  // Regression: `normalizePlanOrder` used to route status through the regular
  // `ORDER_STATUS` map, in which `state:2` means Uncompleted (Open). Trigger
  // orders (plan + position stop) use a DIFFERENT state space where `state:2`
  // means Cancelled. The old wiring resurrected every canceled TP/SL row via
  // the WS `push.personal.plan.order` cancel notification — a live trace
  // showed dozens of successful cancel POSTs, dozens of matching `state:2`
  // WS frames, and the chart lighting up with all "canceled" rows still open.
  // See mexc/CLAUDE.md "Regular vs trigger order state semantics".
  it('marks a locally-Open plan-order row Canceled when push.personal.plan.order arrives with state=2', () => {
    const exchange = createExchange();
    exchange.store.update({
      markets: [market],
      orders: [
        {
          id: 'plan-cancel-1',
          status: OrderStatus.Open,
          symbol: 'BTCUSDT',
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 49000,
          amount: 1,
          filled: 0,
          remaining: 1,
          reduceOnly: true,
          timeInForce: OrderTimeInForce.GoodTillCancel,
          info: { __mexcPlanOrder: true },
        } as Order,
      ],
    });

    // Live MEXC cancel notification: `push.personal.plan.order` with
    // `state:2`. Must be normalized to Canceled, not Open.
    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        channel: 'push.personal.plan.order',
        data: {
          id: 'plan-cancel-1',
          symbol: 'BTC_USDT',
          side: 4,
          triggerPrice: 49000,
          price: 0,
          vol: 10000,
          state: 2,
          triggerType: 2,
          orderType: 5,
          reduceOnly: false,
          updateTime: 1_700_000_000_000,
        },
      }),
    } as MessageEvent);

    expect(exchange.store.orders.find((order) => order.id === 'plan-cancel-1')?.status).toBe(
      OrderStatus.Canceled,
    );
  });

  // The reported symptom, distilled: a plan.order state=2 frame that lands
  // BEFORE the cancel HTTP response used to be misinterpreted as Open. Even
  // the cancel-shield guard could not save it because the shield entry is
  // only written *after* the HTTP response, which arrives after the frame in
  // the observed race. Correct state normalization eliminates the race
  // entirely — the frame carries the Canceled state directly.
  it('lets a plan-order state=2 WS frame land the Canceled state even without a cancel-shield entry', () => {
    const exchange = createExchange();
    exchange.store.update({
      markets: [market],
      orders: [
        {
          id: 'plan-race-1',
          status: OrderStatus.Open,
          symbol: 'BTCUSDT',
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 49000,
          amount: 1,
          filled: 0,
          remaining: 1,
          reduceOnly: true,
          timeInForce: OrderTimeInForce.GoodTillCancel,
          info: { __mexcPlanOrder: true },
        } as Order,
      ],
    });

    // No `noteRecentlyCanceledStopLikeOrders` call yet — mirrors the "WS
    // frame beats HTTP response" race captured in the live trace.
    expect((exchange as any).isRecentlyCanceledStopLikeOrderId('plan-race-1')).toBe(false);

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        channel: 'push.personal.plan.order',
        data: {
          id: 'plan-race-1',
          symbol: 'BTC_USDT',
          side: 4,
          triggerPrice: 49000,
          vol: 10000,
          state: 2,
          triggerType: 2,
          orderType: 5,
          reduceOnly: false,
          updateTime: 1_700_000_000_000,
        },
      }),
    } as MessageEvent);

    expect(exchange.store.orders.find((order) => order.id === 'plan-race-1')?.status).toBe(
      OrderStatus.Canceled,
    );
  });

  // Unit-level guard: pin the trigger-order state -> status mapping so a
  // future refactor cannot silently regress `normalizePlanOrder` back to the
  // regular-order `ORDER_STATUS` map (state=2 Open).
  it('normalizePlanOrder maps the MEXC trigger-order state enum to Open/Closed/Canceled correctly', async () => {
    const { normalizePlanOrder } = await import('../reference/mexc/mexc.utils');
    const base = {
      id: 'plan-state-1',
      symbol: 'BTC_USDT',
      side: 4,
      triggerPrice: 49000,
      vol: 1,
      triggerType: 2,
      orderType: 5,
      updateTime: 1_700_000_000_000,
    };

    // state=1 (Untriggered) is Open, state=3 (Executed) is Closed, and
    // state=2/4/5 (Cancelled / Invalid / Failed) all collapse to Canceled.
    expect(normalizePlanOrder({ ...base, state: 1 }, market)?.status).toBe(OrderStatus.Open);
    expect(normalizePlanOrder({ ...base, state: 2 }, market)?.status).toBe(OrderStatus.Canceled);
    expect(normalizePlanOrder({ ...base, state: 3 }, market)?.status).toBe(OrderStatus.Closed);
    expect(normalizePlanOrder({ ...base, state: 4 }, market)?.status).toBe(OrderStatus.Canceled);
    expect(normalizePlanOrder({ ...base, state: 5 }, market)?.status).toBe(OrderStatus.Canceled);
  });

  it('upserts a stop-order row from push.personal.stop.order and marks canceled state', () => {
    const exchange = createExchange();
    exchange.store.update({ markets: [market] });

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        channel: 'push.personal.stop.order',
        data: {
          id: 'stop-ws-1',
          symbol: 'BTC_USDT',
          positionId: 'native-position-1',
          positionType: 1,
          stopLossPrice: 49000,
          takeProfitPrice: 0,
          triggerSide: 2,
          state: 1,
          vol: 10000,
          volType: 2,
          updateTime: 1_700_000_000_000,
        },
      }),
    } as MessageEvent);

    expect(exchange.store.orders.find((order) => order.id === 'stop-ws-1')).toMatchObject({
      id: 'stop-ws-1',
      status: OrderStatus.Open,
      type: OrderType.StopLoss,
    });

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        channel: 'push.personal.stop.order',
        data: {
          id: 'stop-ws-1',
          symbol: 'BTC_USDT',
          positionId: 'native-position-1',
          positionType: 1,
          stopLossPrice: 49000,
          takeProfitPrice: 0,
          triggerSide: 2,
          state: 2, // cancelled
          vol: 10000,
          volType: 2,
          updateTime: 1_700_000_000_001,
        },
      }),
    } as MessageEvent);

    expect(exchange.store.orders.find((order) => order.id === 'stop-ws-1')?.status).toBe(OrderStatus.Canceled);
  });

  it('preserves a recently-placed stop-like row across a lagging symbol-scoped REST replace', async () => {
    const exchange = createExchange();
    const now = 1_700_000_000_000;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);

    // In-store optimistic plan order added by placeOrder right before the
    // shielded snapshot fires.
    exchange.store.update({
      markets: [market],
      orders: [
        {
          id: 'shielded-plan-1',
          status: OrderStatus.Open,
          symbol: 'BTCUSDT',
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 49000,
          amount: 1,
          filled: 0,
          remaining: 1,
          reduceOnly: true,
          timeInForce: OrderTimeInForce.GoodTillCancel,
          lastUpdate: now,
          info: { __mexcPlanOrder: true },
        } as Order,
      ],
    });
    (exchange as any).notePlacedStopLikeOrder('shielded-plan-1');

    const get = vi.fn(async (url: string) => {
      // MEXC's `planorder/list` snapshot lags — none of the three list
      // endpoints know about `shielded-plan-1` yet.
      if (
        url === `${ENDPOINTS.OPEN_ORDERS}/BTC_USDT` ||
        url === ENDPOINTS.PLAN_ORDER_LIST ||
        url === ENDPOINTS.STOP_ORDER_LIST
      ) {
        return { data: { success: true, code: 0, data: [] } };
      }
      throw new Error(`unexpected URL ${url}`);
    });
    exchange.xhr = { get } as any;

    await exchange.fetchAndSetOrders(['BTCUSDT']);

    // Without the shield the wholesale replace would have dropped the row.
    expect(exchange.store.orders.map((order) => order.id)).toEqual(['shielded-plan-1']);
  });

  it('drops a stop-like row from the wholesale replace once the placement shield expires', async () => {
    const exchange = createExchange();
    const now = 1_700_000_000_000;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);

    exchange.store.update({
      markets: [market],
      orders: [
        {
          id: 'stale-plan-1',
          status: OrderStatus.Open,
          symbol: 'BTCUSDT',
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 49000,
          amount: 1,
          filled: 0,
          remaining: 1,
          reduceOnly: true,
          timeInForce: OrderTimeInForce.GoodTillCancel,
          lastUpdate: now - 60_000,
          info: { __mexcPlanOrder: true },
        } as Order,
      ],
    });
    // Placement was long enough ago that the shield entry has expired — no
    // shield lookup should preserve the row anymore.
    (exchange as any).recentlyPlacedStopLikeOrderIds.set('stale-plan-1', now - 60_000);

    const get = vi.fn(async (url: string) => {
      if (
        url === `${ENDPOINTS.OPEN_ORDERS}/BTC_USDT` ||
        url === ENDPOINTS.PLAN_ORDER_LIST ||
        url === ENDPOINTS.STOP_ORDER_LIST
      ) {
        return { data: { success: true, code: 0, data: [] } };
      }
      throw new Error(`unexpected URL ${url}`);
    });
    exchange.xhr = { get } as any;

    await exchange.fetchAndSetOrders(['BTCUSDT']);

    expect(exchange.store.orders).toEqual([]);
  });

  it('coalesces rapid scheduleStopOrderSnapshotRefresh bursts into one delayed refresh per symbol', async () => {
    vi.useFakeTimers();
    try {
      const exchange = createExchange();
      exchange.store.update({ markets: [market] });
      const fetchAndSetOrders = vi
        .spyOn(exchange, 'fetchAndSetOrders')
        .mockResolvedValue([]);

      // Simulate a chart-drag burst placing five TP/SL rows on the same
      // symbol in ~50 ms. Without coalescing, each placement would fan out
      // to `GET /planorder/list` + `GET /stoporder/list` after 1.5 s, five
      // rows deep — the exact pattern seen in the user's HAR dump.
      for (let i = 0; i < 5; i++) {
        (exchange as any).scheduleStopOrderSnapshotRefresh('BTCUSDT');
      }

      expect(fetchAndSetOrders).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(1_499);
      expect(fetchAndSetOrders).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(1);
      expect(fetchAndSetOrders).toHaveBeenCalledTimes(1);
      expect(fetchAndSetOrders).toHaveBeenCalledWith(['BTCUSDT']);
    } finally {
      vi.useRealTimers();
    }
  });

  it('cancels pending stop-order snapshot refreshes on dispose', async () => {
    vi.useFakeTimers();
    try {
      const exchange = createExchange();
      exchange.store.update({ markets: [market] });
      const fetchAndSetOrders = vi
        .spyOn(exchange, 'fetchAndSetOrders')
        .mockResolvedValue([]);

      (exchange as any).scheduleStopOrderSnapshotRefresh('BTCUSDT');
      exchange.dispose();

      await vi.advanceTimersByTimeAsync(5_000);
      expect(fetchAndSetOrders).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it('drops a stale push.personal.plan.order Open frame for a locally-canceled id (no re-open flicker)', async () => {
    const exchange = createExchange();
    exchange.store.update({
      markets: [market],
      orders: [
        {
          id: 'plan-flicker-1',
          status: OrderStatus.Canceled,
          symbol: 'BTCUSDT',
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 49000,
          amount: 1,
          filled: 0,
          remaining: 1,
          reduceOnly: true,
          timeInForce: OrderTimeInForce.GoodTillCancel,
          info: { __mexcPlanOrder: true },
        } as Order,
      ],
    });
    // Simulate the cancel path having just noted the id (post-cancel-HTTP,
    // status already Canceled locally).
    (exchange as any).noteRecentlyCanceledStopLikeOrders([exchange.store.orders[0]]);

    // Stale Open frame flushed by MEXC after our cancel HTTP already returned.
    // Without the guard this would flip the row back to Open, then the next
    // Canceled frame (or REST tick) would flip it Canceled again — the
    // Canceled → Open → Canceled ping-pong the user is seeing as flicker.
    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        channel: 'push.personal.plan.order',
        data: {
          id: 'plan-flicker-1',
          symbol: 'BTC_USDT',
          side: 4,
          triggerPrice: 49000,
          vol: 10000,
          state: 1,
          triggerType: 2,
          orderType: 5,
          reduceOnly: true,
          updateTime: 1_700_000_000_050,
        },
      }),
    } as MessageEvent);

    expect(exchange.store.orders.find((order) => order.id === 'plan-flicker-1')?.status).toBe(OrderStatus.Canceled);
  });

  it('honors a real Canceled push.personal.stop.order frame even when the id is in the cancel shield', async () => {
    const exchange = createExchange();
    exchange.store.update({
      markets: [market],
      orders: [
        {
          id: 'stop-flicker-1',
          status: OrderStatus.Open,
          symbol: 'BTCUSDT',
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 49000,
          amount: 0,
          filled: 0,
          remaining: 0,
          reduceOnly: true,
          timeInForce: OrderTimeInForce.GoodTillCancel,
          info: { __mexcStopOrder: true },
        } as Order,
      ],
    });
    (exchange as any).noteRecentlyCanceledStopLikeOrders([exchange.store.orders[0]]);

    // The authoritative Canceled frame from MEXC. The cancel-shield lookup
    // must NOT drop this — only Open frames are treated as stale for shielded
    // ids. This is what actually lands the row in its final Canceled state
    // when the shield check would otherwise be over-eager.
    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        channel: 'push.personal.stop.order',
        data: {
          id: 'stop-flicker-1',
          symbol: 'BTC_USDT',
          positionType: 1,
          triggerSide: 2,
          stopLossPrice: 49000,
          state: 2, // canceled
          vol: 10000,
          volType: 2,
          updateTime: 1_700_000_000_100,
        },
      }),
    } as MessageEvent);

    expect(exchange.store.orders.find((order) => order.id === 'stop-flicker-1')?.status).toBe(OrderStatus.Canceled);
  });

  it('clears the placement shield after cancelling the stop-like order', async () => {
    const exchange = createExchange();
    const post = vi.fn(async () => ({ data: { success: true, code: 0 } }));
    exchange.xhr = { post } as any;
    (exchange as any).notePlacedStopLikeOrder('stop-cancel-1');
    expect((exchange as any).recentlyPlacedStopLikeOrderIds.has('stop-cancel-1')).toBe(true);

    await exchange.cancelOrders([
      {
        id: 'stop-cancel-1',
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        status: OrderStatus.Open,
        amount: 0,
        remaining: 0,
        price: 49000,
        info: { __mexcStopOrder: true },
      } as Order,
    ]);

    expect((exchange as any).recentlyPlacedStopLikeOrderIds.has('stop-cancel-1')).toBe(false);
  });

  it('lets a real WS order payload overwrite the optimistic regular row without duplicating it', async () => {
    const exchange = createExchange();
    const now = 1_700_000_000_000;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);

    const post = vi.fn(async () => ({
      data: { success: true, code: 0, data: { orderId: 'regular-new' } },
    }));
    exchange.store.update({ markets: [market] });
    exchange.xhr = { post } as any;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 10,
      price: 60000,
      reduceOnly: false,
      timeInForce: OrderTimeInForce.GoodTillCancel,
    });

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        channel: 'push.personal.order',
        data: {
          orderId: 'regular-new',
          symbol: 'BTC_USDT',
          side: 1,
          type: 1,
          state: 2,
          price: 60000,
          vol: 10,
          dealVol: 1,
          updateTime: now + 100,
        },
      }),
    } as MessageEvent);

    expect(exchange.store.orders).toHaveLength(1);
    expect(exchange.store.orders[0]).toMatchObject({
      id: 'regular-new',
      status: OrderStatus.Open,
      symbol: 'BTCUSDT',
      filled: 0.0001,
    });
    // Optimistic marker is replaced by the real payload.
    expect((exchange.store.orders[0].info as Record<string, any>).__mexcOptimisticRegular).toBeUndefined();
  });
});
