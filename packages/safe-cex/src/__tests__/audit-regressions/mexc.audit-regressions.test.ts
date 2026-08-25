import { describe, expect, it, vi } from 'vitest';

import { getExchangeMetadata } from '../../exchangeMetadata';
import { signMexcRequest, signMexcWsLogin, serializeMexcParams } from '../../exchanges/mexc/mexc.api';
import { MexcExchange } from '../../exchanges/mexc/mexc.exchange';
import { ENDPOINTS } from '../../exchanges/mexc/mexc.types';
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
} from '../../exchanges/mexc/mexc.utils';
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

  it('normalizes tickers with venue volume converted from native contracts', () => {
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
    expect(ticker.openInterest).toBeCloseTo(65245.2606);
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

  it('cancels regular orders with the documented orderIds payload', async () => {
    const exchange = createExchange();
    const post = vi.fn(async () => ({ data: { success: true, code: 0 } }));
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

    expect(post).toHaveBeenCalledWith(ENDPOINTS.ORDER_CANCEL, {
      orderIds: ['regular-1'],
    });
  });

  it('replaces normalized MEXC orders with native contract volume on price moves', async () => {
    const exchange = createExchange();
    const post = vi
      .fn()
      .mockResolvedValueOnce({ data: { success: true, code: 0 } })
      .mockResolvedValueOnce({ data: { success: true, code: 0, data: 'replacement-1' } });
    exchange.store.update({ markets: [market] });
    exchange.xhr = { post } as any;

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

    expect(post).toHaveBeenNthCalledWith(1, ENDPOINTS.ORDER_CANCEL, {
      orderIds: ['regular-1'],
    });
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
});
