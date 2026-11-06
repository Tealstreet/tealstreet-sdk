import { afterEach, describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../reference/binance/binance.exchange';
import { ENDPOINTS } from '../../exchanges/binance/binance.types';
import { BinancePrivateWebsocket } from '../reference/binance/binance.ws-private';
import { DefaultStore } from '../../store/store.base';
import { OrderTimeInForce } from '../../types';

const native = {
  symbol: 'BTCUSDT',
  orderId: 41,
  clientOrderId: 'previous-client-id',
  type: 'LIMIT',
  side: 'BUY',
  status: 'NEW',
  timeInForce: 'GTX',
  price: '90000',
  origQty: '0.001',
  executedQty: '0',
  reduceOnly: false,
};

const subject = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC/USDT:USDT',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 125 } },
      },
    ],
  });
  const exchange = new BinanceExchange('binance', { key: 'test-key', secret: 'test-secret' }, store);
  const cancel = vi.spyOn(exchange.xhr, 'delete').mockResolvedValue({ data: { ...native, status: 'CANCELED' } });
  const place = vi.spyOn(exchange.unlimitedXHR, 'post').mockResolvedValue({ data: { ...native, orderId: 42 } });
  return { store, exchange, cancel, place };
};

describe('Binance USD-M maker protection survives cancel and replace', () => {
  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it.each(['REST', 'private stream'] as const)(
    '%s mapper retains GTX through the actual replacement wire formatter',
    async (source) => {
      vi.useFakeTimers();
      const { store, exchange, cancel, place } = subject();
      if (source === 'REST') {
        vi.spyOn(exchange.xhr, 'get').mockImplementation(async (url) => ({
          data: url === ENDPOINTS.OPEN_ORDERS ? [native] : [],
        }));
        store.update({ orders: await exchange.fetchOrders() });
      } else {
        const socket = new BinancePrivateWebsocket(exchange);
        socket.handleOrderEvents([
          {
            o: {
              s: 'BTCUSDT',
              i: 41,
              c: native.clientOrderId,
              X: 'NEW',
              ot: 'LIMIT',
              S: 'BUY',
              f: 'GTX',
              p: '90000',
              q: '0.001',
              z: '0',
              R: false,
            },
          },
        ]);
      }
      const order = store.orders[0];
      expect(order.timeInForce).toBe(OrderTimeInForce.PostOnly);
      await expect(exchange.updateOrder({ order, update: { price: 89000 } })).resolves.toEqual(['42']);
      expect(cancel).toHaveBeenCalledWith(ENDPOINTS.ORDER, { params: { symbol: 'BTCUSDT', orderId: '41' } });
      expect(place).toHaveBeenCalledOnce();
      expect(place).toHaveBeenCalledWith(
        ENDPOINTS.ORDER,
        expect.objectContaining({
          type: 'LIMIT',
          timeInForce: 'GTX',
          price: '89000',
          quantity: '0.001',
        }),
      );
      expect(place.mock.calls[0][1]).not.toHaveProperty('newClientOrderId', native.clientOrderId);
    },
  );
});
