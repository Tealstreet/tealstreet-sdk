import type { Axios } from 'axios';
import type { StoreData } from '../../generated/StoreData';
import type { Order } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { OrderSide, OrderStatus, OrderType } from '../../types';
import { applyReferenceFixture } from '../cutover/referenceFixture';
import { BinanceExchange } from '../reference/binance/binance.exchange';
import { BinancePMExchange } from '../reference/binance/binancepm.exchange';

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
  const store = new FrameStore();
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
        applyReferenceFixture(
          store,
          { orders: [closed, live] },
          store.markets.map((row) => row.symbol),
        );
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
        applyReferenceFixture(
          store,
          { orders: [original] },
          store.markets.map((row) => row.symbol),
        );
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
        (() => {
          const target = { id: original.id };
          const changes: Partial<StoreData['orders'][number]> = { filled: 3, remaining: 0 };
          return applyReferenceFixture(
            store,
            { orders: store.orders.map((row) => (row.id === target.id ? { ...row, ...changes } : row)) },
            store.markets.map((row) => row.symbol),
          );
        })();
        (() => {
          const targets: Array<Pick<StoreData['orders'][number], 'id'>> = [original];
          const status: StoreData['orders'][number]['status'] = OrderStatus.Closed;
          return applyReferenceFixture(
            store,
            {
              orders: store.orders.map((row) =>
                targets.some((target) => target.id === row.id) ? { ...row, status } : row,
              ),
            },
            store.markets.map((row) => row.symbol),
          );
        })();
        resolve({
          data:
            profile === 'binancepm' && type === OrderType.StopLoss
              ? { complete: true }
              : { orderId: original.id, status: 'CANCELED' },
        });
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
      applyReferenceFixture(
        store,
        { orders: [original] },
        store.markets.map((row) => row.symbol),
      );
      const replies: Array<(value: unknown) => void> = [];
      transport(
        exchange,
        vi.fn().mockImplementation(() => new Promise((resolve) => replies.push(resolve))),
      );
      const pending = exchange.cancelSymbolOrders('BTCUSDT');
      await vi.waitFor(() => expect(replies.length).toBe(profile === 'binance' ? 2 : 1));
      (() => {
        const incoming: StoreData['orders'] = [later];
        return applyReferenceFixture(
          store,
          {
            orders: [
              ...store.orders.map((row) => {
                const change = incoming.find((item) => item.id === row.id);
                return change ? { ...row, ...change } : row;
              }),
              ...incoming.filter((row) => !store.orders.some((old) => old.id === row.id)),
            ],
          },
          store.markets.map((row) => row.symbol),
        );
      })();
      // USD-M dispatches both families together; PM sends them in sequence.
      for (let i = 0; i < 2; i++) {
        await vi.waitFor(() => expect(replies.length).toBeGreaterThan(i));
        replies[i]({ data: { code: 200 } });
      }
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
          applyReferenceFixture(
            store,
            { orders: [local, closed] },
            store.markets.map((row) => row.symbol),
          );
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
      applyReferenceFixture(
        store,
        { orders: [first, failed] },
        store.markets.map((row) => row.symbol),
      );
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
  applyReferenceFixture(
    store,
    { orders: [first] },
    store.markets.map((row) => row.symbol),
  );
  transport(exchange, vi.fn().mockResolvedValue({ data: {} }));
  await exchange.cancelOrders([first]);
  expect(store.orders.find((row) => row.id === first.id)?.status).toBe(OrderStatus.Canceled);
  const replacement = { ...order('caller-id'), amount: 5, remaining: 5, info: { orderId: 13 } };
  (() => {
    const incoming: StoreData['orders'] = [replacement];
    return applyReferenceFixture(
      store,
      {
        orders: [
          ...store.orders.map((row) => {
            const change = incoming.find((item) => item.id === row.id);
            return change ? { ...row, ...change } : row;
          }),
          ...incoming.filter((row) => !store.orders.some((old) => old.id === row.id)),
        ],
      },
      store.markets.map((row) => row.symbol),
    );
  })();
  expect(store.orders.find((row) => row.id === first.id)).toMatchObject({
    status: OrderStatus.Open,
    amount: 5,
    info: { orderId: 13 },
  });
});
