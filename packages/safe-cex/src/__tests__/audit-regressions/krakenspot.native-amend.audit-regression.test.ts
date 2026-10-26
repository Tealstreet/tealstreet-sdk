import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { KrakenSpotExchange } from '../reference/kraken/krakenspot.exchange';
import { ENDPOINTS } from '../reference/kraken/krakenspot.types';
import { DefaultStore } from '../../store/store.base';
import {
  ExchangeOptions,
  Market,
  Order,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
} from '../../types';

const MARKET: Market = {
  id: 'BTC/USD',
  symbol: 'BTC/USD',
  base: 'BTC',
  quote: 'USD',
  active: true,
  precision: { amount: 0.0001, price: 0.1 },
  limits: {
    amount: { min: 0.0001, max: 1_000_000 },
    leverage: { min: 1, max: 1 },
  },
  contractSize: 1,
  info: { pairName: 'XXBTZUSD' },
};

const createOrder = (overrides: Partial<Order> = {}): Order => ({
  id: 'KRAKEN-ORDER-TXID',
  status: OrderStatus.Open,
  symbol: MARKET.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 50_000,
  amount: 1.25,
  filled: 0.25,
  remaining: 1,
  reduceOnly: false,
  info: {
    cl_ord_id: '0123456789abcdef0123456789abcdef',
    descr: { ordertype: 'limit' },
  },
  timeInForce: OrderTimeInForce.GoodTillCancel,
  lastUpdate: 1_784_505_600_000,
  ...overrides,
});

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });

  const exchange = new KrakenSpotExchange(
    'krakenspot',
    {
      key: 'test-key',
      secret: Buffer.from('test-secret').toString('base64'),
      tealstreetTunnel: false,
    } as ExchangeOptions,
    store,
  );
  const post = vi.fn();
  exchange.unlimitedXHR = { post } as unknown as Axios;

  return { exchange, post };
};

describe('Kraken Spot native AmendOrder', () => {
  it('amends a post-only limit by txid with documented atomic fields and preserves the order id', async () => {
    const { exchange, post } = createExchange();
    post.mockResolvedValue({ data: { error: [], result: { amend_id: 'KRAKEN-AMEND-ID' } } });
    const order = createOrder({ timeInForce: OrderTimeInForce.PostOnly });

    await expect(exchange.updateOrder({ order, update: { price: 50_100.04 } })).resolves.toEqual([
      'KRAKEN-ORDER-TXID',
    ]);

    expect(post).toHaveBeenCalledOnce();
    expect(post).toHaveBeenCalledWith(ENDPOINTS.AMEND_ORDER, {
      txid: 'KRAKEN-ORDER-TXID',
      limit_price: '50100',
      post_only: true,
    });
  });

  it('sends order_qty as the requested total base-asset quantity after partial fills', async () => {
    const { exchange, post } = createExchange();
    post.mockResolvedValue({ data: { error: [], result: { amend_id: 'KRAKEN-AMEND-ID' } } });
    const order = createOrder();

    await exchange.updateOrder({ order, update: { amount: 0.75004 } });

    expect(post).toHaveBeenCalledWith(ENDPOINTS.AMEND_ORDER, {
      txid: 'KRAKEN-ORDER-TXID',
      order_qty: '0.75',
    });
  });

  it.each([
    ['stop-loss', OrderType.StopLoss],
    ['take-profit', OrderType.TakeProfit],
  ])('uses trigger_price for a reconciled %s order without canceling first', async (rawOrderType, type) => {
    const { exchange, post } = createExchange();
    post.mockResolvedValue({ data: { error: [], result: { amend_id: 'KRAKEN-AMEND-ID' } } });
    const order = createOrder({
      side: OrderSide.Sell,
      type,
      info: { descr: { ordertype: rawOrderType } },
    });

    await expect(exchange.updateOrder({ order, update: { price: 48_500.08 } })).resolves.toEqual([
      'KRAKEN-ORDER-TXID',
    ]);

    expect(post).toHaveBeenCalledOnce();
    expect(post).toHaveBeenCalledWith(ENDPOINTS.AMEND_ORDER, {
      txid: 'KRAKEN-ORDER-TXID',
      trigger_price: '48500.1',
    });
  });

  it.each([
    ['stop-loss-limit', OrderType.StopLoss],
    ['take-profit-limit', OrderType.TakeProfit],
    ['trailing-stop', OrderType.TrailingStopLoss],
    ['market', OrderType.Market],
  ])('rejects ambiguous or unsupported %s edits before any exchange write', async (rawOrderType, type) => {
    const { exchange, post } = createExchange();
    const order = createOrder({ type, info: { descr: { ordertype: rawOrderType } } });

    await expect(exchange.updateOrder({ order, update: { price: 49_000 } })).rejects.toThrow(
      `Kraken Spot cannot safely amend ${rawOrderType}`,
    );

    expect(post).not.toHaveBeenCalled();
  });

  it('rejects an unreconciled order before writing because its exact Kraken type is unknown', async () => {
    const { exchange, post } = createExchange();
    const order = createOrder({ info: undefined });

    await expect(exchange.updateOrder({ order, update: { price: 49_000 } })).rejects.toThrow(
      'only reconciled limit, stop-loss, and take-profit orders are supported',
    );

    expect(post).not.toHaveBeenCalled();
  });

  it('surfaces AmendOrder rejection without canceling or creating a replacement', async () => {
    const { exchange, post } = createExchange();
    post.mockRejectedValue(new Error('EOrder:Insufficient funds'));

    await expect(exchange.updateOrder({ order: createOrder(), update: { amount: 2 } })).rejects.toThrow(
      'EOrder:Insufficient funds',
    );

    expect(post).toHaveBeenCalledOnce();
    expect(post.mock.calls[0][0]).toBe(ENDPOINTS.AMEND_ORDER);
  });

  it('fails closed when Kraken acknowledges no amend transaction id', async () => {
    const { exchange, post } = createExchange();
    post.mockResolvedValue({ data: { error: [], result: {} } });

    await expect(exchange.updateOrder({ order: createOrder(), update: { amount: 2 } })).rejects.toThrow(
      'AmendOrder response did not include an amend_id',
    );

    expect(post).toHaveBeenCalledOnce();
  });
});
