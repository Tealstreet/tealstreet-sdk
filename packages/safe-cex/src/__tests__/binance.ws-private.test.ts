import { describe, expect, it, vi } from 'vitest';

import { BinancePrivateWebsocket } from '../exchanges/binance/binance.ws-private';
import { BinancePMPrivateWebsocket } from '../exchanges/binance/binancepm.ws-private';

describe('Binance private websocket position refresh', () => {
  it('commits fetched USD-M position risk snapshots to the store', async () => {
    const positions = [{ id: 'BTCUSDT:long', liquidationPrice: 45000 }];
    const parent = {
      isDisposed: false,
      fetchPositions: vi.fn().mockResolvedValue(positions),
      store: {
        addOrUpdatePositionsFull: vi.fn(),
      },
    };
    const ws = new BinancePrivateWebsocket(parent as any);

    await (ws as any).executePositionFetch();

    expect(parent.fetchPositions).toHaveBeenCalledTimes(1);
    expect(parent.store.addOrUpdatePositionsFull).toHaveBeenCalledWith(positions);
  });

  it('commits fetched portfolio-margin position risk snapshots to the store', async () => {
    const positions = [{ id: 'ETHUSDT:short', liquidationPrice: 2500 }];
    const parent = {
      isDisposed: false,
      fetchPositions: vi.fn().mockResolvedValue(positions),
      store: {
        addOrUpdatePositionsFull: vi.fn(),
      },
    };
    const ws = new BinancePMPrivateWebsocket(parent as any);

    await (ws as any).executePositionFetch();

    expect(parent.fetchPositions).toHaveBeenCalledTimes(1);
    expect(parent.store.addOrUpdatePositionsFull).toHaveBeenCalledWith(positions);
  });
});
