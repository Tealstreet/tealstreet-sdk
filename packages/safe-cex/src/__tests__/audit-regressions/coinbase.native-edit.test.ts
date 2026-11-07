import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { CoinbaseExchange } from '../reference/coinbase/coinbase.exchange';
import { ENDPOINTS, LIST_ORDERS_RESPONSE, ORDER_EVENT } from '../reference/coinbase/coinbase.types';
import { DefaultStore } from '../../store/store.base';
import { ExchangeOptions, Market, Order, OrderSide, OrderStatus, OrderType } from '../../types';

const SPOT_MARKET: Market = {
  id: 'BTC-USD',
  symbol: 'BTC-USD',
  base: 'BTC',
  quote: 'USD',
  active: true,
  isSpot: true,
  isExpiring: false,
  precision: { amount: 0.001, price: 0.01 },
  limits: {
    amount: { min: 0.001, max: 100 },
    leverage: { min: 1, max: 1 },
  },
  contractSize: 1,
  info: { product_type: 'SPOT' },
};
const SPOT_USDC_MARKET: Market = {
  ...SPOT_MARKET,
  id: 'BTC-USDC',
  symbol: 'BTC-USDC',
  quote: 'USDC',
};

const TINY_NONZERO_DECIMAL = `0.${'0'.repeat(400)}1`;

const rawOrder = (overrides: Partial<ORDER_EVENT['orders'][number]> = {}): ORDER_EVENT['orders'][number] => ({
  order_id: 'coinbase-order-id',
  client_order_id: 'tealstreet-client-id',
  cumulative_quantity: '0',
  leaves_quantity: '1.25',
  avg_price: '0',
  limit_price: '50000',
  stop_price: '',
  total_fees: '0',
  status: 'OPEN',
  product_id: SPOT_MARKET.id,
  creation_time: '2026-07-15T00:00:00Z',
  order_side: 'BUY',
  order_type: 'Limit',
  cancel_reason: '',
  reject_reason: '',
  ...overrides,
});

const rawRestOrder = (
  overrides: Partial<LIST_ORDERS_RESPONSE['orders'][number]> = {},
): LIST_ORDERS_RESPONSE['orders'][number] => ({
  order_id: 'coinbase-order-id',
  client_order_id: 'tealstreet-client-id',
  product_id: SPOT_MARKET.id,
  side: 'BUY',
  status: 'OPEN',
  order_type: 'LIMIT',
  created_time: '2026-07-15T00:00:00Z',
  filled_size: '0',
  average_filled_price: '0',
  total_fees: '0',
  order_configuration: {
    limit_limit_gtc: {
      base_size: '1.25',
      limit_price: '50000',
    },
  },
  ...overrides,
});

const makeOrder = (overrides: Partial<Order> = {}): Order => ({
  id: 'coinbase-order-id',
  status: OrderStatus.Open,
  symbol: SPOT_MARKET.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 50_000,
  amount: 1.25,
  filled: 0,
  remaining: 1.25,
  reduceOnly: false,
  info: rawOrder(),
  lastUpdate: 1_784_505_600_000,
  ...overrides,
});

const createExchange = (market: Market | Market[] = SPOT_MARKET, order: Order = makeOrder()) => {
  const store = new DefaultStore();
  store.update({ markets: Array.isArray(market) ? market : [market], orders: [order] });
  const exchange = new CoinbaseExchange(
    'coinbase',
    { key: 'test-key', secret: 'test-secret', tealstreetTunnel: false } as ExchangeOptions,
    store,
  );
  exchange.selectedPortfolio = { name: 'default', uuid: 'portfolio-id', type: 'DEFAULT', deleted: false };
  const post = vi.fn();
  exchange.xhr = { post } as unknown as Axios;
  const cancelOrders = vi.spyOn(exchange, 'cancelOrders');
  const placeOrder = vi.spyOn(exchange, 'placeOrder');
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
  return { exchange, post, cancelOrders, placeOrder };
};

