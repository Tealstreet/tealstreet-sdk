import type { BITMEX_ORDER } from '../../exchanges/bitmex/types';

import { describe, expect, it, vi } from 'vitest';

import { BitmexExchange } from '../../exchanges/bitmex/bitmex.exchange';
import { ENDPOINTS } from '../../exchanges/bitmex/bitmex.types';
import { DefaultStore } from '../../store/store.base';
import { OrderRejectReason, OrderTimeInForce } from '../../types';

const createExchange = (store = new DefaultStore()) =>
  new BitmexExchange(
    'bitmex' as any,
    { key: 'test-key', secret: 'test-secret', testnet: false, tuning: { consoleLogging: false } } as any,
    store,
  );

const rawOrder = (execInst: string, text = 'Submitted via API.'): BITMEX_ORDER => ({
  orderID: 'order-id',
  account: 123,
  symbol: 'XBTUSD',
  side: 'Buy',
  orderQty: 100,
  price: 50_000,
  currency: 'USD',
  settlCurrency: 'XBt',
  ordType: 'Limit',
  timeInForce: 'GoodTillCancel',
  ordStatus: 'Canceled',
  workingIndicator: false,
  leavesQty: 100,
  cumQty: 0,
  execInst,
  text,
  transactTime: '2026-07-15T00:00:00.000Z',
  timestamp: '2026-07-15T00:00:00.000Z',
} as BITMEX_ORDER);

describe('BitMEX remaining July audit regressions', () => {
  it('uses currency=all and the documented margin account field for identity', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: [
        { account: 1234567, currency: 'XBt' },
        { account: 1234567, currency: 'USDt' },
      ],
    });
    exchange.xhr = { get } as any;

    await expect(exchange.getAccount()).resolves.toEqual({ userId: '1234567' });
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT, { params: { currency: 'all' } });
  });

  it('fails clearly when a margin response has no account identity', async () => {
    const exchange = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');
    exchange.xhr = { get: vi.fn().mockResolvedValue({ data: [{ currency: 'XBt' }] }) } as any;

    await expect(exchange.getAccount()).rejects.toThrow('BitMEX margin response did not include an account id');
    expect(emitAndLogError).toHaveBeenCalledWith('BitMEX margin response did not include an account id');
  });

  it.each(['ParticipateDoNotInitiate', 'PostOnly', 'PostOnlyReprice'])(
    'maps %s execInst orders and cancellations as post-only',
    (execInst) => {
      const exchange = createExchange();
      const order = exchange.mapOrder(
        rawOrder(execInst, `Canceled: Order had execInst of ${execInst} but could not rest on the book`),
      );

      expect(order.timeInForce).toBe(OrderTimeInForce.PostOnly);
      expect(order.extra?.rejectReason).toBe(OrderRejectReason.PostOnly);
    },
  );

  it('subscribes to one explicit aggregated pool and fully removes the topic on unsubscribe', () => {
    const store = new DefaultStore();
    store.update({
      markets: [
        {
          id: 'BTC/USD:BTC',
          symbol: 'XBTUSD',
          base: 'BTC',
          quote: 'USD',
          active: true,
          isInverse: true,
          precision: { amount: 100, price: 0.1 },
          limits: { amount: { min: 100, max: 1_000_000 }, leverage: { min: 1, max: 100 } },
          info: { isQuanto: false, isInverse: true },
        },
      ] as any,
    });
    const exchange = createExchange(store);
    const websocket = exchange.publicWebsocket;
    const send = vi.fn();
    websocket.ws = { readyState: 1, send } as any;
    const callback = vi.fn();

    const unsubscribe = websocket.listenOrderBook('XBTUSD', callback);
    const topic = 'orderBookL2:xbtusd:Aggregated';
    expect(JSON.parse(send.mock.calls[0][0])).toEqual({ op: 'subscribe', args: [topic] });

    const parser = websocket.messageHandlers[topic][0];
    parser([
      {
        table: 'orderBookL2',
        action: 'partial',
        data: [
          { symbol: 'XBTUSD', side: 'Buy', price: 50_000, size: 300, pool: 'Aggregated' },
          { symbol: 'XBTUSD', side: 'Buy', price: 50_000, size: 999, pool: 'Secondary' },
          { symbol: 'XBTUSD', side: 'Sell', price: 50_001, size: 200, pool: 'Aggregated' },
        ],
      },
    ]);

    expect(callback).toHaveBeenCalledWith({
      bids: [{ price: 50_000, amount: 300, total: 300 }],
      asks: [{ price: 50_001, amount: 200, total: 200 }],
    });

    unsubscribe();
    expect(JSON.parse(send.mock.calls[1][0])).toEqual({ op: 'unsubscribe', args: [topic] });
    expect(websocket.topics[topic]).toBeUndefined();
    expect(websocket.messageHandlers[topic]).toBeUndefined();
  });
});
