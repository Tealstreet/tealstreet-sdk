import { afterEach, describe, expect, it, vi } from 'vitest';

import { BitrueExchange } from '../reference/bitrue/bitrue.exchange';
import { DefaultStore } from '../../store/store.base';
import { Market, OrderSide, OrderStatus, OrderTimeInForce, OrderType } from '../../types';

const market: Market = {
  id: 'E-BTC-USDT',
  symbol: 'E-BTC-USDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  contractSize: 10,
  precision: { amount: 10, price: 0.1 },
  limits: { amount: { min: 10, max: 10000 }, leverage: { min: 1, max: 100 } },
  info: { multiplier: 10 },
};
const make = () => {
  vi.useFakeTimers();
  const store = new DefaultStore();
  store.update({ markets: [market] });
  const exchange = new BitrueExchange('bitrue', { key: 'key', secret: 'secret' }, store);
  const fills = vi.fn();
  exchange.on('fill', fills);
  return { exchange, socket: exchange.privateWebsocket, store, fills };
};
const row = (z: number, status = 'PART_FILLED', extra: Record<string, unknown> = {}) => ({
  E: 100 + z,
  e: 'ORDER_TRADE_UPDATE',
  o: {
    s: 'E_BTCUSDT',
    i: '2334151155877155281',
    S: 'BUY',
    o: 'MARKET',
    x: 'TRADE',
    X: status,
    q: '100',
    z: String(z),
    l: '999',
    p: '100',
    ap: '101',
    L: '111',
    ...extra,
  },
});
afterEach(() => vi.useRealTimers());
describe('Bitrue futures private execution contract', () => {
  it.each([
    ['LIMIT', OrderTimeInForce.GoodTillCancel],
    ['IOC', OrderTimeInForce.ImmediateOrCancel],
    ['FOK', OrderTimeInForce.FillOrKill],
    ['POST_ONLY', OrderTimeInForce.PostOnly],
  ] as const)(
    'exposes the native %s time in force when lookup finds a fresh private row',
    async (type, timeInForce) => {
      const { exchange, socket, store, fills } = make();
      const fetchOrders = vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);
      socket.handleOrderTradeUpdate(row(0, 'NEW', { o: type, x: 'NEW', c: 'caller-bitrue-1' }) as any);
      expect(store.orders[0]).toMatchObject({ type: OrderType.Limit, timeInForce });
      expect(await exchange.findOrderByClientOrderId(market.symbol, 'caller-bitrue-1')).toMatchObject({
        found: true,
        order: { id: '2334151155877155281', timeInForce },
      });
      store.update({ orders: [{ ...store.orders[0], timeInForce: OrderTimeInForce.GoodTillCancel }] });
      socket.handleOrderTradeUpdate(row(0, 'NEW', { o: type, x: 'NEW', c: 'caller-bitrue-1' }) as any);
      expect(store.orders[0].timeInForce).toBe(timeInForce);
      expect(fetchOrders).not.toHaveBeenCalled();
      expect(fills).not.toHaveBeenCalled();
      exchange.dispose();
    },
  );
  it('preserves a known time in force when a private delta omits that metadata', () => {
    const { exchange, socket, store } = make();
    socket.handleOrderTradeUpdate(row(0, 'NEW', { o: 'LIMIT', x: 'NEW' }) as any);
    store.update({ orders: [{ ...store.orders[0], timeInForce: OrderTimeInForce.PostOnly }] });
    socket.handleOrderTradeUpdate(row(0, 'NEW', { o: 'LIMIT', x: 'NEW' }) as any);
    expect(store.orders[0].timeInForce).toBe(OrderTimeInForce.PostOnly);
    exchange.dispose();
  });
  it('scales native cumulative fills once, uses the execution price and derives remaining from totals', () => {
    const { socket, store, fills } = make();
    socket.handleOrderTradeUpdate(row(2) as any);
    expect(store.orders[0]).toMatchObject({ amount: 100, filled: 20, remaining: 80 });
    socket.handleOrderTradeUpdate(row(4, 'PART_FILLED', { L: '112' }) as any);
    socket.handleOrderTradeUpdate(row(4, 'PART_FILLED', { L: '112' }) as any);
    socket.handleOrderTradeUpdate(row(3) as any);
    expect(store.orders[0]).toMatchObject({ filled: 40, remaining: 60 });
    expect(fills.mock.calls.map(([fill]) => [fill.price, fill.amount])).toEqual([
      [111, 20],
      [112, 20],
    ]);
  });
  it('retains terminal state after reconciliation and allows a final filled upgrade', () => {
    const { socket, store } = make();
    socket.handleOrderTradeUpdate(row(0, 'CANCELED', { x: 'CANCEL' }) as any);
    socket.handleOrderTradeUpdate(row(0, 'NEW', { x: 'NEW' }) as any);
    expect(store.orders[0].status).toBe(OrderStatus.Canceled);
    store.update({ orders: [] });
    socket.handleOrderTradeUpdate(row(0, 'NEW', { x: 'NEW' }) as any);
    expect(store.orders).toEqual([]);
    socket.handleOrderTradeUpdate(row(10, 'FILLED') as any);
    socket.handleOrderTradeUpdate(row(10, 'CANCELED', { x: 'CANCEL' }) as any);
    expect(store.orders[0].status).toBe(OrderStatus.Closed);
  });
  it('derives an execution price from cumulative traded value when L is absent', () => {
    for (const first of [
      { ap: '100', L: undefined },
      { ap: '0', L: '100' },
    ]) {
      const { socket, fills } = make();
      socket.handleOrderTradeUpdate(row(2, 'PART_FILLED', first) as any);
      socket.handleOrderTradeUpdate(row(4, 'PART_FILLED', { ap: '105', L: undefined }) as any);
      expect(fills.mock.calls.map(([fill]) => [fill.price, fill.amount])).toEqual([
        [100, 20],
        [110, 20],
      ]);
    }
  });
  it('does not re-emit executions already included in an authoritative order snapshot', () => {
    const { socket, store, fills } = make();
    store.update({
      orders: [
        {
          id: '2334151155877155281',
          symbol: market.symbol,
          type: OrderType.Limit,
          side: OrderSide.Buy,
          status: OrderStatus.Open,
          price: 100,
          amount: 100,
          filled: 40,
          remaining: 60,
          reduceOnly: false,
          lastUpdate: 0,
        },
      ],
    });
    socket.handleOrderTradeUpdate(row(4) as any);
    expect(fills).not.toHaveBeenCalled();
  });
  it('keeps numeric 64-bit IDs exact and echoes the private ping timestamp', () => {
    const { socket, store } = make();
    socket.onMessage({ data: JSON.stringify(row(2)).replace('"2334151155877155281"', '2334151155877155281') } as any);
    expect(store.orders[0].id).toBe('2334151155877155281');
    const send = vi.spyOn(socket, 'send').mockImplementation(() => undefined);
    socket.handlePingEvent({ event: 'ping', ts: '1635221621062' });
    expect(send).toHaveBeenCalledWith('{"event":"pong","ts":"1635221621062"}');
    expect(socket.getWatchdogStaleThresholdMs()).toBe(20000);
  });
  it('reseeds cumulative value when REST advances beyond the last stream execution', () => {
    const { socket, store, fills } = make();
    socket.handleOrderTradeUpdate(row(1, 'PART_FILLED', { ap: '100', L: undefined }) as any);
    store.update({ orders: [{ ...store.orders[0], filled: 30, remaining: 70, info: { avgPrice: 110 } }] });
    socket.handleOrderTradeUpdate(row(4, 'PART_FILLED', { ap: '115', L: undefined }) as any);
    expect(fills.mock.calls.map(([fill]) => [fill.price, fill.amount])).toEqual([
      [100, 10],
      [130, 10],
    ]);
  });
});