describe('Coinbase Advanced stable-ID native edit', () => {
  it.each([
    ['price', { price: 50_100.004 }, { order_id: 'coinbase-order-id', price: '50100', size: '1.25' }],
    ['amount', { amount: 1.5004 }, { order_id: 'coinbase-order-id', price: '50000', size: '1.5' }],
  ] as const)(
    'sends the required full payload for a one-field %s edit and preserves the order ID',
    async (_field, update, payload) => {
      const { exchange, post, cancelOrders, placeOrder } = createExchange();
      post.mockResolvedValue({ data: { success: true, errors: [] } });

      await expect(exchange.updateOrder({ order: makeOrder(), update })).resolves.toEqual(['coinbase-order-id']);

      expect(post).toHaveBeenCalledOnce();
      expect(post).toHaveBeenCalledWith(ENDPOINTS.EDIT_ORDER, payload);
      expect(cancelOrders).not.toHaveBeenCalled();
      expect(placeOrder).not.toHaveBeenCalled();
    },
  );

  it('surfaces documented response errors without canceling or creating a replacement', async () => {
    const { exchange, post, cancelOrders, placeOrder } = createExchange();
    post.mockResolvedValue({
      data: {
        success: true,
        errors: [{ edit_failure_reason: 'ORDER_NOT_EDITABLE', preview_failure_reason: 'INVALID_SIZE' }],
      },
    });

    await expect(exchange.updateOrder({ order: makeOrder(), update: { amount: 2 } })).rejects.toThrow(
      'ORDER_NOT_EDITABLE, INVALID_SIZE',
    );

    expect(cancelOrders).not.toHaveBeenCalled();
    expect(placeOrder).not.toHaveBeenCalled();
  });

  it('allows native edit when Coinbase raw product id is the equivalent USD market', async () => {
    const aliasOrder = makeOrder({
      symbol: SPOT_USDC_MARKET.symbol,
      info: rawOrder({ product_id: SPOT_MARKET.id }),
    });
    const { exchange, post, cancelOrders, placeOrder } = createExchange([SPOT_MARKET, SPOT_USDC_MARKET], aliasOrder);
    post.mockResolvedValue({ data: { success: true, errors: [] } });

    await expect(exchange.updateOrder({ order: aliasOrder, update: { price: 50_100 } })).resolves.toEqual([
      'coinbase-order-id',
    ]);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.EDIT_ORDER, {
      order_id: 'coinbase-order-id',
      price: '50100',
      size: '1.25',
    });
    expect(cancelOrders).not.toHaveBeenCalled();
    expect(placeOrder).not.toHaveBeenCalled();
  });

  it('allows native edit when the reconciled canonical order came from REST list-order shape', async () => {
    const restBackedOrder = makeOrder({ info: rawRestOrder() });
    const { exchange, post, cancelOrders, placeOrder } = createExchange(SPOT_MARKET, restBackedOrder);
    post.mockResolvedValue({ data: { success: true, errors: [] } });

    await expect(exchange.updateOrder({ order: restBackedOrder, update: { price: 50_100 } })).resolves.toEqual([
      'coinbase-order-id',
    ]);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.EDIT_ORDER, {
      order_id: 'coinbase-order-id',
      price: '50100',
      size: '1.25',
    });
    expect(cancelOrders).not.toHaveBeenCalled();
    expect(placeOrder).not.toHaveBeenCalled();
  });

  it.each([
    [
      'a partially filled order',
      SPOT_MARKET,
      makeOrder({
        amount: 1.25,
        filled: 0.25,
        remaining: 1,
        info: rawOrder({ cumulative_quantity: '0.25', leaves_quantity: '1' }),
      }),
      'full current spot price and zero-fill base size are not reconciled',
    ],
    [
      'a tiny nonzero fill that Number would underflow to zero',
      SPOT_MARKET,
      makeOrder({ info: rawOrder({ cumulative_quantity: TINY_NONZERO_DECIMAL }) }),
      'full current spot price and zero-fill base size are not reconciled',
    ],
    [
      'a tiny nonzero trigger that Number would underflow to zero',
      SPOT_MARKET,
      makeOrder({ info: rawOrder({ stop_price: TINY_NONZERO_DECIMAL }) }),
      'only exactly reconciled OPEN zero-fill ordinary spot LIMIT orders',
    ],
    [
      'a distinct high-precision size that Number would round to the canonical value',
      SPOT_MARKET,
      makeOrder({ info: rawOrder({ leaves_quantity: '1.2500000000000000001' }) }),
      'full current spot price and zero-fill base size are not reconciled',
    ],
    [
      'a trigger order',
      SPOT_MARKET,
      makeOrder({ type: OrderType.StopLoss, info: rawOrder({ order_type: 'Stop Limit', stop_price: '49000' }) }),
      'only exactly reconciled OPEN zero-fill ordinary spot LIMIT orders',
    ],
    [
      'a non-string order identity lookalike',
      SPOT_MARKET,
      makeOrder({ info: rawOrder({ order_id: { toString: () => 'coinbase-order-id' } as any }) }),
      'only exactly reconciled OPEN zero-fill ordinary spot LIMIT orders',
    ],
    [
      'a CFM order with contract-size units',
      {
        ...SPOT_MARKET,
        id: 'BTC-28MAR27',
        symbol: 'BTC-28MAR27',
        isSpot: false,
        isExpiring: true,
        contractSize: 0.001,
        info: { product_type: 'FUTURE', future_product_details: { contract_expiry_type: 'EXPIRING' } },
      } as Market,
      makeOrder({
        symbol: 'BTC-28MAR27',
        info: rawOrder({ product_id: 'BTC-28MAR27' }),
      }),
      'CFM contract size units are not proven for native edit',
    ],
    [
      'a market whose spot-size provenance is unknown',
      { ...SPOT_MARKET, isSpot: undefined, contractSize: undefined, info: {} },
      makeOrder(),
      'unknown size units are not proven for native edit',
    ],
  ])('fails closed before every write for %s', async (_name, market, order, reason) => {
    const { exchange, post, cancelOrders, placeOrder } = createExchange(market, order);

    await expect(exchange.updateOrder({ order, update: { price: 51_000 } })).rejects.toThrow(reason);

    expect(post).not.toHaveBeenCalled();
    expect(cancelOrders).not.toHaveBeenCalled();
    expect(placeOrder).not.toHaveBeenCalled();
  });

  it('keeps INTX isolated on the existing cancel-and-recreate lifecycle', async () => {
    const { exchange, post, cancelOrders, placeOrder } = createExchange();
    exchange.selectedPortfolio = { name: 'intx', uuid: 'portfolio-id', type: 'INTX', deleted: false };
    cancelOrders.mockResolvedValue(undefined);
    placeOrder.mockResolvedValue(['replacement-order-id']);
    const order = makeOrder();

    await expect(exchange.updateOrder({ order, update: { price: 51_000 } })).resolves.toEqual([
      'replacement-order-id',
    ]);

    expect(cancelOrders).toHaveBeenCalledWith([order]);
    expect(placeOrder).toHaveBeenCalledWith({
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      price: 51_000,
      amount: order.amount,
      reduceOnly: false,
    });
    expect(post).not.toHaveBeenCalled();
  });
});
