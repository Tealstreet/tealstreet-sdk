import type { Axios } from 'axios';
import type { Order } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../reference/binance/binance.exchange';
import { BinancePMExchange } from '../reference/binance/binancepm.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';

// Cancellation is terminal only for the rows still live when its reply arrives.
// https://developers.binance.com/docs/derivatives/usds-margined-futures/trade/rest-api/Cancel-Order
// https://developers.binance.com/docs/derivatives/portfolio-margin/trade/Cancel-UM-Order
const order = (id: string, type = OrderType.Limit): Order => ({
  id,
  symbol: 'BTCUSDT',
  status: OrderStatus.Open,
  type,
  side: OrderSide.Buy,
  amount: 3,
  filled: 0,
  remaining: 3,
  price: 100,
  reduceOnly: false,
  lastUpdate: 0,
});

const subject = (profile: 'binance' | 'binancepm') => {
  const store = new DefaultStore();
  const exchange =
    profile === 'binance'
      ? new BinanceExchange(profile, { key: 'key', secret: 'secret' }, store)
      : new BinancePMExchange(profile, { key: 'key', secret: 'secret' }, store);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
  return { store, exchange };
};
const transport = (exchange: BinanceExchange | BinancePMExchange, deleteRequest: ReturnType<typeof vi.fn>) => {
  if (exchange instanceof BinancePMExchange) {
    exchange.papiXhr = { delete: deleteRequest } as unknown as Axios;
  } else {
    exchange.xhr = { delete: deleteRequest } as unknown as Axios;
  }
};

for (const profile of ['binance', 'binancepm'] as const) {
  describe(`${profile} cancellation preserves fills`, () => {
    it.each([OrderType.Limit, OrderType.StopLoss])(
      'keeps already filled %s rows closed during symbol cancellation',
      async (type) => {
        const { store, exchange } = subject(profile);
        const closed = { ...order('filled', type), status: OrderStatus.Closed, filled: 3, remaining: 0 };
        const live = order('live', type);
        store.update({ orders: [closed, live] });
        transport(exchange, vi.fn().mockResolvedValue({ data: { code: 200 } }));
        await exchange.cancelSymbolOrders('BTCUSDT');
        expect(store.orders.find((row) => row.id === 'filled')).toMatchObject({
          status: OrderStatus.Closed,
          filled: 3,
          remaining: 0,
        });
        expect(store.orders.find((row) => row.id === 'live')?.status).toBe(OrderStatus.Canceled);
      },
    );

    it.each([OrderType.Limit, OrderType.StopLoss])(
      'keeps a %s fill arriving while its cancel acknowledgement is pending',
      async (type) => {
        const { store, exchange } = subject(profile);
        const original = order('racing', type);
        store.update({ orders: [original] });
        let resolve!: (value: unknown) => void;
        transport(
          exchange,
          vi.fn().mockImplementation(
            () =>
              new Promise((done) => {
                resolve = done;
              }),
          ),
        );
        const pending = exchange.cancelOrders([original]);
        await vi.waitFor(() => expect(resolve).toBeTypeOf('function'));
        store.updateOrder({ id: original.id }, { filled: 3, remaining: 0 });
        store.updateOrderStatus(original, OrderStatus.Closed);
        resolve({ data: { orderId: original.id, status: 'CANCELED' } });
        await pending;
        expect(store.orders.find((row) => row.id === original.id)).toMatchObject({
          status: OrderStatus.Closed,
          filled: 3,
          remaining: 0,
        });
      },
    );

    it('does not cancel an order created after symbol cancellation was dispatched', async () => {
      const { store, exchange } = subject(profile);
      const original = order('original');
      const later = order('created-later');
      store.update({ orders: [original] });
      const replies: Array<(value: unknown) => void> = [];
      transport(
        exchange,
        vi.fn().mockImplementation(() => new Promise((resolve) => replies.push(resolve))),
      );
      const pending = exchange.cancelSymbolOrders('BTCUSDT');
      await vi.waitFor(() => expect(replies.length).toBe(profile === 'binance' ? 2 : 1));
      store.addOrUpdateOrders([later]);
      for (const reply of replies) reply({ data: { code: 200 } });
      await pending;
      expect(store.orders.find((row) => row.id === original.id)?.status).toBe(OrderStatus.Canceled);
      expect(store.orders.find((row) => row.id === later.id)?.status).toBe(OrderStatus.Open);
    });

    it.each([OrderStatus.Creating, OrderStatus.Updating, OrderStatus.Canceling])(
      'retires accepted cancellations from the local %s state',
      async (status) => {
        for (const symbolCancellation of [false, true]) {
          const { store, exchange } = subject(profile);
          const local = { ...order('local'), status };
          const closed = { ...order('filled'), status: OrderStatus.Closed, filled: 3, remaining: 0 };
          store.update({ orders: [local, closed] });
          transport(exchange, vi.fn().mockResolvedValue({ data: {} }));
          if (symbolCancellation) {
            await exchange.cancelSymbolOrders('BTCUSDT');
          } else {
            await exchange.cancelOrders([local, closed]);
          }
          expect(store.orders.find((row) => row.id === 'local')?.status).toBe(OrderStatus.Canceled);
          expect(store.orders.find((row) => row.id === 'filled')?.status).toBe(OrderStatus.Closed);
        }
      },
    );

    it('marks accepted cancellations before a later cancellation fails', async () => {
      const { store, exchange } = subject(profile);
      const first = order('accepted');
      const failed = order('rejected', OrderType.StopLoss);
      store.update({ orders: [first, failed] });
      transport(exchange, vi.fn().mockResolvedValueOnce({ data: {} }).mockRejectedValueOnce(new Error('rejected')));
      await expect(exchange.cancelOrders([first, failed])).rejects.toThrow('rejected');
      expect(store.orders.find((row) => row.id === first.id)?.status).toBe(OrderStatus.Canceled);
      expect(store.orders.find((row) => row.id === failed.id)?.status).toBe(OrderStatus.Open);
    });
  });
}

it('keeps immediate PM client-ID reuse available after a successful cancellation', async () => {
  const { store, exchange } = subject('binancepm');
  const first = { ...order('caller-id'), info: { orderId: 12 } };
  store.update({ orders: [first] });
  transport(exchange, vi.fn().mockResolvedValue({ data: {} }));
  await exchange.cancelOrders([first]);
  expect(store.orders.find((row) => row.id === first.id)?.status).toBe(OrderStatus.Canceled);
  const replacement = { ...order('caller-id'), amount: 5, remaining: 5, info: { orderId: 13 } };
  store.addOrUpdateOrders([replacement]);
  expect(store.orders.find((row) => row.id === first.id)).toMatchObject({
    status: OrderStatus.Open,
    amount: 5,
    info: { orderId: 13 },
  });
});
