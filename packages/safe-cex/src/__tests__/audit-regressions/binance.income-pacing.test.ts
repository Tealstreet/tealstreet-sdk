import { afterEach, describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../reference/binance/binance.exchange';
import { DefaultStore } from '../../store/store.base';

describe('Binance income history pacing', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('waits two seconds before issuing the next seven-day chunk', async () => {
    vi.useFakeTimers();
    const exchange = new BinanceExchange('binance', { key: 'key', secret: 'secret' }, new DefaultStore());
    // Spies on `_fetchIncomeHistoryPage`, not the public `fetchIncomeHistory`. The range
    // walker deliberately uses the RETHROWING variant, because the public method swallows
    // errors into `[]` and a pager cannot distinguish that from an empty window — a 429
    // mid-walk would read as "nothing more here" and truncate the ledger silently.
    const fetchIncomeHistory = vi.spyOn(exchange as any, '_fetchIncomeHistoryPage').mockResolvedValue([]);
    const sevenDays = 7 * 24 * 60 * 60 * 1000;

    const result = exchange.fetchIncomeHistoryRange(0, sevenDays * 2);
    await vi.advanceTimersByTimeAsync(0);
    expect(fetchIncomeHistory).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(1_999);
    expect(fetchIncomeHistory).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(fetchIncomeHistory).toHaveBeenCalledTimes(2);
    await expect(result).resolves.toEqual([]);
  });
});
