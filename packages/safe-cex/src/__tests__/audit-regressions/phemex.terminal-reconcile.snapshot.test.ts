import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BinancePrivateWebsocket } from '../reference/phemex/phemex.ws-private';
import { MarginType, PositionMode, PositionSide } from '../../types';

const position = (contracts: number, side = PositionSide.Long, positionMode = PositionMode.Hedged) => ({
  id: `BTCUSDT:${side}`,
  symbol: 'BTCUSDT',
  side,
  contracts,
  entryPrice: 69_000,
  notional: contracts * 69_000,
  leverage: 10,
  unrealizedPnl: 0,
  liquidationPrice: 0,
  marginType: MarginType.Isolated,
  positionMode,
});

const setup = (fullPositions = [position(2)], knownMarket = true) => {
  const priorBalance = { used: 30, free: 970, total: 1000, upnl: 0 };
  const nextBalance = { used: 20, free: 980, total: 1000, upnl: 0 };
  const store = {
    markets: knownMarket ? [{ symbol: 'BTCUSDT' }] : [],
    positions: [position(3)],
    balance: priorBalance,
    loaded: { orders: true, balance: true, positions: true },
    addOrUpdateOrders: vi.fn(),
    addOrUpdatePositions: vi.fn(),
    addOrUpdatePositionsFull: vi.fn(),
    update: vi.fn(),
  };
  store.addOrUpdatePositionsFull.mockImplementation((positions) => {
    store.positions = positions;
  });
  store.update.mockImplementation((patch) => {
    if (patch.balance) store.balance = patch.balance;
    if (patch.loaded) store.loaded = patch.loaded;
  });
  const parent = {
    isDisposed: false,
    name: 'phemex',
    options: { testnet: false, key: '', secret: '' },
    store,
    emitter: { emit: vi.fn() },
    emitAndLogError: vi.fn(),
    formatSymbolIn: (symbol?: string) => symbol?.toUpperCase() || '',
    mapOrderUpdate: vi.fn(() => []),
    mapPosition: vi.fn(),
    fetchPositionsAndBalance: vi.fn().mockResolvedValue({ balance: nextBalance, positions: fullPositions }),
    // A second read may lag behind the authoritative full snapshot.
    fetchPositions: vi.fn().mockResolvedValue([position(1)]),
  };
  const ws = new BinancePrivateWebsocket(parent as any);
  ws.lastReceived = Date.now();
  const terminal = (posSide: string | null = 'Long') =>
    ws.onMessage({
      data: JSON.stringify({
        type: 'incremental',
        orders_p: [
          { orderID: 'terminal-order', symbol: 'BTCUSDT', ordStatus: 'Canceled', posSide: posSide ?? undefined },
        ],
      }),
    } as MessageEvent);
  return { ws, store, parent, terminal, priorBalance, nextBalance };
};

describe('Phemex terminal reconciliation snapshot authority', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.clearAllTimers();
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it.each([2, -2])(
    'keeps the full nonzero position (%s contracts) instead of overlaying a stale symbol read',
    async (contracts) => {
      const { store, parent, terminal, nextBalance } = setup([position(contracts)]);
      terminal();
      await vi.advanceTimersByTimeAsync(1000);
      expect(store.positions).toEqual([position(contracts)]);
      expect(store.balance).toEqual(nextBalance);
      expect(parent.fetchPositionsAndBalance).toHaveBeenCalledTimes(1);
      expect(parent.fetchPositions).not.toHaveBeenCalled();
    },
  );

  it.each([
    ['absent known symbol', [], true],
    ['flat known symbol', [position(0)], true],
    ['absent symbol during hosted startup', [], false],
  ] as const)('retains scoped recovery for %s', async (_label, positions, knownMarket) => {
    const { store, parent, terminal } = setup([...positions], knownMarket);
    terminal();
    await vi.advanceTimersByTimeAsync(1000);
    expect(parent.fetchPositions).toHaveBeenCalledExactlyOnceWith(['BTCUSDT']);
    expect(store.positions).toEqual([position(1)]);
  });

  it.each(['Short', undefined, 'Unrecognized'])(
    'recovers missing or unknown leg %s without rolling back the full Long row',
    async (posSide) => {
      const { store, parent, terminal } = setup();
      parent.fetchPositions.mockResolvedValueOnce([position(1), position(2, PositionSide.Short)]);
      terminal(posSide ?? null);
      await vi.advanceTimersByTimeAsync(1000);
      expect(parent.fetchPositions).toHaveBeenCalledExactlyOnceWith(['BTCUSDT']);
      expect(store.positions).toEqual([position(2), position(2, PositionSide.Short)]);
    },
  );

  it('preserves both affected hedge sides across the debounce window', async () => {
    const { store, parent, terminal } = setup();
    parent.fetchPositions.mockResolvedValueOnce([position(1), position(2, PositionSide.Short)]);
    terminal('Short');
    terminal('Long');
    await vi.advanceTimersByTimeAsync(1000);
    expect(parent.fetchPositionsAndBalance).toHaveBeenCalledTimes(1);
    expect(parent.fetchPositions).toHaveBeenCalledExactlyOnceWith(['BTCUSDT']);
    expect(store.positions).toEqual([position(2), position(2, PositionSide.Short)]);
  });

  it.each([PositionMode.OneWay, PositionMode.Hedged])(
    'covers Merged only with a nonzero one-way row (%s)',
    async (mode) => {
      const { parent, terminal } = setup([position(2, PositionSide.Long, mode)]);
      terminal('Merged');
      await vi.advanceTimersByTimeAsync(1000);
      expect(parent.fetchPositions).toHaveBeenCalledTimes(mode === PositionMode.OneWay ? 0 : 1);
    },
  );

  it('retains positions and balance when the full reconciliation read throws', async () => {
    const { store, parent, terminal, priorBalance } = setup();
    parent.fetchPositionsAndBalance.mockRejectedValueOnce(new Error('unavailable'));
    terminal();
    await vi.advanceTimersByTimeAsync(1000);
    expect(store.positions).toEqual([position(3)]);
    expect(store.balance).toEqual(priorBalance);
    expect(parent.fetchPositions).not.toHaveBeenCalled();
    expect(store.addOrUpdatePositionsFull).not.toHaveBeenCalled();
    expect(store.update).not.toHaveBeenCalled();
  });
});
