import type { OrderFillEvent } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { PolymarketExchange } from '../reference/polymarket/polymarket.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide } from '../../types';

// Raw user-channel contract: https://docs.polymarket.com/trading/realtime-order-updates
const cleanup: Array<() => void> = [];
afterEach(() => cleanup.splice(0).reverse().forEach((dispose) => dispose()));
const setup = () => {
  const store = new DefaultStore();
  const exchange = new PolymarketExchange('polymarket', {
    key: `0x${'11'.repeat(32)}`,
    publicDataOnly: true,
    polymarket: { apiKey: 'ours', apiSecret: 'c2VjcmV0', apiPassphrase: 'pass' },
    tuning: { consoleLogging: false },
  } as any, store);
  cleanup.push(() => exchange.dispose());
  (exchange as any).tokenToSymbol.set('111', 'election-Yes');
  (exchange as any).tokenToSymbol.set('222', 'election-No');
  const refresh = vi.spyOn(exchange, 'fetchAndSetPositions').mockResolvedValue([]);
  const fills: OrderFillEvent[] = [];
  exchange.on('fill', (fill: OrderFillEvent) => fills.push(fill));
  const send = (data: Record<string, unknown>) => exchange.privateWebsocket.onMessage({ data: JSON.stringify(data) } as MessageEvent);
  return { exchange, store, fills, refresh, send };
};
const order = (matched: string, timestamp = '1800000000000') => ({
  event_type: 'order', type: 'UPDATE', id: 'own-order', owner: 'ours', asset_id: '111',
  side: 'BUY', original_size: '10', size_matched: matched, price: '0.5',
  status: 'MATCHED', order_type: 'GTC', created_at: '1800000000', timestamp,
});
const trade = (status = 'MATCHED') => ({
  event_type: 'trade', type: 'TRADE', id: 'match-1', owner: 'ours', trader_side: 'TAKER',
  taker_order_id: 'own-order', asset_id: '111', side: 'BUY', size: '3', price: '0.51', status,
});

describe('Polymarket private execution attribution', () => {
  it('uses trade executions only, never cumulative MATCHED order sizes', () => {
    const { store, fills, send } = setup();
    send(order('3'));
    send(order('3'));
    expect(store.orders[0]).toMatchObject({ id: 'own-order', filled: 3, remaining: 7, status: 'open' });
    send(trade());
    send(order('10', '1800000001000'));
    expect(store.orders).toEqual([]);
    expect(fills).toEqual([{ id: 'own-order', symbol: 'election-Yes', side: OrderSide.Buy, price: 0.51, amount: 3 }]);
  });

  it('deduplicates match, mined, confirmed and duplicate maker/taker representations', () => {
    const { fills, send } = setup();
    ['MATCHED', 'MINED', 'CONFIRMED', 'TRADE_STATUS_CONFIRMED'].forEach((status) => send(trade(status)));
    send({ ...trade(), trader_side: 'MAKER', maker_orders: [{ order_id: 'own-order', owner: 'ours', asset_id: '111', side: 'BUY', matched_amount: '3', price: '0.51' }] });
    expect(fills).toHaveLength(1);
    expect(fills[0].id).toBe('own-order');
  });

  it('reports the owned maker outcome, order, side, amount and execution price', () => {
    const { fills, send } = setup();
    send({ ...trade(), trader_side: 'MAKER', taker_order_id: 'counterparty', size: '99', maker_orders: [
      { order_id: 'own-maker', owner: 'ours', asset_id: '222', side: 'SELL', matched_amount: '2', price: '0.49' },
      { order_id: 'other-maker', owner: 'other', asset_id: '111', side: 'SELL', matched_amount: '97', price: '0.51' },
    ] });
    expect(fills).toEqual([{ id: 'own-maker', symbol: 'election-No', side: OrderSide.Sell, price: 0.49, amount: 2 }]);
  });

  it('uses wallet ownership and suppresses counterparty taker attribution when trader_side is absent', () => {
    const { exchange, fills, send } = setup();
    const wallet = (exchange as any).wallet.address.toUpperCase();
    send({ ...trade(), trader_side: undefined, taker_order_id: 'counterparty', maker_orders: [
      { order_id: 'owned-by-wallet', owner: 'different-api', maker_address: wallet, asset_id: '111', side: 'SELL', matched_amount: '1', price: '0.51' },
    ] });
    expect(fills).toEqual([{ id: 'owned-by-wallet', symbol: 'election-Yes', side: OrderSide.Sell, price: 0.51, amount: 1 }]);
  });

  it('ignores failed/unknown statuses and malformed executions while still refreshing authoritative holdings', () => {
    const { fills, refresh, send } = setup();
    send(trade('FAILED'));
    send(trade('RETRYING'));
    send({ ...trade(), price: 'NaN' });
    send({ ...trade(), size: '-3' });
    expect(fills).toEqual([]);
    expect(refresh).toHaveBeenCalledTimes(4);
  });

  it('keeps cumulative matched size monotonic and ignores older order timestamps', () => {
    const { store, fills, send } = setup();
    send(order('3', '1800000002000'));
    send(order('2', '1800000003000'));
    send(order('4', '1800000001000'));
    expect(store.orders[0]).toMatchObject({ filled: 3, remaining: 7, lastUpdate: 1800000003000 });
    expect(fills).toEqual([]);
  });

  it('fences pre-cancel REST snapshots even if the cancellation arrives before order details', () => {
    const { store, send } = setup();
    send({ event_type: 'order', type: 'CANCELLATION', id: 'own-order', timestamp: '1800000004000' });
    send({ ...order('0'), status: 'LIVE' });
    expect(store.orders).toEqual([]);
    // A successful authoritative read can still have started before cancellation.
    store.update({ orders: [{ id: 'own-order', status: 'open' } as any] });
    expect(store.orders).toEqual([]);
  });
});
