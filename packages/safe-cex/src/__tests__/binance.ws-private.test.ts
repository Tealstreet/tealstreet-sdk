import { beforeEach, describe, expect, it, vi } from 'vitest';

import { BinancePrivateWebsocket } from '../exchanges/binance/binance.ws-private';
import { BinancePMPrivateWebsocket } from '../exchanges/binance/binancepm.ws-private';
import { MarginType, PositionMode, PositionSide } from '../types';

beforeEach(() => {
  vi.clearAllMocks();
});

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

  it('creates a new one-way position immediately and schedules a full repair', () => {
    const parent = {
      isDisposed: false,
      fetchPositions: vi.fn().mockResolvedValue([]),
      store: {
        positions: [],
        addOrUpdatePosition: vi.fn(),
        addOrUpdatePositionsFull: vi.fn(),
      },
    };
    const ws = new BinancePrivateWebsocket(parent as any);
    vi.spyOn(ws as any, 'throttledFetchPositions').mockImplementation(() => undefined);

    ws.handleAccountEvents([{ a: { P: [{ s: 'BTCUSDT', ps: 'BOTH', pa: '0.25', ep: '60000', up: '125' }] } }]);

    expect(parent.store.addOrUpdatePosition).toHaveBeenCalledWith({
      id: 'BTCUSDT',
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      entryPrice: 60000,
      contracts: 0.25,
      notional: 15125,
      unrealizedPnl: 125,
      leverage: 0,
      liquidationPrice: 0,
      marginType: MarginType.Cross,
      positionMode: PositionMode.OneWay,
      info: { s: 'BTCUSDT', ps: 'BOTH', pa: '0.25', ep: '60000', up: '125' },
    });
    expect((ws as any).throttledFetchPositions).toHaveBeenCalledTimes(1);
  });

  it('closes an existing one-way short immediately even without a balance delta', () => {
    const existing = {
      id: 'ETHUSDT',
      symbol: 'ETHUSDT',
      side: PositionSide.Short,
      entryPrice: 3000,
      contracts: 2,
      notional: 6000,
      unrealizedPnl: 0,
      leverage: 10,
      liquidationPrice: 3300,
      marginType: MarginType.Isolated,
      positionMode: PositionMode.OneWay,
      info: { source: 'rest' },
    };
    const parent = {
      isDisposed: false,
      fetchPositions: vi.fn().mockResolvedValue([]),
      store: {
        positions: [existing],
        addOrUpdatePosition: vi.fn(),
        addOrUpdatePositionsFull: vi.fn(),
      },
    };
    const ws = new BinancePrivateWebsocket(parent as any);
    vi.spyOn(ws as any, 'throttledFetchPositions').mockImplementation(() => undefined);

    ws.handleAccountEvents([{ a: { P: [{ s: 'ETHUSDT', ps: 'BOTH', pa: '0', ep: '0', up: '0' }] } }]);

    expect(parent.store.addOrUpdatePosition).toHaveBeenCalledWith(
      expect.objectContaining({ id: existing.id, contracts: 0, leverage: 10, liquidationPrice: 3300 }),
    );
    expect((ws as any).throttledFetchPositions).toHaveBeenCalledTimes(1);
  });

  it('does not create unknown zero-size websocket positions', () => {
    const parent = {
      isDisposed: false,
      store: {
        positions: [],
        addOrUpdatePosition: vi.fn(),
      },
    };
    const ws = new BinancePrivateWebsocket(parent as any);
    vi.spyOn(ws as any, 'throttledFetchPositions').mockImplementation(() => undefined);

    ws.handleAccountEvents([{ a: { P: [{ s: 'SOLUSDT', ps: 'BOTH', pa: '0', ep: '0', up: '0' }] } }]);

    expect(parent.store.addOrUpdatePosition).not.toHaveBeenCalled();
    expect((ws as any).throttledFetchPositions).toHaveBeenCalledTimes(1);
  });
});
