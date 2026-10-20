import { afterEach, expect, it, vi } from 'vitest';
import { ParadexExchange } from '../../exchanges/paradex/paradex.exchange';
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
