import { afterEach, expect, it, vi } from 'vitest';

import { ENDPOINTS, ORDER_HISTORY_MAX_WINDOW_MS } from '../exchanges/bybit/bybit.types';
import { DefaultStore } from '../store/store.base';
import { BybitExchange } from './reference/bybit/bybit.exchange';
import { BybitDemoExchange } from './reference/bybit/bybitdemo.exchange';
import { HyroOrderRecovery } from './reference/bybit/hyrotrader.order-recovery';

const exchanges: BybitExchange[] = [];
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));
const statuses = [undefined, 'Cancelled', 'Rejected', 'Deactivated'];
const row = (orderId: string, orderStatus: string, createdTime = 1786522921789) => ({
  orderId,
  orderStatus,
  symbol: 'BTCUSDT',
  side: 'Buy',
  orderType: 'Limit',
  qty: '1',
  cumExecQty: '0',
  createdTime: String(createdTime),
});
const page = (list: unknown[], nextPageCursor = '') => ({
  data: { retCode: 0, retMsg: 'OK', result: { list, nextPageCursor } },
});
const setup = (Exchange = BybitExchange, enableInverseMarkets = false) => {
  const exchange = new Exchange(
    Exchange === BybitDemoExchange ? 'bybitdemo' : 'bybit',
    { key: 'k', secret: 's', enableInverseMarkets, tuning: { consoleLogging: false } },
    new DefaultStore(),
  );
  exchanges.push(exchange);
  return exchange;
};

it.each([BybitExchange, BybitDemoExchange])(
  '%s merges explicitly requested terminal orders when default history omits them',
  async (Exchange) => {
    const exchange = setup(Exchange);
    const get = vi.fn(async (_url, { params }) =>
      page([row(params.orderStatus || 'filled', params.orderStatus || 'Filled')]),
    );
    vi.spyOn(exchange.xhr, 'get').mockImplementation(get as any);
    const orders = await exchange.fetchHistoricalOrders();
    expect(get.mock.calls.map(([, { params }]) => params.orderStatus)).toEqual(statuses);
    expect(orders.map((order) => order.id)).toEqual(['filled', 'Cancelled', 'Rejected', 'Deactivated']);
    expect(orders.filter((order) => order.id !== 'filled').every((order) => order.filled === 0)).toBe(true);
  },
);

it('keeps independent status/category/window cursors and deduplicates before global limiting', async () => {
  const exchange = setup(BybitExchange, true);
  const from = 1786522921789;
  const to = from + ORDER_HISTORY_MAX_WINDOW_MS + 1;
  const get = vi.fn(async (_url, { params }) => {
    const id = `${params.category}:${params.startTime}:${params.orderStatus || 'default'}`;
    return params.cursor
      ? page([row('duplicate', 'Filled', from)])
      : page([row(id, params.orderStatus || 'Filled', params.orderStatus === 'Deactivated' ? to : from)], 'same');
  });
  vi.spyOn(exchange.xhr, 'get').mockImplementation(get as any);
  const orders = await exchange.fetchHistoricalOrders({ from, to });
  expect(get.mock.calls).toHaveLength(32);
  expect(new Set(orders.map((order) => order.id)).size).toBe(17);
  for (const category of ['linear', 'inverse']) {
    for (const status of statuses) {
      const params = get.mock.calls
        .map(([, config]) => config.params)
        .filter((params) => params.category === category && params.orderStatus === status);
      expect(params).toHaveLength(4);
      expect(params.map((params) => params.cursor)).toEqual([undefined, 'same', undefined, 'same']);
      expect(params.map((params) => [params.startTime, params.endTime])).toEqual([
        [from, from + ORDER_HISTORY_MAX_WINDOW_MS],
        [from, from + ORDER_HISTORY_MAX_WINDOW_MS],
        [to, to],
        [to, to],
      ]);
      expect(params.every((params) => params.limit === 50)).toBe(true);
    }
  }
  expect((await exchange.fetchHistoricalOrders({ from, to, limit: 1 }))[0].id).toContain('Deactivated');
});

it('preserves a symbol and a one-sided time bound on every status request', async () => {
  const exchange = setup();
  const get = vi.fn(async () => page([]));
  vi.spyOn(exchange.xhr, 'get').mockImplementation(get as any);
  await exchange.fetchHistoricalOrders({ symbol: 'BTCUSD', to: 1786522921789 });
  expect(get.mock.calls).toHaveLength(4);
  for (const [, config] of get.mock.calls as any[]) {
    expect(config.params).toMatchObject({ category: 'inverse', symbol: 'BTCUSD', endTime: 1786522921789 });
    expect(config.params).not.toHaveProperty('startTime');
  }
});

it.each(['Cancelled', 'Rejected', 'Deactivated'])(
  'Hyro recovers a lost create from explicit %s history without resubmitting',
  async (orderStatus) => {
    const payload = { category: 'linear', symbol: 'BTCUSDT', orderLinkId: 'stable-link' };
    const get = vi.fn(async (_url, { params }) =>
      page(params.orderStatus === orderStatus ? [{ ...payload, orderId: 'accepted', orderStatus }] : []),
    );
    const post = vi.fn().mockRejectedValue(new Error('Network Error'));
    const recovery = new HyroOrderRecovery(
      () => ({ get, post }) as any,
      () => false,
      () => {},
      () => 'same-session',
    );
    const pending = recovery.submit([payload], () => {});
    if (orderStatus === 'Rejected') await expect(pending).rejects.toThrow('rejected');
    else expect((await pending)[0]).toMatchObject({ data: { result: { orderId: 'accepted' } }, terminal: true });
    expect(post).toHaveBeenCalledTimes(1);
    expect(get.mock.calls[0][0]).toBe(ENDPOINTS.UNFILLED_ORDERS);
    expect(get.mock.calls.slice(1).map(([, { params }]) => params.orderStatus)).toEqual(
      [undefined, 'Cancelled', 'Rejected', 'Deactivated'].slice(0, statuses.indexOf(orderStatus) + 1),
    );
  },
);
