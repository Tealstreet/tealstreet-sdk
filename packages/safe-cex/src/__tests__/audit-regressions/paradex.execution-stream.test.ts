import { afterEach, expect, it, vi } from 'vitest';
import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { DefaultStore } from '../../store/store.base';
import { MarginType, PositionMode, PositionSide } from '../../types';

afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });
const create = () => {
  const exchange = new ParadexExchange('paradex', { publicDataOnly: true, tuning: { consoleLogging: false } } as any, new DefaultStore());
  return { exchange, socket: exchange.privateWebsocket as any };
};
// Official fills.{market_symbol}: size is this execution's base size, independent
// of remaining_size; IDs distinguish identical executions of the same order.
it('emits every unique Paradex execution once across duplicate/replayed fills', () => {
  const { exchange, socket } = create();
  const emit = vi.spyOn(exchange.emitter, 'emit');
  const row = { id: 'fill-1', market: 'BTC-USD-PERP', order_id: 'order-1', side: 'BUY', price: '50000', size: '0.5', remaining_size: '0.5' };
  for (const fill of [row, row, { ...row, id: 'fill-2' }, { ...row, id: 'invalid', size: '-1' }]) {
    socket.onMessage({ data: JSON.stringify({ jsonrpc: '2.0', method: 'subscription', params: { channel: 'fills.ALL', data: fill } }) });
  }
  expect(emit.mock.calls.filter(([event]) => event === 'fill')).toEqual([
    ['fill', { id: 'order-1', symbol: 'BTC-USD-PERP', side: 'buy', price: 50000, amount: 0.5 }],
    ['fill', { id: 'order-1', symbol: 'BTC-USD-PERP', side: 'buy', price: 50000, amount: 0.5 }],
  ]);
  exchange.dispose();
});
it('clears a closed position immediately and schedules authoritative reconciliation', async () => {
  vi.useFakeTimers();
  const { exchange, socket } = create();
  exchange.store.update({ positions: [{ id: 'BTC-USD-PERP', symbol: 'BTC-USD-PERP', side: PositionSide.Long,
    contracts: 1, notional: 50000, entryPrice: 50000, unrealizedPnl: 10, leverage: 10, liquidationPrice: 45000,
    marginType: MarginType.Cross, positionMode: PositionMode.OneWay }] });
  const fetch = vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
  socket.onMessage({ data: JSON.stringify({ jsonrpc: '2.0', method: 'subscription', params: { channel: 'positions', data: { market: 'BTC-USD-PERP', side: 'LONG', size: '0', status: 'CLOSED' } } }) });
  expect(exchange.store.positions[0]?.contracts ?? 0).toBe(0);
  expect(fetch).toHaveBeenCalledOnce();
  await Promise.resolve();
  exchange.dispose();
});

it('retains identical historical fills with distinct IDs and uses the fill size', async () => {
  const { exchange } = create();
  Object.defineProperty(exchange, 'isPublicDataOnly', { value: false });
  exchange.jwtManager = {} as any;
  exchange.store.update({ markets: [{ id: 'BTC-USD-PERP', symbol: 'BTC-USD-PERP', base: 'BTC', quote: 'USD', active: true,
    precision: { amount: 0.001, price: 0.1 }, limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } } }] });
  const fill = { id: 'fill-1', market: 'BTC-USD-PERP', order_id: 'order-1', side: 'BUY', price: '50000', size: '0.5', remaining_size: '0.5', created_at: 1_750_000_000_000 };
  exchange.xhr = { get: vi.fn().mockResolvedValue({ data: { results: [fill, { ...fill, id: 'fill-2' }] } }) } as any;
  const rows = await exchange.fetchHistoricalFills({ symbol: 'BTC-USD-PERP' });
  expect(rows.map(({ id, qty }) => ({ id, qty }))).toEqual([{ id: 'fill-1', qty: 0.5 }, { id: 'fill-2', qty: 0.5 }]);
  exchange.dispose();
});
it('maps the documented public trades route to the shared quantity and timestamp units', async () => {
  const { exchange } = create();
  exchange.store.update({ markets: [{ id: 'BTC-USD-PERP', symbol: 'BTC-USD-PERP', base: 'BTC', quote: 'USD', active: true,
    precision: { amount: 0.001, price: 0.1 }, limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } } }] });
  const get = vi.fn().mockResolvedValue({ data: { results: [{ id: 'trade-1', market: 'BTC-USD-PERP', side: 'SELL', price: '100', size: '2', created_at: 1_750_000_000_000 }] } });
  exchange.xhr = { get } as any;
  await expect(exchange.fetchPublicTrades('BTC-USD-PERP', 10)).resolves.toEqual([{ id: 'trade-1', symbol: 'BTC-USD-PERP', side: 'sell', price: 100, qty: 2, quoteQty: 200, timestamp: 1_750_000_000_000 }]);
  expect(get).toHaveBeenCalledWith('/trades', { params: { market: 'BTC-USD-PERP', page_size: 10 } });
  exchange.dispose();
});

it('preserves native client identity and canonical post-only state in both order readers', () => {
  const { exchange, socket } = create();
  const order = { id: 'order-1', market: 'BTC-USD-PERP', type: 'LIMIT', status: 'OPEN', side: 'BUY', size: '1', remaining_size: '1',
    price: '50000', instruction: 'POST_ONLY', flags: [], client_id: 'caller-exact', created_at: 1_750_000_000_000 };
  expect((exchange as any).mapOrder(order)).toMatchObject({ clientOrderId: 'caller-exact', timeInForce: 'PostOnly' });
  socket.onMessage({ data: JSON.stringify({ jsonrpc: '2.0', method: 'subscription', params: { channel: 'orders.ALL', data: order } }) });
  expect(exchange.store.orders[0]).toMatchObject({ clientOrderId: 'caller-exact', timeInForce: 'PostOnly' });
  exchange.dispose();
});
it('keeps only the supported fixed margin and position modes without a write', async () => {
  const { exchange } = create();
  const post = vi.fn(); exchange.xhr = { post } as any;
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
  await expect(exchange.changeMarginType(false)).resolves.toBeUndefined();
  await expect(exchange.changePositionMode(false)).resolves.toBeUndefined();
  await expect(exchange.changeMarginType(true)).rejects.toThrow('isolated');
  await expect(exchange.changePositionMode(true)).rejects.toThrow('one-way');
  expect(post).not.toHaveBeenCalled(); exchange.dispose();
});
