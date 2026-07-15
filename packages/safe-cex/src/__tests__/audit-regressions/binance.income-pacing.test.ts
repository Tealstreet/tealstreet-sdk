import { afterEach, describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../../exchanges/binance/binance.exchange';
import { DefaultStore } from '../../store/store.base';

describe('Binance income history pacing', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('waits two seconds before issuing the next seven-day chunk', async () => {
    vi.useFakeTimers();
    const exchange = new BinanceExchange('binance', { key: 'key', secret: 'secret' }, new DefaultStore());
    const fetchIncomeHistory = vi.spyOn(exchange, 'fetchIncomeHistory').mockResolvedValue([]);
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
