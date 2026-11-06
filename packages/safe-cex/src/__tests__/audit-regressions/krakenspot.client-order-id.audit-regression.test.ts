import type { Axios } from 'axios';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { createAPI } from '../reference/kraken/krakenspot.api';
import { KrakenSpotExchange } from '../reference/kraken/krakenspot.exchange';
import { ENDPOINTS } from '../reference/kraken/krakenspot.types';
import { DefaultStore } from '../../store/store.base';
import { ExchangeOptions, Market, Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType } from '../../types';

const MARKET: Market = {
  id: 'BTC/USD',
  symbol: 'BTC/USD',
  base: 'BTC',
  quote: 'USD',
  active: true,
  precision: { amount: 0.00000001, price: 0.1 },
  limits: {
    amount: { min: 0.0001, max: 1_000_000 },
    leverage: { min: 1, max: 1 },
  },
  contractSize: 1,
  info: { pairName: 'XXBTZUSD' },
};

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

  return { exchange, store, post };
};

const placeOrderOpts = {
  symbol: MARKET.symbol,
  side: OrderSide.Buy,
  type: OrderType.Limit,
  amount: 1.25,
  price: 50_000,
};

afterEach(() => {
  vi.useRealTimers();
});

describe('Kraken Spot AddOrder client order identifiers', () => {
  it('uses a documented short UUID cl_ord_id, omits userref, and preserves Kraken txids', async () => {
    const { exchange, post } = createExchange();
    post.mockResolvedValue({ data: { error: [], result: { txid: ['KRAKEN-TXID'] } } });

    await expect(exchange.placeOrder(placeOrderOpts)).resolves.toEqual(['KRAKEN-TXID']);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.ADD_ORDER,
      expect.objectContaining({
        pair: 'XXBTZUSD',
        type: 'buy',
        ordertype: 'limit',
        volume: '1.25',
        price: '50000',
      }),
    );

    const payload = post.mock.calls[0][1];
    expect(payload).not.toHaveProperty('userref');
    expect(payload.cl_ord_id).toMatch(/^[a-f0-9]{32}$/);
  });

  it('generates a unique, conforming cl_ord_id for each AddOrder request', async () => {
    const { exchange, post } = createExchange();
    post.mockResolvedValue({ data: { error: [], result: { txid: ['KRAKEN-TXID'] } } });

    await exchange.placeOrder(placeOrderOpts);
    await exchange.placeOrder(placeOrderOpts);

    const clientOrderIds = post.mock.calls.map(([, payload]) => payload.cl_ord_id);
    expect(clientOrderIds).toHaveLength(2);
    expect(new Set(clientOrderIds)).toHaveLength(2);
    expect(clientOrderIds.every((id) => /^[a-f0-9]{32}$/.test(id))).toBe(true);
  });

  it('form-encodes cl_ord_id without userref before signing the private request', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-14T12:00:00.000Z'));

    const { exchange } = createExchange();
    const adapter = vi.fn().mockResolvedValue({
      data: { error: [], result: { txid: ['KRAKEN-TXID'] } },
      status: 200,
      statusText: 'OK',
      headers: {},
      config: {},
    });
    const api = createAPI(
      {
        key: 'test-key',
        secret: Buffer.from('test-secret').toString('base64'),
        tealstreetTunnel: false,
      } as ExchangeOptions,
      exchange,
    );
    api.defaults.adapter = adapter;

    await api.post(ENDPOINTS.ADD_ORDER, {
      pair: 'XXBTZUSD',
      type: 'buy',
      ordertype: 'limit',
      volume: '1.25',
      cl_ord_id: '0123456789abcdef0123456789abcdef',
    });

    const request = adapter.mock.calls[0][0];
    expect(request.headers['Content-Type']).toBe('application/x-www-form-urlencoded');
    expect(request.data).toBe(
      `pair=XXBTZUSD&type=buy&ordertype=limit&volume=1.25&cl_ord_id=0123456789abcdef0123456789abcdef&nonce=${Date.now() * 1000}`,
    );
    expect(request.data).not.toContain('userref=');
  });

  it('continues to identify stop-order amends by the canonical Kraken txid', async () => {
    const { exchange, post } = createExchange();
    const stopOrder: Order = {
      id: 'ORIGINAL-TXID',
      status: OrderStatus.Open,
      symbol: MARKET.symbol,
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      price: 49_000,
      amount: 1.25,
      filled: 0,
      remaining: 1.25,
      reduceOnly: false,
      info: { descr: { ordertype: 'stop-loss' } },
      timeInForce: OrderTimeInForce.GoodTillCancel,
      lastUpdate: Date.now(),
    };
    post.mockResolvedValue({ data: { error: [], result: { amend_id: 'AMEND-TXID' } } });

    await expect(exchange.updateOrder({ order: stopOrder, update: { price: 48_500 } })).resolves.toEqual([
      'ORIGINAL-TXID',
    ]);

    expect(post).toHaveBeenCalledOnce();
    expect(post).toHaveBeenCalledWith(ENDPOINTS.AMEND_ORDER, {
      txid: 'ORIGINAL-TXID',
      trigger_price: '48500',
    });
  });

  it('keeps reconciliation keyed by Kraken txid, not the optional client identifier', async () => {
    const { exchange, store } = createExchange();
    const post = vi.fn().mockResolvedValue({
      data: {
        error: [],
        result: {
          open: {
            'RECONCILED-TXID': {
              descr: { pair: 'BTC/USD', ordertype: 'limit', type: 'buy', price: '50000' },
              vol: '1.25',
              vol_exec: '0.25',
              opentm: 1_784_505_600,
            },
          },
        },
      },
    });
    exchange.xhr = { post } as unknown as Axios;

    const orders = await exchange.fetchAndSetOrders();

    expect(post).toHaveBeenCalledWith(ENDPOINTS.OPEN_ORDERS, {});
    expect(orders).toEqual([
      expect.objectContaining({ id: 'RECONCILED-TXID', symbol: MARKET.symbol, amount: 1.25, filled: 0.25 }),
    ]);
    expect(store.orders[0].id).toBe('RECONCILED-TXID');
  });
});
