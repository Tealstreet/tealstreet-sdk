import { afterEach, expect, it, vi } from 'vitest';

import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { DefaultStore } from '../../store/store.base';

afterEach(() => { vi.restoreAllMocks(); });

it.each(['orders', 'income'] as const)('walks empty %s pages with an unvisited next cursor', async (kind) => {
  const exchange = new ParadexExchange('paradex', { publicDataOnly: true, tuning: { consoleLogging: false } } as any, new DefaultStore());
  Object.defineProperty(exchange, 'isPublicDataOnly', { value: false });
  (exchange as any).jwtManager = {};
  const row = { id: 'history-1', market: 'BTC-USD-PERP', order_id: 'order-1', side: 'BUY', type: 'LIMIT',
    status: 'CLOSED', size: '2', remaining_size: '0', price: '100', avg_fill_price: '100', fee: '0.1',
    realized_pnl: '1', created_at: 1_710_000_000_000, last_updated_at: 1_710_000_000_000 };
  const get = vi.fn().mockResolvedValueOnce({ data: { results: [], next: 'empty-page' } })
    .mockResolvedValueOnce({ data: { results: [], next: 'real-page' } })
    .mockResolvedValueOnce({ data: { results: [row] } });
  exchange.xhr = { get } as any;
  try {
    const rows = kind === 'orders' ? await exchange.fetchHistoricalOrders() : await exchange.fetchIncomeHistory();
    expect(rows.length).toBeGreaterThan(0);
    expect(rows[0].symbol).toBe('BTC-USD-PERP');
    expect(get.mock.calls.map(([, config]) => config.params.cursor)).toEqual([undefined, 'empty-page', 'real-page']);
  } finally { exchange.dispose(); }
});
