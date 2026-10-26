import { afterEach, describe, expect, it, vi } from 'vitest';

import { createExchangeAPI } from '../reference/hyperliquid/hyperliquid.api';
import { HyperliquidExchange } from '../reference/hyperliquid/hyperliquid.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';

const PRIVATE_KEY = `0x${'11'.repeat(32)}`;
const USER = `0x${'22'.repeat(20)}`;
const TERMINAL_CANCEL = 'Order was never placed, already canceled, or filled. asset=0';

const terminalCancelBody = {
  status: 'ok',
  response: {
    type: 'cancel',
    data: {
      statuses: [{ error: TERMINAL_CANCEL }, { error: TERMINAL_CANCEL }, { error: TERMINAL_CANCEL }],
    },
  },
};

const createApi = (errors: string[]) => {
  const exchange = { emitAndLogError: (msg: string) => errors.push(msg) } as any;
  const xhr = createExchangeAPI({ key: 'k', secret: '', testnet: false } as any, exchange);
  xhr.defaults.adapter = (config: any) =>
    Promise.resolve({
      data: terminalCancelBody,
      status: 200,
      statusText: 'OK',
      headers: {},
      config,
    });
  return xhr;
};

const createExchange = () => {
  const exchange = new HyperliquidExchange(
    'hyperliquid',
    {
      key: USER,
      secret: PRIVATE_KEY,
      applicationId: USER,
      testnet: true,
      tuning: { consoleLogging: false },
    },
    new DefaultStore(),
  );

  exchange.store.update({
    markets: [
      {
        id: 'BTC',
        symbol: 'BTC-PERP',
        base: 'BTC',
        quote: 'USDC',
        active: true,
        precision: { amount: 0.00001, price: 1 },
        limits: { amount: { min: 0.00001, max: 1000 } },
        contractSize: 1,
        isSpot: false,
        info: { hyperliquidId: 0 },
      } as any,
    ],
  });

  return exchange;
};

const closeOrders = [
  {
    id: '11',
    symbol: 'BTC-PERP',
    side: OrderSide.Sell,
    type: OrderType.StopLoss,
    status: OrderStatus.Open,
    reduceOnly: true,
    amount: 0,
    remaining: 0,
    filled: 0,
    price: 70000,
  },
  {
    id: '12',
    symbol: 'BTC-PERP',
    side: OrderSide.Sell,
    type: OrderType.TakeProfit,
    status: OrderStatus.Open,
    reduceOnly: true,
    amount: 0,
    remaining: 0,
    filled: 0,
    price: 90000,
  },
  {
    id: '13',
    symbol: 'BTC-PERP',
    side: OrderSide.Sell,
    type: OrderType.Limit,
    status: OrderStatus.Open,
    reduceOnly: true,
    amount: 0.001,
    remaining: 0.001,
    filled: 0,
    price: 78400,
  },
] as any[];

afterEach(() => {
  vi.restoreAllMocks();
});

