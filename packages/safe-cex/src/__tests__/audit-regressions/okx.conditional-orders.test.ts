import { describe, expect, it, vi } from 'vitest';

import { getExchangeMetadata } from '../../exchangeMetadata';
import { OKXExchange } from '../reference/okx/okx.exchange';
import { BROKER_ID, ENDPOINTS } from '../../exchanges/okx/okx.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStopTriggerType, OrderType, PositionMode } from '../../types';

const createExchange = (options: Record<string, any> = {}) => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC-USDT-SWAP',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
        contractSize: 1,
      },
    ] as any,
    tickers: [
      {
        id: 'BTC-USDT-SWAP',
        symbol: 'BTCUSDT',
        bid: 59_999,
        ask: 60_001,
        last: 60_000,
        mark: 60_000,
        index: 60_000,
        percentage: 0,
        openInterest: 0,
        fundingRate: 0,
        fundingTime: 0,
        volume: 0,
        quoteVolume: 0,
      },
    ],
  });

  const exchange = new OKXExchange(
    'okx',
    { key: 'key', secret: 'secret', passphrase: 'passphrase', ...options } as any,
    store,
  );
  exchange._accountPositionMode = PositionMode.OneWay;
  exchange._accountLevel = 2;

  const post = vi.fn().mockResolvedValue({ data: { data: [{ algoId: 'algo-1' }] } });
  const get = vi.fn();
  exchange.xhr = { post, get } as any;
  return { exchange, get, post, store };
};

const withoutDynamicIds = (payload: Record<string, unknown>) => {
  const { algoClOrdId, ...contract } = payload;
  expect(algoClOrdId).toEqual(expect.stringMatching(new RegExp(`^${BROKER_ID}`)));
  return contract;
};

const rawTriggerOrder = (overrides: Record<string, any> = {}) => ({
  algoId: 'trigger-1',
  instId: 'BTC-USDT-SWAP',
  instType: 'SWAP',
  ordType: 'trigger',
  side: 'buy',
  posSide: 'net',
  tdMode: 'cross',
  sz: '0.5',
  triggerPx: '61000',
  triggerPxType: 'last',
  ordPx: '-1',
  last: '60000',
  reduceOnly: 'false',
  state: 'live',
  ...overrides,
});

