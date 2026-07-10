import { beforeEach, describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../exchanges/binance/binance.exchange';
import { loop } from '../utils/loop';

vi.mock('../utils/loop', () => ({ loop: vi.fn() }));

describe('Binance REST reconciliation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('polls account state every 30 seconds and keeps position-risk reconciliation near 90 seconds', async () => {
    const store = {
      loaded: {},
      subscribe: vi.fn(),
      update: vi.fn(),
    };
    const exchange = new BinanceExchange('binance', { key: 'test-key', secret: 'test-secret' }, store as any);
    const fetchPositions = vi.fn().mockResolvedValue([]);
    const fetchPositionsAndBalance = vi.fn().mockResolvedValue({
      balance: { total: 0, free: 0, used: 0, upnl: 0 },
      positions: [],
    });

    Object.assign(exchange, {
      isDisposed: false,
      fetchPositions,
      fetchPositionsAndBalance,
      emitAndLogError: vi.fn(),
    });

    for (const count of [0, 1, 2, 3]) {
      await exchange.tick(count);
    }

    expect(fetchPositionsAndBalance).toHaveBeenCalledTimes(4);
    expect(fetchPositions).toHaveBeenCalledTimes(2);
    expect(loop).toHaveBeenCalledTimes(4);
    expect(loop).toHaveBeenCalledWith(expect.any(Function), 30_000);
  });
});
