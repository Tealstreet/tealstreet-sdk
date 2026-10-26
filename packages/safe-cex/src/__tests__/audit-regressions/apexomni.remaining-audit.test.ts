import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApexOmniExchange } from '../reference/apexomni/apexomni.exchange';
import { ENDPOINTS } from '../reference/apexomni/apexomni.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderStopTriggerType, OrderType, PositionSide } from '../../types';
import { virtualClock } from '../../utils/virtualClock';

const market = (id: string, symbol = id) => ({
  id,
  symbol,
  base: symbol.split('-')[0],
  quote: symbol.split('-')[1] || 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
});

const ticker = (id: string, symbol = id) => ({
  id,
  symbol,
  bid: 99,
  ask: 101,
  last: 100,
  mark: 100,
  index: 100,
  percentage: 0,
  openInterest: 0,
  fundingRate: 0,
  fundingTime: 0,
  volume: 0,
  quoteVolume: 0,
});

const createExchange = (store = new DefaultStore()) =>
  new ApexOmniExchange(
    'apexomni' as any,
    { key: 'key', secret: 'secret', passphrase: 'passphrase', omniKeySeed: 'seed' } as any,
    store,
  );

describe('ApeX Omni remaining audit regressions', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('reads account identity from the documented v3 account envelope', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({ data: { data: { id: 'apex-account-id' } } });
    exchange.xhr = { get } as any;

    await expect(exchange.getAccount()).resolves.toEqual({ userId: 'apex-account-id', affiliateId: null });
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT);
  });

  it('uses freshly fetched position state when setting leverage and fails clearly if still unavailable', async () => {
    const store = new DefaultStore();
    store.update({ markets: [market('BTC-USDT')] as any });
    const exchange = createExchange(store);
    const fetchedPosition = { symbol: 'BTC-USDT', leverage: 5 } as any;
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValueOnce([fetchedPosition]).mockResolvedValueOnce([]);
    const post = vi.fn().mockResolvedValue({ data: { code: 0 } });
    exchange.unlimitedXHR = { post } as any;

    await expect(exchange.setLeverage('BTC-USDT', 10)).resolves.toBeUndefined();
    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.SET_LEVERAGE,
      { symbol: 'BTC-USDT', token: 'USDT', initialMarginRate: '0.10000' },
      { headers: { 'Content-Type': 'multipart/form-data' } },
    );

    store.update({ positions: [] });
    await expect(exchange.setLeverage('BTC-USDT', 10)).rejects.toThrow(
      'Position state for BTC-USDT is unavailable; cannot set leverage',
    );
  });

  it('builds the public socket URL with a live timestamp and one query string', () => {
    const exchange = createExchange();
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(1_800_000_000_123);
    const socket = { addEventListener: vi.fn(), close: vi.fn() };
    const createWebsocket = vi.spyOn(exchange.publicWebsocket, 'createWebsocket').mockReturnValue(socket as any);

    exchange.publicWebsocket.connectAndSubscribe();

    const url = createWebsocket.mock.calls[0][0];
    expect(url).toContain('?v=2&timestamp=1800000000123&requestId=');
    expect(url.match(/\?/g)).toHaveLength(1);
  });

  it('continues a ticker batch when one symbol cannot run local PnL', () => {
    const store = new DefaultStore();
    store.update({
      markets: [market('BTC-USDT')] as any,
      tickers: [ticker('UNKNOWN-USDT'), ticker('BTC-USDT')],
      positions: [
        {
          id: 'UNKNOWN-USDT-long',
          symbol: 'UNKNOWN-USDT',
          side: PositionSide.Long,
          entryPrice: 10,
          contracts: 1,
        },
      ] as any,
    });
    const exchange = createExchange(store);

    exchange.publicWebsocket.handleInstrumentInfoEvent({
      data: [
        { s: 'UNKNOWN-USDT', p: '11', mp: '11' },
        { s: 'BTC-USDT', p: '123', mp: '124' },
      ],
    });

    expect(store.tickersBySymbol['UNKNOWN-USDT'].last).toBe(11);
    expect(store.tickersBySymbol['BTC-USDT']).toMatchObject({ last: 123, mark: 124 });
  });

  it('skips an unmappable private order without aborting later orders in the batch', () => {
    const exchange = createExchange();
    const websocket = exchange.privateWebsocket;
    vi.spyOn(websocket, 'throttleFetchOrders').mockImplementation(() => undefined);
    const mappedOrder = {
      id: 'known',
      symbol: 'BTC-USDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 100,
      amount: 1,
      filled: 0,
      remaining: 1,
      status: OrderStatus.Open,
      reduceOnly: false,
      lastUpdate: 0,
    } as any;
    vi.spyOn(exchange, 'mapOrder').mockReturnValueOnce([]).mockReturnValueOnce([mappedOrder]);
    const addOrUpdateOrders = vi.spyOn(exchange.store, 'addOrUpdateOrders');

    expect(() =>
      websocket.handleOrderTopic([
        { id: 'unknown', status: 'OPEN', updatedAt: Date.now(), price: '1', size: '1' },
        { id: 'known', status: 'OPEN', updatedAt: Date.now(), price: '100', size: '1' },
      ] as any),
    ).not.toThrow();

    expect(addOrUpdateOrders).toHaveBeenCalledOnce();
    expect(addOrUpdateOrders).toHaveBeenCalledWith([mappedOrder]);
  });

  it('preserves the sibling protective leg when updating a virtual bracket child', async () => {
    const store = new DefaultStore();
    const baseOrder = {
      id: 'parent',
      symbol: 'BTC-USDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 100,
      amount: 2,
      status: OrderStatus.Open,
      filled: 0,
      remaining: 2,
      reduceOnly: false,
      info: { id: 'parent' },
    } as any;
    const stopLoss = {
      ...baseOrder,
      id: 'parent__stop_loss',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      price: 90,
      slTriggerType: OrderStopTriggerType.IndexPrice,
    } as any;
    const takeProfit = {
      ...baseOrder,
      id: 'parent__take_profit',
      type: OrderType.TakeProfit,
      side: OrderSide.Sell,
      price: 110,
      tpTriggerType: OrderStopTriggerType.LastPrice,
    } as any;
    store.update({ orders: [baseOrder, stopLoss, takeProfit] });
    const exchange = createExchange(store);
    const cancel = vi.spyOn(exchange, 'cancelOrders').mockResolvedValue(undefined);
    const place = vi.spyOn(exchange, 'placeOrder').mockResolvedValue(['replacement']);

    await expect(
      exchange.updateOrder({ order: takeProfit, update: { price: 115 } }),
    ).resolves.toEqual(['replacement']);

    expect(cancel).toHaveBeenCalledWith([baseOrder]);
    expect(place).toHaveBeenCalledWith({
      symbol: 'BTC-USDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 100,
      amount: 2,
      reduceOnly: false,
      stopLoss: 90,
      slTriggerType: OrderStopTriggerType.IndexPrice,
      takeProfit: 115,
      tpTriggerType: OrderStopTriggerType.LastPrice,
    });
  });

  it('rejects a virtual bracket update when replacement fails after cancellation', async () => {
    const store = new DefaultStore();
    const baseOrder = {
      id: 'parent',
      symbol: 'BTC-USDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 100,
      amount: 2,
      status: OrderStatus.Open,
      filled: 0,
      remaining: 2,
      reduceOnly: false,
      info: { id: 'parent' },
    } as any;
    const stopLoss = {
      ...baseOrder,
      id: 'parent__stop_loss',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      price: 90,
      slTriggerType: OrderStopTriggerType.IndexPrice,
    } as any;
    store.update({ orders: [baseOrder, stopLoss] });
    const exchange = createExchange(store);
    const cancel = vi.spyOn(exchange, 'cancelOrders').mockResolvedValue(undefined);
    const replacementError = new Error('replacement failed');
    vi.spyOn(exchange, 'placeOrder').mockRejectedValue(replacementError);

    await expect(
      exchange.updateOrder({ order: stopLoss, update: { price: 92 } }),
    ).rejects.toBe(replacementError);
    expect(cancel).toHaveBeenCalledWith([baseOrder]);
  });
});
