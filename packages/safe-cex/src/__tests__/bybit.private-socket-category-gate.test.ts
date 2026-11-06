import { afterEach, describe, expect, it } from 'vitest';

import { BybitExchange } from './reference/bybit/bybit.exchange';
import { DefaultStore } from '../store/store.base';

/**
 * Bybit's /v5/private socket carries every category on a unified account. A normal Tealstreet bybit connection
 * trades linear only, so inverse and option rows on that socket must never reach its store, its sizing or its
 * balance; they join only when the account turned that category on.
 */
const live: BybitExchange[] = [];
afterEach(() => live.splice(0).forEach((exchange) => exchange.dispose()));

const create = (toggles: { enableInverseMarkets?: boolean; enableOptionMarkets?: boolean } = {}) => {
  const store = new DefaultStore();
  const exchange = new BybitExchange(
    'bybit',
    { key: 'k', secret: 's', ...toggles, tuning: { consoleLogging: false } } as any,
    store,
  );
  live.push(exchange);
  return { exchange, store };
};

const position = (symbol: string) => ({
  symbol,
  side: 'Buy',
  size: '1000',
  avgPrice: '80000',
  markPrice: '80000',
  positionValue: '1000',
  unrealisedPnl: '0.01',
  leverage: '10',
  positionIdx: 0,
  liqPrice: '',
  updatedTime: '1791860271000',
});

const order = (symbol: string) => ({
  symbol,
  orderId: `o-${symbol}`,
  orderLinkId: '',
  side: 'Buy',
  orderType: 'Limit',
  orderStatus: 'New',
  price: '40000',
  qty: '100',
  leavesQty: '100',
  cumExecQty: '0',
  avgPrice: '',
  timeInForce: 'GTC',
  positionIdx: 0,
  reduceOnly: false,
  stopOrderType: '',
  createdTime: '1791860271000',
  updatedTime: '1791860271000',
});

const OPTION = 'BTC-26DEC26-100000-C';

const deliver = (exchange: BybitExchange, topic: 'position' | 'order', rows: unknown[]) =>
  (exchange as any).privateWebsocket.onMessage({ data: JSON.stringify({ topic, data: rows }) });

describe('Bybit private socket keeps only the categories this connection trades', () => {
  it('drops inverse and option positions on a default linear connection and keeps the linear one', () => {
    const { exchange, store } = create();
    deliver(exchange, 'position', [position('BTCUSDT'), position('BTCUSD'), position(OPTION)]);
    expect(store.positions.map((p) => p.symbol)).toEqual(['BTCUSDT']);
  });

  it('drops inverse and option orders on a default linear connection and keeps the linear one', () => {
    const { exchange, store } = create();
    deliver(exchange, 'order', [order('BTCUSDT'), order('BTCUSD'), order(OPTION)]);
    expect(store.orders.map((o) => o.symbol)).toEqual(['BTCUSDT']);
  });

  it('leaves the store untouched when a frame carries only inverse rows', () => {
    const { exchange, store } = create();
    deliver(exchange, 'position', [position('BTCUSD')]);
    expect(store.positions).toEqual([]);
  });

  it('keeps inverse rows once the account turns inverse on, and option rows once it turns options on', () => {
    const inverse = create({ enableInverseMarkets: true });
    deliver(inverse.exchange, 'position', [position('BTCUSDT'), position('BTCUSD'), position(OPTION)]);
    expect(inverse.store.positions.map((p) => p.symbol).sort()).toEqual(['BTCUSD', 'BTCUSDT']);

    const options = create({ enableOptionMarkets: true });
    deliver(options.exchange, 'position', [position('BTCUSDT'), position('BTCUSD'), position(OPTION)]);
    expect(options.store.positions.map((p) => p.symbol).sort()).toEqual([OPTION, 'BTCUSDT']);
  });
});
