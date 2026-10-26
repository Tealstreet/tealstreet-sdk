import { AxiosError } from 'axios';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { HyrotraderExchange } from '../reference/bybit/hyrotrader.exchange';
import { DefaultStore } from '../../store/store.base';
import { Market, OrderSide, OrderStatus, OrderType } from '../../types';

class TestHyroExchange extends HyrotraderExchange {
  getBybitRestBaseUrl() {
    return 'https://simulator.test';
  }
}
const exchanges: TestHyroExchange[] = [];
const setup = (max = 100) => {
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
        limits: { amount: { min: 0.001, max }, leverage: { min: 1, max: 100 } },
      } as Market,
    ],
  });
  const exchange = new TestHyroExchange('hyrotrader', { key: 'key', secret: 'secret' }, store);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
  exchanges.push(exchange);
  return { exchange, store };
};
const order = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.01, price: 50_000 };
const response = (result: unknown) => ({ data: { retCode: 0, retMsg: 'OK', result } });
const empty = () => response({ list: [], nextPageCursor: '' });
afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  vi.restoreAllMocks();
});

describe('HyroTrader deadline and transport recovery', () => {
  it('retains an identity from a lookup crossing a newer snapshot without publishing or resubmitting', async () => {
    const { exchange, store } = setup();
    let resolve!: (value: unknown) => void;
    const post = vi.fn().mockRejectedValue(new Error('Network Error'));
    const get = vi
      .fn()
      .mockReturnValueOnce(
        new Promise((done) => {
          resolve = done;
        }),
      )
      .mockResolvedValue(empty());
    exchange.xhr = { post, get } as any;
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchRawBalance').mockResolvedValue([
      { accountType: 'UNIFIED', totalEquity: '1000', coin: [] },
    ]);
    const pending = exchange.placeOrder(order);
    const outcome = expect(pending).rejects.toThrow('outcome is unknown');
    await vi.waitFor(() => expect(get).toHaveBeenCalledTimes(1));
    await exchange.reconcilePrivateState();
    resolve(response({ list: [{ ...post.mock.calls[0][1], orderId: 'lookup-before-snapshot', orderStatus: 'New' }] }));
    await outcome;
    expect(store.orders).toEqual([]);
    expect(await exchange.placeOrder(order)).toEqual(['lookup-before-snapshot']);
    expect(store.orders).toEqual([]);
    expect(post).toHaveBeenCalledTimes(1);
  });
  it('checks the publication boundary after submission returns to the group', async () => {
    const { exchange, store } = setup();
    const post = vi.fn().mockResolvedValue(response({ orderId: 'ack-before-snapshot' }));
    const get = vi.fn().mockResolvedValue(empty());
    exchange.xhr = { post, get } as any;
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchRawBalance').mockResolvedValue([
      { accountType: 'UNIFIED', totalEquity: '1000', coin: [] },
    ]);
    const recovery = (exchange as any).orderRecovery;
    const submitOne = recovery.submitOne.bind(recovery);
    vi.spyOn(recovery, 'submitOne').mockImplementationOnce(async (...args: any[]) => {
      const accepted = await submitOne(...args);
      await exchange.reconcilePrivateState();
      return accepted;
    });
    await expect(exchange.placeOrder(order)).rejects.toThrow('outcome is unknown');
    expect(store.orders).toEqual([]);
    expect(await exchange.placeOrder(order)).toEqual(['ack-before-snapshot']);
    expect(store.orders).toEqual([]);
    expect(post).toHaveBeenCalledTimes(1);
  });
  it('retains terminal observations between failed and resumed caller invocations', async () => {
    const { exchange, store } = setup();
    const post = vi.fn().mockRejectedValue(new Error('Network Error'));
    const get = vi.fn().mockRejectedValue(new Error('lookup unavailable'));
    exchange.xhr = { post, get } as any;
    await expect(exchange.placeOrder(order)).rejects.toThrow('outcome is unknown');
    exchange.privateWebsocket.ready = true;
    const raw = {
      ...post.mock.calls[0][1],
      orderId: 'seen-between-calls',
      orderStatus: 'Filled',
      cumExecQty: '0.01',
      leavesQty: '0',
    };
    exchange.privateWebsocket.onMessage(
      new MessageEvent('message', { data: JSON.stringify({ topic: 'order', data: [raw] }) }),
    );
    store.update({ orders: [] });
    get.mockResolvedValue(response({ list: [{ ...raw, orderStatus: 'New', cumExecQty: '0', leavesQty: '0.01' }] }));
    expect(await exchange.placeOrder(order)).toEqual(['seen-between-calls']);
    expect(store.orders).toEqual([]);
    expect(post).toHaveBeenCalledTimes(1);
  });
  it('reconciles a late acknowledgement crossing an authoritative orders snapshot', async () => {
    const { exchange, store } = setup();
    let resolve!: (value: unknown) => void;
    const post = vi.fn().mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    const get = vi.fn().mockResolvedValue(empty());
    exchange.xhr = { post, get } as any;
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchRawBalance').mockResolvedValue([
      { accountType: 'UNIFIED', totalEquity: '1000', coin: [] },
    ]);
    const pending = exchange.placeOrder(order);
    await vi.waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    await exchange.reconcilePrivateState();
    resolve(response({ orderId: 'completed-during-gap' }));
    expect(await pending).toEqual(['completed-during-gap']);
    expect(store.orders).toEqual([]);
    expect(post).toHaveBeenCalledTimes(1);
    expect(get.mock.calls.filter(([path]) => path === ENDPOINTS.USER_ORDERS)).toHaveLength(1);
  });

  it.each([10014, 110072])('preserves an unresolved duplicate identity after code %s', async (code) => {
    const { exchange } = setup();
    const post = vi
      .fn()
      .mockRejectedValueOnce(new Error('Network Error'))
      .mockResolvedValue({ data: { retCode: code, retMsg: 'OrderLinkedID is duplicate' } });
    const get = vi
      .fn()
      .mockResolvedValueOnce(empty())
      .mockResolvedValueOnce(empty())
      .mockRejectedValue(new Error('lookup unavailable'));
    exchange.xhr = { post, get } as any;
    await expect(exchange.placeOrder(order)).rejects.toThrow('outcome is unknown');
    await expect(exchange.placeOrder(order)).rejects.toThrow('outcome is unknown');
    expect(post).toHaveBeenCalledTimes(2);
    expect(post.mock.calls[1][1].orderLinkId).toBe(post.mock.calls[0][1].orderLinkId);
  });
  it('does not resurrect a Filled event when its create acknowledgement arrives late', async () => {
    const { exchange, store } = setup();
    let resolve!: (value: unknown) => void;
    const post = vi.fn().mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    exchange.xhr = { post } as any;
    const pending = exchange.placeOrder(order);
    await vi.waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    exchange.privateWebsocket.ready = true;
    exchange.privateWebsocket.onMessage(
      new MessageEvent('message', {
        data: JSON.stringify({
          topic: 'order',
          data: [
            {
              ...post.mock.calls[0][1],
              orderId: 'filled-before-ack',
              orderStatus: 'Filled',
              cumExecQty: '0.01',
              leavesQty: '0',
            },
          ],
        }),
      }),
    );
    store.update({ orders: [] });
    resolve(response({ orderId: 'filled-before-ack' }));
    await pending;
    expect(store.orders).toEqual([]);
  });
  it('publishes recovered partial fills with their authoritative price and remaining quantity', async () => {
    const { exchange, store } = setup();
    const post = vi.fn().mockRejectedValue(new Error('Network Error'));
    const get = vi.fn(async () =>
      response({
        list: [
          {
            ...post.mock.calls[0][1],
            orderId: 'partial',
            orderStatus: 'PartiallyFilled',
            price: '49900',
            cumExecQty: '0.004',
            leavesQty: '0.006',
          },
        ],
      }),
    );
    exchange.xhr = { post, get } as any;
    expect(await exchange.placeOrder(order)).toEqual(['partial']);
    expect(store.orders[0]).toMatchObject({ status: OrderStatus.Open, price: 49900, filled: 0.004, remaining: 0.006 });
  });
  it('checks readiness after the position-mode await before creating', async () => {
    const { exchange } = setup();
    let resolve!: (value: number) => void;
    vi.spyOn(exchange as any, 'getOrderPositionIdx').mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    const post = vi.fn().mockResolvedValue(response({ orderId: 'should-not-exist' }));
    exchange.xhr = { post } as any;
    const pending = exchange.placeOrder(order);
    exchange.privateWebsocket.started = true;
    exchange.privateWebsocket.ready = false;
    resolve(0);
    await expect(pending).rejects.toThrow('reconnecting');
    expect(post).not.toHaveBeenCalled();
  });
  it('pauses split siblings on disconnect without re-creating the accepted prefix', async () => {
    const { exchange, store } = setup(0.01);
    const post = vi
      .fn()
      .mockImplementationOnce(async () => {
        exchange.privateWebsocket.started = true;
        exchange.privateWebsocket.ready = false;
        return response({ orderId: 'first' });
      })
      .mockResolvedValue(response({ orderId: 'second' }));
    exchange.xhr = { post } as any;
    await expect(exchange.placeOrder({ ...order, amount: 0.02 })).rejects.toThrow('reconnecting');
    expect(post).toHaveBeenCalledTimes(1);
    expect(store.orders.map(({ id }) => id)).toEqual(['first']);
    exchange.privateWebsocket.ready = true;
    expect(await exchange.placeOrder({ ...order, amount: 0.02 })).toEqual(['first', 'second']);
    expect(post).toHaveBeenCalledTimes(2);
  });

  it.each([408, 502, 10016])('reconciles a resolved ambiguous status %s', async (code) => {
    const { exchange } = setup();
    const post = vi
      .fn()
      .mockResolvedValue({ status: code < 600 ? code : 200, data: { retCode: code, retMsg: 'Server error' } });
    const get = vi.fn(async () =>
      response({
        list: [
          {
            ...post.mock.calls[0][1],
            orderId: 'found',
            orderLinkId: post.mock.calls[0][1].orderLinkId,
            orderStatus: 'New',
          },
        ],
      }),
    );
    exchange.xhr = { post, get } as any;
    expect(await exchange.placeOrder(order)).toEqual(['found']);
    expect(post).toHaveBeenCalledTimes(1);
  });
  it.each(['response', 'history'])(
    'retries only a conclusively rejected split sibling after %s rejection',
    async (kind) => {
      const { exchange } = setup(0.01);
      const post = vi.fn().mockResolvedValueOnce(response({ orderId: 'first' }));
      if (kind === 'response')
        post.mockResolvedValueOnce({ data: { retCode: 110007, retMsg: 'Insufficient balance' } });
      else post.mockRejectedValueOnce(new Error('Network Error'));
      const get = vi.fn(async () =>
        response({
          list: [
            {
              orderId: 'rejected',
              orderLinkId: post.mock.calls[1][1].orderLinkId,
              orderStatus: 'Rejected',
              rejectReason: 'Insufficient balance',
            },
          ],
        }),
      );
      exchange.xhr = { post, get } as any;
      const split = { ...order, amount: 0.02 };
      await expect(exchange.placeOrder(split)).rejects.toThrow('Insufficient balance');
      post.mockResolvedValueOnce(response({ orderId: 'second' }));
      expect(await exchange.placeOrder({ ...split })).toEqual(['first', 'second']);
      expect(post).toHaveBeenCalledTimes(3);
      expect(post.mock.calls[2][1].orderLinkId).not.toBe(post.mock.calls[1][1].orderLinkId);
    },
  );
  it('disables transport-level retries before reconciliation takes control', async () => {
    const { exchange } = setup();
    const adapter = vi.fn(async (config) => {
      throw new AxiosError('Network Error', 'ERR_NETWORK', config);
    });
    exchange.xhr.defaults.adapter = adapter;
    await expect(exchange.xhr.post(ENDPOINTS.CREATE_ORDER, { orderLinkId: 'transport-id' })).rejects.toThrow(
      'Network Error',
    );
    expect(adapter).toHaveBeenCalledTimes(1);
  });
  it.each(['transport', 'deadline'])('finds an accepted order after %s without resubmitting', async (failure) => {
    const { exchange } = setup();
    const post = vi.fn(async (_path: string, _payload: any) => {
      if (failure === 'transport') throw new Error('Network Error');
      return { data: { retCode: 10000, retMsg: 'Request deadline exceeded; outcome may be unknown' } };
    });
    const get = vi.fn(async () =>
      response({
        list: [
          {
            ...post.mock.calls[0][1],
            orderId: 'accepted',
            orderLinkId: post.mock.calls[0][1].orderLinkId,
            orderStatus: 'New',
          },
        ],
        nextPageCursor: '',
      }),
    );
    exchange.xhr = { post, get } as any;
    expect(await exchange.placeOrder(order)).toEqual(['accepted']);
    expect(post).toHaveBeenCalledTimes(1);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.UNFILLED_ORDERS, expect.anything());
  });
  it('checks both complete axes before retrying with the same ID', async () => {
    const { exchange } = setup();
    const post = vi
      .fn()
      .mockRejectedValueOnce(new Error('Network Error'))
      .mockResolvedValueOnce(response({ orderId: 'retried' }));
    const get = vi.fn().mockResolvedValue(empty());
    exchange.xhr = { post, get } as any;
    expect(await exchange.placeOrder(order)).toEqual(['retried']);
    expect(get.mock.calls.map(([path]) => path)).toEqual([ENDPOINTS.UNFILLED_ORDERS, ENDPOINTS.USER_ORDERS]);
    expect(post.mock.calls[0][1].orderLinkId).toBe(post.mock.calls[1][1].orderLinkId);
  });
  it('walks history cursors and does not resurrect a recovered filled limit', async () => {
    const { exchange, store } = setup();
    const post = vi.fn().mockRejectedValue(new Error('Network Error'));
    const get = vi.fn(async (path, config) => {
      if (path === ENDPOINTS.UNFILLED_ORDERS) return empty();
      if (!config.params.cursor)
        return response({ list: [{ orderId: 'unrelated', orderLinkId: 'other' }], nextPageCursor: 'next' });
      return response({
        list: [{ orderId: 'filled', orderLinkId: post.mock.calls[0][1].orderLinkId, orderStatus: 'Filled' }],
        nextPageCursor: '',
      });
    });
    exchange.xhr = { post, get } as any;
    expect(await exchange.placeOrder(order)).toEqual(['filled']);
    expect(get).toHaveBeenCalledTimes(3);
    expect(post).toHaveBeenCalledTimes(1);
    expect(store.orders).toEqual([]);
  });
  it('retains an unresolved ID across caller retries and never submits while lookup fails', async () => {
    const { exchange } = setup();
    const post = vi.fn().mockRejectedValue(new Error('Network Error'));
    const get = vi.fn().mockRejectedValue(new Error('lookup unavailable'));
    exchange.xhr = { post, get } as any;
    await expect(exchange.placeOrder(order)).rejects.toThrow('outcome is unknown');
    await expect(exchange.placeOrder({ ...order })).rejects.toThrow('outcome is unknown');
    expect(post).toHaveBeenCalledTimes(1);
    get.mockResolvedValue(
      response({
        list: [
          {
            ...post.mock.calls[0][1],
            orderId: 'found',
            orderLinkId: post.mock.calls[0][1].orderLinkId,
            orderStatus: 'New',
          },
        ],
      }),
    );
    expect(await exchange.placeOrder({ ...order })).toEqual(['found']);
    expect(post).toHaveBeenCalledTimes(1);
  });
  it('preserves accepted split siblings when a later outcome is unknown', async () => {
    const { exchange, store } = setup(0.01);
    const post = vi
      .fn()
      .mockResolvedValueOnce(response({ orderId: 'first' }))
      .mockRejectedValue(new Error('Network Error'));
    const get = vi.fn().mockRejectedValue(new Error('lookup unavailable'));
    exchange.xhr = { post, get } as any;
    const split = { ...order, amount: 0.02 };
    await expect(exchange.placeOrder(split)).rejects.toThrow('outcome is unknown');
    expect(store.orders.map(({ id }) => id)).toEqual(['first']);
    get.mockImplementation(async () =>
      response({
        list: [
          {
            ...post.mock.calls[1][1],
            orderId: 'second',
            orderLinkId: post.mock.calls[1][1].orderLinkId,
            orderStatus: 'New',
          },
        ],
      }),
    );
    expect(await exchange.placeOrder({ ...split })).toEqual(['first', 'second']);
    expect(post).toHaveBeenCalledTimes(2);
  });
  it('retains a successful prefix of a multiple-order submission', async () => {
    const { exchange } = setup();
    const post = vi
      .fn()
      .mockResolvedValueOnce(response({ orderId: 'first' }))
      .mockRejectedValue(new Error('Network Error'));
    const get = vi.fn().mockRejectedValue(new Error('lookup unavailable'));
    exchange.xhr = { post, get } as any;
    const orders = [order, { ...order, price: 49_000 }];
    await expect(exchange.placeOrders(orders)).rejects.toThrow('outcome is unknown');
    get.mockImplementation(async () =>
      response({
        list: [
          {
            ...post.mock.calls[1][1],
            orderId: 'second',
            orderLinkId: post.mock.calls[1][1].orderLinkId,
            orderStatus: 'New',
          },
        ],
      }),
    );
    expect(await exchange.placeOrders(orders.map((value) => ({ ...value })))).toEqual(['first', 'second']);
    expect(post).toHaveBeenCalledTimes(2);
  });
  it('rejects definitive venue errors without reconciliation', async () => {
    const { exchange } = setup();
    const post = vi.fn().mockResolvedValue({ data: { retCode: 110007, retMsg: 'Insufficient balance' } });
    const get = vi.fn();
    exchange.xhr = { post, get } as any;
    await expect(exchange.placeOrder(order)).rejects.toThrow('Insufficient balance');
    expect(get).not.toHaveBeenCalled();
    expect(post).toHaveBeenCalledTimes(1);
  });
  it.each(['malformed', 'cycle', 'failed'])('does not retry after an incomplete %s lookup', async (kind) => {
    const { exchange } = setup();
    const post = vi.fn().mockRejectedValue(new Error('Network Error'));
    const get = vi
      .fn()
      .mockResolvedValue(
        kind === 'malformed'
          ? response({})
          : kind === 'failed'
            ? { data: { retCode: 10005, retMsg: 'permission denied' } }
            : response({ list: [], nextPageCursor: 'repeated' }),
      );
    exchange.xhr = { post, get } as any;
    await expect(exchange.placeOrder(order)).rejects.toThrow('outcome is unknown');
    expect(post).toHaveBeenCalledTimes(1);
  });
  it('surfaces a rejected history row without resubmitting or publishing it', async () => {
    const { exchange, store } = setup();
    const post = vi.fn().mockRejectedValue(new Error('Network Error'));
    const get = vi.fn(async () =>
      response({
        list: [
          {
            orderId: 'rejected',
            orderLinkId: post.mock.calls[0][1].orderLinkId,
            orderStatus: 'Rejected',
            rejectReason: 'Insufficient balance',
          },
        ],
      }),
    );
    exchange.xhr = { post, get } as any;
    await expect(exchange.placeOrder(order)).rejects.toThrow('Insufficient balance');
    expect(post).toHaveBeenCalledTimes(1);
    expect(store.orders).toEqual([]);
  });
  it('shares concurrent recovery but permits a later intentional identical order', async () => {
    const { exchange } = setup();
    let resolve!: (value: unknown) => void;
    const post = vi
      .fn()
      .mockReturnValueOnce(
        new Promise((done) => {
          resolve = done;
        }),
      )
      .mockResolvedValueOnce(response({ orderId: 'later' }));
    exchange.xhr = { post } as any;
    const first = exchange.placeOrder(order);
    const second = exchange.placeOrder({ ...order });
    await vi.waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    resolve(response({ orderId: 'shared' }));
    expect(await Promise.all([first, second])).toEqual([['shared'], ['shared']]);
    expect(await exchange.placeOrder({ ...order })).toEqual(['later']);
    expect(post).toHaveBeenCalledTimes(2);
  });
});
