import type { Market, OrderFillEvent } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { AsterDEXExchange } from '../reference/asterdex/asterdex.exchange';
import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { BybitInverseExchange } from '../reference/bybit/bybitinverse.exchange';
import { DefaultStore } from '../../store/store.base';
import { createIncrementalFills } from '../../utils/incrementalFills';

// A fill event reports THIS execution. Consumers add them up (the chaser,
// provisional positions), so a cumulative amount double-counts every partial.

const MARKET: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
  info: {},
};

const exchanges: Array<{ dispose: () => void }> = [];
afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
  vi.restoreAllMocks();
});

describe('createIncrementalFills', () => {
  it('turns cumulative fills into executions and ignores replays', () => {
    const executed = createIncrementalFills();
    expect(executed('o1', 1, 100)).toEqual({ qty: 1, value: 100 });
    expect(executed('o1', 3, 330)).toEqual({ qty: 2, value: 230 });
    expect(executed('o1', 3, 330)).toEqual({ qty: 0 });
    expect(executed('o1', 2, 200)).toEqual({ qty: 0 });
    expect(executed('o2', 5)).toEqual({ qty: 5 });
  });

  it('subtracts running totals without float noise, even far apart in size', () => {
    const executed = createIncrementalFills();
    executed('big', 100000, 1);
    expect(executed('big', 100000.1, 1.001)).toEqual({ qty: 0.1, value: 0.001 });
    executed('val', 1, 123456.788);
    expect(executed('val', 2, 123456.789)).toEqual({ qty: 1, value: 0.001 });
  });

  it('forgets the oldest orders past its bound', () => {
    const executed = createIncrementalFills(2);
    executed('a', 1);
    executed('b', 1);
    executed('c', 1);
    expect(executed('a', 1)).toEqual({ qty: 1 });
  });
});

describe('Bybit fill events', () => {
  const frame = (status: string, cumExecQty: string, cumExecValue: string) => ({
    orderId: 'bybit-1',
    orderLinkId: '',
    symbol: 'BTCUSDT',
    side: 'Buy',
    orderType: 'Limit',
    price: '50000',
    qty: '3',
    cumExecQty,
    cumExecValue,
    avgPrice: String(Number(cumExecValue) / Number(cumExecQty)),
    leavesQty: String(3 - Number(cumExecQty)),
    orderStatus: status,
    timeInForce: 'GTC',
    positionIdx: 0,
    reduceOnly: false,
    updatedTime: '1000000',
  });

  it('reports each partial once, so the partials sum to the order', () => {
    const store = new DefaultStore();
    store.update({ markets: [MARKET] });
    const exchange = new BybitExchange('bybit' as any, { key: 'k', secret: 's' } as any, store);
    exchanges.push(exchange);
    const fills: OrderFillEvent[] = [];
    exchange.on('fill', (fill: OrderFillEvent) => fills.push(fill));

    const socket = (exchange as any).privateWebsocket;
    socket.handleOrderTopic([frame('PartiallyFilled', '1', '50000')]);
    socket.handleOrderTopic([frame('PartiallyFilled', '1', '50000')]);
    socket.handleOrderTopic([frame('Filled', '3', '150400')]);

    expect(fills.map((fill) => fill.amount)).toEqual([1, 2]);
    expect(fills.reduce((sum, fill) => sum + fill.amount, 0)).toBe(3);
    // The second execution's own price, not the order's running average.
    expect(fills[1].price).toBe(50200);
  });

  // 4847.5704 / 0.058 is 83578.79999999999 in floats — the price a user saw in
  // the fill toast. Differences of cumulative fields carry the same noise.
  it('reports price and size without float noise', () => {
    const store = new DefaultStore();
    store.update({ markets: [MARKET] });
    const exchange = new BybitExchange('bybit' as any, { key: 'k', secret: 's' } as any, store);
    exchanges.push(exchange);
    const fills: OrderFillEvent[] = [];
    exchange.on('fill', (fill: OrderFillEvent) => fills.push(fill));

    const socket = (exchange as any).privateWebsocket;
    socket.handleOrderTopic([{ ...frame('Filled', '0.058', '4847.5704'), orderId: 'noise-1' }]);
    socket.handleOrderTopic([{ ...frame('PartiallyFilled', '0.1', '8357.88'), orderId: 'noise-2' }]);
    socket.handleOrderTopic([{ ...frame('Filled', '0.158', '13205.4504'), orderId: 'noise-2' }]);

    expect(fills.map((fill) => [fill.amount, fill.price])).toEqual([
      [0.058, 83578.8],
      [0.1, 83578.8],
      [0.058, 83578.8],
    ]);
  });
});

