import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';
import { describe, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide } from '../../types';

describe('Rust Kraken journal boundary', () => {
  it('keeps the futures account-wide fills journal path when selecting Rust', async () => {
    const exchange = new RustEngineExchange('kraken', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    adoptEngineCapabilities(exchange);
    const from = 1_700_000_000_000;
    const to = from + 1_000_000;
    const fills = vi.spyOn(exchange, 'fetchHistoricalFills').mockResolvedValue([
      { id: 'entry', timestamp: from / 1000 + 60, symbol: 'PF_ETHUSD', price: 100, qty: 1, side: OrderSide.Buy },
      { id: 'exit', timestamp: from / 1000 + 120, symbol: 'PF_ETHUSD', price: 110, qty: 1, side: OrderSide.Sell, realizedPnl: 10 },
    ]);
    vi.spyOn(exchange, 'fetchIncomeHistoryRange').mockResolvedValue([]);
    try {
      const records = await exchange.fetchHistoricalPositions({ account: 'kraken-account', from, to });
      expect(records).toHaveLength(1);
      expect(records[0]).toMatchObject({ account: 'kraken-account', symbol: 'PF_ETHUSD', state: 'closed', realizedPnl: 10, sourceAxis: 'fills' });
      expect(fills).toHaveBeenCalledExactlyOnceWith({ from: from - 14 * 86_400_000, to, limit: 1000 });
    } finally {
      exchange.dispose();
    }
  });
});
