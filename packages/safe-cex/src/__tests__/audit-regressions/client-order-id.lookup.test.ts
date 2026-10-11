import type { Order } from '../../types';

import { describe, expect, it } from 'vitest';

import { orderCarriesClientOrderId } from '../../exchanges/clientOrderId';
import { OrderSide, OrderStatus, OrderType } from '../../types';

const order = (overrides: Partial<Order> = {}): Order => ({
  id: 'venue-1',
  symbol: 'BTCUSDT',
  side: OrderSide.Buy,
  type: OrderType.Limit,
  status: OrderStatus.Open,
  price: 1,
  amount: 1,
  filled: 0,
  remaining: 1,
  reduceOnly: false,
  lastUpdate: 0,
  ...overrides,
});

describe('orderCarriesClientOrderId', () => {
  it.each([
    ['the order id (Binance Spot/PM, GRVT)', { id: 'cid-1' }],
    ['parentId (Coinbase)', { parentId: 'cid-1' }],
    ['info.clOrdId (OKX)', { info: { clOrdId: 'cid-1' } }],
    ['info.orderLinkId (Bybit)', { info: { orderLinkId: 'cid-1' } }],
    ['info.clientOid (KuCoin, Bitget)', { info: { clientOid: 'cid-1' } }],
    ['info.clOrdID (Phemex)', { info: { clOrdID: 'cid-1' } }],
    ['info.c (Binance/BingX/Bitrue WS)', { info: { c: 'cid-1' } }],
    ['info.client_id (Paradex)', { info: { client_id: 'cid-1' } }],
    ['info.clientId (Bitunix, ApeX)', { info: { clientId: 'cid-1' } }],
  ] as Array<[string, Partial<Order>]>)('finds it in %s', (_where, overrides) => {
    expect(orderCarriesClientOrderId(order(overrides), 'cid-1')).toBe(true);
  });

  it('matches a numeric venue id (WOO) by value', () => {
    expect(orderCarriesClientOrderId(order({ info: { client_order_id: 123456789 } }), '123456789')).toBe(true);
  });

  it('does not match another order, or the placeholders venues use for "no id"', () => {
    expect(orderCarriesClientOrderId(order({ info: { clOrdId: 'cid-2' } }), 'cid-1')).toBe(false);
    expect(orderCarriesClientOrderId(order({ info: { clOrdId: '' } }), '')).toBe(false);
    expect(orderCarriesClientOrderId(order({ info: { client_order_id: 0 } }), '0')).toBe(false);
    expect(orderCarriesClientOrderId(order({ parentId: 'undefined' }), 'undefined')).toBe(false);
  });
});
