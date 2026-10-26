import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BinancePrivateWebsocket } from './reference/binance/binance.ws-private';
import { BinancePMPrivateWebsocket } from './reference/binance/binancepm.ws-private';
import { MarginType, PositionMode, PositionSide } from '../types';

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('Binance private websocket position refresh', () => {
  it('commits fetched USD-M position risk snapshots to the store', async () => {
    const positions = [{ id: 'BTCUSDT:long', liquidationPrice: 45000 }];
    const parent = {
      isDisposed: false,
      isBackgroundReconciliationPaused: vi.fn(() => false),
      getPositionStateVersion: vi.fn().mockReturnValue(0),
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
      getPositionStateVersion: vi.fn().mockReturnValue(0),
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
      markPositionStateUpdatedByWebsocket: vi.fn(),
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
      markPositionStateUpdatedByWebsocket: vi.fn(),
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
      markPositionStateUpdatedByWebsocket: vi.fn(),
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

  it('does not commit a position snapshot made stale by a newer websocket event', async () => {
    let resolveFirstFetch!: (positions: Array<Record<string, unknown>>) => void;
    const firstFetch = new Promise<Array<Record<string, unknown>>>((resolve) => {
      resolveFirstFetch = resolve;
    });
    const freshPositions = [{ id: 'BTCUSDT', contracts: 2, liquidationPrice: 45000 }];
    const parent = {
      isDisposed: false,
      positionStateVersion: 0,
      isBackgroundReconciliationPaused: vi.fn(() => false),
      markPositionStateUpdatedByWebsocket: vi.fn(function (this: { positionStateVersion: number }) {
        this.positionStateVersion += 1;
      }),
      getPositionStateVersion: vi.fn(function (this: { positionStateVersion: number }) {
        return this.positionStateVersion;
      }),
      fetchPositions: vi.fn().mockReturnValueOnce(firstFetch).mockResolvedValueOnce(freshPositions),
      store: {
        positions: [],
        addOrUpdatePosition: vi.fn(),
        addOrUpdatePositionsFull: vi.fn(),
      },
    };
    const ws = new BinancePrivateWebsocket(parent as any);
    const scheduleRepair = vi.spyOn(ws as any, 'throttledFetchPositions').mockImplementation(() => undefined);

    const staleRequest = (ws as any).executePositionFetch();
    ws.handleAccountEvents([{ a: { P: [{ s: 'BTCUSDT', ps: 'BOTH', pa: '2', ep: '60000', up: '0' }] } }]);
    resolveFirstFetch([{ id: 'BTCUSDT', contracts: 1, liquidationPrice: 40000 }]);
    await staleRequest;

    expect(parent.store.addOrUpdatePositionsFull).not.toHaveBeenCalled();
    expect(scheduleRepair).toHaveBeenCalledTimes(2);

    await (ws as any).executePositionFetch();

    expect(parent.store.addOrUpdatePositionsFull).toHaveBeenCalledWith(freshPositions);
  });

  it('caps event-triggered position repairs at one request per ten seconds', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-13T20:00:00Z'));

    const parent = {
      isDisposed: false,
      positionStateVersion: 0,
      markPositionStateUpdatedByWebsocket: vi.fn(function (this: { positionStateVersion: number }) {
        this.positionStateVersion += 1;
      }),
      getPositionStateVersion: vi.fn(function (this: { positionStateVersion: number }) {
        return this.positionStateVersion;
      }),
      isBackgroundReconciliationPaused: vi.fn(() => false),
      fetchPositions: vi.fn().mockResolvedValue([]),
      store: {
        positions: [],
        addOrUpdatePosition: vi.fn(),
        addOrUpdatePositionsFull: vi.fn(),
      },
    };
    const ws = new BinancePrivateWebsocket(parent as any);
    const event = { a: { P: [{ s: 'BTCUSDT', ps: 'BOTH', pa: '0.25', ep: '60000', up: '0' }] } };

    ws.handleAccountEvents([event]);
    await Promise.resolve();
    expect(parent.fetchPositions).toHaveBeenCalledTimes(1);

    for (let i = 0; i < 20; i += 1) ws.handleAccountEvents([event]);
    await vi.advanceTimersByTimeAsync(9_999);
    expect(parent.fetchPositions).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(1);
    expect(parent.fetchPositions).toHaveBeenCalledTimes(2);
  });
});
