import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { PhemexExchange } from '../reference/phemex/phemex.exchange';
import { DefaultStore } from '../../store/store.base';
import { PositionMode, PositionSide } from '../../types';

const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
};

const position = (contracts: number) => ({
  id: `BTCUSDT:${PositionSide.Long}`, symbol: 'BTCUSDT', side: PositionSide.Long,
  contracts, entryPrice: 69000, notional: contracts * 69000,
  leverage: 10, unrealizedPnl: contracts * 1000, liquidationPrice: 0,
  positionMode: PositionMode.Hedged,
});
const stale = () => ({
  balance: { used: 138, free: 1162, total: 1300, upnl: 20 },
  positions: [position(0.01)],
});
const exchanges: PhemexExchange[] = [];
const setup = () => {
  const store = new DefaultStore();
  store.update({
    markets: [{ id: 'BTCUSDT', symbol: 'BTCUSDT', quote: 'USDT' }],
    tickers: [{ symbol: 'BTCUSDT', last: 70000 }],
    positions: [position(0.01)],
    balance: {
      total: 1200, free: 1131, used: 69, upnl: 10,
      currencies: {
        USDT: { total: 1000, free: 931, used: 69, upnl: 10 },
        USDC: { total: 200, free: 200, used: 0, upnl: 0 },
      },
    },
  } as any);
  const exchange = new PhemexExchange('phemex', {
    key: 'test', secret: 'test', extra: { tickInterval: Infinity },
  } as any, store);
  exchanges.push(exchange);
  const ws = exchange.privateWebsocket;
  ws.lastReceived = Date.now();
  vi.spyOn(exchange, 'mapOrderUpdate').mockReturnValue([]);
  const terminal = () => ws.onMessage({ data: JSON.stringify({
    orders_p: [{ orderID: 'terminal', symbol: 'BTCUSDT', ordStatus: 'Canceled', posSide: 'Long' }],
  }) } as MessageEvent);
  const push = (kind: 'both' | 'accounts' | 'positions' = 'both') => ws.onMessage({ data: JSON.stringify({
    ...(kind !== 'accounts' ? { positions_p: [{
      symbol: 'BTCUSDT', side: 'Buy', posSide: 'Long', posMode: 'Hedged',
      size: '0.02', avgEntryPriceRp: '69000', markPriceRp: '70000',
      leverageRr: '10', liquidationPriceRp: '0',
    }] } : {}),
    ...(kind !== 'positions' ? { accounts_p: [{
      currency: 'USDT', accountBalanceRv: '1100', totalUsedBalanceRv: '138',
    }] } : {}),
  }) } as MessageEvent);
  return { exchange, store, ws, terminal, push };
};

