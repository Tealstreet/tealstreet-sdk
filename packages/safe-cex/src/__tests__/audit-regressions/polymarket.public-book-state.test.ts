import type { Market, OrderBook, Ticker } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { PolymarketExchange } from '../reference/polymarket/polymarket.exchange';
import { DefaultStore } from '../../store/store.base';

const cleanup: Array<() => void> = [];
beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(1_800_000_000_000); });
afterEach(() => { cleanup.splice(0).reverse().forEach((fn) => fn()); vi.useRealTimers(); });
const setup = () => {
  const store = new DefaultStore();
  store.update({
    markets: ['111', '222'].map((id) => ({
      id, symbol: `cafe-${id}`, base: 'cafe', quote: 'pUSD', active: true,
      precision: { price: 2, amount: 2 }, limits: { amount: { min: 5, max: 1000000 }, leverage: { min: 1, max: 1 } },
      info: { tokenId: id, conditionId: '0xcondition', tickSize: 0.01, negRisk: true, outcome: id === '111' ? 'Yes' : 'No' },
    } as Market)),
    tickers: ['111', '222'].map((id) => ({ id, symbol: `cafe-${id}`, last: 0.5, bid: 0.4, ask: 0.6, mark: 0.5, index: 0.5 } as Ticker)),
  });
  const exchange = new PolymarketExchange('polymarket', { key: '', secret: '', publicDataOnly: true, tuning: { consoleLogging: false } }, store);
  cleanup.push(() => exchange.dispose());
  const socket = exchange.publicWebsocket;
  const books: OrderBook[] = [];
  const unsubscribe = socket.subscribeToOrderBook('111', (book) => books.push(book));
  cleanup.push(unsubscribe);
  const send = (message: unknown) => socket.onMessage({ data: JSON.stringify(message) } as MessageEvent);
  return { exchange, socket, store, books, send };
};
const snapshot = (timestamp = 100) => ({
  event_type: 'book', asset_id: '111', timestamp: String(timestamp),
  bids: [{ price: '0.3', size: '30' }, { price: '0.5', size: '5' }, { price: '0.4', size: '10' }],
  asks: [{ price: '0.8', size: '8' }, { price: '0.6', size: '6' }, { price: '0.7', size: '7' }],
});
const change = (price: string, size: string, side = 'BUY') => ({ asset_id: '111', side, price, size, best_bid: '0.55', best_ask: '0.6' });

describe('Polymarket complete CLOB book state', () => {
  it('routes initial arrays and applies every absolute-size delta without dropping rapid intermediate levels', () => {
    const { books, send, store } = setup();
    send([snapshot(), { event_type: 'last_trade_price', asset_id: '222', timestamp: '100', price: '0.4' }]);
    expect(books).toHaveLength(1);
    expect(books[0]).toEqual({
      bids: [{ price: 0.5, amount: 5, total: 5 }, { price: 0.4, amount: 10, total: 15 }, { price: 0.3, amount: 30, total: 45 }],
      asks: [{ price: 0.6, amount: 6, total: 6 }, { price: 0.7, amount: 7, total: 13 }, { price: 0.8, amount: 8, total: 21 }],
    });
    expect(store.tickersById['222'].last).toBe(0.4);
    send({ event_type: 'price_change', timestamp: '101', price_changes: [change('0.5', '0'), change('0.55', '11'), change('0.7', '3', 'SELL')] });
    send({ event_type: 'price_change', timestamp: '102', price_changes: [change('0.45', '9')] });
    send({ event_type: 'price_change', timestamp: '103', price_changes: [change('0.3', '0')] });
    expect(books).toHaveLength(6);
    expect(books.at(-1)).toEqual({
      bids: [{ price: 0.55, amount: 11, total: 11 }, { price: 0.45, amount: 9, total: 20 }, { price: 0.4, amount: 10, total: 30 }],
      asks: [{ price: 0.6, amount: 6, total: 6 }, { price: 0.7, amount: 3, total: 9 }, { price: 0.8, amount: 8, total: 17 }],
    });
    expect(store.tickersById['111']).toMatchObject({ bid: 0.55, ask: 0.6, mark: 0.575, index: 0.575 });
  });

  it('rejects malformed snapshots atomically and stale quote/trade/tick events independently', () => {
    const { books, send, store } = setup();
    send(snapshot());
    send({ event_type: 'price_change', timestamp: '101', price_changes: [change('0.55', '11')] });
    const current = books.at(-1);
    for (const invalid of [
      { ...snapshot(102), asks: undefined },
      { ...snapshot(102), asks: [{ price: '0.9', size: '-1' }] },
      { ...snapshot(102), bids: [{ price: '0.3bad', size: '2' }] },
      { ...snapshot(102), bids: [{ price: '1.1', size: '2' }] },
      snapshot(99),
    ]) send(invalid);
    expect(books).toHaveLength(2);
    expect(books.at(-1)).toEqual(current);
    send({ event_type: 'price_change', timestamp: '100', price_changes: [{ ...change('0.55', '0'), best_bid: '0.1', best_ask: '0.9' }] });
    expect(books).toHaveLength(2);
    expect(store.tickersById['111'].bid).toBe(0.55);
    send({ event_type: 'last_trade_price', asset_id: '111', timestamp: '200', price: '0.7' });
    send({ event_type: 'last_trade_price', asset_id: '111', timestamp: '199', price: '0.2' });
    expect(store.tickersById['111'].last).toBe(0.7);
    send({ event_type: 'tick_size_change', asset_id: '111', timestamp: '100', new_tick_size: '0.0001' });
    send({ event_type: 'tick_size_change', asset_id: '111', timestamp: '99', new_tick_size: '0.01' });
    send({ event_type: 'tick_size_change', asset_id: '111', timestamp: '101', new_tick_size: '0.03' });
    expect(store.marketsById['111']).toMatchObject({ precision: { price: 4, amount: 2 }, info: { conditionId: '0xcondition', tickSize: 0.0001, negRisk: true } });
    expect(store.marketsById['222'].precision.price).toBe(2);
    send({ event_type: 'market_resolved', assets_ids: ['111', '222'] });
    expect(store.markets.every((market) => !market.active)).toBe(true);
  });

  it('keeps listener references through reconnect but requires a new complete snapshot before deltas', () => {
    const { books, send, socket } = setup();
    const second: OrderBook[] = [];
    const unsubscribeSecond = socket.subscribeToOrderBook('111', (book) => second.push(book));
    cleanup.push(unsubscribeSecond);
    send(snapshot());
    expect(second).toHaveLength(1);
    socket.onClose();
    send({ event_type: 'price_change', timestamp: '101', price_changes: [change('0.55', '11')] });
    expect(books).toHaveLength(1);
    send({ ...snapshot(102), bids: [], asks: [] });
    expect(books.at(-1)).toEqual({ bids: [], asks: [] });
    expect(second).toHaveLength(2);
    unsubscribeSecond();
    send({ event_type: 'price_change', timestamp: '103', price_changes: [change('0.55', '11')] });
    expect(books.at(-1)).toEqual({ bids: [{ price: 0.55, amount: 11, total: 11 }], asks: [] });
    expect(second).toHaveLength(2);
  });
});