describe('Hyperliquid terminal cancel misses after flatten', () => {
  it('does not toast a cancel batch that is only already-gone orders', async () => {
    const errors: string[] = [];
    const xhr = createApi(errors);

    await expect(xhr.post('/exchange', { action: { type: 'cancel', cancels: [] }, nonce: 1 })).rejects.toThrow(
      TERMINAL_CANCEL,
    );
    expect(errors).toEqual([]);
  });

  it('does not toast the same miss on cancelByCloid', async () => {
    const errors: string[] = [];
    const xhr = createApi(errors);

    await expect(
      xhr.post('/exchange', { action: { type: 'cancelByCloid', cancels: [] }, nonce: 1 }),
    ).rejects.toThrow(TERMINAL_CANCEL);
    expect(errors).toEqual([]);
  });

  it('still toasts the same status string on an order action', async () => {
    const errors: string[] = [];
    const xhr = createApi(errors);

    await expect(xhr.post('/exchange', { action: { type: 'order', orders: [] }, nonce: 1 })).rejects.toThrow(
      TERMINAL_CANCEL,
    );
    expect(errors).toEqual([TERMINAL_CANCEL]);
  });

  it('still toasts a cancel batch that includes a real rejection', async () => {
    const errors: string[] = [];
    const exchange = { emitAndLogError: (msg: string) => errors.push(msg) } as any;
    const xhr = createExchangeAPI({ key: 'k', secret: '', testnet: false } as any, exchange);
    xhr.defaults.adapter = (config: any) =>
      Promise.resolve({
        data: {
          status: 'ok',
          response: {
            type: 'cancel',
            data: {
              statuses: [{ error: TERMINAL_CANCEL }, { error: 'Insufficient margin to place order.' }],
            },
          },
        },
        status: 200,
        statusText: 'OK',
        headers: {},
        config,
      });

    await expect(xhr.post('/exchange', { action: { type: 'cancel', cancels: [] }, nonce: 1 })).rejects.toThrow(
      /Insufficient margin/,
    );
    expect(errors).toEqual([`${TERMINAL_CANCEL}, Insufficient margin to place order.`]);
  });

  it('treats an all-terminal cancel miss as a successful local cleanup', async () => {
    const exchange = createExchange();
    exchange.store.update({ orders: closeOrders });
    exchange.xhr = {
      post: vi.fn().mockRejectedValue(new Error(`${TERMINAL_CANCEL}, ${TERMINAL_CANCEL}, ${TERMINAL_CANCEL}`)),
    } as any;

    await expect(exchange.cancelOrders(closeOrders)).resolves.toBeUndefined();
    expect(exchange.store.orders).toEqual([]);
  });

  it('does not treat a blank cancel failure as an already-gone order', async () => {
    const exchange = createExchange();
    exchange.store.update({ orders: closeOrders });
    exchange.xhr = { post: vi.fn().mockRejectedValue(new Error('')) } as any;

    await expect(exchange.cancelOrders(closeOrders)).rejects.toMatchObject({ message: '' });
    expect(exchange.store.orders).toEqual(closeOrders);
  });

  it('does not let a stale frontendOpenOrders response re-add a WS-cancelled positional stop', async () => {
    const exchange = createExchange();
    const order = closeOrders[0];
    exchange.store.update({ orders: [order] });
    exchange.xhr = {
      post: vi.fn().mockResolvedValue({
        data: [
          {
            coin: 'BTC',
            side: 'A',
            limitPx: '66500',
            sz: '0.0',
            oid: Number(order.id),
            timestamp: 1788209282578,
            triggerCondition: 'Price below 70000',
            isTrigger: true,
            triggerPx: '70000',
            children: [],
            isPositionTpsl: true,
            reduceOnly: true,
            orderType: 'Stop Market',
            origSz: '0.0',
            tif: null,
            cloid: null,
          },
        ],
      }),
    } as any;

    exchange.privateWebsocket.handleOrderUpdates([
      {
        order: {
          coin: 'BTC',
          side: 'A',
          limitPx: '66500',
          sz: '0.0',
          oid: Number(order.id),
          timestamp: 1788209282578,
          origSz: '0.0',
          reduceOnly: true,
        },
        status: 'canceled',
        statusTimestamp: 1788209287188,
      },
    ]);

    expect(exchange.store.orders).toEqual([]);

    await expect(exchange.fetchOrders()).resolves.toEqual([]);
    await expect(exchange.fetchOrders()).resolves.toEqual([]);
    expect(exchange.store.orders).toEqual([]);
  });

  it('still fails cancel when Hyperliquid reports a real error', async () => {
    const exchange = createExchange();
    exchange.xhr = { post: vi.fn().mockRejectedValue(new Error('Cannot cancel. asset=0')) } as any;

    await expect(exchange.cancelOrders([closeOrders[0]])).rejects.toThrow('Cannot cancel. asset=0');
  });
});