describe('audit regression: OKX opening conditional orders', () => {
  it('advertises opening triggers only after OKX trigger lifecycle is wired', () => {
    expect(getExchangeMetadata('okx').supportsOpeningTriggerOrders).toBe(true);
    expect(getExchangeMetadata('okxeu').supportsOpeningTriggerOrders).toBe(true);
  });

  it('places opening conditional market triggers as OKX trigger algo orders', async () => {
    const { exchange, post } = createExchange();

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Conditional,
        side: OrderSide.Buy,
        amount: 0.1254,
        triggerPrice: 60_123.16,
        reduceOnly: true,
      }),
    ).resolves.toEqual(['algo-1']);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ALGO_ORDER,
      expect.objectContaining({
        instId: 'BTC-USDT-SWAP',
        tdMode: 'cross',
        tag: BROKER_ID,
        side: 'buy',
        ordType: 'trigger',
        sz: '0.125',
        triggerPx: '60123.2',
        orderPx: '-1',
        triggerPxType: 'last',
      }),
    );
    expect(post.mock.calls[0][1]).not.toHaveProperty('reduceOnly');
    expect(post.mock.calls[0][1]).not.toHaveProperty('slTriggerPx');
    expect(post.mock.calls[0][1]).not.toHaveProperty('tpTriggerPx');
  });

  it('places opening conditional limit triggers with selectable trigger source', async () => {
    const { exchange, post } = createExchange();

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Conditional,
      side: OrderSide.Sell,
      amount: 0.25,
      price: 59_900,
      triggerPrice: 60_000,
      triggerType: OrderStopTriggerType.IndexPrice,
    });

    expect(withoutDynamicIds(post.mock.calls[0][1])).toEqual({
      instId: 'BTC-USDT-SWAP',
      tdMode: 'cross',
      tag: BROKER_ID,
      side: 'sell',
      ordType: 'trigger',
      sz: '0.25',
      triggerPx: '60000',
      orderPx: '59900',
      triggerPxType: 'index',
    });
  });

  it('keeps opening conditional posSide even when a reduceOnly flag is present in hedge mode', async () => {
    const { exchange, post } = createExchange();
    exchange._accountPositionMode = PositionMode.Hedged;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Conditional,
      side: OrderSide.Buy,
      amount: 0.25,
      triggerPrice: 60_000,
      reduceOnly: true,
    });

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ALGO_ORDER,
      expect.objectContaining({
        ordType: 'trigger',
        posSide: 'long',
      }),
    );
  });

  it.each([undefined, 0, -1, Number.NaN])('emits and rejects invalid opening trigger price %s', async (triggerPrice) => {
    const { exchange, post } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Conditional,
        side: OrderSide.Buy,
        amount: 1,
        triggerPrice,
      }),
    ).rejects.toThrow('Trigger price is required for OKX opening conditional orders');

    expect(emitAndLogError).toHaveBeenCalledWith('Trigger price is required for OKX opening conditional orders');
    expect(post).not.toHaveBeenCalled();
  });

  it('maps OKX trigger algo orders to opening conditionals', () => {
    const { exchange } = createExchange();

    const [order] = exchange.mapOrders([rawTriggerOrder({ triggerPxType: 'mark', ordPx: '60900' })]);

    expect(order.type).toBe(OrderType.Conditional);
    expect(order.id).toBe('trigger-1');
    expect(order.price).toBe(61_000);
    expect(order.amount).toBe(0.5);
    expect(order.remaining).toBe(0.5);
    expect(order.reduceOnly).toBe(false);
    expect(order.triggerType).toBe(OrderStopTriggerType.MarkPrice);
    expect(order.extra?.conditionalOrderPrice).toBe(60_900);
  });

  it('maps reduce-only OKX trigger algo rows as protective, not opening conditional', () => {
    const { exchange } = createExchange();

    const [takeProfit] = exchange.mapOrders([
      rawTriggerOrder({ algoId: 'reduce-tp', side: 'sell', triggerPx: '61000', reduceOnly: 'true' }),
    ]);
    const [stopLoss] = exchange.mapOrders([
      rawTriggerOrder({ algoId: 'reduce-sl', side: 'sell', triggerPx: '59000', reduceOnly: 'true' }),
    ]);

    expect(takeProfit.type).toBe(OrderType.TakeProfit);
    expect(takeProfit.id).toBe('reduce-tp_tp');
    expect(takeProfit.tpTriggerType).toBe(OrderStopTriggerType.LastPrice);
    expect(takeProfit.triggerType).toBeUndefined();
    expect(stopLoss.type).toBe(OrderType.StopLoss);
    expect(stopLoss.id).toBe('reduce-sl_sl');
    expect(stopLoss.slTriggerType).toBe(OrderStopTriggerType.LastPrice);
    expect(stopLoss.triggerType).toBeUndefined();
  });

  it('classifies reduce-only trigger rows against their selected trigger source', () => {
    const { exchange, store } = createExchange();
    exchange.markPriceLoaded('BTC-USDT-SWAP');
    store.update({
      tickers: [
        {
          id: 'BTC-USDT-SWAP',
          symbol: 'BTCUSDT',
          bid: 59_999,
          ask: 60_001,
          last: 60_000,
          mark: 62_000,
          index: 60_000,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        },
      ],
    });

    const [stopLoss] = exchange.mapOrders([
      rawTriggerOrder({
        algoId: 'reduce-mark-sl',
        side: 'sell',
        triggerPx: '61000',
        triggerPxType: 'mark',
        reduceOnly: 'true',
      }),
    ]);

    expect(stopLoss.type).toBe(OrderType.StopLoss);
    expect(stopLoss.id).toBe('reduce-mark-sl_sl');
    expect(stopLoss.slTriggerType).toBe(OrderStopTriggerType.MarkPrice);
  });

  it('does not classify mark/index reduce-only trigger rows from seeded startup prices', () => {
    const { exchange } = createExchange();

    const [order] = exchange.mapOrders([
      rawTriggerOrder({
        algoId: 'reduce-seeded-mark',
        side: 'sell',
        triggerPx: '61000',
        triggerPxType: 'mark',
        reduceOnly: 'true',
      }),
    ]);

    expect(order.type).toBe(OrderType.Conditional);
    expect(order.id).toBe('reduce-seeded-mark');
    expect(order.triggerType).toBe(OrderStopTriggerType.MarkPrice);
  });

  it('classifies mark/index reduce-only trigger rows when in-process public source loaded the source', () => {
    const { exchange: source } = createExchange();
    source.markPriceLoaded('BTC-USDT-SWAP');
    const { exchange } = createExchange({ externalPublicData: { type: 'in-process', source } });

    const [order] = exchange.mapOrders([
      rawTriggerOrder({
        algoId: 'reduce-in-process-mark',
        side: 'sell',
        triggerPx: '61000',
        triggerPxType: 'mark',
        reduceOnly: 'true',
      }),
    ]);

    expect(order.type).toBe(OrderType.TakeProfit);
    expect(order.id).toBe('reduce-in-process-mark_tp');
  });

  it('preserves OKX conditional TP/SL algo mapping separately from trigger orders', () => {
    const { exchange } = createExchange();

    const orders = exchange.mapOrders([
      {
        algoId: 'protective-1',
        instId: 'BTC-USDT-SWAP',
        ordType: 'conditional',
        side: 'sell',
        sz: '0.5',
        slTriggerPx: '59000',
        slTriggerPxType: 'mark',
        tpTriggerPx: '61000',
        tpTriggerPxType: 'last',
        reduceOnly: 'true',
      },
    ]);

    expect(orders.map((order) => order.type)).toEqual([OrderType.StopLoss, OrderType.TakeProfit]);
    expect(orders.map((order) => order.id)).toEqual(['protective-1_sl', 'protective-1_tp']);
  });

  it('amends trigger edits while preserving the original execution price', async () => {
    const { exchange, post, store } = createExchange();
    const [order] = exchange.mapOrders([rawTriggerOrder({ algoId: 'trigger-edit', ordPx: '60900' })]);
    store.update({ orders: [order] });

    await expect(exchange.updateOrder({ order, update: { price: 60_500, amount: 0.25 } })).resolves.toEqual([
      'trigger-edit',
    ]);

    expect(post).toHaveBeenCalledTimes(1);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.UPDATE_ALGO_ORDER, {
      instId: 'BTC-USDT-SWAP',
      algoId: 'trigger-edit',
      newTriggerPx: '60500',
      newOrdPx: '60900',
      newTriggerPxType: 'last',
      newSz: '0.25',
    });
  });

  it('sends required trigger fields for amount-only trigger edits', async () => {
    const { exchange, post, store } = createExchange();
    const [order] = exchange.mapOrders([rawTriggerOrder({ algoId: 'trigger-edit', ordPx: '60900' })]);
    store.update({ orders: [order] });

    await expect(exchange.updateOrder({ order, update: { amount: 0.25 } })).resolves.toEqual(['trigger-edit']);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.UPDATE_ALGO_ORDER, {
      instId: 'BTC-USDT-SWAP',
      algoId: 'trigger-edit',
      newTriggerPx: '61000',
      newOrdPx: '60900',
      newTriggerPxType: 'last',
      newSz: '0.25',
    });
  });

  it('rejects failed trigger amend rows without accepting the edit', async () => {
    const { exchange, post, store } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    post.mockResolvedValueOnce({
      data: { data: [{ algoId: 'trigger-edit', sCode: '51000', sMsg: 'trigger price error' }] },
    });
    const [order] = exchange.mapOrders([rawTriggerOrder({ algoId: 'trigger-edit' })]);
    store.update({ orders: [order] });

    await expect(exchange.updateOrder({ order, update: { price: 60_500 } })).rejects.toThrow('trigger price error');

    expect(emitAndLogError).toHaveBeenCalledWith('trigger price error');
  });

  it('rejects top-level failed trigger amend responses', async () => {
    const { exchange, post, store } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    post.mockResolvedValueOnce({
      data: { code: '51000', msg: 'top-level trigger error', data: [] },
    });
    const [order] = exchange.mapOrders([rawTriggerOrder({ algoId: 'trigger-edit' })]);
    store.update({ orders: [order] });

    await expect(exchange.updateOrder({ order, update: { price: 60_500 } })).rejects.toThrow(
      'top-level trigger error',
    );

    expect(emitAndLogError).toHaveBeenCalledWith('top-level trigger error');
  });

  it('cancels OKX algo orders in batches of ten', async () => {
    const { exchange, post } = createExchange();
    const orders = Array.from({ length: 11 }, (_, index) => ({
      id: `algo-${index + 1}_tp`,
      symbol: 'BTCUSDT',
      type: OrderType.TakeProfit,
    })) as any;

    await exchange.cancelOrders(orders);

    expect(post).toHaveBeenNthCalledWith(
      1,
      ENDPOINTS.CANCEL_ALGO_ORDERS,
      Array.from({ length: 10 }, (_, index) => ({ instId: 'BTC-USDT-SWAP', algoId: `algo-${index + 1}` })),
    );
    expect(post).toHaveBeenNthCalledWith(2, ENDPOINTS.CANCEL_ALGO_ORDERS, [
      { instId: 'BTC-USDT-SWAP', algoId: 'algo-11' },
    ]);
  });
});