describe('Bybit inverse fill events', () => {
  // Inverse value is base coin, quantity is USD contracts: price = qty / value.
  it('prices each execution as contracts over coin, not its reciprocal', () => {
    const store = new DefaultStore();
    store.update({ markets: [{ ...MARKET, id: 'BTCUSD', symbol: 'BTCUSD', quote: 'USD', isInverse: true } as Market] });
    const exchange = new BybitInverseExchange('bybitinverse' as any, { key: 'k', secret: 's' } as any, store);
    exchanges.push(exchange);
    const fills: OrderFillEvent[] = [];
    exchange.on('fill', (fill: OrderFillEvent) => fills.push(fill));

    (exchange as any).privateWebsocket.handleOrderTopic([
      {
        orderId: 'inv-1',
        orderLinkId: '',
        symbol: 'BTCUSD',
        side: 'Buy',
        orderType: 'Limit',
        price: '50000',
        qty: '1000',
        cumExecQty: '1000',
        cumExecValue: '0.02',
        avgPrice: '50000',
        leavesQty: '0',
        orderStatus: 'Filled',
        timeInForce: 'GTC',
        positionIdx: 0,
        reduceOnly: false,
        updatedTime: '1000000',
      },
    ]);

    expect(fills).toEqual([expect.objectContaining({ amount: 1000, price: 50000 })]);
  });
});

describe('AsterDEX fill events', () => {
  it('carry the contract fields, so consumers do not read amount as NaN', () => {
    const store = new DefaultStore();
    store.update({ markets: [MARKET] });
    const exchange = new AsterDEXExchange('asterdex' as any, { key: 'k', secret: 's' } as any, store);
    exchanges.push(exchange as any);
    const fills: OrderFillEvent[] = [];
    exchange.on('fill', (fill: OrderFillEvent) => fills.push(fill));

    const socket = (exchange as any).privateWebsocket;
    const handler = socket.handleOrderUpdate ?? socket.handleOrderTradeUpdate;
    expect(typeof handler).toBe('function');
    handler.call(socket, {
      o: {
        i: 7,
        s: 'BTCUSDT',
        S: 'BUY',
        X: 'PARTIALLY_FILLED',
        x: 'TRADE',
        l: '0.4',
        L: '50100',
        ap: '50000',
        z: '0.4',
        q: '1',
        o: 'LIMIT',
        p: '50000',
      },
    });

    expect(fills).toEqual([expect.objectContaining({ id: '7', orderId: '7', amount: 0.4, price: 50100 })]);
  });

  it('emits nothing for a frame on a filled order that executed nothing', () => {
    const store = new DefaultStore();
    store.update({ markets: [MARKET] });
    const exchange = new AsterDEXExchange('asterdex' as any, { key: 'k', secret: 's' } as any, store);
    exchanges.push(exchange as any);
    const fills: OrderFillEvent[] = [];
    exchange.on('fill', (fill: OrderFillEvent) => fills.push(fill));
    const socket = (exchange as any).privateWebsocket;
    const handler = socket.handleOrderUpdate ?? socket.handleOrderTradeUpdate;

    handler.call(socket, {
      o: {
        i: 8,
        s: 'BTCUSDT',
        S: 'BUY',
        X: 'PARTIALLY_FILLED',
        x: 'AMENDMENT',
        l: '0',
        L: '0',
        ap: '50000',
        z: '0.4',
        q: '1',
        o: 'LIMIT',
        p: '50000',
      },
    });

    expect(fills).toEqual([]);
  });
});
