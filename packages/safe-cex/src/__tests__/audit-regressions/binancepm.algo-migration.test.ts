import type { Order } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderStopTriggerType, OrderType, PositionSide } from '../../types';
import { BinancePMExchange } from '../reference/binance/binancepm.exchange';
import { BinancePMPrivateWebsocket } from '../reference/binance/binancepm.ws-private';

vi.mock('../../utils/uuid', () => ({ uuid: () => 'generated' }));

const regular = {
  clientOrderId: 'regular',
  symbol: 'BTCUSDT',
  type: 'LIMIT',
  side: 'BUY',
  price: '60000',
  origQty: '0.01',
  executedQty: '0',
  reduceOnly: false,
};
const algo = {
  algoId: 123,
  clientAlgoId: 'protection',
  algoType: 'CONDITIONAL',
  algoStatus: 'NEW',
  symbol: 'BTCUSDT',
  orderType: 'STOP_MARKET',
  side: 'SELL',
  positionSide: 'BOTH',
  price: '0',
  triggerPrice: '59000',
  quantity: '0.01',
  reduceOnly: true,
  closePosition: false,
  workingType: 'MARK_PRICE',
};
const subject = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTCUSDT',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
      },
    ] as any,
    tickers: [{ id: 'BTCUSDT', symbol: 'BTCUSDT', last: 60000 }] as any,
    positions: [{ symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 0.01 }] as any,
  });
  const exchange = new BinancePMExchange('binancepm', { key: 'fixture', secret: 'fixture' }, store);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
  return { exchange, store };
};
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe('Binance PM Algo Service migration', () => {
  it('joins both open-order families without the retired endpoint blocking the snapshot', async () => {
    const { exchange } = subject();
    const get = vi.spyOn(exchange.papiXhr, 'get').mockImplementation(
      async (url) =>
        ({
          data:
            url === '/papi/v1/um/openOrders'
              ? [regular]
              : url === '/papi/v1/um/algo/openAlgoOrders'
                ? [algo]
                : { code: 404, msg: 'retired', raw_response: '{}' },
        }) as any,
    );
    const orders = await exchange.fetchOrders();
    expect(orders).toHaveLength(2);
    expect(get).not.toHaveBeenCalledWith('/papi/v1/um/conditional/openOrders');
    expect(orders[1]).toMatchObject({
      id: 'protection',
      type: OrderType.StopLoss,
      amount: 0.01,
      remaining: 0.01,
      filled: 0,
      price: 59000,
      reduceOnly: true,
      close: false,
      slTriggerType: OrderStopTriggerType.MarkPrice,
    });
    expect(exchange.emitAndLogError).not.toHaveBeenCalled();
  });

  it.each([OrderType.StopLoss, OrderType.TakeProfit])(
    'places %s with algo fields and generated identity',
    async (type) => {
      const { exchange } = subject();
      const post = vi.spyOn(exchange.unlimitedXHR, 'post').mockResolvedValue({ data: { algoId: 123 } } as any);
      await expect(
        exchange.placeOrder({
          symbol: 'BTCUSDT',
          side: OrderSide.Sell,
          type,
          amount: 0.01,
          price: 59000,
          reduceOnly: true,
        }),
      ).resolves.toEqual(['x-s3vFFRQpgenerated']);
      expect(post).toHaveBeenCalledWith('/papi/v1/um/algo/order', {
        algoType: 'CONDITIONAL',
        type: type === OrderType.StopLoss ? 'STOP_MARKET' : 'TAKE_PROFIT_MARKET',
        symbol: 'BTCUSDT',
        positionSide: 'BOTH',
        side: 'SELL',
        quantity: '0.01',
        triggerPrice: '59000',
        reduceOnly: 'true',
        workingType: type === OrderType.StopLoss ? 'MARK_PRICE' : 'CONTRACT_PRICE',
        clientAlgoId: 'x-s3vFFRQpgenerated',
      });
    },
  );

  it('sends close-all protection without incompatible quantity or reduceOnly', async () => {
    const { exchange } = subject();
    const post = vi.spyOn(exchange.unlimitedXHR, 'post').mockResolvedValue({ data: { algoId: 123 } } as any);
    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      amount: 0,
      price: 59000,
      reduceOnly: true,
    });
    const payload = post.mock.calls[0][1] as any;
    expect(payload.closePosition).toBe('true');
    expect(payload).not.toHaveProperty('quantity');
    expect(payload).not.toHaveProperty('reduceOnly');
  });

  it.each([0.1, 20])('accepts trailing callbackRate %s on the new endpoint', async (rate) => {
    const { exchange } = subject();
    const post = vi.spyOn(exchange.unlimitedXHR, 'post').mockResolvedValue({ data: { algoId: 123 } } as any);
    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.TrailingStopLoss,
      amount: 0.01,
      price: 60000 * (1 - rate / 100),
    });
    expect(post).toHaveBeenCalledWith(
      '/papi/v1/um/algo/order',
      expect.objectContaining({
        algoType: 'CONDITIONAL',
        type: 'TRAILING_STOP_MARKET',
        callbackRate: String(rate),
      }),
    );
    const payload = post.mock.calls[0][1] as any;
    expect(payload.clientAlgoId).toBeTypeOf('string');
    expect(payload).not.toHaveProperty('newClientOrderId');
    expect(payload).not.toHaveProperty('activationPrice');
  });

  it.each([0, 20.1])('refuses trailing callbackRate %s visibly before a write', async (rate) => {
    const { exchange } = subject();
    const post = vi.spyOn(exchange.unlimitedXHR, 'post').mockResolvedValue({ data: {} } as any);
    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Sell,
        type: OrderType.TrailingStopLoss,
        amount: 0.01,
        price: 60000 * (1 - rate / 100),
      }),
    ).rejects.toThrow('0.1 to 20');
    expect(post).not.toHaveBeenCalled();
    expect(exchange.emitAndLogError).toHaveBeenCalledWith(expect.stringContaining('0.1 to 20'));
  });

  it('cancels one algo by clientAlgoId without strategy field names', async () => {
    const { exchange } = subject();
    const remove = vi.spyOn(exchange.papiXhr, 'delete').mockResolvedValue({ data: { complete: true } } as any);
    await exchange.cancelOrders([{ id: 'protection', symbol: 'BTCUSDT', type: OrderType.StopLoss } as Order]);
    expect(remove).toHaveBeenCalledWith('/papi/v1/um/algo/order', { params: { clientAlgoId: 'protection' } });
  });

  const protection = (): Order => ({
    id: 'protection',
    symbol: 'BTCUSDT',
    type: OrderType.StopLoss,
    side: OrderSide.Sell,
    status: OrderStatus.Open,
    amount: 0.01,
    filled: 0,
    remaining: 0.01,
    price: 59000,
    reduceOnly: true,
    lastUpdate: 0,
  });

  // PM's cancel response proves completion only with the literal boolean true.
  // https://developers.binance.com/en/docs/catalog/advanced-trading-derivatives-trading-portfolio-margin/api/rest-api/trade#cancel-um-algo-order
  it.each([{ complete: false }, {}, { complete: 'true' }, null, []])(
    'preserves an unconfirmed cancellation as unknown for response %j',
    async (data) => {
      const { exchange, store } = subject();
      const original = protection();
      store.update({ orders: [original] });
      const remove = vi.spyOn(exchange.papiXhr, 'delete').mockResolvedValue({ data } as any);

      await expect(exchange.cancelOrders([original])).rejects.toMatchObject({ outcome: 'unknown' });
      expect(remove).toHaveBeenCalledTimes(1);
      expect(store.orders.find((order) => order.id === original.id)).toMatchObject({ status: OrderStatus.Open });
      expect(exchange.emitAndLogError).toHaveBeenCalledWith(expect.stringContaining('outcome is unknown'));
    },
  );

  it.each([{ complete: false }, {}])('never replaces protection after an unconfirmed cancel %j', async (data) => {
    const { exchange, store } = subject();
    const original = protection();
    store.update({ orders: [original] });
    const remove = vi.spyOn(exchange.papiXhr, 'delete').mockResolvedValue({ data } as any);
    const post = vi.spyOn(exchange.unlimitedXHR, 'post').mockResolvedValue({ data: { algoId: 456 } } as any);

    await expect(exchange.updateOrder({ order: original, update: { price: 58000 } })).rejects.toMatchObject({
      outcome: 'unknown',
    });
    expect(remove).toHaveBeenCalledTimes(1);
    expect(post).not.toHaveBeenCalled();
    expect(store.orders.find((order) => order.id === original.id)).toMatchObject({ status: OrderStatus.Open });
  });

  it('retains confirmed prefix cancellation and stops the suffix after an unknown algo cancel', async () => {
    const { exchange, store } = subject();
    const prefix = { ...protection(), id: 'prefix', type: OrderType.Limit };
    const original = protection();
    const suffix = { ...prefix, id: 'suffix' };
    store.update({ orders: [prefix, original, suffix] });
    const remove = vi
      .spyOn(exchange.papiXhr, 'delete')
      .mockResolvedValueOnce({ data: {} } as any)
      .mockResolvedValueOnce({ data: { complete: false } } as any)
      .mockResolvedValue({ data: {} } as any);

    await expect(exchange.cancelOrders([prefix, original, suffix])).rejects.toMatchObject({ outcome: 'unknown' });
    expect(remove).toHaveBeenCalledTimes(2);
    expect(store.orders.map(({ id, status }) => ({ id, status }))).toEqual([
      { id: 'prefix', status: OrderStatus.Canceled },
      { id: 'protection', status: OrderStatus.Open },
      { id: 'suffix', status: OrderStatus.Open },
    ]);
  });

  it('allows replacement only after the algo cancellation proves complete', async () => {
    const { exchange, store } = subject();
    const original = protection();
    store.update({ orders: [original] });
    const remove = vi.spyOn(exchange.papiXhr, 'delete').mockResolvedValue({ data: { complete: true } } as any);
    const post = vi.spyOn(exchange.unlimitedXHR, 'post').mockResolvedValue({ data: { algoId: 456 } } as any);

    await expect(exchange.updateOrder({ order: original, update: { price: 58000 } })).resolves.toEqual([
      'x-s3vFFRQpgenerated',
    ]);
    expect(remove).toHaveBeenCalledTimes(1);
    expect(post).toHaveBeenCalledTimes(1);
    expect(post).toHaveBeenCalledWith(
      '/papi/v1/um/algo/order',
      expect.objectContaining({ triggerPrice: '58000', clientAlgoId: 'x-s3vFFRQpgenerated' }),
    );
    expect(store.orders.find((order) => order.id === original.id)?.status).toBe(OrderStatus.Canceled);
  });

  it('cancels both symbol families and preserves the failed family', async () => {
    const { exchange, store } = subject();
    const base = {
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      status: OrderStatus.Open,
      amount: 0.01,
      filled: 0,
      remaining: 0.01,
      price: 59000,
      reduceOnly: true,
      lastUpdate: 0,
    };
    store.update({
      orders: [
        { ...base, id: 'regular', type: OrderType.Limit },
        { ...base, id: 'protection', type: OrderType.StopLoss },
      ],
    });
    const remove = vi.spyOn(exchange.papiXhr, 'delete').mockImplementation(async (url) => {
      if (url === '/papi/v1/um/algo/allOpenOrders') throw new Error('algo cancel failed');
      return { data: { code: 200 } } as any;
    });
    await expect(exchange.cancelSymbolOrders('BTCUSDT')).rejects.toThrow('algo cancel failed');
    expect(remove.mock.calls.map((call) => call[0])).toEqual([
      '/papi/v1/um/allOpenOrders',
      '/papi/v1/um/algo/allOpenOrders',
    ]);
    expect(store.orders.find((o) => o.id === 'regular')?.status).toBe(OrderStatus.Canceled);
    expect(store.orders.find((o) => o.id === 'protection')?.status).toBe(OrderStatus.Open);
  });

  it.each([OrderType.Limit, OrderType.Market])('keeps ordinary %s request bytes identical to master', async (type) => {
    const { exchange } = subject();
    const post = vi.spyOn(exchange.unlimitedXHR, 'post').mockResolvedValue({ data: {} } as any);
    const remove = vi.spyOn(exchange.papiXhr, 'delete').mockResolvedValue({ data: {} } as any);
    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type,
      price: 60000,
      amount: 0.01,
      clientOrderId: 'ordinary',
    });
    expect(post).toHaveBeenCalledWith('/papi/v1/um/order', {
      symbol: 'BTCUSDT',
      positionSide: 'BOTH',
      side: 'BUY',
      type: type === OrderType.Limit ? 'LIMIT' : 'MARKET',
      quantity: '0.01',
      ...(type === OrderType.Limit ? { price: '60000', timeInForce: 'GTC' } : {}),
      newClientOrderId: 'ordinary',
    });
    expect(JSON.stringify(post.mock.calls[0][1])).toBe(
      type === OrderType.Limit
        ? '{"symbol":"BTCUSDT","positionSide":"BOTH","side":"BUY","type":"LIMIT","quantity":"0.01","price":"60000","timeInForce":"GTC","newClientOrderId":"ordinary"}'
        : '{"symbol":"BTCUSDT","positionSide":"BOTH","side":"BUY","type":"MARKET","quantity":"0.01","newClientOrderId":"ordinary"}',
    );
    await exchange.cancelOrders([{ id: 'ordinary', symbol: 'BTCUSDT', type } as Order]);
    expect(remove).toHaveBeenCalledWith('/papi/v1/um/order', {
      params: { symbol: 'BTCUSDT', origClientOrderId: 'ordinary' },
    });
  });

  it('handles the PM ao wrapper without fabricating a fill at trigger', () => {
    const { exchange, store } = subject();
    const ws = new BinancePMPrivateWebsocket(exchange);
    vi.spyOn(ws as any, 'throttledFetchOrders').mockImplementation(() => undefined);
    const fills = vi.fn();
    exchange.emitter.on('fill', fills);
    const frame = (X: string) =>
      new MessageEvent('message', {
        data: JSON.stringify({
          e: 'ALGO_UPDATE',
          fs: 'UM',
          ao: {
            caid: 'protection',
            aid: 123,
            s: 'BTCUSDT',
            o: 'STOP_MARKET',
            S: 'SELL',
            X,
            tp: '59000',
            q: '0.01',
            cp: false,
            R: true,
            wt: 'MARK_PRICE',
          },
        }),
      });
    ws.onMessage(frame('NEW'));
    expect(store.orders.find((o) => o.id === 'protection')).toMatchObject({
      type: OrderType.StopLoss,
      price: 59000,
      amount: 0.01,
    });
    ws.onMessage(frame('TRIGGERED'));
    expect(store.orders.find((o) => o.id === 'protection')?.status).not.toBe(OrderStatus.Open);
    expect(fills).not.toHaveBeenCalled();
    ws.onMessage(frame('CANCELED'));
    expect(store.orders.find((o) => o.id === 'protection')?.status).toBe(OrderStatus.Canceled);
  });
});