describe('Phemex REST account snapshots cannot overwrite newer private pushes', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    for (const exchange of exchanges.splice(0)) exchange.dispose();
    vi.clearAllTimers();
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it.each(['periodic', 'terminal'] as const)(
    'retains the newer WS balance convention and position after a delayed %s read',
    async (source) => {
      const { exchange, store, terminal, push } = setup();
      const read = deferred<ReturnType<typeof stale>>();
      vi.spyOn(exchange, 'fetchPositionsAndBalance').mockReturnValue(read.promise);
      const running = source === 'periodic' ? exchange.tick() : undefined;
      if (source === 'terminal') {
        terminal();
        await vi.advanceTimersByTimeAsync(1000);
      }
      expect(exchange.fetchPositionsAndBalance).toHaveBeenCalledTimes(1);
      push();
      expect(store.balance).toMatchObject({ total: 1280, free: 1162, used: 138, upnl: 20 });
      expect(store.positions[0].contracts).toBe(0.02);
      read.resolve(stale());
      await running;
      await vi.advanceTimersByTimeAsync(0);
      expect(store.balance).toMatchObject({ total: 1280, free: 1162, used: 138, upnl: 20 });
      expect(store.positions[0].contracts).toBe(0.02);
    },
  );

  it.each(['accounts', 'positions'] as const)('also fences a periodic read across a %s-only push', async (kind) => {
    const { exchange, store, push } = setup();
    const read = deferred<ReturnType<typeof stale>>();
    vi.spyOn(exchange, 'fetchPositionsAndBalance').mockReturnValue(read.promise);
    const running = exchange.tick();
    push(kind);
    const balance = structuredClone(store.balance);
    const positions = structuredClone(store.positions);
    read.resolve(stale());
    await running;
    expect(store.balance).toEqual(balance);
    expect(store.positions).toEqual(positions);
  });

  it('rechecks authority after the additional scoped terminal-recovery read', async () => {
    const { exchange, store, terminal, push } = setup();
    vi.spyOn(exchange, 'fetchPositionsAndBalance').mockResolvedValue({ ...stale(), positions: [] });
    const scoped = deferred<ReturnType<typeof position>[]>();
    vi.spyOn(exchange, 'fetchPositions').mockReturnValue(scoped.promise);
    terminal();
    await vi.advanceTimersByTimeAsync(1000);
    expect(exchange.fetchPositions).toHaveBeenCalledExactlyOnceWith(['BTCUSDT']);
    push();
    scoped.resolve([position(0.01)]);
    await vi.advanceTimersByTimeAsync(0);
    expect(store.balance.total).toBe(1280);
    expect(store.positions[0].contracts).toBe(0.02);
  });

  it.each(['periodic', 'terminal'] as const)('still accepts a %s read without an intervening push', async (source) => {
    const { exchange, store, terminal } = setup();
    vi.spyOn(exchange, 'fetchPositionsAndBalance').mockResolvedValue(stale());
    if (source === 'periodic') await exchange.tick();
    else {
      terminal();
      await vi.advanceTimersByTimeAsync(1000);
    }
    expect(store.balance.total).toBe(1300);
  });

  it('a committed terminal REST snapshot fences an older empty periodic response', async () => {
    const { exchange, store, terminal } = setup();
    const older = deferred<ReturnType<typeof stale>>();
    vi.spyOn(exchange, 'fetchPositionsAndBalance')
      .mockReturnValueOnce(older.promise)
      .mockResolvedValueOnce({
        balance: { used: 138, free: 1162, total: 1280, upnl: 20 },
        positions: [position(0.02)],
      });
    const running = exchange.tick();
    terminal();
    await vi.advanceTimersByTimeAsync(1000);
    expect(exchange.fetchPositionsAndBalance).toHaveBeenCalledTimes(2);
    expect(store.balance.total).toBe(1280);
    expect(store.positions[0].contracts).toBe(0.02);
    older.resolve({ ...stale(), positions: [] });
    await running;
    expect(store.balance.total).toBe(1280);
    expect(store.positions[0].contracts).toBe(0.02);
  });

  it('a committed periodic snapshot also fences an older terminal recovery read', async () => {
    const { exchange, store, terminal } = setup();
    const older = deferred<ReturnType<typeof stale>>();
    vi.spyOn(exchange, 'fetchPositionsAndBalance')
      .mockReturnValueOnce(older.promise)
      .mockResolvedValueOnce({
        balance: { used: 138, free: 1162, total: 1280, upnl: 20 },
        positions: [position(0.02)],
      });
    terminal();
    await vi.advanceTimersByTimeAsync(1000);
    await exchange.tick();
    older.resolve({ ...stale(), positions: [] });
    await vi.advanceTimersByTimeAsync(0);
    expect(store.balance.total).toBe(1280);
    expect(store.positions[0].contracts).toBe(0.02);
  });

  it.each(['periodic', 'terminal'] as const)('disconnect invalidates a pending %s read and preserves base recovery', async (source) => {
    const { exchange, store, ws, terminal } = setup();
    const older = deferred<ReturnType<typeof stale>>();
    vi.spyOn(exchange, 'fetchPositionsAndBalance').mockReturnValue(older.promise);
    const running = source === 'periodic' ? exchange.tick() : undefined;
    if (source === 'terminal') {
      terminal();
      await vi.advanceTimersByTimeAsync(1000);
    }
    const balance = structuredClone(store.balance);
    const positions = structuredClone(store.positions);
    (ws as any).onClose({ code: 1000 });
    expect((ws as any).reconnecting).toBe(true);
    older.resolve({ ...stale(), positions: [] });
    await running;
    await Promise.resolve();
    expect(store.balance).toEqual(balance);
    expect(store.positions).toEqual(positions);
  });

  it('socket reopen invalidates reads issued before its new generation', async () => {
    const { exchange, store, ws } = setup();
    const older = deferred<ReturnType<typeof stale>>();
    vi.spyOn(exchange, 'fetchPositionsAndBalance').mockReturnValue(older.promise);
    vi.spyOn(ws, 'auth').mockResolvedValue(undefined);
    vi.spyOn(ws, 'ping').mockImplementation(() => {});
    const running = exchange.tick();
    const balance = structuredClone(store.balance);
    const positions = structuredClone(store.positions);
    ws.onOpen();
    expect(ws.auth).toHaveBeenCalledTimes(1);
    expect(ws.ping).toHaveBeenCalledTimes(1);
    older.resolve({ ...stale(), positions: [] });
    await running;
    expect(store.balance).toEqual(balance);
    expect(store.positions).toEqual(positions);
  });
});
