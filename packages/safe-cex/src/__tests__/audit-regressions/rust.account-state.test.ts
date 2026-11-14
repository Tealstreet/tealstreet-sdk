import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, Market, Order, PlaceOrderOpts, Position, StoreData } from '../../types';
import type { TwapOptions } from '../../webworker/twap/TwapOptions';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { BaseExchange } from '../../exchanges/base';
import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderStatus, OrderType, PositionSide } from '../../types';
import { ChaserOrderInstance } from '../../webworker/chaserOrder/ChaserOrderInstance';
import { ChaserOrderManager, chaserOrderManagerInstance } from '../../webworker/chaserOrder/ChaserOrderManager';
import { TwapInstance } from '../../webworker/twap/TwapInstance';
import { twapManager } from '../../webworker/twap/TwapManager';
import { consumerFrame } from '../cutover/consumerFrame';
import { engineCapabilities, quantityUnitsCode } from '../fixtures/engineCapabilities';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

const accounts: BaseExchange[] = [];
const loaded = { markets: true, tickers: true, orders: true, positions: true, balance: true };
const markets: Market[] = ['BTCUSDT', 'ETHUSDT'].map((symbol) => ({
  id: symbol,
  symbol,
  base: symbol.slice(0, 3),
  quote: 'USDT',
  active: true,
  precision: { amount: 0.1, price: 0.1 },
  limits: { amount: { min: 0.1, max: 100 }, leverage: { min: 1, max: 100 } },
  contractSize: 0.1,
}));
const order = (id = 'order-1', symbol = 'BTCUSDT'): Order => ({
  id,
  symbol,
  side: OrderSide.Buy,
  type: OrderType.Limit,
  status: OrderStatus.Open,
  price: 100,
  amount: 2,
  filled: 0,
  remaining: 2,
  reduceOnly: false,
  lastUpdate: 1,
});
const position = (symbol = 'BTCUSDT'): Position => ({
  id: symbol,
  symbol,
  side: PositionSide.Long,
  entryPrice: 100,
  contracts: 2,
  notional: 200,
  leverage: 5,
  unrealizedPnl: 1,
  liquidationPrice: 50,
});

// Independent format-1 ACCOUNT_READ marker (section 21, empty payload).
function accountReadFrame(frame: ArrayBuffer): ArrayBuffer {
  const buffer = new ArrayBuffer(frame.byteLength + 8);
  new Uint8Array(buffer).set(new Uint8Array(frame));
  const view = new DataView(buffer);
  view.setUint16(6, view.getUint16(6, true) + 1, true);
  view.setUint16(frame.byteLength, 21, true);
  return buffer;
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.spyOn(console, 'trace').mockImplementation(() => {});
});
afterEach(() => {
  accounts.splice(0).forEach((account) => account.dispose());
  vi.useRealTimers();
  vi.restoreAllMocks();
});

function fixture(name: ExchangeName = 'bybit', nativeLoaded = loaded, publicDataOnly = true) {
  let state: Partial<StoreData> = {
    markets,
    tickers: [
      {
        id: 'BTCUSDT',
        symbol: 'BTCUSDT',
        bid: 100,
        ask: 101,
        last: 100,
        mark: 100,
        index: 100,
        fundingRate: 0,
        fundingTime: 0,
        openInterest: 0,
        percentage: 0,
        volume: 0,
        quoteVolume: 0,
      },
    ],
    orders: [],
    positions: [],
    balance: { free: 100, used: 0, total: 100, upnl: 0 },
    fetchedLeverage: {},
    options: { isHedged: false },
    loaded: { ...nativeLoaded },
  };
  let dirty = true;
  let catalogDirty = true;
  let eventsPending = false;
  let readHook = (): void => {};
  const commands: Array<{ op: string; command?: number; symbol?: string; orders?: PlaceOrderOpts[] }> = [];
  const idle = () => '[]';
  const fullFrame = () =>
    consumerFrame(
      state,
      undefined,
      eventsPending,
      state.markets!.map((m) => m.symbol),
      quantityUnitsCode(name),
    );
  const native: WasmEngineLike = {
    syncHostTime: () => true,
    capabilities: () => engineCapabilities(name),
    start: vi.fn(idle),
    wsOpen: idle,
    wsMessage: idle,
    wsClose: idle,
    wsRouteExhausted: idle,
    httpStart: idle,
    httpChunk: idle,
    httpEnd: idle,
    httpFailed: idle,
    timer: idle,
    command: (json) => {
      // The caller wait is engine receipt metadata (pinned by the host tests),
      // not part of the native command these regressions compare.
      const { timeoutMs: _wait, ...command } = JSON.parse(json);
      commands.push(command);
      // A market close closes: the nuke reads flat back before it reports success.
      if (command.op === 'placeOrders' && command.orders?.every((o: PlaceOrderOpts) => o.reduceOnly)) {
        const closed = command.orders.map(
          (o: PlaceOrderOpts) => `${o.symbol}:${o.side === OrderSide.Sell ? PositionSide.Long : PositionSide.Short}`,
        );
        state = { ...state, positions: state.positions?.filter((p) => !closed.includes(`${p.symbol}:${p.side}`)) };
        dirty = true;
      }
      return command.command === undefined
        ? '[]'
        : JSON.stringify([
            {
              op: 'done',
              command: command.command,
              result:
                command.op === 'placeOrders'
                  ? '["order-1"]'
                  : command.op === 'readPositions'
                    ? JSON.stringify({
                        skipped: [],
                        open: (state.positions ?? []).filter((p) => p.contracts !== 0),
                      })
                    : '[]',
            },
          ]);
    },
    takeFrame: () => {
      if (!dirty) return undefined;
      dirty = false;
      catalogDirty = false;
      const frame = new Uint8Array(fullFrame());
      eventsPending = false;
      return frame;
    },
    snapshotFrame: () => new Uint8Array(fullFrame()),
    accountSnapshotFrame: vi.fn(() => {
      readHook();
      return new Uint8Array(
        accountReadFrame(
          consumerFrame(
            {
              ...(catalogDirty ? { markets: state.markets } : {}),
              orders: state.orders,
              positions: state.positions,
              balance: state.balance,
              fetchedLeverage: state.fetchedLeverage,
              options: state.options,
              loaded: state.loaded,
            },
            undefined,
            false,
            state.markets!.map((m) => m.symbol),
            quantityUnitsCode(name),
          ),
        ),
      );
    }),
  };
  const construct = vi.fn(function () {
    return native;
  });
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const store = new DefaultStore();
  const exchange = new RustEngineExchange(
    name,
    { publicDataOnly, key: 'fixture-key', secret: 'fixture-secret' },
    store,
  );
  exchange.on('error', () => {});
  accounts.push(exchange);
  const start = async () => {
    const starting = exchange.start();
    await vi.advanceTimersByTimeAsync(250);
    await starting;
  };
  return {
    exchange,
    store,
    native,
    commands,
    construct,
    start,
    update: (next: Partial<StoreData>, events = false) => {
      state = { ...state, ...next };
      dirty = true;
      catalogDirty ||= next.markets !== undefined;
      eventsPending ||= events;
    },
    onRead: (hook: typeof readHook) => {
      readHook = hook;
    },
  };
}

