import { describe, expect, it, vi } from 'vitest';

import { OrderlyExchange } from '../reference/orderly/orderly.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC/USDC:USDC',
        symbol: 'BTCUSDC',
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 1_000 } },
      },
    ] as any,
  });

  return {
    store,
    exchange: new OrderlyExchange(
      'woofi',
      { key: 'test-key', secret: 'test-secret', applicationId: 'test-account' },
      store,
    ),
  };
};

describe('audit regression: Orderly private websocket fill mapping', () => {
  it('maps the documented cumulative totalExecutedQuantity for a partial fill while preserving order conventions', () => {
    const { exchange, store } = createExchange();
    const onFill = vi.fn();
    exchange.emitter.on('fill', onFill);

    exchange.privateWebsocket.handleExecutionReport({
      symbol: 'PERP_BTC_USDC',
      orderId: 42,
      type: 'LIMIT',
      side: 'BUY',
      quantity: 10,
      price: 100,
      executedPrice: 101,
      executedQuantity: 1.5,
      totalExecutedQuantity: 4,
      status: 'PARTIALLY_FILLED',
    });

    expect(store.orders).toEqual([
      expect.objectContaining({
        id: '42',
        symbol: 'BTCUSDC',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 10,
        filled: 4,
        remaining: 6,
        status: OrderStatus.Open,
      }),
    ]);
    expect(onFill).toHaveBeenCalledWith({
      id: 42,
      symbol: 'BTCUSDC',
      side: OrderSide.Buy,
      price: 101,
      amount: 1.5,
    });
  });

  it('retains the REST executed fallback when no websocket cumulative field is present', () => {
    const { exchange } = createExchange();

    expect(
      exchange.mapLimitOrder({
        symbol: 'PERP_BTC_USDC',
        order_id: 43,
        order_type: 'LIMIT',
        side: 'SELL',
        order_quantity: 10,
        order_price: 100,
        executed: 2,
      }),
    ).toEqual(expect.objectContaining({ amount: 10, filled: 2, remaining: 8, status: OrderStatus.Open }));
  });
});
