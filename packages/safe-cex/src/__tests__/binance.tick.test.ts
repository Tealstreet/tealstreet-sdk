import { describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../exchanges/binance/binance.exchange';

describe('Binance REST reconciliation ordering', () => {
  it('does not let a stale account snapshot overwrite a newer websocket position', async () => {
    let resolveAccount!: (value: Record<string, unknown>) => void;
    const accountRequest = new Promise<Record<string, unknown>>((resolve) => {
      resolveAccount = resolve;
    });
    const store = {
      loaded: {},
      subscribe: vi.fn(),
      update: vi.fn(),
    };
    const exchange = new BinanceExchange('binance', { key: 'test-key', secret: 'test-secret' }, store as any);
    const balance = { total: 100, free: 80, used: 20, upnl: 5 };
    const stalePositions = [{ id: 'BTCUSDT', contracts: 1 }];

    Object.assign(exchange, {
      isDisposed: false,
      fetchPositions: vi.fn().mockResolvedValue([]),
      fetchPositionsAndBalance: vi.fn().mockReturnValue(accountRequest),
      emitAndLogError: vi.fn(),
    });

    const tick = exchange.tick(1);
    exchange.markPositionStateUpdatedByWebsocket();
    resolveAccount({ balance, positions: stalePositions });
    await tick;

    expect(store.update).toHaveBeenCalledWith({
      balance,
      loaded: { balance: true, positions: true },
    });
  });
});