describe('Rust native current-account reads', () => {
  it('keeps the Base seam a synchronous no-op', () => {
    const store = new DefaultStore();
    const exchange = new BaseExchange('bybit', { key: '', secret: '' }, store);
    accounts.push(exchange);
    const update = vi.fn();
    store.subscribe(update);
    expect(exchange.syncAccountState()).toBeUndefined();
    expect(update).not.toHaveBeenCalled();
  });

  it('reads before-flush orders, positions, balance and mode through the native frame mapping', async () => {
    const r = fixture('mexc');
    await r.start();
    r.update({
      orders: [order()],
      positions: [{ ...position(), extra: { contractsNative: 20 } }],
      balance: { free: 70, used: 30, total: 100, upnl: 2 },
      options: { isHedged: true },
      fetchedLeverage: { BTCUSDT: { buyLeverage: 3, sellLeverage: 7, marginType: MarginType.Isolated } },
    });
    expect(r.store.orders).toEqual([]);
    expect((await r.exchange.fetchOrders())[0]).toMatchObject({ id: 'order-1', amount: 2 });
    expect((await r.exchange.fetchPositions())[0]).toMatchObject({ contracts: 2, extra: { contractsNative: 20 } });
    expect(await r.exchange.fetchBalance()).toMatchObject({ free: 70, used: 30, total: 100, upnl: 2 });
    expect(await r.exchange.fetchPositionMode()).toBe(true);
    expect(r.store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 3, sellLeverage: 7 });
    expect(r.native.accountSnapshotFrame).toHaveBeenCalledTimes(4);
  });

  it('keeps symbol filters and explicit native empties authoritative over a stale JS mirror', async () => {
    const r = fixture();
    await r.start();
    r.update({ orders: [order('a'), order('b', 'ETHUSDT')], positions: [position(), position('ETHUSDT')] });
    expect((await r.exchange.fetchOrders(['ETHUSDT'])).map((o) => o.id)).toEqual(['b']);
    expect((await r.exchange.fetchPositions(['BTCUSDT'])).map((p) => p.symbol)).toEqual(['BTCUSDT']);
    expect(await r.exchange.fetchOrders([])).toEqual([]);
    r.update({ orders: [], positions: [], balance: { free: 0, used: 0, total: 0, upnl: 0 }, fetchedLeverage: {} });
    r.exchange.syncAccountState();
    expect(r.store.orders).toEqual([]);
    expect(r.store.positions).toEqual([]);
    expect(r.store.balance.total).toBe(0);
    expect(r.store.fetchedLeverage).toEqual({});
  });

  it('forwards account frames and a dirty reordered catalog through the same main-thread decoder', async () => {
    const r = fixture();
    const main = new DefaultStore();
    const applier = new FrameApplier(main);
    const frames: Frame[] = [];
    r.exchange.onFrame = (buffer) => {
      const frame = new Frame(buffer);
      frames.push(frame);
      applier.apply(frame);
    };
    await r.start();
    frames.length = 0;
    r.update({ markets: [...markets].reverse(), positions: [position()] });
    await r.exchange.fetchPositions();
    expect(frames).toHaveLength(1);
    expect(frames[0].markets()?.symbol(1)).toBe('BTCUSDT');
    expect(main.positions[0].symbol).toBe('BTCUSDT');
    expect(main.tickers).toEqual(r.store.tickers);
  });

  it('clears main-thread exposure when a forwarded native account snapshot omits closed positions', async () => {
    const r = fixture();
    const main = new DefaultStore();
    const applier = new FrameApplier(main);
    r.exchange.onFrame = (buffer) => applier.apply(new Frame(structuredClone(buffer)));
    await r.start();
    r.update({ positions: [position(), position('ETHUSDT')] });
    await r.exchange.fetchPositions();
    expect(main.positions.map((row) => row.symbol)).toEqual(['BTCUSDT', 'ETHUSDT']);
    r.update({ positions: [position('ETHUSDT')] });
    await r.exchange.fetchPositions();
    expect(main.positions.map((row) => row.symbol)).toEqual(['ETHUSDT']);
    r.update({ positions: [] });
    await r.exchange.fetchPositions();
    expect(main.positions).toEqual([]);
    expect(main.serialize().positions).toEqual([]);
    expect(r.store.positions).toEqual([]);
  });

  it('does not consume or replay pending public book, candle and fill events', async () => {
    const r = fixture();
    await r.start();
    const book = vi.fn();
    const candle = vi.fn();
    const fill = vi.fn();
    r.exchange.listenOrderBook('BTCUSDT', book);
    r.exchange.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, candle);
    r.exchange.on('fill', fill);
    r.update({ positions: [position()] }, true);
    await r.exchange.fetchPositions();
    await r.exchange.fetchOrders();
    expect([book.mock.calls.length, candle.mock.calls.length, fill.mock.calls.length]).toEqual([0, 0, 0]);
    await vi.advanceTimersByTimeAsync(250);
    expect([book.mock.calls.length, candle.mock.calls.length, fill.mock.calls.length]).toEqual([1, 1, 1]);
    await r.exchange.fetchPositions();
    expect([book.mock.calls.length, candle.mock.calls.length, fill.mock.calls.length]).toEqual([1, 1, 1]);
  });

  it('applies current values without advancing freshness on repeated reads in worker or main', async () => {
    const r = fixture();
    const main = new DefaultStore();
    const applier = new FrameApplier(main);
    r.exchange.onFrame = (buffer) => applier.apply(new Frame(buffer));
    await r.start();
    const ids = { ...r.store.updateIds };
    const mainIds = { ...main.updateIds };
    r.update({ orders: [order()], positions: [position()] });
    r.exchange.syncAccountState();
    r.exchange.syncAccountState();
    expect(r.store.orders[0].id).toBe('order-1');
    expect(main.orders[0].id).toBe('order-1');
    expect(r.store.updateIds).toEqual(ids);
    expect(main.updateIds).toEqual(mainIds);
    await vi.advanceTimersByTimeAsync(250);
    expect(r.store.updateIds.orders).toBe(ids.orders + 1);
    expect(r.store.updateIds.positions).toBe(ids.positions + 1);
    expect(main.updateIds.orders).toBe(mainIds.orders + 1);
  });

  it('does not recursively snapshot when a store observer synchronously reads again', async () => {
    const r = fixture();
    await r.start();
    let calls = 0;
    const off = r.store.subscribe(() => {
      if (++calls <= 2) r.exchange.syncAccountState();
    });
    try {
      r.exchange.syncAccountState();
      expect(calls).toBe(1);
      expect(r.native.accountSnapshotFrame).toHaveBeenCalledOnce();
    } finally {
      off();
    }
  });

  it('does not let reads confirm a lost chaser order before two independent native stream observations', async () => {
    const r = fixture();
    await r.start();
    r.update({ orders: [order()] });
    await vi.advanceTimersByTimeAsync(250);
    const manager = new ChaserOrderManager();
    const instance = new ChaserOrderInstance(r.exchange, {
      symbol: 'BTCUSDT',
      size: 2,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });
    manager.add(instance);
    try {
      await vi.advanceTimersByTimeAsync(1000);
      // Establish the tracked order under the actual manager's update listener.
      r.store.update({ latency: 1 });
      await vi.advanceTimersByTimeAsync(0);
      expect(instance.isStopped).toBe(false);
      r.update({ orders: [] });
      r.exchange.syncAccountState();
      r.exchange.syncAccountState();
      await vi.advanceTimersByTimeAsync(0);
      expect(instance.isStopped).toBe(false);
      await vi.advanceTimersByTimeAsync(250);
      expect(instance.isStopped).toBe(false);
      r.exchange.syncAccountState();
      await vi.advanceTimersByTimeAsync(0);
      expect(instance.isStopped).toBe(false);
      r.update({ orders: [] });
      await vi.advanceTimersByTimeAsync(250);
      expect(instance.state.stopReason).toBe('Tracked orders disappeared from store');
      expect(r.commands.filter((c) => c.op === 'placeOrders')).toHaveLength(1);
    } finally {
      manager.cleanupExchange(r.exchange);
    }
  });

  it('rejects unstarted or merely JS-seeded synchronous reads', async () => {
    const r = fixture('bybit', { ...loaded, markets: false });
    r.store.update({ loaded });
    expect(() => r.exchange.syncAccountState()).toThrow('not started');
    const starting = r.exchange.start().catch(() => {});
    await vi.advanceTimersByTimeAsync(250);
    expect(() => r.exchange.syncAccountState()).toThrow('catalog is not ready');
    expect(r.native.accountSnapshotFrame).not.toHaveBeenCalled();
    r.exchange.dispose();
    await starting;
  });

  it('fails visibly for an older ABI instead of returning cached account data', async () => {
    const r = fixture();
    await r.start();
    delete r.native.accountSnapshotFrame;
    r.store.update({ orders: [order()] });
    await expect(r.exchange.fetchOrders()).rejects.toThrow('does not support private account snapshots');
  });

  it('contains read traps and rejects future queries without using cached data', async () => {
    const r = fixture();
    await r.start();
    r.onRead(() => {
      throw new WebAssembly.RuntimeError('unreachable');
    });
    await expect(r.exchange.fetchOrders()).rejects.toMatchObject({ code: 'ENGINE_DEAD' });
    await expect(r.exchange.fetchPositions()).rejects.toMatchObject({ code: 'ENGINE_DEAD' });
    expect(r.native.accountSnapshotFrame).toHaveBeenCalledTimes(1);
  });

  it('rejects disposed reads without native reentry', async () => {
    const r = fixture();
    await r.start();
    r.exchange.dispose();
    await expect(r.exchange.fetchBalance()).rejects.toThrow('disposed');
    expect(() => r.exchange.syncAccountState()).toThrow('disposed');
    expect(r.native.accountSnapshotFrame).not.toHaveBeenCalled();
  });

  it('wakes a cold account read on disposal instead of answering from its JS seed', async () => {
    const r = fixture('bybit', { ...loaded, orders: false }, false);
    r.store.update({ loaded, orders: [order()] });
    const reading = r.exchange.fetchOrders();
    const rejected = expect(reading).rejects.toThrow('disposed');
    await vi.advanceTimersByTimeAsync(250);
    expect(r.native.accountSnapshotFrame).not.toHaveBeenCalled();
    r.exchange.dispose();
    await rejected;
  });

  it('fails promptly when a public-only engine has no private readiness, but accepts native empty state', async () => {
    const missing = fixture('bybit', { ...loaded, orders: false });
    const reading = missing.exchange.fetchOrders();
    const rejected = expect(reading).rejects.toThrow('unavailable for a public-data-only account');
    await vi.advanceTimersByTimeAsync(250);
    await rejected;
    expect(missing.native.accountSnapshotFrame).not.toHaveBeenCalled();
    const empty = fixture();
    await empty.start();
    expect(empty.exchange.isPublicDataOnly).toBe(true);
    expect(await empty.exchange.fetchOrders()).toEqual([]);
    expect(empty.native.accountSnapshotFrame).toHaveBeenCalledOnce();
  });

  it.each(['orders', 'positions', 'balance', 'positionMode'] as const)(
    'starts one cold engine and waits only for %s readiness',
    async (kind) => {
      const r = fixture('bybit', { ...loaded, orders: false, positions: false, balance: false }, false);
      const reading =
        kind === 'orders'
          ? r.exchange.fetchOrders()
          : kind === 'positions'
            ? r.exchange.fetchPositions()
            : kind === 'balance'
              ? r.exchange.fetchBalance()
              : r.exchange.fetchPositionMode();
      const settled = vi.fn();
      void reading.then(settled);
      await vi.advanceTimersByTimeAsync(250);
      expect(settled).not.toHaveBeenCalled();
      expect(r.native.accountSnapshotFrame).not.toHaveBeenCalled();
      const flag = kind === 'positionMode' ? 'positions' : kind;
      r.update({ loaded: { ...loaded, orders: false, positions: false, balance: false, [flag]: true } });
      await vi.advanceTimersByTimeAsync(250);
      await reading;
      expect(settled).toHaveBeenCalledOnce();
      expect(r.construct).toHaveBeenCalledOnce();
    },
  );

  it('selects cancel-all symbols and set-all leverage positions from native state', async () => {
    const r = fixture();
    await r.start();
    r.store.update({ orders: [order('stale', 'ETHUSDT')], positions: [position('ETHUSDT')] });
    r.update({ orders: [order('a'), order('b')], positions: [position()] });
    await r.exchange.cancelAllOrders();
    await r.exchange.setAllLeverage(9);
    expect(r.commands.filter((c) => c.command !== undefined).map(({ command: _id, ...c }) => c)).toEqual([
      { op: 'cancelSymbolOrders', symbol: 'BTCUSDT' },
      { op: 'setLeverage', symbol: 'BTCUSDT', leverage: 9, side: PositionSide.Long },
    ]);
  });

  it.each(['woo', 'grvt', 'kucoin', 'kucoinspot', 'polymarket'] as ExchangeName[])(
    '%s preserves its native venue-global cancel-all command',
    async (name) => {
      const r = fixture(name);
      await r.start();
      await r.exchange.cancelAllOrders();
      expect(r.commands.filter((c) => c.command !== undefined).map(({ command: _id, ...c }) => c)).toEqual([
        { op: 'cancelSymbolOrders', symbol: '' },
      ]);
      expect(r.native.accountSnapshotFrame).not.toHaveBeenCalled();
    },
  );

  it('closes one native preflush snapshot with native linear quantities and separate hedge legs', async () => {
    const r = fixture('mexc');
    await r.start();
    r.store.update({ positions: [{ ...position(), contracts: 9999 }] });
    r.update({
      positions: [
        { ...position(), id: 'BTCUSDT-long' },
        { ...position(), id: 'BTCUSDT-short', side: PositionSide.Short, contracts: 3 },
      ],
      options: { isHedged: true },
    });
    await r.exchange.nukePositions(3);
    expect(r.commands.filter((c) => c.op === 'placeOrders').map((c) => c.orders)).toEqual([
      [{ symbol: 'BTCUSDT', side: OrderSide.Sell, type: OrderType.Market, amount: 20, reduceOnly: true }],
      [{ symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Market, amount: 30, reduceOnly: true }],
    ]);
    expect(r.exchange.isNuking).toBe(false);
  });

  it('uses inverse notional even when the published base quantity is zero', async () => {
    const r = fixture('binanceinverse');
    await r.start();
    r.update({
      markets: markets.map((m) => ({ ...m, contractSize: 100, isInverse: true })),
      positions: [{ ...position(), contracts: 0, notional: 40 }],
    });
    const nuking = r.exchange.nukePositions();
    await vi.advanceTimersByTimeAsync(600);
    await nuking;
    expect(r.commands.find((c) => c.op === 'placeOrders')?.orders).toEqual([
      { symbol: 'BTCUSDT', side: OrderSide.Sell, type: OrderType.Market, amount: 0.4, reduceOnly: true },
    ]);
  });

  it('reports unloaded rows as unknown, never as none, when a nuke could not read them', async () => {
    const r = fixture();
    await r.start();
    const logs: string[] = [];
    r.exchange.on('log', (message: string) => logs.push(message));
    r.exchange.fetchPositions = () => Promise.reject(new Error('positions did not load within 30s'));
    r.exchange.syncAccountState = () => undefined;
    r.store.update({ loaded: { ...r.store.loaded, positions: false } });
    await expect(r.exchange.nukePositions()).rejects.toThrow('positions did not load');
    expect(logs.join()).toContain('Open positions: unknown, not loaded');
  });

  it('cancels before it closes, so a close cannot fill against the account own bid', async () => {
    const r = fixture();
    await r.start();
    r.update({ positions: [position()] });
    const cancel = vi.spyOn(r.exchange, 'cancelAllOrders');
    const place = vi.spyOn(r.exchange, 'placeOrders');
    const nuking = r.exchange.nuke(3);
    await vi.advanceTimersByTimeAsync(600);
    await nuking;
    expect(cancel.mock.invocationCallOrder[0]).toBeLessThan(place.mock.invocationCallOrder[0]);
  });

  const placeRead = (r: ReturnType<typeof fixture>) => r.commands.map((c) => c.op);

  it('cancels a position symbol and an attempted placement with no order in the store (EachOpenSymbol)', async () => {
    const r = fixture();
    await r.start();
    r.update({ positions: [position('ETHUSDT')], orders: [] });
    await r.exchange
      .placeOrders([{ symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Limit, amount: 1, price: 90 }])
      .catch(() => undefined);
    const before = r.commands.length;
    const nuking = r.exchange.nuke();
    await vi.advanceTimersByTimeAsync(600);
    await nuking;
    const nuke = r.commands.slice(before);
    const close = nuke.findIndex((c) => c.op === 'placeOrders');
    const cancelled = nuke.slice(0, close).filter((c) => c.op === 'cancelSymbolOrders');
    expect(cancelled.map((c) => c.symbol).sort()).toEqual(['BTCUSDT', 'ETHUSDT']);
  });

  it('cancels what the stopping engine placed, and leaves it out of the result', async () => {
    const r = fixture();
    await r.start();
    r.update({ positions: [], orders: [] });
    r.exchange.stopAccountAlgos = () =>
      Promise.resolve({ stoppedChasers: [], stoppedTwaps: [], placedSymbols: ['ETHUSDT'] });
    const nuking = r.exchange.nuke();
    await vi.advanceTimersByTimeAsync(600);
    expect(await nuking).toEqual({ stoppedChasers: [], stoppedTwaps: [] });
    expect(r.commands.filter((c) => c.op === 'cancelSymbolOrders').map((c) => c.symbol)).toEqual(['ETHUSDT']);
  });

  it.each(['nuke', 'nukeOrders'] as const)(
    '%s sends the account-wide cancel where the venue has one, before any close, and does not warn',
    async (method) => {
      const r = fixture('bitget');
      await r.start();
      r.update({ positions: [position()], orders: [] });
      const nuking = r.exchange[method]();
      await vi.advanceTimersByTimeAsync(600);
      const result = await nuking;
      const ops = r.commands.map((c) => c.op);
      expect(ops).toContain('cancelAccountOrders');
      if (ops.includes('placeOrders'))
        expect(ops.indexOf('cancelAccountOrders')).toBeLessThan(ops.indexOf('placeOrders'));
      expect(result.warning).toBeUndefined();
    },
  );

  it('a failed account-wide cancel says orders may remain', async () => {
    const r = fixture('bitget');
    await r.start();
    r.update({ positions: [], orders: [] });
    const exchange = r.exchange as unknown as { command: (c: { op: string }) => Promise<unknown> };
    const command = exchange.command;
    exchange.command = (c) => (c.op === 'cancelAccountOrders' ? Promise.reject(new Error('refused')) : command(c));
    const nuking = r.exchange.nukeOrders().catch((error: Error) => error);
    await vi.advanceTimersByTimeAsync(600);
    expect(((await nuking) as Error).message).toContain('including triggers and TP/SL, may remain on this venue');
  });

  it('sends no account-wide cancel from a plain cancel-all, nor on a venue without one', async () => {
    const bitget = fixture('bitget');
    await bitget.start();
    await bitget.exchange.cancelAllOrders();
    expect(bitget.commands.map((c) => c.op)).not.toContain('cancelAccountOrders');
    const bybit = fixture('bybit');
    await bybit.start();
    bybit.update({ positions: [position()] });
    const nuking = bybit.exchange.nuke();
    await vi.advanceTimersByTimeAsync(600);
    expect((await nuking).warning).toBeUndefined();
    expect(bybit.commands.map((c) => c.op)).not.toContain('cancelAccountOrders');
  });

  it.each(['nuke', 'nukeOrders'] as const)(
    '%s says unseen orders may rest where no cancel reaches them, in success and in failure',
    async (method) => {
      const unseen = /Orders placed in the last few seconds may not be cancelled on this venue: check open orders/;
      const r = fixture('hyperliquid');
      await r.start();
      r.update({ positions: [position()] });
      const nuking = r.exchange[method]();
      await vi.advanceTimersByTimeAsync(600);
      expect((await nuking).warning).toMatch(unseen);
      vi.spyOn(r.exchange, 'cancelAllOrders').mockRejectedValue(new Error('cancel down'));
      const failing = r.exchange[method]().catch((error: Error) => error.message);
      await vi.advanceTimersByTimeAsync(600);
      const message = await failing;
      expect(message).toMatch(/cancel down/);
      expect(message).toMatch(unseen);
    },
  );

  it('does not warn on nukePositions, which cancels nothing', async () => {
    const r = fixture('hyperliquid');
    await r.start();
    r.update({ positions: [position()] });
    const nuking = r.exchange.nukePositions();
    await vi.advanceTimersByTimeAsync(600);
    expect((await nuking).warning).toBeUndefined();
  });

  it.each(['bybit', 'bybitdemo'] as const)(
    'never issues the %s proving read under 500 ms after a close, even with the store already flat',
    async (venue) => {
      const r = fixture(venue);
      await r.start();
      r.update({ positions: [position()] });
      const nuking = r.exchange.nukePositions();
      await vi.advanceTimersByTimeAsync(499);
      expect(placeRead(r)).toContain('placeOrders');
      expect(placeRead(r)).not.toContain('readPositions');
      await vi.advanceTimersByTimeAsync(2);
      expect(placeRead(r).filter((op) => op === 'readPositions')).toHaveLength(1);
      await nuking;
    },
  );

  it('stops a bybitdemo nuke after a failed proof following a close', async () => {
    const r = fixture('bybitdemo');
    await r.start();
    r.update({ positions: [position()] });
    const cause = Object.assign(new Error('positions snapshot unavailable'), { kind: 'network' });
    const read = vi.spyOn(r.exchange, 'readPositions').mockRejectedValue(cause);
    const nuking = r.exchange.nukePositions().catch((error: unknown) => error);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(await nuking).toMatchObject({
      message: 'Nuke could not confirm the account is flat: positions snapshot unavailable',
      cause,
    });
    expect(read).toHaveBeenCalledOnce();
    expect(r.commands.filter((c) => c.op === 'placeOrders')).toHaveLength(1);
  });

  it('does not report flat from a flat store while the read still shows a position', async () => {
    const r = fixture();
    await r.start();
    r.update({ positions: [position()] });
    const open = [{ symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 13.8 }];
    r.exchange.readPositions = () => Promise.resolve({ skipped: [], open });
    const nuking = r.exchange.nukePositions().catch((thrown: Error) => thrown.message);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(await nuking).toBe('Nuke did not leave the account flat: BTCUSDT long 13.8 still open');
  });

  it.each([
    ['an old array', []],
    ['null', null],
    ['a partial row', { skipped: [], open: [{ symbol: 'BTCUSDT' }] }],
  ])('treats %s from the read as not confirmed flat, with no further close', async (_name, body) => {
    const r = fixture();
    await r.start();
    r.update({ positions: [position()] });
    r.exchange.readPositions = () => Promise.resolve(body as never);
    const nuking = r.exchange.nukePositions().catch((thrown: Error) => thrown.message);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(await nuking).toMatch(/could not confirm the account is flat/);
    expect(r.commands.filter((c) => c.op === 'placeOrders')).toHaveLength(1);
  });

  it('reads with nothing in the store, then closes what the read committed', async () => {
    const r = fixture();
    await r.start();
    r.update({ positions: [] });
    const read = vi.spyOn(r.exchange, 'readPositions').mockImplementationOnce(async () => {
      r.update({ positions: [position()] });
      await vi.advanceTimersByTimeAsync(0);
      return { skipped: [], open: [{ symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 2 }] };
    });
    const nuking = r.exchange.nukePositions();
    await vi.advanceTimersByTimeAsync(1_000);
    await nuking;
    expect(read).toHaveBeenCalledTimes(2);
    expect(r.commands.filter((c) => c.op === 'placeOrders')).toHaveLength(1);
  });

  it('closes again when a successful close left a position, and says so if it never goes flat', async () => {
    vi.useFakeTimers();
    try {
      const r = fixture();
      await r.start();
      r.update({ positions: [position()] });
      const place = vi.spyOn(r.exchange, 'placeOrders').mockResolvedValue([]);
      const nuking = r.exchange.nukePositions(3).catch((thrown: unknown) => thrown);
      await vi.advanceTimersByTimeAsync(10_000);
      expect(((await nuking) as Error).message).toMatch(/did not leave the account flat: BTCUSDT/);
      expect(place).toHaveBeenCalledTimes(3);
    } finally {
      vi.useRealTimers();
    }
  });

  it.each([
    [true, 0],
    [false, 1],
  ])('with the host owning placement failures %s, a refusal is reported %i times here', async (owned, reports) => {
    const r = fixture();
    await r.start();
    (r.exchange as any).options = { ...r.exchange.options, callerReportsPlacementFailures: owned };
    (r.exchange as any).host.command = vi.fn().mockRejectedValue(new Error('Insufficient margin'));
    const error = vi.fn();
    r.exchange.on('error', error);
    await expect(
      r.exchange.placeOrders([{ symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Market, amount: 1 }]),
    ).rejects.toThrow('Insufficient margin');
    expect(error).toHaveBeenCalledTimes(reports);
  });

  it('forces an algo error past the gates that drop errors on a down stream', async () => {
    const r = fixture();
    await r.start();
    const error = vi.fn();
    r.exchange.on('error', error);
    (r.exchange as any).receiveAlgoEmit('error', 'Chaser failed to place orders after 3 tries');
    expect(error).toHaveBeenCalledWith('Chaser failed to place orders after 3 tries', { forceToast: true });
  });

  it('names a cause shared by both nuke legs once', async () => {
    const r = fixture();
    await r.start();
    r.exchange.fetchPositions = () => Promise.reject(new Error('account is closed'));
    r.exchange.cancelAllOrders = () => Promise.reject(new Error('account is closed'));
    await expect(r.exchange.nuke()).rejects.toThrow(/^account is closed$/);
  });

  it('reports not confirmed flat when the read skipped an open row', async () => {
    const r = fixture();
    await r.start();
    r.exchange.readPositions = () => Promise.resolve({ skipped: ['DOGEUSD_PERP'], open: [] });
    await expect(r.exchange.nukePositions()).rejects.toThrow(
      'Nuke could not confirm the account is flat: DOGEUSD_PERP on no listed market',
    );
  });

  it('does not invent a close for empty native positions despite a stale JS row', async () => {
    const r = fixture();
    await r.start();
    r.store.update({ positions: [position()] });
    await r.exchange.nukePositions(3);
    expect(r.commands.filter((c) => c.op === 'placeOrders')).toEqual([]);
  });

  it.each(['partial', 'unknown'] as const)(
    'keeps a %s close outcome, names the symbol, retries reduce-only 3 times and cancels orders',
    async (kind) => {
      const r = fixture();
      await r.start();
      r.update({ positions: [position()], orders: [order()] });
      const failure = Object.freeze(
        Object.assign(
          new Error('Close interrupted'),
          kind === 'unknown'
            ? { outcome: 'unknown', clientOrderId: 'close-request', actualClientOrderIds: ['native-close'] }
            : { partialOrderIds: ['accepted'], partialOrderCount: 1, totalOrderCount: 2 },
        ),
      );
      const place = vi.spyOn(r.exchange, 'placeOrders').mockRejectedValue(failure);
      const error = vi.fn();
      r.exchange.on('error', error);
      const nuking = r.exchange.nuke(3).catch((thrown: unknown) => thrown);
      await vi.advanceTimersByTimeAsync(300);
      const thrown = (await nuking) as Error;
      expect(thrown.message).toBe('Nuke could not close BTCUSDT: Close interrupted');
      expect(thrown).toMatchObject(
        kind === 'unknown' ? { outcome: 'unknown', clientOrderId: 'close-request' } : { partialOrderIds: ['accepted'] },
      );
      expect(place).toHaveBeenCalledTimes(4);
      expect(place.mock.calls.every(([[close]]) => close.reduceOnly === true)).toBe(true);
      expect(error).toHaveBeenCalledOnce();
      expect(r.commands.filter((c) => c.op === 'cancelSymbolOrders')).toHaveLength(1);
      expect(r.exchange.isNuking).toBe(false);
    },
  );

  it('a transient close failure closes on retry with no final error', async () => {
    const r = fixture();
    await r.start();
    r.update({ positions: [position()] });
    const place = vi
      .spyOn(r.exchange, 'placeOrders')
      .mockRejectedValueOnce(new Error('busy'))
      .mockImplementationOnce(async () => {
        r.update({ positions: [] });
        return ['closed'];
      });
    const nuking = r.exchange.nukePositions();
    await vi.advanceTimersByTimeAsync(700);
    await expect(nuking).resolves.toBeDefined();
    expect(place).toHaveBeenCalledTimes(2);
  });

  it('a successful close behind a lagging store is not resent and is not a failure', async () => {
    const r = fixture();
    await r.start();
    r.update({ positions: [position('BTCUSDT'), position('ETHUSDT')] });
    const place = vi.spyOn(r.exchange, 'placeOrders').mockImplementation(async ([close]) => {
      if (close.symbol === 'ETHUSDT') throw new Error('busy');
      return ['close'];
    });
    const nuking = r.exchange.nukePositions().catch((thrown: unknown) => thrown);
    await vi.advanceTimersByTimeAsync(300);
    expect(((await nuking) as Error).message).toBe('Nuke could not close ETHUSDT: busy');
    expect(place.mock.calls.filter(([[close]]) => close.symbol === 'BTCUSDT')).toHaveLength(1);
    expect(place.mock.calls.filter(([[close]]) => close.symbol === 'ETHUSDT')).toHaveLength(4);
  });

  it("stops retrying once a failed close's position is gone", async () => {
    const r = fixture();
    await r.start();
    r.update({ positions: [position()] });
    const place = vi.spyOn(r.exchange, 'placeOrders').mockImplementation(async () => {
      r.update({ positions: [] });
      throw new Error('answer lost');
    });
    const nuking = r.exchange.nukePositions();
    await vi.advanceTimersByTimeAsync(600);
    await expect(nuking).resolves.toBeDefined();
    expect(place).toHaveBeenCalledOnce();
  });

  it('tries every symbol after one cancel fails, then names the failure and what is left', async () => {
    const r = fixture('bybit');
    await r.start();
    r.update({
      positions: [],
      orders: [order('eth-stop', 'ETHUSDT'), order('btc-limit', 'BTCUSDT')],
    });
    const cancel = vi.spyOn(r.exchange, 'cancelSymbolOrders').mockImplementation(async (symbol) => {
      if (symbol === 'ETHUSDT') throw new Error('left eth-stop');
    });
    const logged = vi.spyOn(r.exchange, 'log');
    await expect(r.exchange.nuke(3)).rejects.toThrow('ETHUSDT: left eth-stop');
    expect(cancel.mock.calls.map(([symbol]) => symbol)).toEqual(['ETHUSDT', 'BTCUSDT']);
    expect(logged.mock.calls.flat().join(' ')).toContain('ETHUSDT: eth-stop');
    expect(r.exchange.isNuking).toBe(false);
  });

  // Ruling 2026-10-30: a running chaser or TWAP would place again behind the nuke.
  it('stops every running chaser and TWAP on the account first, then cancels and closes, and reports them', async () => {
    const r = fixture();
    await r.start();
    r.update({ positions: [position()], orders: [order()] });
    await vi.advanceTimersByTimeAsync(250);
    const chaser = new ChaserOrderInstance(r.exchange, {
      symbol: 'BTCUSDT',
      size: 2,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });
    chaserOrderManagerInstance.add(chaser);
    const twap = new TwapInstance(
      r.exchange,
      { postMessage: vi.fn() } as unknown as Worker,
      { accountName: 'a', activeMarket: markets[0], size: 1, duration: 60, ordersCount: 2 } as TwapOptions,
      () => {},
    );
    twapManager.add(twap);
    // Another account's chaser is not this nuke's to stop.
    const other = fixture();
    await other.start();
    const foreign = new ChaserOrderInstance(other.exchange, {
      symbol: 'BTCUSDT',
      size: 2,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });
    chaserOrderManagerInstance.add(foreign);
    try {
      await vi.advanceTimersByTimeAsync(1000);
      const start = r.commands.length;
      const nuking = r.exchange.nuke(3);
      await vi.advanceTimersByTimeAsync(1000);
      expect(await nuking).toEqual({ stoppedChasers: [chaser.id], stoppedTwaps: [twap.id] });
      expect(chaser.isStopped).toBe(true);
      expect(foreign.isStopped).toBe(false);
      expect(twapManager.getById(twap.id)).toBeUndefined();
      const ops = r.commands.slice(start).map((c) => c.op);
      // The chaser's own cancel lands before the nuke closes anything.
      expect(ops.indexOf('placeOrders')).toBeGreaterThan(-1);
      expect(ops.slice(0, ops.indexOf('placeOrders'))).toContain('cancelOrders');
      await vi.advanceTimersByTimeAsync(30_000);
      expect(r.commands.slice(start).filter((c) => c.op === 'placeOrders')).toHaveLength(1);
    } finally {
      chaserOrderManagerInstance.cleanupExchange(r.exchange);
      chaserOrderManagerInstance.cleanupExchange(other.exchange);
    }
  });

  it('nukeOrders also stops the account algos before its cancel-all', async () => {
    const r = fixture();
    await r.start();
    r.update({ orders: [order()] });
    await vi.advanceTimersByTimeAsync(250);
    const chaser = new ChaserOrderInstance(r.exchange, {
      symbol: 'BTCUSDT',
      size: 2,
      side: OrderSide.Buy,
      maxAttempts: 10,
      reduceOnly: false,
      delay: 5000,
    });
    chaserOrderManagerInstance.add(chaser);
    try {
      await vi.advanceTimersByTimeAsync(1000);
      const nuking = r.exchange.nukeOrders();
      await vi.advanceTimersByTimeAsync(1000);
      expect(await nuking).toEqual({ stoppedChasers: [chaser.id], stoppedTwaps: [] });
      expect(chaser.isStopped).toBe(true);
      expect(r.exchange.isNuking).toBe(false);
    } finally {
      chaserOrderManagerInstance.cleanupExchange(r.exchange);
    }
  });

  it('keeps the first nuke lock when a concurrent caller is rejected', async () => {
    const r = fixture();
    await r.start();
    r.update({ positions: [position()] });
    let finish!: (ids: string[]) => void;
    vi.spyOn(r.exchange, 'placeOrders').mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const first = r.exchange.nukePositions();
    await vi.advanceTimersByTimeAsync(0);
    await expect(r.exchange.nukePositions()).rejects.toThrow('already in progress');
    expect(r.exchange.isNuking).toBe(true);
    r.update({ positions: [] });
    finish(['accepted']);
    await vi.advanceTimersByTimeAsync(600);
    await first;
    expect(r.exchange.isNuking).toBe(false);
  });

  it('still closes valid positions when another one cannot be sized', async () => {
    const r = fixture('mexc');
    await r.start();
    r.update({
      positions: [
        { ...position(), id: 'BTCUSDT-long' },
        { ...position(), id: 'ETHUSDT-long', symbol: 'ETHUSDT', contracts: NaN },
      ],
    });
    await expect(r.exchange.nukePositions()).rejects.toThrow('Nuke could not close ETHUSDT: invalid close size');
    expect(r.commands.filter((c) => c.op === 'placeOrders').map((c) => c.orders)).toEqual([
      [{ symbol: 'BTCUSDT', side: OrderSide.Sell, type: OrderType.Market, amount: 20, reduceOnly: true }],
    ]);
    expect(r.exchange.isNuking).toBe(false);
  });

  it('reports a missing market without closing anything for it', async () => {
    const r = fixture();
    await r.start();
    r.store.update({ markets: [] });
    r.update({ positions: [position()] });
    await expect(r.exchange.nukePositions()).rejects.toThrow('BTCUSDT: market not found');
    expect(r.commands.filter((c) => c.op === 'placeOrders')).toEqual([]);
    expect(r.exchange.isNuking).toBe(false);
  });

  it('refuses a nuke close when native/derived size evidence disagrees', async () => {
    const r = fixture();
    await r.start();
    r.update({ positions: [{ ...position(), extra: { contractsNative: 10 } }] });
    await expect(r.exchange.nukePositions()).rejects.toThrow(/invalid close size|unknown/i);
    expect(r.commands.filter((c) => c.op === 'placeOrders')).toEqual([]);
    expect(r.exchange.isNuking).toBe(false);
  });

  it('reports nonfinite native sizing inputs without dispatching that close', async () => {
    const r = fixture();
    await r.start();
    r.update({ positions: [{ ...position(), contracts: NaN }] });
    await expect(r.exchange.nukePositions()).rejects.toThrow('BTCUSDT: invalid close size');
    expect(r.commands.filter((c) => c.op === 'placeOrders')).toEqual([]);
    expect(r.exchange.isNuking).toBe(false);
  });
});
